# Lab: Falcon3 clima Passo Fundo/RS
# Injeta via QMP sendkey o transcript pos-STT
# (CTC sintetico nao decodifica fala real - honesty SESSION_346/352).
#
# Uso:
#   powershell -File tools\lab-mesh2-clima-passo-fundo.ps1
#   powershell -File tools\lab-mesh2-clima-passo-fundo.ps1 -SkipBuild -SingleNode
#
# Aceite (logs A):
#   - llm=LOADED + CTC/Piper OK
#   - USER_INTENT / InferQueue submit apos inject
#   - HERMES_RESPONSE ou TTS com passo/fundo/temp/clima (weatherish)
# Nota: mesh STATIC (10.0.3.x) NAO tem slirp - HTTP wttr/open-meteo indisponivel;
#       Falcon3 processa o pedido (generate); temperatura AO VIVO = AWAITING net bridge.

param(
    [int]$RamGB = 8,
    [int]$Smp = 6,
    [ValidateSet('whpx','tcg')][string]$Accel = 'whpx',
    [int]$WaitBootSec = 300,
    [int]$WaitInferSec = 360,
    [switch]$SkipBuild,
    [switch]$SingleNode,
    [switch]$NoDisplay,
    [string]$Phrase = 'jarbas, que temperatura esta agora em passo fundo?'
)

$ErrorActionPreference = 'Stop'
$Root = 'C:\DEV\neural-os-core-latest'
Set-Location $Root

$PromptFile = 'logs\lab_clima_passo_fundo_prompt.txt'
$ResultFile = 'logs\lab_clima_passo_fundo_result.txt'
New-Item -ItemType Directory -Force -Path logs, target | Out-Null

function Write-Result([string]$msg) {
    Add-Content -Path $ResultFile -Value $msg -Encoding utf8
    Write-Host $msg
}

Set-Content -Path $ResultFile -Value ("started {0}" -f (Get-Date -Format o)) -Encoding utf8

# --- host RAM gate ---
$freeGB = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 2)
Write-Host ("host freeGB={0}" -f $freeGB) -ForegroundColor Yellow

# 2x guests need ~2*RamGB + 2GB headroom; else force SingleNode + shrink RamGB
$wantDual = -not $SingleNode
$needDual = (2 * $RamGB) + 2.5
if ($wantDual -and ($freeGB -lt $needDual)) {
    Write-Host ("WARN freeGB={0} < need~{1} for 2x{2}G -> SingleNode" -f $freeGB, $needDual, $RamGB) -ForegroundColor Yellow
    $SingleNode = $true
    Write-Result ("AUTO SingleNode (freeGB={0} needDual~{1})" -f $freeGB, $needDual)
}
# STT pin @0x163000000 exige guest >=6G; se host apertado, desce Smp e aceita 6G overcommit
if ($RamGB -lt 6) { $RamGB = 6 }
if ($SingleNode -and ($freeGB -lt ($RamGB + 1.5)) -and ($RamGB -gt 5)) {
    Write-Host ("WARN tight RAM free={0} guest={1}G - overcommit pagefile" -f $freeGB, $RamGB) -ForegroundColor Yellow
    if ($Smp -gt 4) { $Smp = 4 }
}

@"
LAB: clima Passo Fundo/RS via Falcon3
PHRASE: $Phrase
NODES: $(if ($SingleNode) { '1 (A)' } else { '2 (A+B mesh)' }) RamGB=$RamGB Smp=$Smp Accel=$Accel
MODE: inject sendkey = pos-STT USER_INTENT (voz real AWAITING CTC+wav)
NET: STATIC mesh - sem HTTP clima ao vivo; LLM processa o pedido
HOST_FREE_GB: $freeGB
"@ | Set-Content -Path $PromptFile -Encoding utf8

# --- rebuild kernel+uefi ---
if (-not $SkipBuild) {
    Write-Host 'BUILD boot (release)...' -ForegroundColor Cyan
    $env:CARGO_TARGET_DIR = 'target/agent-lab-clima'
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    cargo build --release -p boot > logs\lab_clima_build.txt 2>&1
    $buildEc = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    if ($buildEc -ne 0) { throw 'cargo build -p boot FAILED - see logs/lab_clima_build.txt' }
    $canon = Join-Path $Root 'target\uefi.img'
    $alt = Join-Path $Root 'target\agent-lab-clima\uefi.img'
    if ((Test-Path $alt)) {
        $altFull = (Resolve-Path $alt).Path
        $canonFull = if (Test-Path $canon) { (Resolve-Path $canon).Path } else { '' }
        if ($altFull -ne $canonFull) {
            Copy-Item $alt $canon -Force
            Write-Host ("synced uefi.img from {0}" -f $alt) -ForegroundColor Green
        }
    }
}

