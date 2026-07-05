param(
    [string]$Accel = "whpx",
    [int]$MemoryMB = 6144,
    [string]$Model = "q35",
    [string]$Nic = "user",
    [string]$NetDev = "rtl8139",
    [string]$Bitnet = "target\bitnet-1.5b.bitnet",
    [string]$Image = "target\neural-os-bios.img",
    [string]$Display = "none",
    [switch]$SerialLog,
    [int]$TimeoutSec = 180
)

$now = Get-Date
$timestamp = $now.ToString("yyyyMMdd-HHmmss")
$logName = "session-$timestamp.log"
$logPath = Join-Path "logs" $logName

# Ensure logs directory exists
if (-not (Test-Path "logs")) { New-Item -ItemType Directory -Path "logs" | Out-Null }

$cmd = @(
    "-m $($MemoryMB)M"
    "-drive format=raw,file=$Image"
    "-display $Display"
    "-M $Model"
    "-accel $Accel"
    "-smp 2"
    "-device virtio-rng-pci"
    "-nic $Nic,model=$NetDev"
    "-global ICH9-LPC.disable_s3=1"
    "-no-reboot"
    "-no-shutdown"
)

# BitNet loader (if file exists)
if (Test-Path $Bitnet) {
    $cmd += "-device loader,file=$Bitnet,addr=0x100000000"
}

# Serial log
$cmd += "-serial file:$logPath"

Write-Host "=== QEMU Session ===" -ForegroundColor Cyan
Write-Host "  Log:     $logPath" -ForegroundColor Green
Write-Host "  Accel:   $Accel" -ForegroundColor Yellow
Write-Host "  Memory:  $MemoryMB MB" -ForegroundColor Yellow
Write-Host "  Model:   $Model" -ForegroundColor Yellow
Write-Host "  Timeout: ${TimeoutSec}s" -ForegroundColor Yellow
Write-Host "====================="

$process = Start-Process -FilePath "C:\Program Files\qemu\qemu-system-x86_64.exe" -ArgumentList $cmd -NoNewWindow -PassThru
$process | Wait-Process -Timeout $TimeoutSec -ErrorAction SilentlyContinue

if (-not $process.HasExited) {
    Write-Host "Timeout ${TimeoutSec}s reached. Stopping QEMU..." -ForegroundColor Red
    $process.Kill()
}

Write-Host "Session log saved: $logPath" -ForegroundColor Cyan
Write-Host "Lines: $(@(Get-Content $logPath).Length)" -ForegroundColor Cyan
