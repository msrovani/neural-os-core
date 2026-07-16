# neural-os-core - QEMU BIOS legado (boot via bios.img)
# Preferir UEFI: .\run-qemu-uefi.ps1 -Window
# Fluxo: cargo build --release -> python tools\build_image.py -> .\run-qemu.ps1 [-Window]
param(
    [int]$Cores = 4,
    [int]$RamGB = 6,
    [string]$Accel = "tcg",
    [switch]$Window
)

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$logfile = Join-Path $logDir "boot_bios_$timestamp.txt"
$bios = Join-Path $Root "target\bios.img"
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"

if (!(Test-Path $bios)) {
    Write-Host "ERROR: bios.img not found. Run 'cargo build --release' first."
    Write-Host "NOTE: BIOS e legado; use .\run-qemu-uefi.ps1 para o caminho canonico."
    exit 1
}

function Resolve-FatDisk {
    $targetDisk = Join-Path $Root "target\disk_qemu.raw"
    $toolsDisk  = Join-Path $Root "tools\disk_qemu.raw"
    if (Test-Path $targetDisk) { return $targetDisk }
    if (Test-Path $toolsDisk) {
        New-Item -ItemType Directory -Force -Path (Join-Path $Root "target") | Out-Null
        Copy-Item -Force $toolsDisk $targetDisk
        return $targetDisk
    }
    return $null
}

$disk = Resolve-FatDisk

Write-Host "=== NEURAL-OS-CORE QEMU (BIOS legado) ===" -ForegroundColor DarkYellow
Write-Host "RAM: ${RamGB}G | CPU: $Cores ($Accel) | NIC: e1000 NAT"
Write-Host "Serial log: $logfile"
if ($disk) { Write-Host "FAT32: $disk (IDE index=1)" -ForegroundColor Green }
else { Write-Host "AVISO: sem disk_qemu.raw - python tools\build_image.py" -ForegroundColor Yellow }

$diskArg = if ($disk) { @("-drive", "format=raw,file=$disk,if=ide,index=1") } else { @() }

$qemuArgs = @(
    "-m", "${RamGB}G", "-smp", "$Cores", "-accel", $Accel,
    "-drive", "format=raw,file=$bios,if=ide,index=0"
) + $diskArg

if ($Window) {
    $qemuArgs += @(
        "-serial", "file:$logfile",
        "-serial", "tcp:127.0.0.1:4444",
        "-nic", "user,model=e1000,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446",
        "-vga", "std"
    )
    & $qemu @qemuArgs
    Write-Host ""
    Write-Host "Serial log saved to: $logfile"
    if (Test-Path $logfile) { Get-Content -LiteralPath $logfile -Tail 10 }
} else {
    $qemuArgs += @(
        "-serial", "stdio",
        "-serial", "tcp:127.0.0.1:4444",
        "-nic", "user,model=e1000,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446",
        "-vga", "none", "-display", "none", "-nographic"
    )
    $p = Start-Process -FilePath $qemu -ArgumentList $qemuArgs `
        -WindowStyle Hidden -RedirectStandardOutput $logfile -RedirectStandardError $logfile -PassThru
    Write-Host "QEMU PID: $($p.Id)"
    Write-Host "Close QEMU or press Ctrl+C to stop."
    $p.WaitForExit()
    Write-Host ""
    Write-Host "=== LOG (last 20 lines) ==="
    if (Test-Path $logfile) { Get-Content -LiteralPath $logfile -Tail 20 }
}