# --- kill leftovers ---
Get-Process qemu-system-x86_64 -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name='python.exe'" -EA SilentlyContinue |
    Where-Object { $_.CommandLine -match 'qemu_l2_hub' } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -EA SilentlyContinue }
Start-Sleep 2

$qemu = 'C:\PROGRA~1\qemu\qemu-system-x86_64.exe'
if (-not (Test-Path $qemu)) { $qemu = (Get-Command qemu-system-x86_64).Source }
# SHORT path: Start-Process re-parses ArgumentList and splits on spaces (C:\Program -> fail)
$ovmf = 'C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd'
if (-not (Test-Path $ovmf)) { throw "OVMF missing: $ovmf" }
$uefi = (Resolve-Path 'target\uefi.img').Path
$disk = if (Test-Path 'target1\disk_qemu.raw') { (Resolve-Path 'target1\disk_qemu.raw').Path }
        else { (Resolve-Path 'target\disk_qemu.raw').Path }

[System.IO.File]::WriteAllBytes('target\netmode_a.flag', [byte[]]@([byte][char]'S', 10, 0, 3, 2))
[System.IO.File]::WriteAllBytes('target\netmode_b.flag', [byte[]]@([byte][char]'S', 10, 0, 3, 3))

# LINJ lab inject @ 0x02100000 (hermes::lab_inject) — deterministico pos llm=LOADED
$linjPath = Join-Path $Root 'target\lab_inject_clima.bin'
$linjText = [System.Text.Encoding]::ASCII.GetBytes($Phrase)
if ($linjText.Length -gt 240) { $linjText = $linjText[0..239] }
$linjBytes = New-Object byte[] (4 + $linjText.Length + 1)
[System.Text.Encoding]::ASCII.GetBytes('LINJ').CopyTo($linjBytes, 0)
$linjText.CopyTo($linjBytes, 4)
$linjBytes[4 + $linjText.Length] = 0
[System.IO.File]::WriteAllBytes($linjPath, $linjBytes)
Write-Host ("LINJ blob {0} bytes phrase='{1}'" -f $linjBytes.Length, $Phrase) -ForegroundColor Magenta

# Pack slim clima: Falcon1B + Piper + STT + vocab (+ AGENT se couber)
$ordered = @(
    'target1\FALCON3_1B.V6',
    'target1\PIPER_PT_BR.BIN',
    'target1\bpe_vocab.bin',
    'target1\AGENT.v6'
)

$modelGap = [uint64]0x100000
$modelAddr = [uint64]0x100000000
$modelEnd = $modelAddr
$windowEnd = [uint64]$RamGB * 1GB
$loaders = New-Object System.Collections.Generic.List[string]
foreach ($rel in $ordered) {
    $fp = Join-Path $Root $rel
    if (-not (Test-Path $fp)) { continue }
    $len = [uint64](Get-Item $fp).Length
    if ($len -lt 10240) { continue }
    $need = $modelAddr + $len
    if ($need -gt $windowEnd) {
        Write-Host ("skip OOB {0}" -f $rel) -ForegroundColor Yellow
        continue
    }
    $hex = $modelAddr.ToString('X')
    $loaders.Add('-device') | Out-Null
    $loaders.Add(('loader,file={0},addr=0x{1}' -f $fp, $hex)) | Out-Null
    Write-Host ('LOADER {0} @0x{1}' -f $rel, $hex) -ForegroundColor Green
    $modelAddr = [uint64](([math]::Ceiling([double]($need + $modelGap) / [double]$modelGap)) * [double]$modelGap)
    if ($need -gt $modelEnd) { $modelEnd = $need }
}

$sttFp = Join-Path $Root 'target1\STT.BIN'
$sttAddr = [uint64]0x163000000
if ((Test-Path $sttFp) -and ($sttAddr + (Get-Item $sttFp).Length -le $windowEnd)) {
    $loaders.Add('-device') | Out-Null
    $loaders.Add(('loader,file={0},addr=0x{1}' -f $sttFp, $sttAddr.ToString('X'))) | Out-Null
    Write-Host 'LOADER STT.BIN @0x163000000' -ForegroundColor Green
}

