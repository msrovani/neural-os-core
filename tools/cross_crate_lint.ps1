<#
.SYNOPSIS
  Cross-crate integrity lint — previne shadow/drift entre neural-kernel (bin) e crates K³CHJ.
  Roda antes de commits que tocam >1 crate.
.PARAMETER Quick
  Pula cargo clean (mais rápido, mascara cache incremental).
#>
param([switch]$Quick)
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$exit = 0

Write-Host "=== cross_crate_lint.ps1 ===" -ForegroundColor Cyan

# 1. Build check
Write-Host "`n[1] Build..." -ForegroundColor Yellow
if (-not $Quick) {
    Write-Host "  cargo clean -p neural-kernel..." -ForegroundColor Gray
    cargo clean -p neural-kernel 2>&1 | Out-Null
}
$build = cargo check --release 2>&1
if ($LASTEXITCODE -eq 0) {
    $warn = ($build | Select-String -Pattern "^warning" | Measure-Object -Line).Lines
    Write-Host "  PASS: 0 errors, $warn warnings" -ForegroundColor Green
} else {
    Write-Host "  FAIL: build errors" -ForegroundColor Red
    $build | Select-String -Pattern "^error" | ForEach-Object { Write-Host "    $_" -ForegroundColor Red }
    $exit = 1
}

# 2. Module declaration integrity
Write-Host "`n[2] Module declarations..." -ForegroundColor Yellow
$libs = @(
    "$root/crates/k_nano/src/lib.rs",
    "$root/crates/k_hal/src/lib.rs",
    "$root/crates/k_ai/src/lib.rs",
    "$root/crates/cortex/src/lib.rs",
    "$root/crates/hermes/src/lib.rs",
    "$root/crates/jarbas/src/lib.rs",
    "$root/crates/agent-core/src/lib.rs"
)
$missing = @()
foreach ($lib in $libs) {
    if (-not (Test-Path $lib)) { continue }
    $dir = Split-Path $lib -Parent
    $text = Get-Content $lib -Raw
    $mods = [regex]::Matches($text, '(?:pub\s+)?mod\s+(\w+)\s*;')
    foreach ($m in $mods) {
        $name = $m.Groups[1].Value
        $f = "$dir/$name.rs"
        $d = "$dir/$name"
        if (-not (Test-Path $f) -and -not (Test-Path $d)) {
            $missing += "$(Split-Path $lib -Leaf): $name"
        }
    }
}
if ($missing.Count -gt 0) {
    Write-Host "  FAIL: $($missing.Count) mods sem arquivo" -ForegroundColor Red
    $missing | ForEach-Object { Write-Host "    $_" -ForegroundColor Red }
    $exit = 1
} else {
    Write-Host "  PASS: todos mods tem arquivo" -ForegroundColor Green
}

# 3. lazy_static shadow
Write-Host "`n[3] Shadow detection..." -ForegroundColor Yellow
$bin = "$root/crates/neural-kernel/src/main.rs"
if (Test-Path $bin) {
    $text = Get-Content $bin -Raw
    $statics = [regex]::Matches($text, 'lazy_static!\s*\{\s*static\s+ref\s+(\w+)')
    if ($statics.Count -gt 0) {
        Write-Host "  INFO: $($statics.Count) lazy_static no bin (verificar se há pub static com mesmo nome em crate)" -ForegroundColor Gray
    } else {
        Write-Host "  PASS: sem lazy_static no bin" -ForegroundColor Green
    }
}

Write-Host "`n=== FIM (exit=$exit) ===" -ForegroundColor Cyan
exit $exit
