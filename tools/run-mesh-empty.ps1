# Mesh 4-nós VAZIO (sem modelo via QEMU loader) — testa se o #PF no tick 1 é de base
# Uso: .\tools\run-mesh-empty.ps1 [-TempoSeg 120] [-Accel whpx|tcg] [-Force]
param(
    [int]$TempoSeg = 120,
    [ValidateSet("whpx", "tcg")]
    [string]$Accel = "whpx",
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
if (-not (Test-Path (Join-Path $Root "Cargo.toml"))) { $Root = $PSScriptRoot }
Set-Location $Root

. (Join-Path $Root "tools\qemu_mem_preflight.ps1")

$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$logDir = Join-Path $Root "logs"
$nmDir = Join-Path $Root "target"
New-Item -ItemType Directory -Force -Path $logDir, $nmDir | Out-Null

Write-Host "=== Mesh 4-node VAZIO (sem modelo) ===" -ForegroundColor Cyan

# limpa QEMUs e hub antigos
Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue |
    Stop-Process -Force -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name='python.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -match "qemu_l2_hub" } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep -Seconds 2

$free = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 1)
Write-Host "freeGB=$free (4x1G commit)" -ForegroundColor Yellow
Assert-QemuMemFree -NeededGB 6.0 -Label "mesh-empty" -Force:$Force

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemu)) {
    $alt = Get-Command qemu-system-x86_64 -ErrorAction SilentlyContinue
    if ($alt) { $qemu = $alt.Source } else { Write-Host "[ERRO] QEMU" -ForegroundColor Red; exit 1 }
}
$ovmf = "C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmf)) { $ovmf = Join-Path $Root "target\ovmf.fd" }
$uefi = Join-Path $Root "target\uefi.img"
if (-not (Test-Path $uefi)) { $uefi = Join-Path $Root "target1\uefi.img" }
if (-not (Test-Path $uefi)) { Write-Host "[ERRO] uefi.img ausente" -ForegroundColor Red; exit 1 }
$disk = Join-Path $Root "target\disk_qemu.raw"
if (-not (Test-Path $disk)) { Write-Host "[ERRO] disk_qemu.raw ausente" -ForegroundColor Red; exit 1 }

$nodes = @(
    @{ Name = "a"; Octet = 2; Mem = "1G"; Smp = 4; Port = 19001 },
    @{ Name = "b"; Octet = 3; Mem = "1G"; Smp = 4; Port = 19002 },
    @{ Name = "c"; Octet = 4; Mem = "1G"; Smp = 4; Port = 19003 },
    @{ Name = "d"; Octet = 5; Mem = "1G"; Smp = 4; Port = 19004 }
)

foreach ($n in $nodes) {
    $flag = Join-Path $nmDir ("netmode_{0}.flag" -f $n.Name)
    [System.IO.File]::WriteAllBytes($flag, [byte[]]@([byte][char]"S", 10, 0, 3, [byte]$n.Octet))
}

