# Pack NKP lab artifacts into target/nkp-lab/ (ADR-0105 B0).
# Host-only. Requires nvcc for real CUBIN; without it emits CpuStub (never Ready).
param(
    [string[]]$Sm = @('sm_75', 'sm_80', 'sm_86', 'sm_89'),
    [switch]$IncludeSm61,
    [switch]$IntelStub,
    [switch]$AmdStub
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Out = Join-Path $Root 'target\nkp-lab'
New-Item -ItemType Directory -Force -Path $Out | Out-Null

$sms = @($Sm)
if ($IncludeSm61) { $sms = @('sm_61') + $sms }

foreach ($sm in $sms) {
    $tag = $sm.Replace('sm_', 'SM')
    Write-Host "pack $sm vector_add + w2a8 -> $Out"
    python (Join-Path $Root 'tools\pack_nvidia_kernels.py') `
        --sm $sm --op vector_add --unsigned `
        -o (Join-Path $Out "NKP_$tag.BIN")
    python (Join-Path $Root 'tools\pack_nvidia_kernels.py') `
        --sm $sm --op w2a8 --unsigned `
        -o (Join-Path $Out "NKP_W2A8_$tag.BIN")
}

if ($IntelStub) {
    python (Join-Path $Root 'tools\pack_intel_kernels.py') `
        --isa gen9 --op w2a8 --unsigned `
        -o (Join-Path $Out 'NKP_W2A8_GEN9.BIN')
}
if ($AmdStub) {
    python (Join-Path $Root 'tools\pack_amd_kernels.py') `
        --gfx gfx1030 --op w2a8 --unsigned `
        -o (Join-Path $Out 'NKP_W2A8_GFX1030.BIN')
}

Write-Host "done. Regenerar FAT: python tools/mkfat32.py (ou build_image)."
Get-ChildItem $Out -Filter 'NKP*.BIN' | Select-Object Name, Length
