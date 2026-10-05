#!/usr/bin/env pwsh
# QEMU lab loop: 6GB / 8 cores / WHPX / max HW simulation / boot monitor + auto restart.
#
# WHY A SEPARATE SCRIPT (and a separate binary copy):
#   Another thread in this repo runs a loop that does
#       Get-Process qemu-system-x86_64 | Stop-Process -Force
#   and shares target\uefi.img + target\disk_qemu.raw + hostfwd 4445/4446 +
#   monitor 5555. Sharing any of those corrupts the other run or dies with it.
#   So this loop uses:
#     - its own binary copy  target\lab\lab8c-vm.exe   (process name differs,
#       so a "qemu-system-x86_64" kill filter cannot reach it)
#     - its own images      target\lab\*.img|.raw|.fd
#     - its own ports       hostfwd 4455/4456, monitor 5556
#     - `-L <qemu share>`   because the binary moved out of Program Files\qemu
#   This loop NEVER stops a process it did not start.
#
# ASCII-ONLY (PS 5.1 reads this file as CP1252; non-ASCII becomes garbage
# without a syntax error). Non-ASCII would break the pattern matches.
param(
    [int]$Cores = 8,
    [int]$RamGB = 6,
    [int]$BootSeconds = 480,   # how long each boot is observed
    [int]$StallSeconds = 120,  # no new serial bytes for this long => stalled
    [int]$MaxCycles = 0,       # 0 = run until stopped
    [int]$MonitorPort = 5556,
    [int]$FwdA = 4455,
    [int]$FwdB = 4456,
    [switch]$NoRestore,        # skip the per-cycle pristine disk restore
    [switch]$Tcg,
    [switch]$NoModels,         # skip ALL model loaders (LLM + experts)
    [switch]$NoExtraModels,    # load only the LLM, not the expert/TTS blobs
    [switch]$VirtioBlk,        # data disk on virtio-blk (TICKV backend=file), like run-f15.ps1
    [switch]$DryRun            # build the command line, write it out, never launch
)

$ErrorActionPreference = "Continue"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root

$LabDir = Join-Path $Root "target\lab"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $LabDir, $logDir | Out-Null

$vm = Join-Path $LabDir "lab8c-vm.exe"
$uefi = Join-Path $LabDir "uefi.img"
$disk = Join-Path $LabDir "disk_lab.raw"
$pristine = Join-Path $LabDir "disk_lab.pristine.raw"
$ovmfCode = Join-Path $LabDir "ovmf_code.fd"
$ovmfVars = Join-Path $LabDir "ovmf_vars.fd"
$netmodeFile = Join-Path $LabDir "netmode.flag"
$modelFile = Join-Path $Root "target\FALCON3.BIN"
$qemuShare = "C:\Program Files\qemu\share"
$qemuBinDir = "C:\Program Files\qemu"
$statusFile = Join-Path $logDir "lab_loop_status.txt"
$csvFile = Join-Path $logDir "lab_loop_cycles.csv"
# Identificador da execucao. Sem ele o ciclo 1 de uma rodada abortada e o
# ciclo 1 de uma boa ficam com a mesma chave no CSV (medido: 3 linhas QEMU_EXIT
# do bug de overlap ao lado de um PASS, sem nada que os separe). Mesmo spiritu
# do carimbo s14: um veredito sem identificar a execucao que o produziu nao e
# citavel.
$runStamp = Get-Date -Format "yyyyMMdd_HHmmss"

# The binary copy only resolves its DLLs when the QEMU install dir is on PATH
# (measured: without it the copy dies with 0xC0000135 STATUS_DLL_NOT_FOUND
# before emitting a single serial byte, which looks exactly like a WHPX
# failure). The data dir (-L) and the DLL dir (-L's parent) are different
# things; only -L was not enough.
$env:PATH = "$qemuBinDir;$env:PATH"

foreach ($f in @($vm, $uefi, $disk, $ovmfCode, $ovmfVars)) {
    if (-not (Test-Path $f)) { Write-Host "ERRO: artifact missing: $f" -ForegroundColor Red; exit 1 }
}
# Preflight: prove the VM binary can start at all. Without this, a broken
# binary burns a full BootSeconds window and reports "QEMU_EXIT" with no cause.
$preOut = & $vm --version 2>&1
if ($LASTEXITCODE -ne 0 -or -not ($preOut -join " ")) {
    Write-Host "ERRO: $vm nao roda (rc=$LASTEXITCODE): $preOut" -ForegroundColor Red
    Write-Host "Cheque \$env:PATH contenha '$qemuBinDir' e que -L aponte para $qemuShare" -ForegroundColor Yellow
    exit 2
}
Write-Host "preflight vm ok: $($preOut[0])"

