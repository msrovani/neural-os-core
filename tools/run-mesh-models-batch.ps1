# Batch de testes de mesh 4-node, um modelo por vez
# Uso: .\tools\run-mesh-models-batch.ps1 [-TempoSeg 120] [-Accel whpx|tcg] [-Force]
param(
    [int]$TempoSeg = 240,
    [ValidateSet("whpx", "tcg")]
    [string]$Accel = "whpx",
    [switch]$Force,
    [string]$Mem = "1G",
    [int]$NeededGB = 6,
    [switch]$Single
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
if (-not (Test-Path (Join-Path $Root "Cargo.toml"))) { $Root = $PSScriptRoot }
Set-Location $Root

$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$batchLog = Join-Path (Join-Path $Root "logs") "mesh_batch_$timestamp.txt"

# Modelos a testar (caminho, rótulo)
$modelos = @(
    @{ Path = "target\FALCON3.BIN"; Label = "falcon3-3b" },
    @{ Path = "target\BITNET2B.BIN"; Label = "bitnet-2a-4b" },
    @{ Path = "target1\BGE_M3.BIN"; Label = "bge-m3" },
    @{ Path = "target1\bpe_vocab.bin"; Label = "bpe-vocab" },
    @{ Path = "target1\hw_expert_v6.bitnet"; Label = "hw-expert-v6" },
    @{ Path = "target1\ROUTER.BITNET"; Label = "router-bitnet" },
    @{ Path = "target1\STT.BIN"; Label = "stt" },
    @{ Path = "target1\PIPER_PT_BR.BIN"; Label = "piper-pt-br" }
)

"=== BATCH MESH 4-NODE - UM MODELO POR VEZ ===" | Tee-Object -FilePath $batchLog
"Início: $(Get-Date)" | Tee-Object -FilePath $batchLog -Append
"Accel: $Accel | Tempo por modelo: $TempoSeg s" | Tee-Object -FilePath $batchLog -Append
"" | Tee-Object -FilePath $batchLog -Append

$resultados = @()

foreach ($m in $modelos) {
    $caminho = Join-Path $Root $m.Path
    if (-not (Test-Path $caminho)) {
        Write-Host "[SKIP] $($m.Label): arquivo não encontrado ($caminho)" -ForegroundColor Yellow
        "$($m.Label): SKIP (arquivo não encontrado)" | Tee-Object -FilePath $batchLog -Append
        continue
    }
    $tamMB = [math]::Round((Get-Item $caminho).Length / 1MB, 1)
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host " MODELO: $($m.Label) ($tamMB MB)" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
    "=== $($m.Label) ($tamMB MB) ===" | Tee-Object -FilePath $batchLog -Append

    # Roda o mesh com este modelo
    & (Join-Path $Root "tools\run-mesh-model.ps1") -Modelo $caminho -TempoSeg $TempoSeg -Accel $Accel -Force -Mem $Mem -NeededGB $NeededGB -Single:$Single
    $exit = $LASTEXITCODE

    # Lê o resumo gerado
    $summaryFile = Get-ChildItem -Path (Join-Path $Root "logs") -Filter "mesh_summary_$($m.Label)_*.txt" | Sort-Object CreationTime -Descending | Select-Object -First 1
    if ($summaryFile) {
        $resumo = Get-Content $summaryFile.FullName -ErrorAction SilentlyContinue
        $resumo | Tee-Object -FilePath $batchLog -Append
    }

    $status = if ($exit -eq 0) { "OK" } else { "FALHA ($exit)" }
    Write-Host "Resultado $($m.Label): $status" -ForegroundColor $(if ($exit -eq 0) { "Green" } else { "Red" })
    "$($m.Label): $status" | Tee-Object -FilePath $batchLog -Append
}

Write-Host ""
Write-Host "=== BATCH CONCLUÍDO ===" -ForegroundColor Cyan
"Fim: $(Get-Date)" | Tee-Object -FilePath $batchLog -Append
Write-Host "Log completo: $batchLog" -ForegroundColor Green
