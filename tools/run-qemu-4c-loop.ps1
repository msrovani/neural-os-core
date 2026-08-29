#!/usr/bin/env pwsh
# Run QEMU 4 cores (TCG) with Falcon3 3B via QEMU-loader.
# Monitors log until greeting or PF storm or timeout.
# NSGDB.BIN lives in disk_qemu.raw (FAT32). Under TCG, ATA is skipped,
# so FileFlash falls back to RAM: each run starts fresh RAM NSGDB.
param(
    [int]$Cores = 4,
    [int]$TimeoutSec = 120,
    [string]$LogPath = "logs\qemu4c_3b_loop.txt"
)
$ErrorActionPreference = "Stop"
$Root = (Resolve-Path ".").Path
$Qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$Ovmf = "C:\Program Files\qemu\share\edk2-x86_64-code.fd"
$UefiImg = Join-Path $Root "target\uefi.img"
$DiskImg = Join-Path $Root "target\disk_qemu.raw"
$LoaderBin = Join-Path $Root "models\FALCON3.BIN"
if (-not (Test-Path $Qemu))    { throw "qemu nao encontrado" }
if (-not (Test-Path $Ovmf))    { throw "ovmf nao encontrado" }
if (-not (Test-Path $UefiImg)) { throw "uefi.img nao encontrado" }
if (-not (Test-Path $DiskImg)) { throw "disk_qemu.raw nao encontrado" }
New-Item -ItemType Directory -Force -Path (Split-Path $LogPath -Parent) | Out-Null
Remove-Item -Force $LogPath -ErrorAction SilentlyContinue
$args = @(
    "-m","4G","-smp",$Cores,"-accel","tcg","-net","none",
    "-drive","format=raw,file=$UefiImg,if=ide,index=0",
    "-drive","format=raw,file=$DiskImg,if=ide,index=1",
    "-drive","if=pflash,format=raw,file=`"$Ovmf`",readonly=on",
    "-serial","file=$LogPath","-serial","null","-display","none"
)
if (Test-Path $LoaderBin) {
    $args += @("-device","loader,file=$LoaderBin,addr=0x100000000")
}
Write-Host "[loop] cores=$Cores timeout=${TimeoutSec}s log=$LogPath"
Write-Host "[loop] cmd: $Qemu $($args -join ' ')"
$p = Start-Process -FilePath $Qemu -ArgumentList $args -PassThru
$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
$lastLines = 0
$greetingSeen = $false
$pfStorm = $false
while (!$p.HasExited) {
    Start-Sleep -Seconds 2
    $elapsed = [int]$stopwatch.Elapsed.TotalSeconds
    if ($elapsed -ge $TimeoutSec) {
        Write-Host "[loop] timeout ${TimeoutSec}s"
        break
    }
    if (Test-Path $LogPath) {
        $content = Get-Content $LogPath -Raw -ErrorAction SilentlyContinue
        if ($content) {
            $lines = ($content -split "`n").Count
            if ($lines -ne $lastLines) {
                $tail = ($content -split "`n") | Select-Object -Last 1
                Write-Host "[loop][${elapsed}s] $lines | $tail"
                $lastLines = $lines
            }
            if ($content -match "saudacao|Greeting|salutation|JARBAS greet|Hub ready|desktop_ready|MOUSE\] desktop" -and -not $greetingSeen) {
                Write-Host "[loop] *** GREETING DETECTADO ***"
                $greetingSeen = $true
                Start-Sleep -Seconds 5
                break
            }
            if ($content -match "#PF ip=") {
                $pfCount = ([regex]::Matches($content, "#PF ip=")).Count
                if ($pfCount -gt 50 -and -not $pfStorm) {
                    Write-Host "[loop] *** PF STORM ($pfCount) ***"
                    $pfStorm = $true
                    Start-Sleep -Seconds 10
                    break
                }
            }
        }
    }
}
if (!$p.HasExited) { Stop-Process -Id $p.Id -Force; Start-Sleep -Seconds 1 }
$final = (Get-Content $LogPath -ErrorAction SilentlyContinue | Measure-Object -Line).Lines
Write-Host "[loop] done. lines=$final log=$LogPath"