if (-not (Test-Path $csvFile)) {
    "cycle,stamp,log,bytes,phase7,tick_max,verdict,pf,panic,exc_runtime,exc_demo,corrupt,fail,stalled,restore_sec,lab_state_before,seconds,run" |
        Out-File -FilePath $csvFile -Encoding ascii
}

# Read a file QEMU still has open for writing (FileShare ReadWrite), without
# locking it. A plain Get-Content fails with "used by another process".
function Read-LogShared([string]$path) {
    if (-not (Test-Path $path)) { return "" }
    try {
        $fs = [System.IO.File]::Open($path, 'Open', 'Read', 'ReadWrite')
        try {
            $sr = New-Object System.IO.StreamReader($fs)
            return $sr.ReadToEnd()
        } finally { $sr.Close(); $fs.Close() }
    } catch { return "" }
}

function Strip-Ansi([string]$t) {
    return [regex]::Replace($t, '\x1b\[[0-9;?]*[ -/]*[@-~]', '')
}

function Count-Match([string]$text, [string]$pattern) {
    $m = [regex]::Matches($text, $pattern, 'IgnoreCase')
    return $m.Count
}

# Singleton guard. Two instances of this loop writing the same CSV and the same
# status file is not a duplicate, it is two instruments disagreeing about the
# same boot (measured: an old instance kept running the previous version of this
# script and interleaved its output, which is how a "WHPX falhou" verdict appeared
# with no WHPX VM in the log at all).
$me = $PID
$others = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
    Where-Object {
        $_.ProcessId -ne $me -and
        $_.Name -like "powershell*" -and
        $_.CommandLine -and
        $_.CommandLine.Contains("-File") -and
        $_.CommandLine.Contains("run-qemu-lab-loop.ps1")
    })
if ($others.Count -gt 0 -and -not $DryRun) {
    Write-Host "ABORT: ja existe $($others.Count) instancia(s) deste loop (pids: $(($others | ForEach-Object { $_.ProcessId }) -join ','))" -ForegroundColor Red
    exit 4
}
Set-Content -Path (Join-Path $logDir "lab_loop.pid") -Value $me -Encoding ascii

# Kill only VMs of THIS lab. The isolation boundary is the binary name: the
# file target\lab\lab8c-vm.exe is a copy nobody else runs, so matching on the
# name is precise. A wildcard "stop all qemu*" would kill the other thread's
# boot lab; the cmdline check on our own disk is kept as a second condition
# because QEMU can fork a WHPX worker whose command line is empty (measured: a
# killed parent left a live worker holding the disk and the monitor port, and
# every later cycle died with "Failed to find an available port" / Errno 22).
function Stop-OurVms {
    for ($round = 0; $round -lt 4; $round++) {
        $left = @(Get-Process -Name "lab8c-vm" -ErrorAction SilentlyContinue)
        if ($left.Count -eq 0) { break }
        foreach ($p in $left) {
            Write-Host "[loop] stopando VM orfa pid=$($p.Id)" -ForegroundColor DarkYellow
            Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        }
        Start-Sleep -Milliseconds 800
    }
    Start-Sleep -Seconds 1
}

