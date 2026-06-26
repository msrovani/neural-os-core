param(
    [string]$Smp = "2",
    [int]$Memory = 2048,
    [string]$Nic = "user,model=rtl8139",
    [switch]$NoGraphic = $true,
    [switch]$DebugInt = $false
)

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$bootImg = "target\x86_64-unknown-none\release\bootimage-neural-kernel.bin"
$logFile = "logs\neural-boot-$timestamp.log"

if (-not (Test-Path $bootImg)) {
    Write-Error "Boot image not found. Run 'cargo bootimage --release' first."
    exit 1
}

$qemuArgs = @(
    "-m", "${Memory}M",
    "-serial", "file:$logFile",
    "-nic", $Nic,
    "-drive", "format=raw,file=$bootImg",
    "-no-reboot",
    "-smp", $Smp
)

if ($NoGraphic) {
    $qemuArgs += "-nographic"
}

if ($DebugInt) {
    $qemuArgs += "-d", "int,cpu_reset,guest_errors"
    $logTrace = "logs\neural-trace-$timestamp.log"
    $qemuArgs += "-D", $logTrace
}

Write-Host "[QEMU] Boot log -> $logFile"
if ($DebugInt) { Write-Host "[QEMU] Trace log -> $logTrace" }
Write-Host "[QEMU] Starting Neural OS Hermes v0.40.0..."
Write-Host ""

# Run QEMU and also show serial output live
qemu-system-x86_64 @qemuArgs

# Show last 30 lines of log
Write-Host ""
if (Test-Path $logFile) {
    Write-Host "[QEMU] Exit. Last 30 lines:"
    Write-Host "----------------------------------------"
    Get-Content $logFile -Tail 30
    Write-Host "----------------------------------------"
    Write-Host "[QEMU] Full log: $logFile"
}
