<#
.SYNOPSIS
  Lanca 2 instancias QEMU em multicast para teste P2P Mesh (ADR-0081).
  Instancia A (Cloverleaf) e Instancia B (Hal9000) se descobrem via broadcast.
  Sem TAP, sem admin, sem bridge Python.
.DESCRIPTION
  Cada instancia usa -netdev socket,mcast=230.0.0.1:1234 para se comunicar.
  Logs separados: logs/boot_mesh_a.txt e logs/boot_mesh_b.txt.
  IPs estaticos: A=10.0.3.2, B=10.0.3.3.
  NetMode: each instance uses 'hostfwd' for its own serial debug.
#>

$Root = Split-Path -Parent $PSScriptRoot
$target = Join-Path $Root "target"
$logDir = Join-Path $Root "logs"

# Ensure log directory exists
New-Item -ItemType Directory -Path $logDir -Force | Out-Null

# Find the UEFI image and disk image
$uefi = Join-Path $target "uefi.img"
$disk = Join-Path $target "disk_qemu.raw"
if (-not (Test-Path $uefi)) {
    Write-Host "[ERRO] uefi.img nao encontrado em $target" -ForegroundColor Red
    Write-Host "       Rode 'cargo build --release' primeiro" -ForegroundColor Yellow
    exit 1
}
if (-not (Test-Path $disk)) {
    Write-Host "[AVISO] disk_qemu.raw nao encontrado — segundo drive sem dados" -ForegroundColor Yellow
    $disk = ""
}

# QEMU path
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemu)) {
    # Try common alternatives
    $alt = Get-Command "qemu-system-x86_64" -ErrorAction SilentlyContinue
    if ($alt) { $qemu = $alt.Source }
    else {
        Write-Host "[ERRO] QEMU nao encontrado. Instale em C:\Program Files\qemu\ ou adicione ao PATH" -ForegroundColor Red
        exit 1
    }
}

# Common base config
$baseArgs = @(
    "-m", "2G",
    "-smp", "2",
    "-cpu", "max",
    "-accel", "tcg",
    "-drive", "format=raw,file=$uefi,if=ide,index=0",
    "-vga", "std",
    "-display", "none",
    "-no-reboot"
)

# ─── NetMode flags: IP customizado para cada instancia ───
# SESSION_233: endereco corrigido! Kernel espera NETMODE_LOADER_PHYS = 0x164000000
# (5.56GB, dentro de 8GB de RAM). Os valores antigos (0x16400000000 e 0x1640000000)
# eram 16x/160x maiores e FORA da RAM -> flag nunca era lido -> ambas caiam no
# default slirp 10.0.2.15.
$netmodeAddr = 0x164000000  # NETMODE_LOADER_PHYS (corrigido)
$netmodeA = Join-Path $target "netmode_a.flag"
$netmodeB = Join-Path $target "netmode_b.flag"

# Instance A: 10.0.3.2 → 'S' + [10, 0, 3, 2]
[System.IO.File]::WriteAllBytes($netmodeA, [byte[]]@([byte][char]'S', 10, 0, 3, 2))
# Instance B: 10.0.3.3 → 'S' + [10, 0, 3, 3]
[System.IO.File]::WriteAllBytes($netmodeB, [byte[]]@([byte][char]'S', 10, 0, 3, 3))

# ─── Instance A: Cloverleaf (10.0.3.2) ───
$logA = Join-Path $logDir "boot_mesh_a.txt"
$argsA = $baseArgs + @(
    "-name", "mesh-cloverleaf",
    "-netdev", "socket,mcast=230.0.0.1:1234,id=n0",
    "-device", "e1000,netdev=n0,mac=52:54:00:AA:00:01",
    "-device", "loader,file=$netmodeA,addr=$netmodeAddr",
    "-serial", "file:$logA"
)
if ($disk) { $argsA += @("-drive", "format=raw,file=$disk,if=ide,index=1") }

# ─── Instance B: Hal9000 (10.0.3.3) ───
$logB = Join-Path $logDir "boot_mesh_b.txt"
$argsB = $baseArgs + @(
    "-name", "mesh-hal9000",
    "-netdev", "socket,mcast=230.0.0.1:1234,id=n0",
    "-device", "e1000,netdev=n0,mac=52:54:00:BB:00:02",
    "-device", "loader,file=$netmodeB,addr=$netmodeAddr",
    "-serial", "file:$logB"
)
if ($disk) { $argsB += @("-drive", "format=raw,file=$disk,if=ide,index=1") }

# ─── Launch ───
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  P2P MESH TEST — ADR-0081" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "  A (Cloverleaf): 10.0.3.2" -ForegroundColor Green
Write-Host "    Log: $logA" -ForegroundColor Gray
Write-Host "    MAC: 52:54:00:AA:00:01" -ForegroundColor Gray
Write-Host ""
Write-Host "  B (Hal9000):    10.0.3.3" -ForegroundColor Green
Write-Host "    Log: $logB" -ForegroundColor Gray
Write-Host "    MAC: 52:54:00:BB:00:02" -ForegroundColor Gray
Write-Host ""
Write-Host "  Rede: multicast 230.0.0.1:1234" -ForegroundColor Yellow
Write-Host "  TCG (sem WHPX) — mais lento, sem falha VP exit" -ForegroundColor Yellow
Write-Host ""
Write-Host "  O que observar:" -ForegroundColor Cyan
Write-Host "  1. Ambas sobem ate NetAgent" -ForegroundColor Cyan
Write-Host "  2. UDP broadcast na porta 42069" -ForegroundColor Cyan
Write-Host "  3. MESH_ENGINE descobre o outro no" -ForegroundColor Cyan
Write-Host "  4. Eleicao: um vira Master, outro Worker" -ForegroundColor Cyan
Write-Host "  5. Marketplace: skills anunciadas entre nos" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Logs:" -ForegroundColor Gray
Write-Host "    tail -f logs/boot_mesh_a.txt" -ForegroundColor Gray
Write-Host "    tail -f logs/boot_mesh_b.txt" -ForegroundColor Gray
Write-Host ""

# Launch A in background
Write-Host "[LANCANDO] Instancia A (Cloverleaf)..." -ForegroundColor Green
$procA = Start-Process -FilePath $qemu -ArgumentList $argsA -NoNewWindow -PassThru
Write-Host "           PID: $($procA.Id)" -ForegroundColor Gray
Start-Sleep -Seconds 2

# Launch B in background
Write-Host "[LANCANDO] Instancia B (Hal9000)..." -ForegroundColor Green
$procB = Start-Process -FilePath $qemu -ArgumentList $argsB -NoNewWindow -PassThru
Write-Host "           PID: $($procB.Id)" -ForegroundColor Gray

Write-Host ""
Write-Host "Ambas as instancias estao rodando." -ForegroundColor Green
Write-Host "Acompanhe os logs para ver a descoberta P2P!" -ForegroundColor Cyan
Write-Host ""
Write-Host "Para encerrar: Ctrl+C ou:" -ForegroundColor Gray
Write-Host "  Stop-Process -Id $($procA.Id) -Force" -ForegroundColor Gray
Write-Host "  Stop-Process -Id $($procB.Id) -Force" -ForegroundColor Gray
Write-Host ""

# Wait for either process to exit
$procA.WaitForExit()
$procB.WaitForExit()
Write-Host "[FIM] Ambas as instancias encerradas." -ForegroundColor Yellow
