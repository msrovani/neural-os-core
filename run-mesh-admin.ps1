# Requires: run as Administrator (UAC)
# Lanca duas instancias QEMU com multicast P2P
Write-Host "=== Neural-OS-Core P2P Mesh ===" -ForegroundColor Cyan
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$ovmf = "$root\target\ovmf.fd"
$uefi = "$root\target\uefi.img"
$disk = "$root\target\disk_mesh_min.raw"

# Prepara netmode flags
[System.IO.File]::WriteAllBytes("$root\target\netmode_a.flag", [byte[]]@([byte][char]'S', 10, 0, 3, 2))
[System.IO.File]::WriteAllBytes("$root\target\netmode_b.flag", [byte[]]@([byte][char]'S', 10, 0, 3, 3))

$base = @("-m", "6G", "-smp", "2", "-cpu", "qemu64", "-accel", "whpx",
    "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
    "-drive", "format=raw,file=$uefi,if=ide,index=0",
    "-netdev", "socket,mcast=230.0.0.1:1234,id=n0",
    "-device", "e1000,netdev=n0",
    "-display", "none", "-no-reboot")

# Cloverleaf (10.0.3.2)
Write-Host "[LANCANDO] Cloverleaf (10.0.3.2)..." -ForegroundColor Green
Remove-Item "$root\logs\boot_cloverleaf.log" -Force -ErrorAction SilentlyContinue
$argsA = $base + @(
    "-drive", "format=raw,file=$disk,if=ide,index=1",
    "-device", "loader,file=$root\target\netmode_a.flag,addr=0x16400000000",
    "-serial", "file:$root\logs\boot_cloverleaf.log")
Start-Process -FilePath $qemu -ArgumentList $argsA -WindowStyle Hidden
Start-Sleep -Seconds 10

# Hal9000 (10.0.3.3)
Write-Host "[LANCANDO] Hal9000 (10.0.3.3)..." -ForegroundColor Green
Remove-Item "$root\logs\boot_hal9000.log" -Force -ErrorAction SilentlyContinue
$argsB = $base + @(
    "-device", "loader,file=$root\target\netmode_b.flag,addr=0x16400000000",
    "-serial", "file:$root\logs\boot_hal9000.log")
Start-Process -FilePath $qemu -ArgumentList $argsB -WindowStyle Hidden

Write-Host ""
Write-Host "=== Mesh lancado! ===" -ForegroundColor Cyan
Write-Host "Acompanhe: Get-Content logs\boot_cloverleaf.log -Tail 30 -Wait" -ForegroundColor Gray
Write-Host "Acompanhe: Get-Content logs\boot_hal9000.log -Tail 30 -Wait" -ForegroundColor Gray
Write-Host ""
Write-Host "Pressione ENTER para encerrar as instancias..."
Read-Host | Out-Null
Get-Process -Name "qemu*" | Stop-Process -Force
Write-Host "Instancias encerradas." -ForegroundColor Yellow
