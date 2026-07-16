# neural-os-core - QEMU UEFI interativo (janela / stdio)
# Fluxo: cargo build --release -> python tools\build_image.py -> .\interactive.ps1
# Anexa target\disk_qemu.raw (IDE index=1) igual a run-qemu-uefi.ps1
param([switch]$Serial)

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$uefi = Join-Path $Root "target\uefi.img"
$ovmf = Join-Path $Root "target\ovmf.fd"
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null

if (!(Test-Path $uefi)) { Write-Host "ERRO: target\uefi.img ausente. cargo build --release"; exit 1 }
if (!(Test-Path $ovmf)) { Write-Host "ERRO: target\ovmf.fd ausente"; exit 1 }

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

$disk = Resolve-FatDisk
Write-Host "=== NEURAL-OS-CORE INTERATIVO ===" -ForegroundColor Cyan
Write-Host "RAM: 6G | CPU: 2 (TCG) | NIC: e1000 | Video: std" -ForegroundColor Gray
if ($disk) { Write-Host "FAT32: $disk (IDE index=1)" -ForegroundColor Green }
else { Write-Host "AVISO: sem disk_qemu.raw - rode python tools\build_image.py" -ForegroundColor Yellow }

$driveBoot = @("-drive", "format=raw,file=$uefi,if=ide,index=0")
$driveFat  = if ($disk) { @("-drive", "format=raw,file=$disk,if=ide,index=1") } else { @() }
$driveOvmf = @("-drive", "if=pflash,format=raw,file=$ovmf,readonly=on")

if ($Serial) {
    Write-Host "Digite comandos no terminal (Ctrl+C para sair)" -ForegroundColor Yellow
    $qemuArgs = @("-m", "6G", "-smp", "2", "-accel", "tcg") + $driveBoot + $driveFat + $driveOvmf + @(
        "-serial", "stdio",
        "-nic", "user,model=e1000",
        "-vga", "std"
    )
    & $qemu @qemuArgs
} else {
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $logfile = Join-Path $logDir "interactive_$timestamp.txt"
    Write-Host "QEMU window. Log serial: $logfile" -ForegroundColor Yellow
    Write-Host "Interaja na janela QEMU (teclado PS/2)" -ForegroundColor Yellow
    $qemuArgs = @("-m", "6G", "-smp", "2", "-accel", "tcg") + $driveBoot + $driveFat + $driveOvmf + @(
        "-serial", "file:$logfile",
        "-serial", "tcp:127.0.0.1:4444",
        "-nic", "user,model=e1000,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446",
        "-vga", "std"
    )
    & $qemu @qemuArgs
    Write-Host ""
    Write-Host "Serial log: $logfile"
}
