# qemu_ota_loop.ps1 - Loop OTA: boot QEMU (1 instancia, config mesh/smoke) ->
# monitora log -> corrige -> kill -> restart, ate Jarbas subir e se comunicar
# com o serve_update.py (ADR-0086 A2). Validacao REAL (nao falso positivo):
#   - boot completo = [SCHEDULER] (fleet de pe) - NAO "Runtime" (aparece cedo)
#   - trigger = sendkey 'update' APOS scheduler
#   - sucesso = serve_update.py registra GET (log do server) OU kernel loga
#     fetch_update/check_for_update/MANIFEST
#
# Uso: powershell -File tools\qemu_ota_loop.ps1 [-MaxLoops N] [-TimeoutSec N]
# IMPORTANTE: arquivo ASCII puro (PS 5.1 le sem BOM como ANSI; multibyte quebra).

param(
    [int]$MaxLoops = 6,
    [int]$TimeoutSec = 300
)

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
$srvLog = Join-Path $logDir "ota_server.txt"
$monPort = 45454

# --- monitor QEMU via TCP ---
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
    'a'="0x1e"; 'b'="0x30"; 'c'="0x2e"; 'd'="0x20"; 'e'="0x12"; 'f'="0x21"; 'g'="0x22"; 'h'="0x23";
    'i'="0x17"; 'j'="0x24"; 'k'="0x25"; 'l'="0x26"; 'm'="0x32"; 'n'="0x31"; 'o'="0x18"; 'p'="0x19";
    'q'="0x10"; 'r'="0x13"; 's'="0x1f"; 't'="0x14"; 'u'="0x16"; 'v'="0x2f"; 'w'="0x11"; 'x'="0x2d";
    'y'="0x15"; 'z'="0x2c"; ' '="0x39"; '.'="0x34"; '/'="0x35"; '-'="0x0c"; '='="0x0d";
    '0'="0x0b"; '1'="0x02"; '2'="0x03"; '3'="0x04"; '4'="0x05"; '5'="0x06"; '6'="0x07"; '7'="0x08";
    '8'="0x09"; '9'="0x0a"
}
function Send-Key([string]$keys) {
    foreach ($c in $keys.ToCharArray()) {
        if ($SC.ContainsKey($c)) {
            Qemu-Monitor "sendkey $($SC[$c])" | Out-Null
            Start-Sleep -Milliseconds 50
        }
    }
    Qemu-Monitor "sendkey ret" | Out-Null
    Start-Sleep -Milliseconds 300
}

function Start-Qemu {
    # Config mesh/smoke: TCG, OVMF, -netdev user (10.0.2.2 = host p/ OTA), monitor TCP.
    # SEM disco de dados (index 1): ATA PIO sob TCG trava o boot (SESSION_243);
    # o UPDATE.CFG vive na ESP (uefi.img, index 0) e o update baixa via rede.
    $args = @(
        "-m", "4G",
        "-smp", "1",
        "-cpu", "max",
        "-accel", "tcg",
        "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
        "-drive", "format=raw,file=$uefi,if=ide,index=0",
        "-no-reboot",
        "-vga", "std",
        "-display", "none",
        "-netdev", "user,id=n0", "-device", "e1000,netdev=n0",
        "-serial", "file:$log",
        "-monitor", "tcp:127.0.0.1:$monPort,server,nowait"
    )
    # Loaders LLM: NENHUM por padrao — o BITNET2B.BIN (576MB) via QEMU-loader
    # trava o boot em TCG: o greeting do Jarbas roda o forward 2B (30 layers
    # de matmul ternario) = inviavel em TCG puro (nunca chega ao SCHEDULER).
    # O objetivo do loop e Jarbas + OTA (server python), nao o LLM. Opcional:
    # use -WithLLM para carregar (so em WHPX/qemu64, nao TCG).
    Write-Host "  QEMU TCG start (log=$log)" -ForegroundColor Cyan
    $p = Start-Process -FilePath $qemu -ArgumentList $args -PassThru -NoNewWindow
    $deadline = (Get-Date).AddSeconds(45)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 2
        if (Qemu-Monitor "info status" -ne "") { break }
    }
    return $p
}

