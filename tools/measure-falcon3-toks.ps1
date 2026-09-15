#!/usr/bin/env pwsh
# Measure Falcon3-3B Instruct 1.58-bit decode tok/s on QEMU (TCG then WHPX).
# Lab canonico ADR-0101: models\FALCON3.BIN = 22L / h=3072 (~990MB).
#
# PRIORIDADE testes/dev (sucesso medido):
#   .\tools\measure-falcon3-toks.ps1 -Accel whpx -RamGB 6 -Smp 4
# Com UI + FAT + HDA:
#   .\tools\measure-falcon3-toks.ps1 -Accel whpx -Window -WithFat -AudioBridge
param(
    [ValidateSet("tcg", "whpx", "both")]
    [string]$Accel = "both",
    [int]$RamGB = 6,
    [int]$Smp = 4,
    [int]$TimeoutSec = 2400,
    [string]$ModelPath = "",
    [switch]$Window,
    [switch]$WithFat,
    [switch]$AudioBridge
)
$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root

$Qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$Ovmf = Join-Path $Root "target\ovmf.fd"
if (-not (Test-Path $Ovmf)) { $Ovmf = Join-Path $Root "target\ovmf.bin" }
$UefiImg = Join-Path $Root "target\uefi.img"
$DiskImg = Join-Path $Root "target\disk_qemu.raw"
if ($ModelPath -eq "") {
    $cands = @(
        (Join-Path $Root "models\FALCON3.BIN"),
        (Join-Path $Root "target1\FALCON3_BASE.V6"),
        (Join-Path $Root "target1\FALCON3.V6")
    )
    $ModelPath = $cands | Where-Object { Test-Path $_ } | Select-Object -First 1
}
foreach ($p in @($Qemu, $Ovmf, $UefiImg, $ModelPath)) {
    if (-not (Test-Path $p)) { throw "missing: $p" }
}
if ($WithFat -and -not (Test-Path $DiskImg)) {
    throw "missing FAT disk: $DiskImg (python tools\build_image.py)"
}

function Read-LogShared([string]$path) {
    if (-not (Test-Path $path)) { return "" }
    $fs = [System.IO.File]::Open($path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        $sr = New-Object System.IO.StreamReader($fs)
        return $sr.ReadToEnd()
    } finally {
        $sr.Close(); $fs.Close()
    }
}

function Invoke-Measure([string]$Acc) {
    $log = Join-Path $Root ("logs\falcon3_toks_{0}.txt" -f $Acc)
    New-Item -ItemType Directory -Force -Path (Split-Path $log) | Out-Null
    Remove-Item -Force $log -ErrorAction SilentlyContinue
    $cpu = if ($Acc -eq "whpx") { "Haswell" } else { "max" }
    $args = @(
        "-m", "${RamGB}G", "-smp", "$Smp",
        "-accel", $Acc, "-cpu", $cpu,
        "-drive", "format=raw,file=$UefiImg,if=ide,index=0"
    )
    if ($WithFat) {
        $args += @("-drive", "format=raw,file=$DiskImg,if=ide,index=1")
        Write-Host "FAT: $DiskImg (IDE index=1)" -ForegroundColor Cyan
    } else {
        Write-Host "Note: no FAT (loader-only). Use -WithFat for disk_qemu.raw" -ForegroundColor DarkYellow
    }
    $args += @(
        "-device", "loader,file=$ModelPath,addr=0x100000000",
        "-drive", "if=pflash,format=raw,file=$Ovmf,readonly=on",
        "-serial", "file:$log", "-serial", "null",
        "-netdev", "user,id=n0", "-device", "e1000,netdev=n0"
    )
    # HDA: parity com run-qemu-whpx.ps1 (intel-hda + hda-duplex). QEMU codec = degraded/AWAITING_HW.
    $audioDev = if ($AudioBridge) { "dsound,id=snd0" } else { "none,id=snd0" }
    $args += @(
        "-audiodev", $audioDev,
        "-device", "intel-hda,id=hda0",
        "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0"
    )
    if ($AudioBridge) {
        Write-Host "HDA: intel-hda + hda-duplex audiodev=dsound" -ForegroundColor Green
    } else {
        Write-Host "HDA: intel-hda + hda-duplex audiodev=none (pass -AudioBridge for host speakers)" -ForegroundColor Gray
    }
    if ($Window) {
        $args += @("-vga", "std", "-display", "gtk")
        Write-Host "Display: gtk (-Window)" -ForegroundColor Green
    } else {
        $args += @("-vga", "std", "-display", "none")
    }
    Write-Host ("=== Falcon3-3B tok/s accel={0} ram={1}G smp={2} model={3} ===" -f $Acc, $RamGB, $Smp, $ModelPath) -ForegroundColor Cyan
    $p = Start-Process -FilePath $Qemu -ArgumentList $args -PassThru
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $hit = $false
    while (-not $p.HasExited) {
        Start-Sleep -Seconds 8
        $elapsed = [int]$sw.Elapsed.TotalSeconds
        $txt = Read-LogShared $log
        if ($txt -match "Falcon3 decode_tok/s=(\d+)") {
            $hit = $true
            $tpsLine = ($txt -split "`n" | Where-Object { $_ -match "decode_tok/s=|milli=" } | Select-Object -Last 5) -join "`n"
            Write-Host "[hit] ${elapsed}s"
            Write-Host $tpsLine -ForegroundColor Green
            if ($Window) {
                Write-Host "Window mode: QEMU left running (close window or Ctrl+C host to stop)." -ForegroundColor Cyan
                break
            }
            break
        }
        if ($txt -match "LLM LOADED") {
            Write-Host ("[{0}s] LLM LOADED - waiting decode..." -f $elapsed) -ForegroundColor Yellow
        } elseif ($txt -match "Runtime|SCHEDULER") {
            Write-Host ("[{0}s] runtime live" -f $elapsed) -ForegroundColor DarkYellow
        } else {
            Write-Host ("[{0}s] boot... log_bytes={1}" -f $elapsed, $txt.Length) -ForegroundColor Gray
        }
        if ($elapsed -ge $TimeoutSec) {
            Write-Host ("[TIMEOUT] {0}s" -f $TimeoutSec) -ForegroundColor Red
            break
        }
    }
    if (-not $Window -and -not $p.HasExited) {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 2
    }
    $txt = Read-LogShared $log
    return [ordered]@{
        accel = $Acc
        hit = $hit
        log = $log
        model = $ModelPath
        qemu_pid = $p.Id
        lines = @(($txt -split "`n" | Where-Object { $_ -match "decode_tok/s=|LLM LOADED|BENCH|Falcon3|h=3072|L=22|Probe 4GB|HDA|FAT" } | Select-Object -Last 30))
    }
}

$runs = @()
if ($Accel -eq "both") { $runs = @("tcg", "whpx") } else { $runs = @($Accel) }
$results = @()
foreach ($a in $runs) {
    if (-not $Window) {
        Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 2
    }
    $results += ,(Invoke-Measure $a)
}

Write-Host ""
Write-Host "======== RESULTADOS Falcon3-3B 1.58 ========" -ForegroundColor Cyan
foreach ($r in $results) {
    Write-Host ("accel={0} hit={1} log={2} qemu_pid={3}" -f $r.accel, $r.hit, $r.log, $r.qemu_pid)
    $r.lines | ForEach-Object { Write-Host ("  {0}" -f $_) }
}