# Start-Process com UMA string de args (array re-parseia e parte paths com espaco).
# Nao redirecionar stderr (ReadToEnd bloqueia enquanto QEMU vive).
function Start-QemuDirect([string]$qemuExe, [System.Collections.Generic.List[string]]$qa) {
    $argLine = ($qa | ForEach-Object {
        $a = [string]$_
        if ($a -match '\s') { '"{0}"' -f ($a -replace '"','\"') } else { $a }
    }) -join ' '
    return (Start-Process -FilePath $qemuExe -ArgumentList $argLine -PassThru)
}

$hub = $null
if (-not $SingleNode) {
    $hub = Start-Process python -ArgumentList @('tools\qemu_l2_hub.py', '--port', '19000') `
        -WorkingDirectory $Root -WindowStyle Minimized -PassThru `
        -RedirectStandardOutput 'logs\l2_hub.txt' -RedirectStandardError 'logs\l2_hub.err.txt'
    Start-Sleep 1
    if ($hub.HasExited) { throw 'hub L2 failed' }
} else {
    # Single node still gets STATIC flag (no DHCP); hub optional for mesh traffic
    $hub = Start-Process python -ArgumentList @('tools\qemu_l2_hub.py', '--port', '19000') `
        -WorkingDirectory $Root -WindowStyle Minimized -PassThru `
        -RedirectStandardOutput 'logs\l2_hub.txt' -RedirectStandardError 'logs\l2_hub.err.txt'
    Start-Sleep 1
}

$cpu = if ($Accel -eq 'whpx') { 'Haswell' } else { 'max' }
$accelArg = if ($Accel -eq 'whpx') { 'whpx' } else { 'tcg,thread=multi' }
$disp = if ($NoDisplay) { 'none' } else { 'gtk' }

$nodes = @(
    @{ Name = 'a'; Port = 19001; Mac = '52:54:00:AA:00:01'; Flag = 'target\netmode_a.flag'; Mon = 45501 }
)
if (-not $SingleNode) {
    $nodes += @{ Name = 'b'; Port = 19002; Mac = '52:54:00:BB:00:02'; Flag = 'target\netmode_b.flag'; Mon = 45502 }
}

$pids = @{}
foreach ($n in $nodes) {
    $log = Join-Path $Root ("logs\boot_mesh_{0}.txt" -f $n.Name)
    if (Test-Path $log) { Remove-Item $log -Force }
    $flag = (Resolve-Path $n.Flag).Path
    $qa = New-Object System.Collections.Generic.List[string]
    foreach ($x in @(
        '-m', ("{0}G" -f $RamGB),
        '-smp', ("{0}" -f $Smp),
        '-cpu', $cpu,
        '-accel', $accelArg,
        '-drive', ("if=pflash,format=raw,file={0},readonly=on" -f $ovmf),
        '-drive', ("format=raw,file={0},if=ide,index=0,snapshot=on" -f $uefi),
        '-drive', ("format=raw,file={0},if=ide,index=1,snapshot=on" -f $disk)
    )) { $qa.Add($x) | Out-Null }
    foreach ($x in $loaders) { $qa.Add($x) | Out-Null }
    foreach ($x in @(
        '-device', ('loader,file={0},addr=0x2000000' -f $flag),
        '-device', ('loader,file={0},addr=0x2100000' -f $linjPath),
        '-netdev', ("socket,id=n0,udp=127.0.0.1:19000,localaddr=127.0.0.1:{0}" -f $n.Port),
        '-device', ("e1000,netdev=n0,mac={0}" -f $n.Mac),
        '-vga', 'std',
        '-display', $disp,
        '-audiodev', 'dsound,id=snd0,out.mixing-engine=on,in.mixing-engine=on',
        '-device', 'intel-hda,id=hda0',
        '-device', 'hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0',
        '-device', 'qemu-xhci,id=xhci',
        '-device', 'usb-tablet,bus=xhci.0',
        '-device', 'usb-kbd,bus=xhci.0',
        '-serial', ("file:{0}" -f $log),
        '-monitor', ("tcp:127.0.0.1:{0},server,nowait" -f $n.Mon),
        '-no-reboot',
        '-name', ("mesh-clima-{0}" -f $n.Name.ToUpper())
    )) { $qa.Add($x) | Out-Null }

    $p = Start-QemuDirect $qemu $qa
    Start-Sleep 8
    if ($p.HasExited -and $Accel -eq 'whpx') {
        Write-Host ("{0} WHPX exit={1} - retry TCG" -f $n.Name, $p.ExitCode) -ForegroundColor Yellow
        for ($i = 0; $i -lt $qa.Count; $i++) {
            if ($qa[$i] -eq '-accel') { $qa[$i + 1] = 'tcg,thread=multi' }
            if ($qa[$i] -eq '-cpu') { $qa[$i + 1] = 'max' }
        }
        $p = Start-QemuDirect $qemu $qa
        Start-Sleep 10
    }
    $alive = -not $p.HasExited
    $pids[$n.Name] = $p.Id
    Write-Host ("{0} pid={1} mon={2} alive={3}" -f $n.Name.ToUpper(), $p.Id, $n.Mon, $alive) `
        -ForegroundColor $(if ($alive) { 'Green' } else { 'Red' })
    if (-not $alive) {
        throw ("QEMU {0} exited immediately (exit={1})" -f $n.Name, $p.ExitCode)
    }
}

function Qemu-Monitor([int]$port, [string]$cmd) {
    try {
        $c = New-Object System.Net.Sockets.TcpClient('127.0.0.1', $port)
        $s = $c.GetStream()
        $bytes = [System.Text.Encoding]::ASCII.GetBytes($cmd + "`n")
        $s.Write($bytes, 0, $bytes.Length)
        $s.Flush()
        Start-Sleep -Milliseconds 80
        $buf = New-Object byte[] 4096
        $n = 0
        if ($s.CanRead) {
            $s.ReadTimeout = 400
            try { $n = $s.Read($buf, 0, 4096) } catch { $n = 0 }
        }
        $c.Close()
        if ($n -gt 0) { [System.Text.Encoding]::ASCII.GetString($buf, 0, $n) } else { '' }
    } catch { '' }
}

