# neural-os-core - QEMU UEFI com WHPX (fallback TCG via -Tcg) - v1.7.0
# Fluxo: cargo build --release -> python tools\build_image.py -> .\run-qemu-whpx.ps1 [-Window]
#
# Bypass rede (SLIP/COM2): sobe tools\serial_bridge.py ANTES do QEMU e mata no exit/Ctrl+C.
#   Bridge = TCP server 127.0.0.1:4444 | QEMU = cliente (-serial tcp:127.0.0.1:4444)
#   NAO use server=on no QEMU (disputa a porta com o bridge).
#   Skip: -NoSerialBridge   |  WinTAP NIC: -Bridge (netdev distinto do SLIP)
# NOTA: ficheiro ASCII-only (PS5 le UTF-8 sem BOM como CP1252; em-dash partia strings).
param(
    [switch]$Window,
    [switch]$BuildDisk,
    [switch]$Tcg,
    [switch]$Bridge,
    [switch]$NoSerialBridge,
    [int]$SerialBridgePort = 4444,
    [int]$RamGB = 6,
    [int]$Smp = 4
)

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$logfile = Join-Path $logDir "boot_whpx_$timestamp.txt"
$bridgeLog = Join-Path $logDir "bridge_$timestamp.log"
$bridgeErr = Join-Path $logDir "bridge_$timestamp.err.log"

$uefi = Join-Path $Root "target\uefi.img"
$ovmf = Join-Path $Root "target\ovmf.fd"
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$bridgeScript = Join-Path $Root "tools\serial_bridge.py"

if (!(Test-Path $uefi)) { Write-Host "ERRO: target\uefi.img ausente. cargo build --release"; exit 1 }
if (!(Test-Path $ovmf)) { Write-Host "ERRO: target\ovmf.fd ausente"; exit 1 }
if (!(Test-Path $qemu)) { Write-Host "ERRO: QEMU nao encontrado"; exit 1 }

$script:bridgeProc = $null

function Stop-SerialBridge {
    if ($null -eq $script:bridgeProc) { return }
    $bridgePid = $script:bridgeProc.Id
    try {
        if (-not $script:bridgeProc.HasExited) {
            Stop-Process -Id $bridgePid -Force -ErrorAction SilentlyContinue
            Get-CimInstance Win32_Process -Filter "ParentProcessId=$bridgePid" -ErrorAction SilentlyContinue |
                ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
            Write-Host "[BRIDGE] killed pid=$bridgePid" -ForegroundColor Yellow
        } else {
            Write-Host "[BRIDGE] ja encerrado pid=$bridgePid" -ForegroundColor Gray
        }
    } catch {
        Write-Host "[BRIDGE] kill falhou pid=$bridgePid : $_" -ForegroundColor Yellow
    }
    $script:bridgeProc = $null
}

function Test-PortListening {
    param([int]$Port)
    # Get-NetTCPConnection pode retornar vazio (sem throw) sem admin / race —
    # sempre cruzar com netstat se o cmdlet nao achar LISTEN.
    try {
        $c = @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
        if ($c.Count -gt 0) { return $true }
    } catch { }
    $out = netstat -an | Select-String -Pattern ":$Port\s+.*LISTEN"
    return $null -ne $out
}

function Start-SerialBridge {
    param([int]$Port)
    if (!(Test-Path $bridgeScript)) {
        Write-Host "[BRIDGE] ERRO: $bridgeScript ausente" -ForegroundColor Red
        return $false
    }
    if (Test-PortListening -Port $Port) {
        Write-Host "[BRIDGE] porta $Port ja em LISTEN - reutilizando (nao sobe novo)" -ForegroundColor Yellow
        return $true
    }
    $py = Get-Command python -ErrorAction SilentlyContinue
    if (-not $py) {
        Write-Host "[BRIDGE] ERRO: python nao encontrado no PATH" -ForegroundColor Red
        return $false
    }
    $script:bridgeProc = Start-Process -FilePath $py.Source `
        -ArgumentList @("`"$bridgeScript`"", "--port", "$Port", "--watchdog", "0") `
        -WorkingDirectory $Root `
        -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput $bridgeLog `
        -RedirectStandardError $bridgeErr
    # aguarda bind (ate ~5s; logging vai p/ stderr redirecionado)
    $ok = $false
    for ($i = 0; $i -lt 34; $i++) {
        Start-Sleep -Milliseconds 150
        if ($script:bridgeProc.HasExited) { break }
        if (Test-PortListening -Port $Port) { $ok = $true; break }
    }
    if ($ok) {
        Write-Host "[BRIDGE] started pid=$($script:bridgeProc.Id) listen=127.0.0.1:$Port log=$bridgeLog" -ForegroundColor Green
        return $true
    }
    Write-Host "[BRIDGE] FALHA ao escutar :$Port (pid=$($script:bridgeProc.Id) exited=$($script:bridgeProc.HasExited))" -ForegroundColor Red
    if (Test-Path $bridgeErr) {
        Get-Content $bridgeErr -ErrorAction SilentlyContinue | Select-Object -Last 8 | ForEach-Object {
            Write-Host "  $_" -ForegroundColor DarkRed
        }
    }
    Stop-SerialBridge
    return $false
}

