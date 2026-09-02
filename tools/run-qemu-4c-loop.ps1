#!/usr/bin/env pwsh
# QEMU 4 cores (TCG) — loop até greeting Jarbas + NSGDB/P6 Ring3 no log.
param(
    [int]$Cores = 4,
    [int]$TimeoutSec = 900,
    [string]$LogPath = "",
    [switch]$NoLoader
)
function Read-LogShared([string]$path) {
    if (-not (Test-Path $path)) { return "" }
    $fs = [System.IO.File]::Open($path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        $sr = New-Object System.IO.StreamReader($fs)
        return $sr.ReadToEnd()
    } finally {
        $sr.Close()
        $fs.Close()
    }
}

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ($LogPath -eq "") {
    $LogPath = Join-Path $Root "logs\qemu4c_ring3_loop.txt"
}
$Qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$Ovmf = Join-Path $Root "target\ovmf.bin"
$UefiImg = Join-Path $Root "target\uefi.img"
$DiskImg = Join-Path $Root "target\disk_qemu.raw"
$LoaderBin = Join-Path $Root "models\FALCON3.BIN"
foreach ($p in @($Qemu, $Ovmf, $UefiImg, $DiskImg)) {
    if (-not (Test-Path $p)) { throw "missing: $p" }
}
New-Item -ItemType Directory -Force -Path (Split-Path $LogPath -Parent) | Out-Null
Remove-Item -Force $LogPath -ErrorAction SilentlyContinue
$args = @(
    "-m", "4G", "-smp", $Cores, "-accel", "tcg", "-net", "none",
    "-drive", "format=raw,file=$UefiImg,if=ide,index=0",
    "-drive", "format=raw,file=$DiskImg,if=virtio",
    "-drive", "if=pflash,format=raw,file=$Ovmf,readonly=on",
    "-serial", "file:$LogPath", "-serial", "null", "-display", "none"
)
if ((Test-Path $LoaderBin) -and -not $NoLoader) {
    $args += @("-device", "loader,file=$LoaderBin,addr=0x100000000")
}
Write-Host "[loop] cores=$Cores timeout=${TimeoutSec}s log=$LogPath"
$p = Start-Process -FilePath $Qemu -ArgumentList $args -PassThru
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$lastBytes = 0
$goal = $false
$markers = @(
    "saudacao suit-boot",
    "TTS boot greeting",
    "desktop_ready",
    "P6 Ring3 OK",
    "ring3_can_iretq=true",
    "NSGDB",
    "sgdb.*demo.*PASS",
    "\[SCHEDULER\]"
)
while (-not $p.HasExited) {
    Start-Sleep -Seconds 5
    $elapsed = [int]$sw.Elapsed.TotalSeconds
    if ($elapsed -ge $TimeoutSec) {
        Write-Host "[loop] TIMEOUT ${TimeoutSec}s"
        break
    }
    if (-not (Test-Path $LogPath)) { continue }
    $bytes = (Get-Item $LogPath).Length
    if ($bytes -ne $lastBytes) {
        $lastBytes = $bytes
        $tail = ((Read-LogShared $LogPath) -split "`n" | Select-Object -Last 1)
        Write-Host "[loop][${elapsed}s] bytes=$bytes | $tail"
    }
    $raw = (Read-LogShared $LogPath) -replace '\x1b\[[0-9;?]*[a-zA-Z]', ''
    foreach ($m in $markers) {
        if ($raw -match $m) {
            if ($m -match "saudacao|TTS boot|desktop_ready") {
                Write-Host "[loop] *** GOAL: $m ***"
                $goal = $true
                Start-Sleep -Seconds 8
                break
            }
        }
    }
    if ($goal) { break }
    $pf = ([regex]::Matches($raw, "#PF ip=")).Count
    if ($pf -gt 80) {
        Write-Host "[loop] *** PF STORM ($pf) ***"
        break
    }
}
if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
Start-Sleep -Seconds 1
$clean = Join-Path $Root "logs\qemu4c_clean.txt"
if (Test-Path $LogPath) {
    $text = (Read-LogShared $LogPath) -replace '\x1b\[[0-9;?]*[a-zA-Z]', ''
    $text | Out-File $clean -Encoding utf8
    Write-Host "[loop] clean log: $clean lines=$((($text -split "`n").Count))"
    foreach ($k in @("P6", "Ring3", "Jarbas", "saudacao", "TTS", "NSGDB", "SCHEDULER", "Runtime")) {
        $n = ([regex]::Matches($text, $k, "IgnoreCase")).Count
        if ($n -gt 0) { Write-Host "  marker $k = $n" }
    }
}
if ($goal) { exit 0 } else { exit 1 }
