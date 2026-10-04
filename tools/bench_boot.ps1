#!/usr/bin/env pwsh
# E4 (OPCODE-0098): bench de boot — N boots, extrai `BENCH ready_us`,
# `BENCH boot_start_us`, a linha `ok talc ...`, e `decode_tok/s=`;
# agrega P50/P99/mean/stddev + n_valid/n_missing e rotula accel/hv.
# NAO rodar em CI. Ex.: .\tools\bench_boot.ps1 -Boots 30 -Accel whpx
param(
    [int]$Boots = 30,
    [ValidateSet("tcg", "whpx")]
    [string]$Accel = "whpx",
    [int]$RamGB = 6,
    [int]$Smp = 4,
    [int]$TimeoutSec = 180,
    [string]$OutDir = ""
)
$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$Ovmf = Join-Path $Root "target\ovmf.bin"
if (-not (Test-Path $Ovmf)) { $Ovmf = Join-Path $Root "target\ovmf.fd" }
$Uefi = Join-Path $Root "target\uefi.img"
if ($OutDir -eq "") { $OutDir = Join-Path $Root "logs\bench" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
foreach ($p in @($Qemu, $Ovmf, $Uefi)) { if (-not (Test-Path $p)) { throw "missing: $p" } }

function Get-Stat([double[]]$xs, [double]$p) {
    if ($xs.Count -eq 0) { return $null }
    $s = $xs | Sort-Object
    $rank = [math]::Ceiling($p / 100.0 * $s.Count)
    $idx = [math]::Max(0, [math]::Min($s.Count - 1, $rank - 1))
    return $s[$idx]
}
function Get-Mean([double[]]$xs) {
    if ($xs.Count -eq 0) { return $null }
    return ($xs | Measure-Object -Average).Average
}
function Get-Std([double[]]$xs) {
    if ($xs.Count -lt 2) { return 0.0 }
    $m = Get-Mean $xs
    $v = ($xs | ForEach-Object { ($_ - $m) * ($_ - $m) } | Measure-Object -Average).Average
    return [math]::Sqrt($v)
}
function Report([string]$label, [double[]]$xs, [int]$nMissing) {
    if ($xs.Count -eq 0) {
        Write-Host ("  {0}: n/a (n_valid=0 n_missing={1})" -f $label, $nMissing)
        return
    }
    Write-Host ("  {0}: n_valid={1} n_missing={2} mean={3:N1} std={4:N1} p50={5:N1} p99={6:N1}" -f `
        $label, $xs.Count, $nMissing, (Get-Mean $xs), (Get-Std $xs), (Get-Stat $xs 50), (Get-Stat $xs 99))
}

$ready = @(); $boot = @(); $tps = @()
$readyMissing = 0; $bootMissing = 0; $tpsMissing = 0
$hvSeen = @{}; $talcLines = @()
for ($i = 1; $i -le $Boots; $i++) {
    $log = Join-Path $OutDir ("bench_{0}.txt" -f $i)
    Remove-Item -Force $log -ErrorAction SilentlyContinue
    $cpu = if ($Accel -eq "whpx") { "Haswell" } else { "max" }
    $qargs = @(
        "-m", "${RamGB}G", "-smp", "$Smp", "-accel", $Accel, "-cpu", $cpu, "-no-reboot",
        "-drive", "format=raw,file=$Uefi,if=ide,index=0",
        "-drive", "if=pflash,format=raw,file=$Ovmf,readonly=on",
        "-serial", "file:$log", "-serial", "null", "-display", "none", "-net", "none"
    )
    Write-Host ("[{0}/{1}] accel={2} log={3}" -f $i, $Boots, $Accel, $log)
    $p = Start-Process -FilePath $Qemu -ArgumentList $qargs -PassThru
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while (-not $p.HasExited -and $sw.Elapsed.TotalSeconds -lt $TimeoutSec) { Start-Sleep -Milliseconds 500 }
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Milliseconds 300

    $txt = if (Test-Path $log) { Get-Content -Raw $log } else { "" }
    $hasReady = $false; $hasBoot = $false; $hasTps = $false
    if ($txt -match "hv=(\w+)") { $hvSeen[$Matches[1]] = $true }
    if ($txt -match "BENCH ready_us=(\d+)") { $ready += [double]$Matches[1]; $hasReady = $true }
    if ($txt -match "BENCH boot_start_us=(\d+)") { $boot += [double]$Matches[1]; $hasBoot = $true }
    foreach ($m in [regex]::Matches($txt, "decode_tok/s=(\d+)")) { $tps += [double]$m.Groups[1].Value; $hasTps = $true }
    foreach ($m in [regex]::Matches($txt, "ok talc [^\r\n]*")) { $talcLines += $m.Value }
    if (-not $hasReady) { $readyMissing++ }
    if (-not $hasBoot) { $bootMissing++ }
    if (-not $hasTps) { $tpsMissing++ }
}

Write-Host ""
Write-Host ("==== BENCH accel={0} boots={1} hv={2} ====" -f `
    $Accel, $Boots, (($hvSeen.Keys | Sort-Object) -join ","))
Report "ready_us" $ready $readyMissing
Report "boot_start_us" $boot $bootMissing
Report "decode_tok_s" $tps $tpsMissing
if ($talcLines.Count -gt 0) { Write-Host ("  talc (last): {0}" -f $talcLines[-1]) }
