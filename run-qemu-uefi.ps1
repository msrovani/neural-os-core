# neural-os-core - QEMU UEFI (caminho canonico) — v1.6.0
# Fluxo: cargo build --release -> python tools\build_image.py -> .\run-qemu-uefi.ps1 [-Window]
# Disco FAT32: target\disk_qemu.raw (fallback: tools\ -> copia p/ target)
param(
    [switch]$Window,
    [switch]$BuildDisk,
    [switch]$Bridge,
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

# -smp: tenta Smp, fallback 2 se QEMU/WHPX/TCG reclamar
$smpTry = @($Smp)
if ($Smp -ne 2) { $smpTry += 2 }

# Net: bridge se -Bridge; senao user + hostfwd (Windows host tipico)
$netMode = "user"
$netArgs = @("-netdev", "user,id=n0,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446", "-device", "e1000,netdev=n0")
if ($Bridge) {
    Write-Host "Tentando -netdev bridge,id=n0 ..." -ForegroundColor Cyan
    $netMode = "bridge"
    $netArgs = @("-netdev", "bridge,id=n0,br=bridge", "-device", "e1000,netdev=n0")
} else {
    Write-Host "WARN: netdev=user (bridge nao pedido). hostfwd :4445/:4446" -ForegroundColor Yellow
}

Write-Host "=== NEURAL-OS-CORE (UEFI) ===" -ForegroundColor Cyan
Write-Host "RAM: ${RamGB}G | CPU: try $($smpTry -join ',') (TCG) | NIC: e1000 ($netMode)"
Write-Host "Boot:  $uefi"
Write-Host "OVMF:  $ovmf"
if ($disk) { Write-Host "FAT32: $disk (IDE index=1)" -ForegroundColor Green }
Write-Host "Log:   $logfile"

function Build-QemuArgs {
    param([int]$SmpN)
    $a = @(
        "-m", "${RamGB}G", "-smp", "$SmpN", "-accel", "tcg",
        "-drive", "format=raw,file=$uefi,if=ide,index=0"
    )
    if ($disk) {
        $a += @("-drive", "format=raw,file=$disk,if=ide,index=1")
    }
    $a += @(
        "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
        "-serial", "file:$logfile",
        "-serial", "tcp:127.0.0.1:4444,server=on,wait=off"
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
    Write-Host "QEMU: -smp $smpN -m ${RamGB}G accel=tcg net=$netMode" -ForegroundColor Gray
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
