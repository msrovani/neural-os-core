param([switch]$Window)
$ErrorActionPreference = "Stop"
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$logfile = "C:\DEV\neural-os-core\logs\boot_uefi_$timestamp.txt"
$uefi = "C:\DEV\neural-os-core\target\uefi.img"
$ovmf = "C:\DEV\neural-os-core\target\ovmf.fd"
if (!(Test-Path $uefi)) { Write-Host "Build first: cargo build --release"; exit 1 }
if (!(Test-Path $ovmf)) { Write-Host "OVMF not found"; exit 1 }
New-Item -ItemType Directory -Force -Path "C:\DEV\neural-os-core\logs" | Out-Null
Write-Host "=== NEURAL-OS-CORE (UEFI) ==="
Write-Host "RAM: 6G | CPU: 2 cores (TCG) | NIC: e1000"
Write-Host "Serial log: $logfile"
$disk = "C:\DEV\neural-os-core\target\disk_qemu.raw"
if (!(Test-Path $disk)) { Write-Host "Disk image not found. Run: python tools/build_image.py"; $disk = $null }
$diskArg = if ($disk) { @("-drive","format=raw,file=$disk,if=ide,index=1") } else { @() }
$args = @("-m","6G","-smp","2","-accel","tcg","-drive","format=raw,file=$uefi,if=ide,index=0") + $diskArg + @("-drive","if=pflash,format=raw,file=$ovmf,readonly=on","-serial","file:$logfile","-serial","tcp:127.0.0.1:4444,server=on,wait=off","-nic","user,model=e1000,hostfwd=tcp::4444-:4444,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446")
if ($Window) { & "C:\Program Files\qemu\qemu-system-x86_64.exe" @args -vga std }
else { & "C:\Program Files\qemu\qemu-system-x86_64.exe" @args -vga none -display none -nographic }
