param(
    [string]$Smp = "2",
    [int]$Memory = 2048,
    [string]$Nic = "user,model=rtl8139",
    [switch]$NoGraphic = $false,
    [switch]$DebugInt = $false,
    [switch]$NoAccel = $false,
    [string]$Vga = "std"
)

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$bootImg = "target\x86_64-unknown-none\release\bootimage-neural-kernel.bin"
$logFile = "logs\neural-boot-$timestamp.log"

if (-not (Test-Path $bootImg)) {
    Write-Error "Boot image not found. Run 'cargo bootimage --release' first."
    exit 1
}

# Check if WHPX is available (Windows Hypervisor Platform)
$accel = ""
if (-not $NoAccel) {
    # Test WHPX by trying to initialize with a real machine type
    $test = qemu-system-x86_64 -accel whpx -M pc -S 2>&1 | Select-String -SimpleMatch "not found"
    if (-not $test) {
        $accel = "whpx"
    } else {
        $testHax = qemu-system-x86_64 -accel hax -M pc -S 2>&1 | Select-String -SimpleMatch "not found"
        if (-not $testHax) {
            $accel = "hax"
        }
    }
}

$qemuArgs = @(
    "-m", "${Memory}M",
    "-serial", "file:$logFile",
    "-nic", $Nic,
    "-drive", "format=raw,file=$bootImg",
    "-no-reboot",
    "-smp", $Smp,
    "-vga", $Vga
)

if ($accel -eq "whpx") {
    $qemuArgs = @("-accel", "whpx") + $qemuArgs
    Write-Host "[QEMU] WHPX acceleration enabled (Windows Hypervisor Platform)"
} elseif ($accel -eq "hax") {
    $qemuArgs = @("-accel", "hax") + $qemuArgs
    Write-Host "[QEMU] HAXM acceleration enabled"
} else {
    Write-Host "[QEMU] No hardware acceleration available. Using TCG (slow)."
}

if ($DebugInt) {
    $qemuArgs += @("-d", "int,cpu_reset,guest_errors", "-D", "logs\neural-trace-$timestamp.log")
}

Write-Host "[QEMU] Boot log -> $logFile"
Write-Host "[QEMU] Starting Neural OS Hermes v0.55.0..."
Write-Host ""

# Run QEMU
qemu-system-x86_64 @qemuArgs

# Show summary after QEMU exits
Write-Host ""
if (Test-Path $logFile) {
    Write-Host "[QEMU] Exit. Last lines:"
    Write-Host "----------------------------------------"
    Get-Content $logFile -Tail 20
    Write-Host "----------------------------------------"
    Write-Host "[QEMU] Full log: $logFile"
}