# Phase markers the boot publishes on the EventBus; n=7 Runtime is the goal.
function Test-Boot([string]$clean) {
    $phases = @()
    foreach ($m in [regex]::Matches($clean, 'PHASE n=(\d) name=(\w+) status=(\w+)')) {
        $phases += $m.Groups[1].Value
    }
    $tickMax = 0
    foreach ($m in [regex]::Matches($clean, '\[T\+(\d+)\]')) {
        $v = [int]$m.Groups[1].Value
        if ($v -gt $tickMax) { $tickMax = $v }
    }
    # Exceptions split in two classes. Measured on a good boot: 8 [EXC] lines
    # that are the P6/P7 self-test demos (demand-paging #PF at ip=0x7000_0003_000a,
    # #UD from the Ring3 probe) - all expected, all BEFORE the Runtime phase.
    # Counting them as failures is how an instrument starts lying, so the boot
    # verdict only counts exceptions that happen AFTER the Runtime phase, plus
    # the unambiguous corruption markers.
    $runtimeIdx = $clean.LastIndexOf("PHASE n=8")
    if ($runtimeIdx -lt 0) { $runtimeIdx = $clean.LastIndexOf("PHASE n=7") }
    $pre = $(if ($runtimeIdx -gt 0) { $clean.Substring(0, $runtimeIdx) } else { $clean })
    $post = $(if ($runtimeIdx -gt 0) { $clean.Substring($runtimeIdx) } else { "" })
    $excDemo = Count-Match $pre '\[EXC\]'
    $exc = Count-Match $post '\[EXC\]'
    $pf = Count-Match $clean '\[PF_DBG\]|#PF storm'
    $panic = Count-Match $clean 'PANIC|panic=|Triple fault|KERNEL_ERROR'
    $corrupt = Count-Match $clean 'REMAP bei|corrupt diag|heap.*corrupt'
    $remedap = Count-Match $clean 'REMAP'
    $failL = Count-Match $clean '\[fail\]'
    return @{
        Phases = $phases
        Phase7 = ($phases -contains "7")
        PhaseLast = $(if ($phases.Count -gt 0) { $phases[-1] } else { "-" })
        TickMax = $tickMax
        Pf = $pf
        Panic = $panic
        Exc = $exc
        ExcDemo = $excDemo
        Corrupt = $corrupt
        FailL = $failL
        Remedap = $remedap
    }
}

function Write-Status([string]$text) {
    $tmp = "$statusFile.tmp"
    Set-Content -Path $tmp -Value $text -Encoding ascii -NoNewline
    Move-Item -Force $tmp $statusFile
}

