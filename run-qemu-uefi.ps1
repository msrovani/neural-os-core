# neural-os-core - QEMU UEFI (caminho canonico) — v1.7.0
# Fluxo: cargo build --release -> python tools\build_image.py -> .\run-qemu-uefi.ps1 [-Window]
# Disco FAT32: target\disk_qemu.raw (fallback: tools\ -> copia p/ target)
# SLIP: sobe tools\serial_bridge.py (TCP server :4444) antes do QEMU; QEMU = cliente COM2.
# Preferencia WHPX+lifecycle: .\run-qemu-whpx.ps1
param(
    [switch]$Window,
    [switch]$BuildDisk,
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
$logfile = Join-Path $logDir "boot_uefi_$timestamp.txt"

$uefi = Join-Path $Root "target\uefi.img"
$ovmf = Join-Path $Root "target\ovmf.fd"
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"

if (!(Test-Path $uefi)) {
    Write-Host "ERRO: target\uefi.img ausente. Rode: cargo build --release" -ForegroundColor Red
    exit 1
}
if (!(Test-Path $ovmf)) {
    Write-Host "ERRO: target\ovmf.fd ausente. Copie OVMF.fd para target\ovmf.fd" -ForegroundColor Red
    exit 1
}
if (!(Test-Path $qemu)) {
    Write-Host "ERRO: QEMU nao encontrado em $qemu" -ForegroundColor Red
    exit 1
}

function Resolve-FatDisk {
    $targetDisk = Join-Path $Root "target\disk_qemu.raw"
    $toolsDisk  = Join-Path $Root "tools\disk_qemu.raw"
    if (Test-Path $targetDisk) { return $targetDisk }
    if (Test-Path $toolsDisk) {
        New-Item -ItemType Directory -Force -Path (Join-Path $Root "target") | Out-Null
        Copy-Item -Force $toolsDisk $targetDisk
        Write-Host "Disco copiado: tools\disk_qemu.raw -> target\disk_qemu.raw" -ForegroundColor Yellow
        return $targetDisk
    }
    return $null
}

$disk = Resolve-FatDisk
if (-not $disk) {
    if ($BuildDisk) {
        Write-Host "Gerando disco FAT32 via tools\build_image.py ..." -ForegroundColor Cyan
        python (Join-Path $Root "tools\build_image.py")
        if ($LASTEXITCODE -ne 0) { Write-Host "ERRO: build_image.py falhou" -ForegroundColor Red; exit 1 }
        $disk = Resolve-FatDisk
    }
}
if (-not $disk) {
    Write-Host "AVISO: disk_qemu.raw nao encontrado (target\ nem tools\)." -ForegroundColor Yellow
    Write-Host "  Rode: python tools\build_image.py   ou   .\run-qemu-uefi.ps1 -BuildDisk" -ForegroundColor Yellow
    Write-Host "  Continuando SEM segundo disco IDE (sem .bitnet / FAT32)." -ForegroundColor Yellow
}

$bridgeScript = Join-Path $Root "tools\serial_bridge.py"
$bridgeLog = Join-Path $logDir "bridge_uefi_$timestamp.log"
$bridgeErr = Join-Path $logDir "bridge_uefi_$timestamp.err.log"
$script:bridgeProc = $null

function Stop-SerialBridge {
    if ($null -eq $script:bridgeProc) { return }
    $bridgePid = $script:bridgeProc.Id
    try {
        if (-not $script:bridgeProc.HasExited) {
            Stop-Process -Id $bridgePid -Force -ErrorAction SilentlyContinue
            Write-Host "[BRIDGE] killed pid=$bridgePid" -ForegroundColor Yellow
        }
    } catch { }
    $script:bridgeProc = $null
}

function Test-PortListening {
    param([int]$Port)
    try {
        return $null -ne (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
    } catch {
        return $null -ne (netstat -an | Select-String -Pattern ":$Port\s+.*LISTEN")
    }
}

try {
# -smp: tenta Smp, fallback 2 se QEMU/WHPX/TCG reclamar
$smpTry = @($Smp)
if ($Smp -ne 2) { $smpTry += 2 }

# Net: -Bridge = WinTAP; 4444 reservado ao SLIP peer
$netMode = "user"
$netArgs = @("-netdev", "user,id=n0,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446", "-device", "e1000,netdev=n0")
if ($Bridge) {
    Write-Host "Tentando -netdev bridge,id=n0 ..." -ForegroundColor Cyan
    $netMode = "bridge"
    $netArgs = @("-netdev", "bridge,id=n0,br=bridge", "-device", "e1000,netdev=n0")
} else {
    Write-Host "WARN: netdev=user. hostfwd :4445/:4446 (4444 = SLIP bridge)" -ForegroundColor Yellow
}

if (-not $NoSerialBridge) {
    if (Test-PortListening -Port $SerialBridgePort) {
        Write-Host "[BRIDGE] porta $SerialBridgePort ja em LISTEN — reutilizando" -ForegroundColor Yellow
    } else {
        $py = Get-Command python -ErrorAction SilentlyContinue
        if (-not $py -or -not (Test-Path $bridgeScript)) {
            Write-Host "[BRIDGE] ERRO: python ou tools\serial_bridge.py ausente" -ForegroundColor Red
            exit 1
        }
        $script:bridgeProc = Start-Process -FilePath $py.Source `
            -ArgumentList @("`"$bridgeScript`"", "--port", "$SerialBridgePort", "--watchdog", "0") `
            -WorkingDirectory $Root -PassThru -WindowStyle Hidden `
            -RedirectStandardOutput $bridgeLog -RedirectStandardError $bridgeErr
        $okListen = $false
        for ($i = 0; $i -lt 20; $i++) {
            Start-Sleep -Milliseconds 150
            if ($script:bridgeProc.HasExited) { break }
            if (Test-PortListening -Port $SerialBridgePort) { $okListen = $true; break }
        }
        if (-not $okListen) {
            Write-Host "[BRIDGE] FALHA listen :$SerialBridgePort" -ForegroundColor Red
            Stop-SerialBridge
            exit 1
        }
        Write-Host "[BRIDGE] started pid=$($script:bridgeProc.Id) listen=127.0.0.1:$SerialBridgePort" -ForegroundColor Green
    }
} else {
    Write-Host "[BRIDGE] skip (-NoSerialBridge)" -ForegroundColor Yellow
}

Write-Host "=== NEURAL-OS-CORE (UEFI) ===" -ForegroundColor Cyan
Write-Host "RAM: ${RamGB}G | CPU: try $($smpTry -join ',') (TCG) | NIC: e1000 ($netMode)"
Write-Host "Boot:  $uefi"
Write-Host "OVMF:  $ovmf"
if ($disk) { Write-Host "FAT32: $disk (IDE index=1)" -ForegroundColor Green }
Write-Host "Log:   $logfile"
Write-Host "Serial: COM2=tcp client -> 127.0.0.1:$SerialBridgePort" -ForegroundColor Gray

function Build-QemuArgs {
    param([int]$SmpN)
    $a = @(
        "-m", "${RamGB}G", "-smp", "$SmpN", "-accel", "tcg",
        "-drive", "format=raw,file=$uefi,if=ide,index=0"
    )
    if ($disk) {
        $a += @("-drive", "format=raw,file=$disk,if=ide,index=1")
    }
    # ─── MoE Model Loaders — AUTO-SCAN da pasta target/ ───
    # Descobre TODOS os arquivos .bitnet / .BIN / .bin na pasta target/
    # e os carrega em endereços sequenciais (gap de 1MB entre modelos).
    # O usuário só precisa colocar o arquivo na pasta — o script descobre.
    # Prioridade: arquivos maiores primeiro (LLM base), depois os pequenos (experts).
    # Endereço base: 0x100000000 (4GB), sobe em passos de 0x100000 (1MB) + tamanho.
    # SESSION_275: modelos em D:\modelos (repo externo) + target/ (legado)
    $extModelDir = "D:\modelos"
    $modelDirs = @((Join-Path $Root "target"))
    if (Test-Path $extModelDir) { $modelDirs += $extModelDir }
    $modelFiles = @()
    foreach ($d in $modelDirs) {
        $modelFiles += @(Get-ChildItem -Path $d -Filter "*.bitnet" -ErrorAction SilentlyContinue) +
                        @(Get-ChildItem -Path $d -Filter "*.BIN" -ErrorAction SilentlyContinue) +
                        @(Get-ChildItem -Path $d -Filter "*.bin" -ErrorAction SilentlyContinue)
    }
    # Remove duplicatas (mesmo nome, extensões diferentes) e ordena por tamanho decrescente
    $seen = @{}
    $unique = @()
    foreach ($f in $modelFiles) {
        $base = [System.IO.Path]::GetFileNameWithoutExtension($f.Name).ToUpper()
        if (-not $seen.ContainsKey($base)) {
            $seen[$base] = $true
            $unique += $f
        }
    }
    $unique = $unique | Sort-Object -Property Length -Descending
    # Filtra placeholders vazios (<10KB) e modelos grandes (>70MB = OOM no heap 1024MB)
    $unique = @($unique | Where-Object { $_.Length -gt 10240 -and $_.Length -le 70MB })
    $modelAddr = 0x100000000  # 4GB base
    $modelGap = 0x100000      # 1MB gap entre modelos
    $loaded = 0
    foreach ($f in $unique) {
        $sizeMB = [math]::Round($f.Length / 1MB, 1)
        $hexAddr = [Convert]::ToString($modelAddr, 16).ToUpper()
        $a += @("-device", "loader,file=$($f.FullName),addr=$modelAddr")
        Write-Host "MoE loader: $($f.Name) ($sizeMB MB) @0x$hexAddr" -ForegroundColor Green
        $loaded++
        # Próximo endereço: atual + tamanho arredondado para 1MB + gap
        $modelAddr += [math]::Ceiling($f.Length / $modelGap) * $modelGap + $modelGap
    }
    if ($loaded -eq 0) {
        Write-Host "MoE: nenhum modelo .bitnet/.BIN encontrado em target/" -ForegroundColor Yellow
    } else {
        Write-Host "MoE: $loaded modelo(s) carregados via QEMU loader" -ForegroundColor Cyan
    }
    # COM1=log; COM2=SLIP (QEMU cliente → bridge TCP server; SEM server=on)
    $a += @(
        "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
        "-serial", "file:$logfile",
        "-serial", "tcp:127.0.0.1:${SerialBridgePort}"
    )
    $a += $netArgs
    if ($Window) {
        $a += @("-vga", "std")
    } else {
        $a += @("-vga", "none", "-display", "none", "-nographic")
    }
    return $a
}

$launched = $false
foreach ($smpN in $smpTry) {
    $qemuArgs = Build-QemuArgs -SmpN $smpN
    Write-Host "QEMU: -smp $smpN -m ${RamGB}G accel=tcg net=$netMode serial-tcp=client:$SerialBridgePort" -ForegroundColor Gray
    try {
        & $qemu @qemuArgs
        $launched = $true
        break
    } catch {
        Write-Host "WARN: QEMU -smp $smpN falhou: $_" -ForegroundColor Yellow
        if ($netMode -eq "bridge") {
            Write-Host "WARN: bridge falhou — fallback user+hostfwd" -ForegroundColor Yellow
            $netMode = "user"
            $netArgs = @("-netdev", "user,id=n0,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446", "-device", "e1000,netdev=n0")
        }
    }
}

if (-not $launched) {
    Write-Host "ERRO: QEMU nao iniciou (smp/bridge)" -ForegroundColor Red
    exit 1
}
Write-Host ""
Write-Host "Serial log: $logfile"
}
finally {
    Stop-SerialBridge
}
