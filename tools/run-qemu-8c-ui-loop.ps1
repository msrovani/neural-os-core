#!/usr/bin/env pwsh
# QEMU 8 cores — loop ate UI/desktop fluida (desktop_ready + ticks + BOOT SCORE).
# Depois: use run-qemu-p2p-mesh.ps1 -Cores 8 -Mem 4 -NoModels -Instance Both
param(
    [int]$Cores = 8,
    [int]$RamGB = 6,
    [int]$TimeoutSec = 240,
    [int]$MaxAttempts = 5,
    [switch]$Window,
    [switch]$Tcg
)
function Read-LogShared([string]$path) {
    if (-not (Test-Path $path)) { return "" }
    $fs = [System.IO.File]::Open($path, 'Open', 'Read', 'ReadWrite')
    try {
        $sr = New-Object System.IO.StreamReader($fs)
        return $sr.ReadToEnd()
    } finally { $sr.Close(); $fs.Close() }
}

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null

function Stop-QemuAll {
    Get-Process qemu* -ErrorAction SilentlyContinue | ForEach-Object {
        Write-Host "[loop] stop qemu pid=$($_.Id)"
        Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Seconds 2
}

function Test-UiFluid([string]$text) {
    $c = [regex]::Replace($text, '\x1b\[[0-9;?]*[A-Za-z]', '')
    $need = @{
        phase7      = ($c -match 'PHASE n=7 name=Runtime status=ok')
        score       = ($c -match 'BOOT SCORE')
        desktop     = ($c -match 'desktop_ready')
        sched       = ($c -match 'sched_enter|51 runtime agents|SCHEDULER')
        ticks       = ($c -match '\[T\+[1-9]\d*\]')  # passou de T+0
        bootlog     = ($c -match 'flush BOOT\.LOG ok=true')
        no_hang_pf  = -not ($c -match 'Triple fault|PANIC')
    }
    $ok = $need.Values | Where-Object { $_ -eq $false }
    return @{
        Pass = ($ok.Count -eq 0)
        Need = $need
        Text = $c
    }
}

Write-Host "[loop] 8c UI fluid MaxAttempts=$MaxAttempts TimeoutSec=$TimeoutSec Window=$Window Tcg=$Tcg"

for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
    Stop-QemuAll
    $stamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $attemptLog = Join-Path $logDir "ui8c_attempt${attempt}_$stamp.txt"
    Write-Host "`n=== ATTEMPT $attempt/$MaxAttempts log=$attemptLog ===" -ForegroundColor Cyan

    $args = @("-Smp", "$Cores", "-RamGB", "$RamGB")
    if ($Window) { $args += "-Window" }
    if ($Tcg) { $args += "-Tcg" }

    $proc = Start-Process -FilePath "powershell.exe" `
        -ArgumentList (@("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", (Join-Path $Root "run-qemu-whpx.ps1")) + $args) `
        -PassThru -WorkingDirectory $Root

    $sw = [Diagnostics.Stopwatch]::StartNew()
    $goal = $false
    $latestBoot = $null
    while (-not $proc.HasExited -and $sw.Elapsed.TotalSeconds -lt $TimeoutSec) {
        Start-Sleep -Seconds 8
        $latestBoot = Get-ChildItem (Join-Path $logDir "boot_whpx_*.txt") -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (-not $latestBoot) { continue }
        $raw = Read-LogShared $latestBoot.FullName
        $chk = Test-UiFluid $raw
        $miss = @($chk.Need.GetEnumerator() | Where-Object { -not $_.Value } | ForEach-Object { $_.Key }) -join ","
        Write-Host ("[loop] t={0:N0}s pass={1} miss=[{2}] bytes={3}" -f $sw.Elapsed.TotalSeconds, $chk.Pass, $miss, $latestBoot.Length)
        if ($chk.Pass) {
            $goal = $true
            # Copia snapshot do log de aceite
            Copy-Item $latestBoot.FullName $attemptLog -Force
            break
        }
    }

    if ($goal) {
        Write-Host "[loop] UI FLUID OK attempt=$attempt log=$($latestBoot.FullName)" -ForegroundColor Green
        # Deixa QEMU rodando para o operador ver a janela; retorna path
        Write-Host "READY_LOG=$($latestBoot.FullName)"
        Write-Host "NEXT: .\run-qemu-p2p-mesh.ps1 -Cores 8 -Mem 4 -NoModels -Accel tcg -Instance Both"
        exit 0
    }

    Write-Host "[loop] attempt $attempt FAIL — salvando diagnostico" -ForegroundColor Yellow
    if ($latestBoot) {
        Copy-Item $latestBoot.FullName $attemptLog -Force
        $raw = Read-LogShared $latestBoot.FullName
        $c = [regex]::Replace($raw, '\x1b\[[0-9;?]*[A-Za-z]', '')
        ($c -split "`n" | Where-Object { $_ -match 'fail|EXC|PHASE|desktop|BOOT SCORE|flush BOOT|saudacao|urgency|hang' } | Select-Object -Last 40) |
            Out-File -FilePath ($attemptLog + ".diag.txt") -Encoding utf8
    }
    Stop-QemuAll
}

Write-Host "[loop] ESGOTADO — UI nao ficou fluida em $MaxAttempts tentativas" -ForegroundColor Red
exit 1
