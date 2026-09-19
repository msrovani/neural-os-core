# Lab 3x QEMU: 2GB RAM, 2 cores, mesh via L2 hub (ADR-0081 / s360).
# AutoLearn+SleepCycle METRIC in guest; monitor serial.
param(
    [ValidateSet("whpx","tcg")]
    [string]$Accel = "whpx",
    [int]$Seconds = 90,
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
if (-not (Test-Path (Join-Path $Root "Cargo.toml"))) { $Root = $PSScriptRoot }
Set-Location $Root

$logDir = Join-Path $Root "logs"
$target = Join-Path $Root "target"
New-Item -ItemType Directory -Path $logDir -Force | Out-Null
New-Item -ItemType Directory -Path $target -Force | Out-Null

$uefi = Join-Path $target "uefi.img"
if (-not (Test-Path $uefi)) { $uefi = Join-Path $Root "target1\uefi.img" }
if (-not (Test-Path $uefi)) {
    Write-Host "[ERRO] uefi.img ausente - rode cargo build --release -p boot" -ForegroundColor Red
    exit 1
}

if (-not $NoBuild) {
    Write-Host "[BUILD] cargo build --release -p boot ..." -ForegroundColor Cyan
    $env:CARGO_TARGET_DIR = "target/agent-mesh3"
    cargo build --release -p boot 2>&1 | Select-Object -Last 15
    $built = Join-Path $Root "target\uefi.img"
    if (Test-Path $built) { $uefi = $built }
}

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemu)) {
    $alt = Get-Command qemu-system-x86_64 -ErrorAction SilentlyContinue
    if ($alt) { $qemu = $alt.Source } else { Write-Host "[ERRO] QEMU" -ForegroundColor Red; exit 1 }
}
$ovmf = "C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmf)) {
    $ovmf = Join-Path (Split-Path $qemu -Parent) "share\edk2-x86_64-code.fd"
}

function Write-Netmode([string]$Path, [byte]$HostOctet) {
    [System.IO.File]::WriteAllBytes($Path, [byte[]]@([byte][char]'S', 10, 0, 3, $HostOctet))
}
$nmA = Join-Path $target "netmode_a.bin"; Write-Netmode $nmA 2
$nmB = Join-Path $target "netmode_b.bin"; Write-Netmode $nmB 3
$nmC = Join-Path $target "netmode_c.bin"; Write-Netmode $nmC 4
Copy-Item $nmA (Join-Path $target "netmode_a.flag") -Force
Copy-Item $nmB (Join-Path $target "netmode_b.flag") -Force
Copy-Item $nmC (Join-Path $target "netmode_c.flag") -Force

$hubPort = 19000
$netmodeAddr = "0x16400000000"

Write-Host "[HUB] python tools/qemu_l2_hub.py --port $hubPort" -ForegroundColor Green
$hubOut = Join-Path $logDir "l2_hub.txt"
$hubErr = Join-Path $logDir "l2_hub.err.txt"
$hub = Start-Process -FilePath "python" -ArgumentList @("tools/qemu_l2_hub.py","--port","$hubPort") `
    -NoNewWindow -PassThru -RedirectStandardOutput $hubOut -RedirectStandardError $hubErr

function Start-Node([string]$Id, [string]$Mac, [int]$LocalUdp, [string]$Netmode, [string]$Log) {
    if (Test-Path $Log) { Remove-Item $Log -Force }
    $qa = @(
        "-m", "2G", "-smp", "2",
        "-accel", $Accel,
        "-drive", "if=pflash,format=raw,readonly=on,file=$ovmf",
        "-drive", "format=raw,file=$uefi,if=ide,index=0",
        "-netdev", "socket,id=n0,udp=127.0.0.1:${hubPort},localaddr=127.0.0.1:${LocalUdp}",
        "-device", "e1000,netdev=n0,mac=$Mac",
        "-device", "loader,file=$Netmode,addr=$netmodeAddr",
        "-serial", "file:$Log",
        "-display", "none",
        "-name", "mesh3-$Id"
    )
    Write-Host "[QEMU] $Id 2G/2c accel=$Accel udp=$LocalUdp" -ForegroundColor Yellow
    return Start-Process -FilePath $qemu -ArgumentList $qa -NoNewWindow -PassThru
}

$logA = Join-Path $logDir "boot_mesh3_a.txt"
$logB = Join-Path $logDir "boot_mesh3_b.txt"
$logC = Join-Path $logDir "boot_mesh3_c.txt"

$pA = Start-Node "A" "52:54:00:AA:00:01" 19001 $nmA $logA
Start-Sleep -Seconds 2
$pB = Start-Node "B" "52:54:00:BB:00:02" 19002 $nmB $logB
Start-Sleep -Seconds 2
$pC = Start-Node "C" "52:54:00:CC:00:03" 19003 $nmC $logC

Write-Host "[WAIT] ${Seconds}s watching METRIC AutoLearn/SleepCycle + mesh..." -ForegroundColor Cyan
$deadline = (Get-Date).AddSeconds($Seconds)
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 10
    $elapsed = [int]($Seconds - ($deadline - (Get-Date)).TotalSeconds)
    $sa = 0; $sb = 0; $sc = 0
    if (Test-Path $logA) { $sa = [int]((Get-Item $logA).Length / 1KB) }
    if (Test-Path $logB) { $sb = [int]((Get-Item $logB).Length / 1KB) }
    if (Test-Path $logC) { $sc = [int]((Get-Item $logC).Length / 1KB) }
    Write-Host ("  t+{0}s A={1}KB B={2}KB C={3}KB" -f $elapsed, $sa, $sb, $sc)
}

Write-Host "[PARSE] tools/measure_learn_sleep_logs.py" -ForegroundColor Cyan
python tools/measure_learn_sleep_logs.py --logs $logA $logB $logC

Write-Host "[DONE] see logs/boot_mesh3_a.txt (and b/c)" -ForegroundColor DarkGray
