param(
    [int]$Cores = 4,
    [int]$RamGB = 6,
    [string]$Accel = "tcg",
    [switch]$Window
)

$ErrorActionPreference = "Stop"
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$logfile = "C:\DEV\neural-os-core\logs\boot_serial_$timestamp.txt"
$bios = "C:\DEV\neural-os-core\target\bios.img"

if (!(Test-Path $bios)) {
    Write-Host "ERROR: bios.img not found. Run 'cargo build --release' first."
    exit 1
}

New-Item -ItemType Directory -Force -Path "C:\DEV\neural-os-core\logs" | Out-Null

Write-Host "=== NEURAL-OS-CORE QEMU ==="
Write-Host "RAM: ${RamGB}G | CPU: ${Cores} cores ($Accel) | NIC: e1000 NAT"
Write-Host "Serial log: $logfile"
Write-Host "Screen: $(if ($Window) { 'QEMU window' } else { 'stdout (redirected to log)' })"

if ($Window) {
    # With window: serial to file (data only after QEMU exits)
    & "C:\Program Files\qemu\qemu-system-x86_64.exe" `
        -m "${RamGB}G" -smp $Cores -accel $Accel `
        -drive "format=raw,file=$bios,if=ide" `
        -serial "file:$logfile" `
        -serial "tcp:127.0.0.1:4444,server=on,wait=off" `
        -nic "user,model=e1000,hostfwd=tcp::4444-:4444,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446" `
        -vga std
    Write-Host "`nSerial log saved to: $logfile"
    Get-Content -LiteralPath $logfile -Tail 10
} else {
    # No window: redirect serial stdout to file
    $p = Start-Process -FilePath "C:\Program Files\qemu\qemu-system-x86_64.exe" `
        -ArgumentList "-m ${RamGB}G -smp $Cores -accel $Accel -drive format=raw,file=$bios,if=ide -serial stdio -serial tcp:127.0.0.1:4444,server=on,wait=off -nic user,model=e1000,hostfwd=tcp::4444-:4444,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446 -vga none -display none -nographic" `
        -WindowStyle Hidden -RedirectStandardOutput $logfile -RedirectStandardError $logfile -PassThru
    Write-Host "QEMU PID: $($p.Id)"
    Write-Host "Close QEMU or press Ctrl+C to stop."
    $p.WaitForExit()
    Write-Host "`n=== LOG (last 20 lines) ==="
    Get-Content -LiteralPath $logfile -Tail 20
}