$SC = @{
    'a'='0x1e'; 'b'='0x30'; 'c'='0x2e'; 'd'='0x20'; 'e'='0x12'; 'f'='0x21'; 'g'='0x22'; 'h'='0x23'
    'i'='0x17'; 'j'='0x24'; 'k'='0x25'; 'l'='0x26'; 'm'='0x32'; 'n'='0x31'; 'o'='0x18'; 'p'='0x19'
    'q'='0x10'; 'r'='0x13'; 's'='0x1f'; 't'='0x14'; 'u'='0x16'; 'v'='0x2f'; 'w'='0x11'; 'x'='0x2d'
    'y'='0x15'; 'z'='0x2c'; ' '='0x39'; ','='0x33'; '.'='0x34'; '-'='0x0c'
    '0'='0x0b'; '1'='0x02'; '2'='0x03'; '3'='0x04'; '4'='0x05'; '5'='0x06'; '6'='0x07'; '7'='0x08'
    '8'='0x09'; '9'='0x0a'; '/'='0x35'; '?'='shift-0x35'
}

function Send-Phrase([int]$port, [string]$text) {
    Write-Host ("INJECT mon={0}: {1}" -f $port, $text) -ForegroundColor Magenta
    foreach ($ch in $text.ToLowerInvariant().ToCharArray()) {
        $key = [string]$ch
        if (-not $SC.ContainsKey($key)) { continue }
        $sk = $SC[$key]
        [void](Qemu-Monitor $port ("sendkey {0}" -f $sk))
        Start-Sleep -Milliseconds 60
    }
    [void](Qemu-Monitor $port 'sendkey 0x1c')
    Start-Sleep -Milliseconds 400
}

# --- wait boot+LLM ---
Write-Host ("waiting up to {0}s for llm=LOADED on A..." -f $WaitBootSec) -ForegroundColor Cyan
$deadline = (Get-Date).AddSeconds($WaitBootSec)
$ready = $false
$emptyStreak = 0
while ((Get-Date) -lt $deadline) {
    Start-Sleep 5
    $qaAlive = Get-Process -Id $pids['a'] -EA SilentlyContinue
    if (-not $qaAlive) {
        Write-Result 'FAIL: QEMU A morreu durante boot'
        exit 4
    }
    $logA = 'logs\boot_mesh_a.txt'
    if (-not (Test-Path $logA)) { continue }
    $sz = (Get-Item $logA).Length
    if ($sz -eq 0) {
        $emptyStreak++
        Write-Host ("... boot A sizeKB=0 (empty#{0})" -f $emptyStreak)
        if ($emptyStreak -ge 24) {
            Write-Result 'FAIL: serial A vazio 120s - OVMF/WHPX sem boot'
            exit 5
        }
        continue
    }
    $emptyStreak = 0
    $hit = Select-String -Path $logA -Pattern 'llm=LOADED|LLM LOADED falcon' -EA SilentlyContinue | Select-Object -Last 1
    $sched = Select-String -Path $logA -Pattern 'SCHEDULER|Runtime tick|PHASE 7' -EA SilentlyContinue | Select-Object -Last 1
    if ($hit -and $sched) {
        Write-Host ("READY: {0}" -f $hit.Line.Substring(0, [Math]::Min(140, $hit.Line.Length))) -ForegroundColor Green
        $ready = $true
        break
    }
    Write-Host ("... boot A sizeKB={0}" -f [math]::Round($sz/1KB))
}
if (-not $ready) {
    Write-Result 'FAIL: llm=LOADED nao apareceu a tempo'
    exit 2
}

