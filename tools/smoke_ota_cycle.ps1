# smoke_ota_cycle.ps1 - ADR-0086 A2: ciclo OTA no QEMU (base: run-qemu-p2p-mesh.ps1, 1 instancia/ato).
#   Ato 1: boot pendrive (uefi.img + disk_mini.raw) -> shell `install` -> SysInstaller grava target.raw
#   Ato 2: boot do target (target.raw no index=0) -> shell `provision` -> baixa modelos do serve_update.py
#
# Uso: powershell -File tools\smoke_ota_cycle.ps1 [-SkipInstall] [-SkipProvision]
# IMPORTANTE: arquivo ASCII puro (PS 5.1 le sem BOM como ANSI; multibyte quebra parse).
param(
    [switch]$SkipInstall,
    [switch]$SkipProvision,
    [int]$AtoTimeoutSec = 240
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
$target = Join-Path $Root "target"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Path $logDir -Force | Out-Null

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemu)) { Write-Host "[ERRO] QEMU nao encontrado"; exit 1 }
$ovmf = "C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmf)) { Write-Host "[ERRO] OVMF nao encontrado em $ovmf"; exit 1 }

$monPort = 45454
$logAto1 = Join-Path $logDir "smoke_ota_ato1.txt"
$logAto2 = Join-Path $logDir "smoke_ota_ato2.txt"

# --- monitor QEMU via TCP (PowerShell nativo p/ sendkey) ---
function Qemu-Monitor([string]$cmd) {
    try {
        $c = New-Object System.Net.Sockets.TcpClient("127.0.0.1", $monPort)
        $s = $c.GetStream()
        $bytes = [System.Text.Encoding]::ASCII.GetBytes($cmd + "`n")
        $s.Write($bytes, 0, $bytes.Length)
        $s.Flush()
        Start-Sleep -Milliseconds 150
        $buf = New-Object byte[] 4096
        $n = 0
        if ($s.CanRead) {
            $s.ReadTimeout = 500
            try { $n = $s.Read($buf, 0, 4096) } catch { $n = 0 }
        }
        $c.Close()
        if ($n -gt 0) { [System.Text.Encoding]::ASCII.GetString($buf, 0, $n) } else { "" }
    } catch { "" }
}

$SC = @{
    'a'="0x1e";'b'="0x30";'c'="0x2e";'d'="0x20";'e'="0x12";'f'="0x21";'g'="0x22";'h'="0x23";
    'i'="0x17";'j'="0x24";'k'="0x25";'l'="0x26";'m'="0x32";'n'="0x31";'o'="0x18";'p'="0x19";
    'q'="0x10";'r'="0x13";'s'="0x1f";'t'="0x14";'u'="0x16";'v'="0x2f";'w'="0x11";'x'="0x2d";
    'y'="0x15";'z'="0x2c";' '="0x39";'.'="0x34";'/'="0x35";'-'="0x0c";'='="0x0d";
    '0'="0x0b";'1'="0x02";'2'="0x03";'3'="0x04";'4'="0x05";'5'="0x06";'6'="0x07";'7'="0x08";
    '8'="0x09";'9'="0x0a"
}

function Send-Key([string]$keys) {
    foreach ($c in $keys.ToCharArray()) {
        if ($SC.ContainsKey($c)) {
            Qemu-Monitor "sendkey $($SC[$c])" | Out-Null
            Start-Sleep -Milliseconds 50
        }
    }
    Qemu-Monitor "sendkey ret" | Out-Null   # Enter
    Start-Sleep -Milliseconds 300
}

function Start-Qemu([string]$log, [string[]]$extraDrives) {
    # Base mesh: TCG puro (MTTCG -smp 2 ~4x), OVMF, -no-reboot. Sem WHPX (VP exit 4).
    $args = @(
        "-m", "6G",
        "-smp", "2",
        "-cpu", "max",
        "-accel", "tcg",
        "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
        "-drive", "format=raw,file=$target\uefi.img,if=ide,index=0",
        "-no-reboot",
        "-vga", "std",
        "-display", "none",
        "-netdev", "user,id=n0", "-device", "e1000,netdev=n0",
        "-serial", "file:$log",
        "-monitor", "tcp:127.0.0.1:$monPort,server,nowait"
    )
    foreach ($d in $extraDrives) { $args += $d }
    Write-Host "  QEMU TCG start (log=$log)" -ForegroundColor Cyan
    $p = Start-Process -FilePath $qemu -ArgumentList $args -PassThru -NoNewWindow
    # espera o monitor aceitar conexao
    $deadline = (Get-Date).AddSeconds(45)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 2
        if (Qemu-Monitor "info status" -ne "" ) { break }
    }
    return $p
}

