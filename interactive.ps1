param([switch]$Serial)
$ErrorActionPreference = "Stop"
$uefi = "C:\DEV\neural-os-core\target\uefi.img"
$ovmf = "C:\DEV\neural-os-core\target\ovmf.fd"
Write-Host "=== NEURAL-OS-CORE INTERATIVO ===" -ForegroundColor Cyan
Write-Host "RAM: 6G | CPU: 2 (TCG) | NIC: e1000 | Video: 1280x800" -ForegroundColor Gray
if ($Serial) {
    Write-Host "Digite comandos no terminal (Ctrl+C para sair)" -ForegroundColor Yellow
    Write-Host "Ex: 'hello', 'status', 'help'" -ForegroundColor Yellow
    & "C:\Program Files\qemu\qemu-system-x86_64.exe" -m 6G -smp 2 -accel tcg -drive "format=raw,file=$uefi,if=ide" -drive "if=pflash,format=raw,file=$ovmf,readonly=on" -serial stdio -nic "user,model=e1000" -vga std
} else {
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $logfile = "C:\DEV\neural-os-core\logs\interactive_$timestamp.txt"
    Write-Host "QEMU window aberta. Log serial em: $logfile" -ForegroundColor Yellow
    Write-Host "Para interagir: use a janela QEMU (teclado PS/2)" -ForegroundColor Yellow
    & "C:\Program Files\qemu\qemu-system-x86_64.exe" -m 6G -smp 2 -accel tcg -drive "format=raw,file=$uefi,if=ide" -drive "if=pflash,format=raw,file=$ovmf,readonly=on" -serial "file:$logfile" -serial "tcp:127.0.0.1:4444,server=on,wait=off" -nic "user,model=e1000,hostfwd=tcp::4444-:4444,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446" -vga std
}