# settle: Hermes poll LINJ apos model_is_loaded
Start-Sleep 20
Write-Result 'ARMED: waiting LINJ USER_INTENT (loader @0x2100000)'

# optional sendkey backup (hex scancodes — path produto; LINJ e o aceite)
$monA = 45501
$probe = Qemu-Monitor $monA 'info status'
if ($probe) {
    Write-Result 'monitor A OK'
    Send-Phrase $monA $Phrase
    Write-Result ("SENDKEY_BACKUP: {0}" -f $Phrase)
} else {
    Write-Result 'WARN: monitor A inacessivel — so LINJ'
}

Write-Host ("waiting up to {0}s for USER_INTENT/InferQ/TTS..." -f $WaitInferSec) -ForegroundColor Cyan
$d2 = (Get-Date).AddSeconds($WaitInferSec)
$okIntent = $false
$okInfer = $false
$okWx = $false
$okTts = $false
$okLinj = $false
while ((Get-Date) -lt $d2) {
    Start-Sleep 8
    $logA = 'logs\boot_mesh_a.txt'
    $lines = @(
        Select-String -Path $logA -Pattern 'LINJ|USER_INTENT|Texto:|InferQueue submit|HERMES_RESPONSE|TTS Piper|TTS Formant|passo|fundo|temperatura|Celsius|clima' -EA SilentlyContinue |
            Select-Object -Last 30
    )
    foreach ($l in $lines) {
        $s = $l.Line
        if ($s -match 'inject USER_INTENT \(LINJ\)|ENTER — USER_INTENT') { $okIntent = $true }
        if ($s -match 'LINJ') { $okLinj = $true }
        if ($s -match 'InferQueue submit') { $okInfer = $true }
        if ($s -match '(?i)passo fundo|temperatura.*passo|passo.*temperatura') { $okWx = $true }
        elseif ($s -match '(?i)USER_INTENT|Texto:' -and $s -match '(?i)passo|fundo|temperatura') { $okWx = $true }
        if ($s -match 'TTS Piper|TTS Formant') { $okTts = $true }
    }
    Write-Host ("linj={0} intent={1} infer={2} wx={3} tts={4}" -f $okLinj, $okIntent, $okInfer, $okWx, $okTts)
    if ($okIntent -and $okInfer -and $okWx) { break }
}

Write-Result '=== EVIDENCE (tail) ==='
Select-String -Path 'logs\boot_mesh_a.txt' -Pattern 'LINJ|USER_INTENT|Texto:|InferQueue|TTS Piper|TTS Formant|passo|fundo|temperatura|HERMES_RESPONSE' -EA SilentlyContinue |
    Select-Object -Last 30 |
    ForEach-Object { Write-Result $_.Line.Substring(0, [Math]::Min(180, $_.Line.Length)) }

$verdict = if (-not $okIntent) {
    'FAIL: sem USER_INTENT (LINJ/sendkey) — inject nao chegou ao Hermes'
} elseif (-not $okWx) {
    'FAIL: USER_INTENT sem lexico clima (passo/fundo/temperatura)'
} elseif ($okInfer) {
    'PASS_PARTIAL: Falcon3 recebeu pedido clima Passo Fundo (STATIC=sem HTTP ao vivo)'
} else {
    'PASS_WEAK: intent clima ok mas sem InferQueue submit observavel'
}
Write-Result ("VERDICT: {0}" -f $verdict)
Write-Host $verdict -ForegroundColor $(if ($verdict -match 'PASS') { 'Green' } else { 'Red' })
Write-Host 'QEMU ainda rodando - confira GUI. Pare com Stop-Process qemu* quando terminar.' -ForegroundColor Cyan
exit $(if ($verdict -match 'FAIL') { 1 } else { 0 })
