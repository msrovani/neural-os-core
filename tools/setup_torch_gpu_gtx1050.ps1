# setup_torch_gpu_gtx1050.ps1 — GTX 1050 (sm_61) + Python 3.14
# cu130 detecta a GPU mas NAO tem kernels Pascal. Use cu126.
# Uso: .\tools\setup_torch_gpu_gtx1050.ps1

$ErrorActionPreference = "Stop"
$env:CUDA_VISIBLE_DEVICES = "0"

Write-Host "[1/3] Removendo torch atual..."
python -m pip uninstall -y torch 2>$null

Write-Host "[2/3] Instalando torch==2.13.0+cu126..."
python -m pip install --no-cache-dir "torch==2.13.0+cu126" `
  --index-url https://download.pytorch.org/whl/cu126

Write-Host "[3/3] Probe CUDA..."
python tools\_probe_cuda.py
if ($LASTEXITCODE -ne 0) {
  Write-Error "GPU probe falhou"
  exit 1
}
Write-Host "OK — treino: `$env:CUDA_VISIBLE_DEVICES='0'; python -u tools\prepare_extra_models.py --tiny15 --rustcoder"
