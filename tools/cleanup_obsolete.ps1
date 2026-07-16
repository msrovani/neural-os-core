#Requires -Version 5.1
<#
.SYNOPSIS
  Remove artefatos obsoletos/regeneraveis do neural-os-core.

.DESCRIPTION
  Dry-run por padrao (lista o que seria removido). Use -Apply para executar.

  NUNCA toca (sem flag perigosa):
    keys/, skills/, firmware/, crates/, docs/, *.md fonte,
    modelos .bitnet/.BIN, discos .raw/.img, ovmf.fd, uefi.img

  Niveis:
    1. Seguro (padrao)     - builds isolados, __pycache__, logs vazios, leftovers target-*
    2. -IncludeCargoCache  - pastas cargo release/debug (cargo nk regenera)
    3. -IncludeEmptyLogs   - *.log / *-out.txt vazios ou quase vazios em target/
    4. -IncludeBootLogs    - conteudo de logs/ (evidencia QEMU; regeneravel mas util)
    5. -IncludeDiskImages  - PERIGOSO: disk_*.raw, usb_hw.img (regeneraveis, ~GB)
    6. -IncludeModels      - PERIGOSO: .bitnet / PIPER / STT / BGE (caros de regenerar)

.EXAMPLE
  .\tools\cleanup_obsolete.ps1
  .\tools\cleanup_obsolete.ps1 -Apply
  .\tools\cleanup_obsolete.ps1 -IncludeCargoCache -Apply
  .\tools\cleanup_obsolete.ps1 -IncludeCargoCache -IncludeEmptyLogs -Apply
#>

[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [switch]$Apply,
    [switch]$IncludeCargoCache,
    [switch]$IncludeEmptyLogs,
    [switch]$IncludeBootLogs,
    [switch]$IncludeDiskImages,
    [switch]$IncludeModels,
    [int]$EmptyLogMaxBytes = 64
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not (Test-Path (Join-Path $Root "AGENTS.md"))) {
    throw "Nao parece a raiz do neural-os-core: $Root"
}

$script:RemovedBytes = [int64]0
$script:RemovedCount = 0
$script:Skipped = New-Object System.Collections.Generic.List[string]
$script:Plan = New-Object System.Collections.Generic.List[object]

function Format-Size([int64]$bytes) {
    if ($bytes -ge 1GB) { return "{0:N2} GB" -f ($bytes / 1GB) }
    if ($bytes -ge 1MB) { return "{0:N2} MB" -f ($bytes / 1MB) }
    if ($bytes -ge 1KB) { return "{0:N1} KB" -f ($bytes / 1KB) }
    return "$bytes B"
}

function Get-PathSize([string]$path) {
    if (-not (Test-Path $path)) { return [int64]0 }
    $item = Get-Item -LiteralPath $path -Force
    if (-not $item.PSIsContainer) { return [int64]$item.Length }
    $sum = [int64]0
    Get-ChildItem -LiteralPath $path -Recurse -Force -File -ErrorAction SilentlyContinue |
        ForEach-Object { $sum += $_.Length }
    return $sum
}