function Wait-Boot([string]$log, [string]$label) {
    # Detecta boot pelo log (Runtime/AgentFleet) em vez de sleep fixo.
    Write-Host "[$label] aguardando boot (Runtime)..." -ForegroundColor Cyan
    $deadline = (Get-Date).AddSeconds(300)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 3
        if (Test-Path $log) {
            $content = Get-Content $log -Raw -ErrorAction SilentlyContinue
            if ($content -match "Runtime|AgentFleet|SCHEDULER") {
                Write-Host "[$label] boot OK (Runtime detectado)" -ForegroundColor Green
                return $true
            }
        }
    }
    Write-Host "[$label] TIMEOUT no boot" -ForegroundColor Red
    return $false
}

function Invoke-ShellCmd([string]$log, [string]$cmd, [string]$expect, [string]$label) {
    Write-Host "[$label] shell: $cmd" -ForegroundColor Cyan
    Send-Key $cmd
    $deadline = (Get-Date).AddSeconds($AtoTimeoutSec)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 3
        if (Test-Path $log) {
            $content = Get-Content $log -Raw -ErrorAction SilentlyContinue
            if ($content -match $expect) {
                Write-Host "[$label] OK: '$expect'" -ForegroundColor Green
                return $true
            }
        }
    }
    Write-Host "[$label] TIMEOUT esperando '$expect'" -ForegroundColor Red
    return $false
}

# --- Ato 1: install (boot pendrive, target.raw no index=2) ---
if (-not $SkipInstall) {
    Remove-Item $logAto1 -ErrorAction SilentlyContinue
    & "C:\Program Files\qemu\qemu-img.exe" create -f raw "$target\install_target.raw" 2G | Out-Null
    $extra = @("-drive", "format=raw,file=$target\disk_mini.raw,if=ide,index=1",
               "-drive", "format=raw,file=$target\install_target.raw,if=ide,index=2")
    Write-Host "=== ATO 1: install ===" -ForegroundColor Green
    $p = Start-Qemu $logAto1 $extra
    if (-not (Wait-Boot $logAto1 "Ato1")) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue; exit 1 }
    $ok = Invoke-ShellCmd $logAto1 "install" "SYS-INST" "Ato1"
    if (-not $ok) { Write-Host "FALHA Ato1" -ForegroundColor Red; Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue; exit 1 }
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
}

# --- Ato 2: boot do target + provision ---
if (-not $SkipProvision) {
    Remove-Item $logAto2 -ErrorAction SilentlyContinue
    $srv = Start-Process python -ArgumentList "tools\serve_update.py","--port","8080","--version","1.9.10" -PassThru -NoNewWindow
    Start-Sleep -Seconds 2
    $extra = @("-drive", "format=raw,file=$target\install_target.raw,if=ide,index=0",
               "-drive", "format=raw,file=$target\disk_mini.raw,if=ide,index=1")
    Write-Host "=== ATO 2: boot do target + provision ===" -ForegroundColor Green
    $p = Start-Qemu $logAto2 $extra
    if (-not (Wait-Boot $logAto2 "Ato2")) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue; Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue; exit 1 }
    $ok = Invoke-ShellCmd $logAto2 "provision" "PROV" "Ato2"
    if (-not $ok) { Write-Host "FALHA Ato2" -ForegroundColor Red; Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue; Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue; exit 1 }
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
}

Write-Host ""
Write-Host "=== SMOKE OTA: ATOS 1-2 OK ===" -ForegroundColor Green
Write-Host "Ato1 log: $logAto1"
Write-Host "Ato2 log: $logAto2"
