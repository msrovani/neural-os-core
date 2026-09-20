# Lab mesh 6-node (SESSION_360 / ADR-0081): hub L2 + STATIC 10.0.3.2..7
# Topologia: A=3G/3c - B=2G/1c - C-F=1G/1c  |  Accel default WHPX+Haswell
# Uso: .\tools\run-mesh6-lab.ps1 [-Accel whpx|tcg] [-NoDisplay]
param(
    [ValidateSet("whpx", "tcg")]
    [string]$Accel = "whpx",
    [switch]$NoDisplay,
    [switch]$KeepRunning  # nao mata QEMUs existentes (default: limpa)
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
if (-not (Test-Path (Join-Path $Root "Cargo.toml"))) { $Root = $PSScriptRoot }
Set-Location $Root

$logDir = Join-Path $Root "logs"
$nmDir = Join-Path $Root "target"
New-Item -ItemType Directory -Force -Path $logDir, $nmDir | Out-Null

if (-not $KeepRunning) {
    Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
    Get-CimInstance Win32_Process -Filter "Name='python.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -match "qemu_l2_hub" } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 2
}

$free = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 1)
Write-Host "freeGB=$free (guest sum ~9G - overcommit esperado)" -ForegroundColor Yellow

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

# A Master 3G/3c; B 2G/1c; C-F 1G/1c - IPs 10.0.3.2..7
$nodes = @(
    @{ Name = "a"; Octet = 2; Mem = "3G"; Smp = 3; Port = 19001 },
    @{ Name = "b"; Octet = 3; Mem = "2G"; Smp = 1; Port = 19002 },
    @{ Name = "c"; Octet = 4; Mem = "1G"; Smp = 1; Port = 19003 },
    @{ Name = "d"; Octet = 5; Mem = "1G"; Smp = 1; Port = 19004 },
    @{ Name = "e"; Octet = 6; Mem = "1G"; Smp = 1; Port = 19005 },
    @{ Name = "f"; Octet = 7; Mem = "1G"; Smp = 1; Port = 19006 }
)

foreach ($n in $nodes) {
    $flag = Join-Path $nmDir ("netmode_{0}.flag" -f $n.Name)
    [System.IO.File]::WriteAllBytes($flag, [byte[]]@([byte][char]"S", 10, 0, 3, [byte]$n.Octet))
}

$hubOut = Join-Path $logDir "l2_hub.txt"
$hubErr = Join-Path $logDir "l2_hub.err.txt"
$hub = Start-Process -FilePath "python" -ArgumentList @(
    (Join-Path $Root "tools\qemu_l2_hub.py"), "--port", "19000"
) -WorkingDirectory $Root -WindowStyle Minimized -PassThru `
    -RedirectStandardOutput $hubOut -RedirectStandardError $hubErr
Write-Host "hub pid=$($hub.Id) :19000" -ForegroundColor Green
Start-Sleep -Seconds 1
# Fail-closed: se o hub morreu, nao sobe ilhas Master
Start-Sleep -Milliseconds 500
if ($hub.HasExited) {
    Write-Host "[ERRO] hub L2 saiu exit=$($hub.ExitCode) - veja logs\l2_hub.err.txt" -ForegroundColor Red
    Get-Content $hubErr -ErrorAction SilentlyContinue | Select-Object -Last 5
    exit 1
}
$hubListen = Get-Content $hubOut -ErrorAction SilentlyContinue | Select-Object -First 1
Write-Host "hub: $hubListen" -ForegroundColor Green

$cpu = if ($Accel -eq "whpx") { "Haswell" } else { "qemu64" }
$accelArg = if ($Accel -eq "whpx") { "whpx" } else { "tcg,thread=multi" }
$display = if ($NoDisplay) { @("-display", "none") } else { @("-vga", "std", "-display", "gtk") }

# MAC unico por no (sem isso o e1000 default colide e o mesh nao ve peers)
$macs = @{
    a = "52:54:00:AA:00:01"
    b = "52:54:00:BB:00:02"
    c = "52:54:00:CC:00:03"
    d = "52:54:00:DD:00:04"
    e = "52:54:00:EE:00:05"
    f = "52:54:00:FF:00:06"
}

if (Test-Path $disk) {
    Write-Host ("disk=YES {0} ({1:N0} MB) — anexado snapshot nos 6 nos" -f $disk, ((Get-Item $disk).Length / 1MB)) -ForegroundColor Green
} else {
    Write-Host "disk=NO target\disk_qemu.raw ausente — Model ABSENT esperado" -ForegroundColor Yellow
}

$procs = @()
foreach ($n in $nodes) {
    $log = Join-Path $logDir ("boot_mesh_{0}.txt" -f $n.Name)
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
        "-device", "loader,file=$flag,addr=0x2000000",
        "-netdev", "socket,id=n0,udp=127.0.0.1:19000,localaddr=127.0.0.1:$($n.Port)",
        "-device", "e1000,netdev=n0,mac=$mac",
        "-serial", "file:$log",
        "-no-reboot",
        "-name", ("mesh6-{0}" -f $n.Name.ToUpper())
    ) + $display
    if (Test-Path $disk) {
        $qa += @("-drive", "format=raw,file=$disk,if=ide,index=1,snapshot=on")
    }
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
        Exit  = $p.ExitCode
        Log   = ("boot_mesh_{0}.txt" -f $n.Name)
    }
    Write-Host ("{0} pid={1} {2} smp={3} alive={4} mac={5}" -f $n.Name.ToUpper(), $p.Id, $n.Mem, $n.Smp, $alive, $mac) `
        -ForegroundColor $(if ($alive) { "Green" } else { "Red" })
}

Start-Sleep -Seconds 4
Write-Host "`n=== mesh-6 $Accel ===" -ForegroundColor Cyan
$procs | Format-Table -AutoSize
$q = @(Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue)
Write-Host ("qemu_alive={0} freeGB={1}" -f $q.Count, [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 1))
$q | Format-Table Id, @{ n = "MB"; e = { [math]::Round($_.WS / 1MB) } } -AutoSize

foreach ($n in $nodes) {
    $log = Join-Path $logDir ("boot_mesh_{0}.txt" -f $n.Name)
    if (Test-Path $log) {
        $len = (Get-Item $log).Length
        $tail = Get-Content $log -Tail 2 -ErrorAction SilentlyContinue
        Write-Host ("{0}: {1}B :: {2}" -f $n.Name.ToUpper(), $len, ($tail -join " / "))
    }
}

Write-Host "`nMonitor: Get-Content -Tail 20 -Wait logs\boot_mesh_a.txt" -ForegroundColor DarkGray
Write-Host "Hub log: logs\l2_hub.txt" -ForegroundColor DarkGray