$hubOut = Join-Path $logDir "l2_hub_empty.txt"
$hubErr = Join-Path $logDir "l2_hub_empty.err.txt"
$hub = Start-Process -FilePath "python" -ArgumentList @(
    (Join-Path $Root "tools\qemu_l2_hub.py"), "--port", "19000"
) -WorkingDirectory $Root -WindowStyle Minimized -PassThru `
    -RedirectStandardOutput $hubOut -RedirectStandardError $hubErr
Write-Host "hub pid=$($hub.Id) :19000" -ForegroundColor Green
Start-Sleep -Milliseconds 1500
if ($hub.HasExited) {
    Write-Host "[ERRO] hub L2 saiu exit=$($hub.ExitCode)" -ForegroundColor Red
    Get-Content $hubErr -ErrorAction SilentlyContinue | Select-Object -Last 5
    exit 1
}

$cpu = if ($Accel -eq "whpx") { "Haswell" } else { "qemu64" }
$accelArg = if ($Accel -eq "whpx") { "whpx" } else { "tcg,thread=multi" }

$macs = @{
    a = "52:54:00:AA:00:01"
    b = "52:54:00:BB:00:02"
    c = "52:54:00:CC:00:03"
    d = "52:54:00:DD:00:04"
}

$procs = @()
foreach ($n in $nodes) {
    $log = Join-Path $logDir ("boot_mesh_empty_{0}.txt" -f $n.Name)
    if (Test-Path $log) { Remove-Item $log -Force -ErrorAction SilentlyContinue }
    $flag = Join-Path $nmDir ("netmode_{0}.flag" -f $n.Name)
    $mac = $macs[$n.Name]
    $qa = @(
        "-m", $n.Mem,
        "-smp", "$($n.Smp)",
        "-cpu", $cpu,
        "-accel", $accelArg,
        "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
        "-drive", "format=raw,file=$uefi,if=ide,index=0,snapshot=on",
        "-drive", "format=raw,file=$disk,if=ide,index=1,snapshot=on",
        "-device", "loader,file=$flag,addr=0x2000000",
        "-netdev", "socket,id=n0,udp=127.0.0.1:19000,localaddr=127.0.0.1:$($n.Port)",
        "-device", "e1000,netdev=n0,mac=$mac",
        "-audiodev", "none,id=snd0",
        "-device", "intel-hda,id=hda0",
        "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
        "-device", "qemu-xhci,id=xhci",
        "-device", "usb-tablet,bus=xhci.0",
        "-device", "usb-kbd,bus=xhci.0",
        "-serial", "file:$log",
        "-no-reboot",
        "-name", ("mesh4-empty-{0}" -f $n.Name.ToUpper()),
        "-display", "none"
    )
    $p = Start-Process -FilePath $qemu -ArgumentList $qa -PassThru
    Start-Sleep -Seconds 2
    $alive = -not $p.HasExited
    $procs += [pscustomobject]@{
        Name  = $n.Name.ToUpper()
        IP    = ("10.0.3.{0}" -f $n.Octet)
        Mem   = $n.Mem
        Smp   = $n.Smp
        Pid   = $p.Id
        Alive = $alive
        Log   = ("boot_mesh_empty_{0}.txt" -f $n.Name)
        Proc  = $p
    }
    Write-Host ("{0} pid={1} {2} smp={3} alive={4} mac={5}" -f $n.Name.ToUpper(), $p.Id, $n.Mem, $n.Smp, $alive, $mac) `
        -ForegroundColor $(if ($alive) { "Green" } else { "Red" })
}

Start-Sleep -Seconds 4
Write-Host ""
Write-Host "=== mesh-4 $Accel VAZIO (4c cada) rodando por $TempoSeg s ===" -ForegroundColor Cyan
$procs | Format-Table -AutoSize -Property Name, IP, Mem, Smp, Pid, Alive

Write-Host "Aguardando $TempoSeg segundos..." -ForegroundColor Gray
Start-Sleep -Seconds $TempoSeg

Write-Host "Encerrando QEMUs..." -ForegroundColor Yellow
foreach ($p in $procs) {
    try { if (-not $p.Proc.HasExited) { Stop-Process -Id $p.Pid -Force -ErrorAction SilentlyContinue } } catch {}
}
Stop-Process -Id $hub.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

Write-Host ""
Write-Host "=== METRICAS POR NO (VAZIO) ===" -ForegroundColor Cyan
foreach ($p in $procs) {
    $logPath = Join-Path $logDir $p.Log
    $text = Get-Content $logPath -ErrorAction SilentlyContinue | Out-String
    $peers = "?"
    $tick = "?"
    $pf = "?"
    $panic = "?"
    if ($text) {
        $mPeers = [regex]::Matches($text, "peers=(\d+)") | Select-Object -Last 1
        if ($mPeers) { $peers = $mPeers.Groups[1].Value }
        $mTick = [regex]::Matches($text, "tick=(\d+)") | Select-Object -Last 1
        if ($mTick) { $tick = $mTick.Groups[1].Value }
        $mPF = [regex]::Matches($text, "#PF") | Measure-Object
        $pf = $mPF.Count
        $mPanic = [regex]::Matches($text, "panic") | Measure-Object
        $panic = $mPanic.Count
    }
    Write-Host ("{0} ({1}): peers={2} tick={3} PF={4} panic={5}" -f $p.No, $p.IP, $peers, $tick, $pf, $panic)
}
