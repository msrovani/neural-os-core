# ota_launch.ps1 - Sobe serve_update.py + QEMU (detached), retorna imediato.
# PIDs salvos em logs/ota_pids.txt para o kill/estagios seguintes.
# T-046: TCG only (WHPX OVMF #GP). T-047: 4G. T-045: smp 1 (smp 2 TCG SIPI hang).
# Uso: powershell -File tools\ota_launch.ps1
# IMPORTANTE: ASCII puro (PS 5.1 le sem BOM como ANSI).

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
$target = Join-Path $Root "target"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Path $logDir -Force | Out-Null

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$ovmf = "C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $qemu)) { Write-Host "[ERRO] QEMU nao encontrado"; exit 1 }
if (-not (Test-Path $ovmf)) { Write-Host "[ERRO] OVMF nao encontrado"; exit 1 }

$uefi = Join-Path $target "uefi.img"
$log  = Join-Path $logDir "ota_loop.txt"
$pids = Join-Path $logDir "ota_pids.txt"

# Mata sobras
Get-Process qemu-system-x86_64, python -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Remove-Item $log, (Join-Path $logDir "ota_server.txt"), (Join-Path $logDir "ota_server_err.txt") -ErrorAction SilentlyContinue

# Server OTA (detached, redirect para arquivo). --token "" desliga auth (S2):
# o kernel no_std nao envia Authorization: Bearer; dev localhost/QEMU.
# ArgumentList como string unica (array com "" vazio falha no Start-Process).
$srvArgs = 'tools\serve_update.py --port 8080 --version 1.9.10 --base-url http://10.0.2.2:8080 --token ""'
$srv = Start-Process python -ArgumentList $srvArgs -WindowStyle Hidden -RedirectStandardError (Join-Path $logDir "ota_server_err.txt") -RedirectStandardOutput (Join-Path $logDir "ota_server.txt") -PassThru

# Flag OTA via QEMU loader (padrão netmode.flag, SESSION_252): o kernel escaneia
# a janela da RAM procurando 'O' e dispara check_for_update() no boot — contorna
# o teclado IRQ1 (não entregue via IOAPIC no QEMU) e valida o fluxo OTA e2e
# (UPDATE.CFG -> GET manifest -> serve_update.py).
$otaFlag = Join-Path $target "ota.flag"
[System.IO.File]::WriteAllBytes($otaFlag, [byte[]]@([byte][char]'O'))

# QEMU (detached, SEM -NoNewWindow p/ nao segurar console; sem disco de dados:
# ATA PIO sob TCG trava boot; UPDATE.CFG na ESP index 0)
$args = @(
    "-m", "4G",
    "-smp", "1",
    "-cpu", "max",
    "-accel", "tcg",
    "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
    "-drive", "format=raw,file=$uefi,if=ide,index=0",
    "-device", "loader,file=$otaFlag,addr=0x160000000",
    "-no-reboot",
    "-vga", "std",
    "-display", "none",
    "-netdev", "user,id=n0", "-device", "e1000,netdev=n0",
    "-serial", "file:$log",
    "-monitor", "tcp:127.0.0.1:45454,server,nowait"
)
$q = Start-Process -FilePath $qemu -ArgumentList $args -PassThru -WindowStyle Hidden

"$($srv.Id) $($q.Id)" | Out-File -Encoding ASCII $pids
Write-Host "server_pid=$($srv.Id) qemu_pid=$($q.Id) log=$log"