function Add-Plan([string]$path, [string]$reason) {
    if (-not (Test-Path -LiteralPath $path)) { return }
    $full = (Resolve-Path -LiteralPath $path).Path
    $norm = $full.Replace("/", "\")
    $forbidden = @(
        "\keys\", "\skills\", "\firmware\", "\crates\", "\docs\",
        "\AGENTS.md", "\CHANGELOG.md", "\README.md"
    )
    foreach ($f in $forbidden) {
        if ($norm -like "*$f*" -or $norm.EndsWith($f.TrimStart("\"))) {
            $script:Skipped.Add("PROTECTED: $full ($reason)")
            return
        }
    }
    $size = Get-PathSize $full
    $script:Plan.Add([pscustomobject]@{
        Path   = $full
        Reason = $reason
        Size   = $size
        IsDir  = (Get-Item -LiteralPath $full -Force).PSIsContainer
    })
}

function Invoke-Plan {
    Write-Host ""
    Write-Host "=== PLANO DE LIMPEZA ===" -ForegroundColor Cyan
    Write-Host "Root: $Root"
    if ($Apply) {
        Write-Host "Modo: APPLY (vai remover)"
    } else {
        Write-Host "Modo: DRY-RUN (nada removido)"
    }
    Write-Host ""

    if ($script:Plan.Count -eq 0) {
        Write-Host "Nada para remover." -ForegroundColor Green
        return
    }

    $total = [int64]0
    $script:Plan | Sort-Object Size -Descending | ForEach-Object {
        $tag = if ($_.IsDir) { "[DIR] " } else { "[FILE]" }
        Write-Host ("{0} {1,-12}  {2}" -f $tag, (Format-Size $_.Size), $_.Reason)
        Write-Host ("       {0}" -f $_.Path) -ForegroundColor DarkGray
        $total += $_.Size
    }

    Write-Host ""
    Write-Host ("Itens: {0}  |  Espaco estimado: {1}" -f $script:Plan.Count, (Format-Size $total)) -ForegroundColor Yellow

    if ($script:Skipped.Count -gt 0) {
        Write-Host ""
        Write-Host "Protegidos / pulados:" -ForegroundColor DarkYellow
        $script:Skipped | Select-Object -First 20 | ForEach-Object { Write-Host "  $_" }
    }

    if (-not $Apply) {
        Write-Host ""
        Write-Host "Dry-run OK. Para executar:" -ForegroundColor Green
        Write-Host "  .\tools\cleanup_obsolete.ps1 -Apply"
        return
    }

    Write-Host ""
    Write-Host "Removendo..." -ForegroundColor Red
    foreach ($p in $script:Plan) {
        try {
            if ($p.IsDir) {
                Remove-Item -LiteralPath $p.Path -Recurse -Force -ErrorAction Stop
            } else {
                Remove-Item -LiteralPath $p.Path -Force -ErrorAction Stop
            }
            $script:RemovedCount++
            $script:RemovedBytes += $p.Size
            Write-Host ("  OK  {0}" -f $p.Path) -ForegroundColor DarkGreen
        } catch {
            Write-Host ("  FAIL {0}: {1}" -f $p.Path, $_.Exception.Message) -ForegroundColor Red
        }
    }
    Write-Host ""
    Write-Host ("Removidos: {0}  |  Liberado: {1}" -f $script:RemovedCount, (Format-Size $script:RemovedBytes)) -ForegroundColor Green
}

# ---------------------------------------------------------------------------
# 1) SEGURO - builds isolados sob target/
# ---------------------------------------------------------------------------
$target = Join-Path $Root "target"
if (Test-Path $target) {
    $safeDirPatterns = @(
        "check-*",
        "agent-*",
        "s106",
        "s107*",
        "nk-*",
        "p4-*", "p5-*", "p6-*", "p7-*", "p8-*", "p9-*",
        "n16*",
        "mvp-*",
        "verify-compile",
        "boot-host",
        "k2chj-staging",
        "adr47",
        "fix-fat-hang",
        "i386-*", "i686-*", "x86_64-stage-*", "stage-*"
    )
    foreach ($pat in $safeDirPatterns) {
        Get-ChildItem -LiteralPath $target -Directory -Force -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like $pat } |
            ForEach-Object { Add-Plan $_.FullName "build isolado regeneravel ($pat)" }
    }
}

# Leftovers legado target-* na raiz
Get-ChildItem -LiteralPath $Root -Directory -Force -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -like "target-*" } |
    ForEach-Object { Add-Plan $_.FullName "leftover legado target-* (usar target/check-*)" }

# ---------------------------------------------------------------------------
# 2) SEGURO - Python cache / pcap / jsonl em tools/
# ---------------------------------------------------------------------------
$tools = Join-Path $Root "tools"
if (Test-Path $tools) {
    Get-ChildItem -LiteralPath $tools -Recurse -Directory -Force -Filter "__pycache__" -ErrorAction SilentlyContinue |
        ForEach-Object { Add-Plan $_.FullName "Python __pycache__" }
    Get-ChildItem -LiteralPath $tools -File -Force -ErrorAction SilentlyContinue |
        Where-Object { $_.Extension -in ".jsonl", ".pcap", ".pyc" } |
        ForEach-Object { Add-Plan $_.FullName "tools cache $($_.Extension)" }
    $modelCache = Join-Path $tools "model_cache"
    if (Test-Path $modelCache) {
        Add-Plan $modelCache "tools/model_cache (download cache)"
    }
}

# ---------------------------------------------------------------------------
# 3) SEGURO - logs vazios / lixo de build na raiz de target/
# ---------------------------------------------------------------------------
if ($IncludeEmptyLogs -and (Test-Path $target)) {
    $emptyPatterns = @(
        "*.log", "*_out.txt", "*-out.txt", "nk-*.txt", "boot-*-out.txt",
        "iter_*.txt", "iter_*.pid", "probe_*.log", "check_*.txt",
        "_git_status_sb.txt"
    )
    foreach ($pat in $emptyPatterns) {
        Get-ChildItem -LiteralPath $target -File -Force -ErrorAction SilentlyContinue |
            Where-Object {
                $_.Name -like $pat -and $_.Length -le $EmptyLogMaxBytes
            } |
            ForEach-Object { Add-Plan $_.FullName "log vazio/tiny (<=$EmptyLogMaxBytes B)" }
    }
}

# ---------------------------------------------------------------------------
# 4) CARGO CACHE - regeneravel com cargo nk
# ---------------------------------------------------------------------------
if ($IncludeCargoCache -and (Test-Path $target)) {
    $cargoDirs = @(
        "x86_64-unknown-none",
        "x86_64-unknown-uefi",
        "release",
        "debug",
        "doc",
        "tmp",
        "incremental"
    )
    foreach ($d in $cargoDirs) {
        $p = Join-Path $target $d
        if (Test-Path $p) {
            Add-Plan $p "cargo cache (regenera com cargo nk / cargo build -p boot)"
        }
    }
    $mk = Join-Path $target "mk_uefi"
    if (Test-Path $mk) { Add-Plan $mk "cargo cache mk_uefi" }
}

# ---------------------------------------------------------------------------
# 5) BOOT LOGS - evidencia QEMU (opcional)
# ---------------------------------------------------------------------------
if ($IncludeBootLogs) {
    $logsDir = Join-Path $Root "logs"
    if (Test-Path $logsDir) {
        Get-ChildItem -LiteralPath $logsDir -File -Force -ErrorAction SilentlyContinue |
            ForEach-Object { Add-Plan $_.FullName "boot log (IncludeBootLogs)" }
    }
}

# ---------------------------------------------------------------------------
# 6) DISK IMAGES - regeneraveis, GB (perigoso)
# ---------------------------------------------------------------------------
if ($IncludeDiskImages -and (Test-Path $target)) {
    Write-Host "AVISO: -IncludeDiskImages remove imagens de disco (regeneraveis com build_image.py)." -ForegroundColor Magenta
    $diskNames = @(
        "disk_qemu.raw", "disk_hw.raw", "disk_hw_unified.raw",
        "usb_hw.img", "uefi.img", "bios.img", "uefi_s107_weather.img"
    )
    foreach ($n in $diskNames) {
        $p = Join-Path $target $n
        if (Test-Path $p) { Add-Plan $p "disk image regeneravel (IncludeDiskImages)" }
    }
}

# ---------------------------------------------------------------------------
# 7) MODELS - caros; so com flag explicita
# ---------------------------------------------------------------------------
if ($IncludeModels -and (Test-Path $target)) {
    Write-Host "AVISO CRITICO: -IncludeModels remove .bitnet / Piper / STT / BGE (podem levar horas a regenerar)." -ForegroundColor Magenta
    $modelGlobs = @(
        "*.bitnet", "*.bitnet.bak-header",
        "BITNET*", "PIPER*", "STT*", "BGE*", "MICRO.BITNET",
        "bpe_vocab.bin", "tokenizer.json", "model.safetensors", "pipenv"
    )
    foreach ($g in $modelGlobs) {
        Get-ChildItem -LiteralPath $target -File -Force -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like $g } |
            ForEach-Object { Add-Plan $_.FullName "modelo/asset (IncludeModels) - REGENERAR CUSTOSO" }
    }
}

# Dedup por path
$unique = $script:Plan | Sort-Object Path -Unique
$script:Plan.Clear()
$unique | ForEach-Object { $script:Plan.Add($_) }

Invoke-Plan

Write-Host ""
Write-Host "Referencia rapida:" -ForegroundColor Cyan
Write-Host "  Dry-run seguro:     .\tools\cleanup_obsolete.ps1"
Write-Host "  Aplicar seguro:     .\tools\cleanup_obsolete.ps1 -Apply"
Write-Host "  + cargo cache:      .\tools\cleanup_obsolete.ps1 -IncludeCargoCache -IncludeEmptyLogs -Apply"
Write-Host "  NUNCA sem backup:   keys/  skills/  firmware/"
Write-Host "  Regenerar discos:   python tools\build_image.py [--hw [--unified]]"
Write-Host "  Regenerar kernel:   cargo nk"
