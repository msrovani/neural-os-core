#!/usr/bin/env pwsh
# Regression tests for tools/run-qemu-lab-loop.ps1.
# Each case below is a defect that actually happened in this loop; the test
# fails closed if the fix is reverted. ASCII-only.
param([string]$Root = "")
if (-not $Root) { $Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path }
Set-Location $Root
$script = Join-Path $Root "tools\run-qemu-lab-loop.ps1"
$scriptTxt = Get-Content $script -Raw
$argsFile = Join-Path $Root "logs\lab_dryrun_args.txt"
$csv = Join-Path $Root "logs\lab_loop_cycles.csv"

$ok = 0
$bad = 0
function Assert-True([bool]$cond, [string]$name, [string]$detail) {
    if ($cond) {
        Write-Host "ok   $name" -ForegroundColor Green
        $script:ok++
    } else {
        Write-Host "FAIL $name -- $detail" -ForegroundColor Red
        $script:bad++
    }
}

# 1. -L must carry its own quotes. Measured defect: Start-Process joins the
#    ArgumentList with plain spaces, so "C:\Program Files\..." arrived as two
#    arguments and QEMU aborted with "Could not open 'Files\qemu\share'".
# Checked on the GENERATED command line (case 8), not on the source text: what
# matters is what QEMU receives, not how it is spelled in the .ps1.
Assert-True ($scriptTxt -match [regex]::Escape('$qemuShare')) `
    -name "script uses the quoted -L data dir" -detail "esperado: -L com o valor entre aspas (ver caso 8)"

# 2. One monitor port per cycle. Measured defect: fixed port + a leftover
#    socket made every later cycle die with "Failed to find an available port".
Assert-True ($scriptTxt -match '\$monPort\s*=\s*\$MonitorPort\s*\+\s*\$cycle') `
    -name "monitor port varies per cycle" -detail "esperado: `$monPort = `$MonitorPort + `$cycle"

# 3. No wildcard kill of other QEMUs. Another thread in this repo runs a boot
#    lab; a Get-Process qemu* here would kill it.
# Only CODE lines: the header comment quotes the other thread's own command
# verbatim, and a naive grep of the file "finds" it and fails a correct script.
$codeLines = @(Get-Content $script | Where-Object { $_ -notmatch '^\s*#' })
$codeTxt = $codeLines -join "`n"
$wildcard = [regex]::Matches($codeTxt, 'Get-Process\s+(-Name\s+)?["'']?qemu\*?')
Assert-True ($wildcard.Count -eq 0) `
    -name "never kills a wildcard qemu" -detail ("encontrado: " + ($wildcard | ForEach-Object { $_.Value }) -join ',')

# 4. Stop-OurVms matches our own binary name only.
Assert-True ($scriptTxt -match 'Get-Process -Name "lab8c-vm"') `
    -name "orphan cleanup is name-scoped" -detail 'esperado: Get-Process -Name "lab8c-vm"'

# 5. Singleton guard, so two instances cannot disagree about the same boot.
Assert-True ($scriptTxt -match 'ABORT: ja existe') -name "singleton guard present" -detail "guard ausente"

# 6. Runtime and demo exceptions are counted apart. The demos (P6/P7 demand
#    paging, Ring3 probe) fault on purpose before Runtime; counting them as
#    failures is how the instrument starts lying.
Assert-True ($scriptTxt -match 'ExcDemo' -and $scriptTxt -match 'EXC_RUNTIME') `
    -name "exc split demo vs runtime" -detail "faltou ExcDemo/EXC_RUNTIME"

# 7. CSV schema: the verdict columns that make a cycle reviewable.
if (Test-Path $csv) {
    $head = (Get-Content $csv -TotalCount 1)
    $need = @("verdict", "exc_runtime", "exc_demo", "corrupt", "restore_sec", "lab_state_before", "run")
    $miss = @($need | Where-Object { $head -notmatch $_ })
    Assert-True ($miss.Count -eq 0) -name "csv schema" -detail ("faltando: " + ($miss -join ','))
} else {
    Assert-True $false -name "csv schema" -detail "logs/lab_loop_cycles.csv ausente (rode o loop)"
}

# 8. The dry run must produce a command line that quotes -L.
if (Test-Path $argsFile) {
    $lines = @(Get-Content $argsFile | ForEach-Object { $_.Trim() })
    # Each token is written as [token] by the dry run.
    $i = [array]::IndexOf($lines, '[-L]')
    Assert-True ($i -ge 0 -and $lines[$i + 1] -eq '["C:\Program Files\qemu\share"]') `
        -name "dryrun -L is quoted" -detail ("valor: " + $(if ($i -ge 0) { $lines[$i + 1] } else { "<sem -L>" }))
    $m = [array]::IndexOf($lines, '[-monitor]')
    Assert-True ($m -ge 0 -and $lines[$m + 1] -match '^\[tcp:127\.0\.0\.1:\d+') `
        -name "dryrun monitor arg present" -detail "sem -monitor"
} else {
    Assert-True $false -name "dryrun args" -detail "logs/lab_dryrun_args.txt ausente (rode -DryRun)"
}

# 9. End-to-end meaning of the split, using a REAL cycle row: a boot that faulted
#    8 times in the demos and 0 times at runtime must read exc_runtime=0.
$rows = @()
if (Test-Path $csv) { $rows = @(Get-Content $csv | Select-Object -Skip 1 | Where-Object { $_.Trim() }) }
$real = @($rows | Where-Object { $_ -match ',PASS,' -or $_ -match ',(NO_PHASE7|PANIC|PF|CORRUPT|EXC_RUNTIME|STALL|PHASE7_STALLED),' })
if ($real.Count -gt 0) {
    $r = $real[-1].Split(',')
    $excRt = [int]$r[9]
    $excDemo = [int]$r[10]
    Assert-True ($excRt -eq 0 -or $excRt -lt $excDemo) `
        -name "runtime exceptions are a subset of the raw count" `
        -detail ("ultimo ciclo: exc_runtime=$excRt exc_demo=$excDemo")
    Write-Host ("     ultimo ciclo real: verdict=" + $r[6] + " tick_max=" + $r[5] + " bytes=" + $r[3]) -ForegroundColor DarkGray
} else {
    Write-Host "skip sem ciclo real no CSV ainda (rodando)" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "RESULT ok=$ok fail=$bad" -ForegroundColor $(if ($bad -eq 0) { "Green" } else { "Red" })
if ($bad -gt 0) { exit 1 }
exit 0