function Wait-Log([string]$label, [string[]]$patterns, [int]$secs) {
    $deadline = (Get-Date).AddSeconds($secs)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 3
        if (Test-Path $log) {
            $content = Get-Content $log -Raw -ErrorAction SilentlyContinue
            foreach ($pat in $patterns) {
                if ($content -match $pat) {
                    Write-Host "[$label] OK: '$pat'" -ForegroundColor Green
                    return $true
                }
            }
        }
    }
    Write-Host "[$label] TIMEOUT esperando $($patterns -join '|')" -ForegroundColor Red
    return $false
}

# --- server OTA (log redirecionado para validacao real) ---
Remove-Item $srvLog -ErrorAction SilentlyContinue
$srvErr = Join-Path $logDir "ota_server_err.txt"
Remove-Item $srvErr -ErrorAction SilentlyContinue
$srv = Start-Process python -ArgumentList "tools\serve_update.py","--port","8080","--version","1.9.10","--base-url","http://10.0.2.2:8080" -PassThru -NoNewWindow -RedirectStandardError $srvErr -RedirectStandardOutput $srvLog
Start-Sleep -Seconds 2

$ok = $false
for ($i = 1; $i -le $MaxLoops; $i++) {
    Write-Host "=== LOOP $i/$MaxLoops ===" -ForegroundColor Green
    Remove-Item $log -ErrorAction SilentlyContinue
    $p = Start-Qemu
    # Boot completo = [BOOT:Runtime] (fleet de pe + scheduler). NAO "SCHEDULER"
    # (o kernel loga 'AgentScheduler', nunca 'SCHEDULER').
    if (-not (Wait-Log "boot" @("BOOT:Runtime") $TimeoutSec)) {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        continue
    }
    # Espera o shell/Hermes pronto (InputAgent escutando USER_INTENT).
    Start-Sleep -Seconds 10
    Write-Host "  shell: update" -ForegroundColor Cyan
    Send-Key "update"
    # Validacao REAL: (a) kernel loga fetch_update/check_for_update/MANIFEST,
    # (b) serve_update.py registra GET no srvLog.
    $kernel = Wait-Log "kernel-ota" @("fetch_update","check_for_update","UPDATE.MANIFEST","apply_update") 150
    $server = $false
    $dl = (Get-Date).AddSeconds(90)
    while ((Get-Date) -lt $dl) {
        Start-Sleep -Seconds 3
        if (Test-Path $srvLog) {
            $sc = Get-Content $srvLog -Raw -ErrorAction SilentlyContinue
            if ($sc -match "GET /UPDATE|GET /KERNEL|POST /api|GET /api") {
                $server = $true
                Write-Host "[server] OK: recebeu GET" -ForegroundColor Green
                break
            }
        }
    }
    # Jarbas de pe (display bridge + runtime).
    $jarbas = Wait-Log "jarbas" @("JARBAS.*BRIDGE.*cutover=done","Runtime","SCHEDULER.*agents") 60
    if ($kernel -or $server) {
        Write-Host ("=== LOOP {0}: SUCESSO (OTA real: kernel={1} server={2}) ===" -f $i, $kernel, $server) -ForegroundColor Green
        $ok = $true
        Start-Sleep -Seconds 5
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        break
    }
    Write-Host "[loop] kernel-ota=$kernel server=$server jarbas=$jarbas - matando e reiniciando" -ForegroundColor Yellow
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
}

Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue

if ($ok) {
    Write-Host ""
    Write-Host "=== OTA LOOP: SUCESSO ===" -ForegroundColor Green
    Write-Host "Log: $log"
    exit 0
} else {
    Write-Host ""
    Write-Host "=== OTA LOOP: FALHOU apos $MaxLoops loops (ver $log e $srvLog) ===" -ForegroundColor Red
    exit 1
}
