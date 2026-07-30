<#
.SYNOPSIS
  Legacy Integrity Lint — verifica se módulos LEGACY foram corretamente integrados no codebase ativo.
  Roda após reintegração de qualquer módulo LEGACY.
.PARAMETER CheckUnused
  Verifica se há módulos no LEGACY que NÃO têm correspondência no ativo (podem ser reintegrados).
.PARAMETER Quick
  Pula cargo check (mais rápido, mas não valida build).
#>

param([switch]$CheckUnused, [switch]$Quick)
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$exit = 0
$errors = @()
$warnings = @()

Write-Host "=== legacy_integrity_lint.ps1 ===" -ForegroundColor Cyan

# ─── 1. Build check ───
if (-not $Quick) {
    Write-Host "`n[1/5] Build check..." -ForegroundColor Yellow
    Write-Host "  cargo clean -p neural-kernel..." -ForegroundColor Gray
    cargo clean -p neural-kernel 2>&1 | Out-Null
    $build = cargo check --release 2>&1
    if ($LASTEXITCODE -eq 0) {
        $warn = ($build | Select-String -Pattern "^warning" | Measure-Object -Line).Lines
        Write-Host "  PASS: 0 errors, $warn warnings" -ForegroundColor Green
    } else {
        Write-Host "  FAIL: build errors" -ForegroundColor Red
        $build | Select-String -Pattern "^error" | ForEach-Object { Write-Host "    $_" -ForegroundColor Red; $errors += $_ }
        $exit = 1
    }
}

# ─── 2. Module declaration integrity ───
Write-Host "`n[2/5] Module declarations..." -ForegroundColor Yellow
$libs = @(
    "$root/crates/k_nano/src/lib.rs",
    "$root/crates/k_ai/src/lib.rs",
    "$root/crates/cortex/src/lib.rs",
    "$root/crates/hermes/src/lib.rs",
    "$root/crates/jarbas/src/lib.rs",
    "$root/crates/agent-core/src/lib.rs",
    "$root/crates/event-bus/src/lib.rs"
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
    $missing | ForEach-Object { Write-Host "    $_" -ForegroundColor Red; $errors += $_ }
    $exit = 1
} else {
    Write-Host "  PASS: todos mods tem arquivo" -ForegroundColor Green
}

# ─── 3. LEGACY→active mapping check ───
Write-Host "`n[3/5] LEGACY symbol mapping..." -ForegroundColor Yellow
$mappings = @(
    @("chunker.rs", "RabinFingerprint", "k_ai::chunker"),
    @("dma.rs", "PhysicalBuffer", "k_nano::dma"),
    @("hal.rs", "Architecture", "k_nano::hal"),
    @("budget.rs", "AgentBudget", "agent-core::budget"),
    @("hooks.rs", "HookRegistry", "agent-core::hooks"),
    @("audit.rs", "AuditTrail", "k_ai::audit"),
    @("clock.rs", "LogicalClock", "k_nano::sync::clock"),
    @("noproto.rs", "AiosTaskPacket", "k_nano::net::noproto"),
    @("core_pair.rs", "CorePairAllocator", "k_nano::scheduler::core_pair"),
    @("cfs.rs", "CfsScheduler", "k_nano::scheduler::cfs"),
    @("ipw_monitor.rs", "IpwMonitor", "jarbas::jarvis"),
    @("boot_log_agent.rs", "BootLogAgent", "k_ai::boot_log_agent")
)
$unmapped = @()
foreach ($m in $mappings) {
    $legacyFile = $m[0]
    $symbol = $m[1]
    $target = $m[2]
    # Check if symbol exists in the active crate path
    $targetDir = $target -replace '::.*', ''
    $targetFile = $target -replace '.*::', ''
    # Simple heuristic: grep for the symbol in the crate
    $found = $false
    $searchPath = "$root/crates/$($targetDir -replace '-', '_')/src"
    if (Test-Path $searchPath) {
        $found = (Select-String -Pattern $symbol -Path "$searchPath/*.rs" -SimpleMatch -ErrorAction SilentlyContinue) -ne $null
    }
    if (-not $found) {
        $unmapped += "$legacyFile → $symbol → $target (NÃO ENCONTRADO)"
    }
}
if ($unmapped.Count -gt 0) {
    Write-Host "  WARN: $($unmapped.Count) símbolos LEGACY sem correspondência no ativo" -ForegroundColor Yellow
    $unmapped | ForEach-Object { Write-Host "    $_" -ForegroundColor Yellow; $warnings += $_ }
} else {
    Write-Host "  PASS: todos símbolos mapeados" -ForegroundColor Green
}

# ─── 4. Check for unused modules in LEGACY (opt-in) ───
if ($CheckUnused) {
    Write-Host "`n[4/5] LEGACY módulos não integrados..." -ForegroundColor Yellow
    $legacyDirs = @(
        "$root/LEGACY/v1.9.9-test/k_nano/p2p",
        "$root/LEGACY/v1.9.9-test/k_nano/net",
        "$root/LEGACY/v1.9.9-test/k_nano/scheduler",
        "$root/LEGACY/v1.9.9-test/hermes",
        "$root/LEGACY/v1.9.9-test/agent-core",
        "$root/LEGACY/k_ia/src",
        "$root/LEGACY/jarvis/src"
    )
    $unused = @()
    foreach ($dir in $legacyDirs) {
        if (-not (Test-Path $dir)) { continue }
        Get-ChildItem -Path $dir -Filter "*.rs" | ForEach-Object {
            $name = $_.BaseName
            $content = Get-Content $_.FullName -Raw
            # Check if the main struct/fn of this module exists in active code
            $structMatch = [regex]::Match($content, '(?:pub\s+)?(?:struct|fn|trait|enum)\s+(\w+)')
            if ($structMatch.Success) {
                $symbol = $structMatch.Groups[1].Value
                # Skip common names
                if ($symbol -in @("main", "new", "test", "Error", "Result", "None")) { continue }
                $found = (Select-String -Pattern "\b$symbol\b" -Path "$root/crates/*/src/*.rs" -SimpleMatch -ErrorAction SilentlyContinue) -ne $null
                if (-not $found) {
                    $unused += "$($_.FullName): $symbol"
                }
            }
        }
    }
    if ($unused.Count -gt 0) {
        Write-Host "  INFO: $($unused.Count) símbolos só em LEGACY (candidatos a reintegração)" -ForegroundColor Cyan
        $unused | ForEach-Object { Write-Host "    $_" -ForegroundColor Gray }
    } else {
        Write-Host "  PASS: todos símbolos LEGACY têm correspondência no ativo" -ForegroundColor Green
    }
}

# ─── 5. Resumo ───
Write-Host "`n[5/5] Resumo" -ForegroundColor Cyan
if ($errors.Count -gt 0) {
    Write-Host "  $($errors.Count) erro(s) - corrija antes de commit" -ForegroundColor Red
}
if ($warnings.Count -gt 0) {
    Write-Host "  $($warnings.Count) aviso(s) - verificar se intencional" -ForegroundColor Yellow
}
if ($errors.Count -eq 0 -and $warnings.Count -eq 0) {
    Write-Host "  TUDO OK - integracao LEGACY consistente" -ForegroundColor Green
}
$x = "=== FIM (exit=$exit) ==="
Write-Host $x -ForegroundColor Cyan
exit $exit