# ---------------------------------------------------------------------------
# One cycle: restore disk -> launch -> observe -> verdict -> stop (own pid only)
# ---------------------------------------------------------------------------
function Invoke-Cycle([int]$cycle, [string]$accel, [string]$cpu) {
    $stamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $bootLog = Join-Path $logDir "lab8c_c${cycle}_$stamp.txt"

    # --- [7] ablation arm: start every boot from a known-clean disk ---------
    $restoreSec = 0.0
    $labBefore = "skip"
    if (-not $NoRestore -and (Test-Path $pristine)) {
        $swR = [Diagnostics.Stopwatch]::StartNew()
        $out = & python (Join-Path $Root "tools\f15_pristine.py") restore --disk $disk --pristine $pristine 2>&1
        $rc = $LASTEXITCODE
        $swR.Stop()
        $restoreSec = [math]::Round($swR.Elapsed.TotalSeconds, 1)
        if ($rc -ne 0) {
            # Fail-closed: a cycle that boots a disk we could not restore is not
            # evidence about the boot, it is evidence about our own tooling.
            # Retry once after making sure no VM of ours still holds the file.
            Write-Host "[cycle $cycle] restore falhou (rc=$rc) - tentando de novo sem VM viva" -ForegroundColor Red
            Stop-OurVms
            $out2 = & python (Join-Path $Root "tools\f15_pristine.py") restore --disk $disk --pristine $pristine 2>&1
            $rc = $LASTEXITCODE
            if ($rc -ne 0) {
                Write-Host "[cycle $cycle] CICLO CANCELADO: restore falhou de novo (rc=$rc): $out2" -ForegroundColor Red
                return "SKIP_RESTORE"
            }
        }
        $scan = & python (Join-Path $Root "tools\f15_pristine.py") scan --disk $disk 2>&1
        $labBefore = (($scan | Select-String 'lab_state=(\d)').Matches.Groups[1].Value)
        if ($labBefore -ne "0") {
            Write-Host "[cycle $cycle] AVISO: disco ainda com estado do lab (lab_state=$labBefore)" -ForegroundColor Yellow
        }
    } else {
        $out = & python (Join-Path $Root "tools\f15_pristine.py") scan --disk $disk 2>&1
        $labBefore = (($out | Select-String 'lab_state=(\d)').Matches.Groups[1].Value)
    }

    # --- launch -------------------------------------------------------------
    # One monitor port per cycle: a leftover socket in TIME_WAIT from the
    # previous VM makes QEMU abort with "Failed to find an available port"
    # (measured), and a fixed port turns that into a blind retry loop.
    $monPort = $MonitorPort + $cycle
    # -L needs the quotes IN the string: Start-Process joins the ArgumentList
    # array with plain spaces and never quotes, so an unquoted "C:\Program
    # Files\..." arrives at QEMU as two arguments and it dies with
    # "Could not open 'Files\qemu\share'". Measured, both WHPX and TCG.
    # Barramento do disco de dados. run-f15.ps1 (o harness que fecha o F1.5) usa
    # virtio-blk com cache=writethrough: e o caminho que leva o TICKV a
    # backend=file, que e o segundo degrau do funil D3 (persistencia real).
# Sem o switch, o loop usa IDE, que e o bus do launcher canonico.
    $diskArg = "format=raw,file=$disk,if=ide,index=1"
    $diskIf = "ide"
    if ($VirtioBlk) {
        $diskArg = "format=raw,file=$disk,if=virtio,cache=writethrough"
        $diskIf = "virtio"
    }
    $a = @(
        "-L", ('"{0}"' -f $qemuShare),
        "-m", "${RamGB}G", "-smp", "$Cores",
        "-accel", $accel, "-cpu", $cpu,
        "-drive", "format=raw,file=$uefi,if=ide,index=0",
        "-drive", $diskArg,
        "-drive", "if=pflash,format=raw,file=$ovmfCode,readonly=on","-drive", "if=pflash,format=raw,file=$ovmfVars",
        "-monitor", "tcp:127.0.0.1:$monPort,server,nowait",
        "-serial", "file:$bootLog",
        "-serial", "null",
        "-netdev", "user,id=n0,hostfwd=tcp::$FwdA-:$FwdA,hostfwd=tcp::$FwdB-:$FwdB",
        "-device", "e1000,netdev=n0",
        "-audiodev", "none,id=snd0",
        "-device", "intel-hda,id=hda0",
        "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
        "-device", "qemu-xhci,id=xhci",
        "-device", "usb-tablet,bus=xhci.0",
        "-device", "usb-kbd,bus=xhci.0",
        "-device", "virtio-gpu-pci,id=vgpu",
        "-vga", "std",
        "-display", "none"
    )
    # Artifacts the AIOS needs: model image + net mode flag, above 4GB so the
    # kernel finds them by address instead of loading 1GB through the heap.
    $modelEnd = 0x100000000
    if (-not $NoModels -and (Test-Path $modelFile)) {
        $sz = (Get-Item $modelFile).Length
        $a += @("-device", "loader,file=$modelFile,addr=$modelEnd")
        $modelEnd = $modelEnd + $sz
    }
    # Expert / TTS / vocab blobs. run-qemu-whpx.ps1 only scans target\, so these
    # never reach the guest there (measured: "RUSTCODER QEMU-loader scan
    # [0x129000000..0x129200000] - 0xBE11BE11 ausente"). The kernel discovers
    # them by magic in [0x100000000..0x180000000), so what matters is only that
    # they sit in that window without overlapping each other or the two expert
    # windows the kernel reads (RUSTCODER 0x129000000, HWEXPERT 0x129200000).
    # Addresses below start at 0x160000000, above the 989MB LLM and clear of
    # both expert windows, and stay under 6GB.
    if (-not $NoModels -and -not $NoExtraModels) {
        # Endereco FIXO nao serve: o LLM (989MB) ocupa 0x100000000..0x13DD9BAB0,
        # e a janela que o kernel associa ao HWEXPERT comeca em 0x129200000,
        # dentro do modelo. Colocar o expert la faz QEMU abortar com
        # "The following two regions overlap" (medido). Por isso os extras sao
        # empilhados APOS o fim do LLM, com 1MB de folga, como o launcher padrao.
        # O kernel varre [0x129200000..0x180000000) por magic: achando o expert
        # logo apos o modelo, ele para no primeiro e nao alcanca os outros.
        $extras = @(
            "target1\hw_expert_v6.bitnet",   # expert de HW (era a lacuna real)
            "target1\ROUTER.BITNET",
            "target1\STT.BIN",
            "models\PIPER_PT_BR.BIN",
            "models\bpe_vocab.bin"
        )
        # [math]::Ceiling devolve double e o formatador {2:X} exige inteiro:
        # sem o cast o Write-Host estoura com FormatError (medido).
        $extraAddr = [uint64]([math]::Ceiling($modelEnd / 0x100000) * 0x100000)
        foreach ($rel in $extras) {
            $p = Join-Path $Root $rel
            if (Test-Path $p) {
                $a += @("-device", "loader,file=$p,addr=$extraAddr")
                $sz = [uint64](Get-Item $p).Length
                Write-Host ("[ciclo {0}] extra loader: {1} @0x{2:X} ({3} B)" -f $cycle, $rel, $extraAddr, $sz) -ForegroundColor DarkGray
                $extraAddr = $extraAddr + [uint64]([math]::Ceiling($sz / 0x100000) * 0x100000) + 0x100000
            } else {
                Write-Host "[ciclo $cycle] extra ausente: $rel" -ForegroundColor DarkYellow
            }
        }
    }
    # netmode flag goes last, above everything (modelEnd is the LLM end).
    if (Test-Path $netmodeFile) {
        $a += @("-device", "loader,file=$netmodeFile,addr=$modelEnd")
    }

    if ($DryRun) {
        $dryFile = Join-Path $logDir "lab_dryrun_args.txt"
        Set-Content -Path $dryFile -Value (($a | ForEach-Object { '[' + $_ + ']' }) -join "`n") -Encoding ascii
        Write-Host "[dryrun] args gravados em $dryFile ($($a.Count) tokens)"
        return "DRYRUN"
    }

    Write-Host "[cycle $cycle] accel=$accel cpu=$cpu smp=$Cores ram=${RamGB}G restore=${restoreSec}s lab_state_before=$labBefore" -ForegroundColor Cyan
    Write-Host "[cycle $cycle] log=$bootLog"

    $qemuErr = Join-Path $logDir "lab8c_c${cycle}_${stamp}.qemu.err"
    # A linha de comando EXATA que o QEMU recebe. Sem isto, um erro de parse
    # do QEMU ("drive with bus=0, unit=0 exists") so diz qual spec foi
'    # rejeitada, nunca o que mais foi passado junto \u2014 e bissecar no escuro'
    # custa um boot por tentativa (medido: 4 tentativas ate isolar).
    $argDump = Join-Path $logDir "lab8c_c${cycle}_${stamp}.args.txt"
    Set-Content -Path $argDump -Encoding ascii -Value (($a | ForEach-Object { "[$_]" }) -join "`n")
    $proc = Start-Process -FilePath $vm -ArgumentList $a -PassThru -WindowStyle Hidden `
        -RedirectStandardError $qemuErr
    $proc.Handle | Out-Null   # cache handle so ExitCode/HasExited are reliable

    $sw = [Diagnostics.Stopwatch]::StartNew()
    $lastBytes = -1
    $lastChange = $sw.Elapsed.TotalSeconds
    $verdict = "TIMEOUT"
    $chk = $null
    while ($sw.Elapsed.TotalSeconds -lt $BootSeconds) {
        Start-Sleep -Seconds 10
        $len = 0
        if (Test-Path $bootLog) { $len = (Get-Item $bootLog).Length }
        if ($len -ne $lastBytes) {
            $lastBytes = $len
            $lastChange = $sw.Elapsed.TotalSeconds
        }
        $clean = Strip-Ansi (Read-LogShared $bootLog)
        $chk = Test-Boot $clean
        $status = ("cycle={0} t={1:N0}s bytes={2} phase_last={3} tick={4} pf={5} panic={6} excRT={7} excDemo={8} corrupt={9} fail={10} accel={11} disk_if={12} log={13}" -f `
                $cycle, $sw.Elapsed.TotalSeconds, $len, $chk.PhaseLast, $chk.TickMax, $chk.Pf, $chk.Panic, $chk.Exc, $chk.ExcDemo, $chk.Corrupt, $chk.FailL, $accel, $diskIf, (Split-Path -Leaf $bootLog))
        Write-Status $status
        Write-Host $status
        if ($proc.HasExited) {
            $verdict = "QEMU_EXIT"
            $qemuMsg = ""
            if (Test-Path $qemuErr) {
                $qemuMsg = ((Get-Content $qemuErr -ErrorAction SilentlyContinue | Select-Object -Last 3) -join " / ")
            }
            Write-Host "[cycle $cycle] QEMU saiu: rc=$($proc.ExitCode) err=$qemuMsg" -ForegroundColor Red
            break
        }
        # Stall: no serial traffic at all for StallSeconds. Only meaningful
        # after the kernel started logging; before that the guest is silent.
        if ($len -gt 20000 -and ($sw.Elapsed.TotalSeconds - $lastChange) -gt $StallSeconds) {
            $verdict = "STALL"
            break
        }
    }
    if ($verdict -eq "TIMEOUT" -and $chk) {
        if ($chk.Panic -gt 0) { $verdict = "PANIC" }
        elseif ($chk.Pf -gt 0) { $verdict = "PF" }
        elseif ($chk.Corrupt -gt 0) { $verdict = "CORRUPT" }
        elseif ($chk.Exc -gt 0) { $verdict = "EXC_RUNTIME" }
        elseif ($chk.Phase7 -and $chk.TickMax -gt 100) { $verdict = "PASS" }
        elseif ($chk.Phase7) { $verdict = "PHASE7_STALLED" }
        else { $verdict = "NO_PHASE7" }
    }
    $secs = [math]::Round($sw.Elapsed.TotalSeconds, 0)

    Write-Host "[cycle $cycle] VERDICT=$verdict after ${secs}s log=$bootLog" -ForegroundColor $(if ($verdict -eq "PASS") { "Green" } else { "Yellow" })

    # Stop ONLY our own VM. Never a wildcard kill: another thread runs VMs here.
    try {
        if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    } catch { }
    # QEMU with WHPX can leave the child vp worker; clean only ours by parent id.
    try {
        Get-CimInstance Win32_Process -Filter "ParentProcessId=$($proc.Id)" -ErrorAction SilentlyContinue |
            ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    } catch { }
    try { $proc.WaitForExit(15000) | Out-Null } catch { }
    # Flush the guest disk so the next cycle reads a consistent image.
    Start-Sleep -Seconds 3

    $row = ("{0},{1},{2},{3},{4},{5},{6},{7},{8},{9},{10},{11},{12},{13},{14},{15},{16},{17}" -f `
            $cycle, $stamp, $bootLog, $lastBytes, $(if ($chk) { $chk.Phase7 } else { "?" }), $(if ($chk) { $chk.TickMax } else { 0 }),
            $verdict, $(if ($chk) { $chk.Pf } else { 0 }), $(if ($chk) { $chk.Panic } else { 0 }),
            $(if ($chk) { $chk.Exc } else { 0 }), $(if ($chk) { $chk.ExcDemo } else { 0 }),
            $(if ($chk) { $chk.Corrupt } else { 0 }), $(if ($chk) { $chk.FailL } else { 0 }),
            $(if ($verdict -eq "STALL") { 1 } else { 0 }), $restoreSec, $labBefore, $secs, $runStamp)
    Add-Content -Path $csvFile -Value $row -Encoding ascii
    return $verdict
}

$accel = "whpx"
$cpu = "Haswell"
if ($Tcg) { $accel = "tcg"; $cpu = "max" }

Write-Host "=== QEMU LAB LOOP: ${RamGB}G / ${Cores}c / accel=$accel / BootSeconds=$BootSeconds ===" -ForegroundColor Green
Write-Host "vm=$vm"
Write-Host "disk=$disk (pristine restore: $(-not $NoRestore))"
Write-Host "csv=$csvFile  status=$statusFile  run=$runStamp"
Write-Host "NOTE: this loop never kills a qemu it did not start (another thread shares this host)." -ForegroundColor DarkGray

$cycle = 0
$consecutiveExit = 0
while ($true) {
    $cycle++
    if ($MaxCycles -gt 0 -and $cycle -gt $MaxCycles) { break }
    Stop-OurVms   # a previous run of this loop may have been killed and orphaned one
    $v = Invoke-Cycle -cycle $cycle -accel $accel -cpu $cpu
    if ($v -eq "DRYRUN") { break }
    # A VM that never starts is a broken command line, not a flaky boot. Retry
    # twice so the loop reports the cause instead of spinning forever.
    if ($v -eq "QEMU_EXIT") { $consecutiveExit++ } else { $consecutiveExit = 0 }
    if ($v -eq "SKIP_RESTORE") {
        Write-Host "[loop] ciclo pulado: disco nao pode ser restaurado (ver acima)" -ForegroundColor Yellow
    }
    if ($consecutiveExit -ge 3) {
        Write-Host "[loop] ABORT: 3 QEMU_EXIT seguidos. Veja o .qemu.err do ultimo ciclo." -ForegroundColor Red
        exit 3
    }
    if ($v -eq "QEMU_EXIT" -and $cycle -eq 1 -and $accel -eq "whpx") {
        # WHPX refused the VM (VP exit / already running). One TCG retry, same
        # hardware simulation, slower but real.
        Write-Host "[loop] WHPX falhou no 1o ciclo; tentando TCG" -ForegroundColor Yellow
        $accel = "tcg"; $cpu = "max"
        $cycle--
        continue
    }
    Start-Sleep -Seconds 5
}
Write-Host "[loop] finished $cycle cycles" -ForegroundColor Cyan
exit 0