function Resolve-FatDisk {
    $targetDisk = Join-Path $Root "target\disk_qemu.raw"
    $toolsDisk  = Join-Path $Root "tools\disk_qemu.raw"
    if (Test-Path $targetDisk) { return $targetDisk }
    if (Test-Path $toolsDisk) {
        New-Item -ItemType Directory -Force -Path (Join-Path $Root "target") | Out-Null
        Copy-Item -Force $toolsDisk $targetDisk
        Write-Host "Disco copiado: tools\ -> target\disk_qemu.raw" -ForegroundColor Yellow
        return $targetDisk
    }
    return $null
}

try {
    $disk = Resolve-FatDisk
    if (-not $disk -and $BuildDisk) {
        python (Join-Path $Root "tools\build_image.py")
        $disk = Resolve-FatDisk
    }

    $accel = "tcg"
    $cpu = "max"
    if (-not $Tcg) {
        $accel = "whpx"
        # host + APX/MPX em QEMU 11 -> OVMF #GP no PlatformPei; Haswell estavel + AVX2
        $cpu = "Haswell"
    }

    $smpTry = @($Smp)
    if ($Smp -ne 2) { $smpTry += 2 }

    $netMode = "user"
    $netArgs = @("-netdev", "user,id=n0,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446", "-device", "e1000,netdev=n0")
    if ($Bridge) {
        Write-Host "Tentando -netdev bridge,id=n0 ..." -ForegroundColor Cyan
        $netMode = "bridge"
        $netArgs = @("-netdev", "bridge,id=n0,br=bridge", "-device", "e1000,netdev=n0")
    } else {
        Write-Host "WARN: netdev=user (WinTAP -Bridge nao pedido). hostfwd :4445/:4446 (porta 4444 reservada ao SLIP)" -ForegroundColor Yellow
    }

    # SLIP peer ANTES do QEMU (cliente TCP precisa de LISTEN)
    if (-not $NoSerialBridge) {
        if (-not (Start-SerialBridge -Port $SerialBridgePort)) {
            Write-Host "[BRIDGE] abortando: sem peer serial a bypass nao funciona" -ForegroundColor Red
            exit 1
        }
    } else {
        Write-Host "[BRIDGE] skip (-NoSerialBridge) - COM2 tcp:$SerialBridgePort sem peer" -ForegroundColor Yellow
    }

    Write-Host "=== NEURAL-OS-CORE (UEFI + $accel) ===" -ForegroundColor Cyan
    Write-Host "RAM: ${RamGB}G | CPU: $cpu smp-try=$($smpTry -join ',') | NIC: e1000 ($netMode)"
    if ($disk) { Write-Host "FAT32: $disk (IDE index=1)" -ForegroundColor Green }
    else { Write-Host "AVISO: sem disk_qemu.raw - python tools\build_image.py" -ForegroundColor Yellow }
    Write-Host "Log: $logfile"
    Write-Host "Serial: COM1=file log | COM2=tcp client -> 127.0.0.1:$SerialBridgePort (SLIP)" -ForegroundColor Gray
    Write-Host "Nota: se WHPX falhar (VP exit), rode: .\run-qemu-whpx.ps1 -Tcg -Window" -ForegroundColor Gray

    function Build-QemuArgs {
        param([string]$Acc, [string]$Cpu, [int]$SmpN)
        $a = @(
            "-m", "${RamGB}G", "-smp", "$SmpN",
            "-accel", $Acc, "-cpu", $Cpu,
            "-drive", "format=raw,file=$uefi,if=ide,index=0"
        )
        if ($disk) { $a += @("-drive", "format=raw,file=$disk,if=ide,index=1") }
        # Phys loader map (non-overlapping; precisa -m 6G+). Janelas 1MB apos BPE.
        #   0x100000000  target\bitnet_2B.bitnet     LLM BitNet 2B (~577MB); alt kernel 0x120000000
        #   0x130000000  target\PIPER_PT_BR.BIN      Piper TTS PT-BR (~60MB)
        #   0x140000000  (reservado probe firmware / legado)
        #   0x150000000  target\bpe_vocab.bin        BPE HF vocab BPB1 (~1.5MB)
        #   0x160000000  target\hw_expert_v3.bitnet  HW Expert Trinity MoE (~260KB)
        #   0x161000000  target\rust_coder.bitnet    RustCoder expert (~270KB)
        #   0x162000000  target\bge-small.bitnet     BGE embeddings (~393KB)
        #   0x163000000  target\STT.BIN              STT CTC tiny (~222KB)
        # MICRO.BITNET (~13KB) fica no FAT (PIO leve); sem loader dedicado.
        $bitnet2b = Join-Path $Root "target\bitnet_2B.bitnet"
        if (Test-Path $bitnet2b) {
            $a += @("-device", "loader,file=$bitnet2b,addr=0x100000000")
            Write-Host "BitNet2B loader: $bitnet2b @0x100000000" -ForegroundColor Green
        }
        $piper = Join-Path $Root "target\PIPER_PT_BR.BIN"
        if (-not (Test-Path $piper)) { $piper = Join-Path $Root "target\PIPER.BIN" }
        if (Test-Path $piper) {
            $a += @("-device", "loader,file=$piper,addr=0x130000000")
            Write-Host "Piper loader: $piper @0x130000000" -ForegroundColor Green
        }
        $bpe = Join-Path $Root "target\bpe_vocab.bin"
        if (Test-Path $bpe) {
            $a += @("-device", "loader,file=$bpe,addr=0x150000000")
            Write-Host "BPE vocab loader: $bpe @0x150000000" -ForegroundColor Green
        }
        $hwExpert = Join-Path $Root "target\hw_expert_v3.bitnet"
        if (-not (Test-Path $hwExpert)) { $hwExpert = Join-Path $Root "target\hw_expert_tf.bitnet" }
        if (-not (Test-Path $hwExpert)) { $hwExpert = Join-Path $Root "target\hw_expert.bitnet" }
        if (Test-Path $hwExpert) {
            $a += @("-device", "loader,file=$hwExpert,addr=0x160000000")
            Write-Host "HW Expert loader: $hwExpert @0x160000000" -ForegroundColor Green
        }
        $rustCoder = Join-Path $Root "target\rust_coder.bitnet"
        if (-not (Test-Path $rustCoder)) { $rustCoder = Join-Path $Root "tools\target\rust_coder.bitnet" }
        if (Test-Path $rustCoder) {
            $a += @("-device", "loader,file=$rustCoder,addr=0x161000000")
            Write-Host "RustCoder loader: $rustCoder @0x161000000" -ForegroundColor Green
        }
        $bge = Join-Path $Root "target\bge-small.bitnet"
        if (-not (Test-Path $bge)) { $bge = Join-Path $Root "target\BGE.BIN" }
        if (Test-Path $bge) {
            $a += @("-device", "loader,file=$bge,addr=0x162000000")
            Write-Host "BGE loader: $bge @0x162000000" -ForegroundColor Green
        }
        $stt = Join-Path $Root "target\STT.BIN"
        if (Test-Path $stt) {
            $a += @("-device", "loader,file=$stt,addr=0x163000000")
            Write-Host "STT loader: $stt @0x163000000" -ForegroundColor Green
        }
        # COM1 = log file; COM2 = SLIP bypass (QEMU CLIENTE -> bridge servidor)
        $a += @(
            "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
            "-serial", "file:$logfile",
            "-serial", "tcp:127.0.0.1:${SerialBridgePort}"
        )
        $a += $netArgs
        # -Window: VGA std (janela GUI visivel). Sem -Window: headless (-nographic).
        if ($Window) { $a += @("-vga", "std") }
        else { $a += @("-vga", "none", "-display", "none", "-nographic") }
        return $a
    }

    $ok = $false
    foreach ($smpN in $smpTry) {
        $qemuArgs = Build-QemuArgs -Acc $accel -Cpu $cpu -SmpN $smpN
        Write-Host "QEMU: -smp $smpN accel=$accel net=$netMode serial-tcp=client:$SerialBridgePort" -ForegroundColor Gray
        try {
            & $qemu @qemuArgs
            $ok = $true
            break
        } catch {
            Write-Host "WARN: falhou smp=$smpN accel=$accel : $_" -ForegroundColor Yellow
            if ($accel -eq "whpx") {
                Write-Host "WHPX falhou - tentando TCG smp=$smpN ..." -ForegroundColor Yellow
                $accel = "tcg"
                $cpu = "max"
                try {
                    $qemuArgs = Build-QemuArgs -Acc $accel -Cpu $cpu -SmpN $smpN
                    & $qemu @qemuArgs
                    $ok = $true
                    break
                } catch {
                    Write-Host "WARN: TCG tambem falhou: $_" -ForegroundColor Yellow
                }
            }
            if ($netMode -eq "bridge") {
                Write-Host "WARN: bridge -> user+hostfwd" -ForegroundColor Yellow
                $netMode = "user"
                $netArgs = @("-netdev", "user,id=n0,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446", "-device", "e1000,netdev=n0")
            }
        }
    }
    if (-not $ok) { Write-Host "ERRO: QEMU nao iniciou"; exit 1 }
    Write-Host ""
    Write-Host "Serial log: $logfile"
}
finally {
    Stop-SerialBridge
}
