# Testa os blocos [7] (ablacao) e [14] (carimbo de identidade) do run-f15.ps1
# EXECUTANDO O BLOCO REAL extraido do arquivo (nao uma copia redigitada): se o
# texto mudar, o teste testa o texto novo.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\test_f15_stamp.ps1
#
# Cobre o que a checagem de sintaxe nao cobre - o bloco rodar e produzir campos
# corretos, incluindo o probe dentro de uma imagem de 128 MB e a leitura do
# estado do disco (scan de 3 GB). Precisa de target\uefi.img e
# target\disk_qemu.raw (build canonico: cargo build -p boot + build_image).
#
# A versao anterior desse teste vivia em target/ (gitignored) e sumia com um
# `git clean`; e dependia so do bloco [14], que agora usa $Prist do [7].
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

# --- extrai os blocos [7]..[14] por LINHAS (regex no texto inteiro e fragil) --
$src = Get-Content -LiteralPath (Join-Path $Root 'tools\run-f15.ps1')
$ini = -1
for ($i = 0; $i -lt $src.Count; $i++) {
    if ($src[$i].StartsWith('# --- [7]')) { $ini = $i; break }
}
if ($ini -lt 0) { Write-Host 'FALHOU: marcador [7] nao encontrado'; exit 1 }
$fim = -1
for ($i = $ini; $i -lt $src.Count; $i++) {
    if ($src[$i] -like '*WriteAllLines($ImgIdPath*') {
        $fim = $i
        if (($i + 1) -lt $src.Count -and $src[$i + 1] -like '*Write-Host*') { $fim = $i + 1 }
        break
    }
}
if ($fim -lt $ini) { Write-Host 'FALHOU: nao achei a escrita do sidecar'; exit 1 }
$bloco = ($src[$ini..$fim]) -join "`n"
Write-Host ("bloco extraido: linhas {0}..{1} ({2} linhas)" -f $ini, $fim, ($fim - $ini + 1))

# --- mesmo ambiente que o launcher monta ------------------------------------
$LogPath = Join-Path $Root 'target\imgid_test\f15_boot1.txt'
New-Item -ItemType Directory -Force -Path (Split-Path $LogPath -Parent) | Out-Null
$Uefi = Join-Path $Root 'target\uefi.img'
$Disk = Join-Path $Root 'target\disk_qemu.raw'
$Prist = Join-Path $Root 'target\disk_qemu.pristine.raw'
foreach ($p in @($Uefi, $Disk)) {
    if (-not (Test-Path -LiteralPath $p)) {
        Write-Host "FALHOU: imagem ausente $p (rode: cargo build -p boot; python tools\build_image.py --bios)"
        exit 1
    }
}
# restore desligado de proposito: target\disk_qemu.raw e estado COMPARTILHADO do
# lab e este teste nao pode sobrescrever o boot de outra thread.
$Boot = 1
$RestorePristine = $false

# --- executa o bloco real ---------------------------------------------------
Invoke-Expression $bloco

# --- verifica o sidecar -----------------------------------------------------
$ImgIdPath = [System.IO.Path]::ChangeExtension($LogPath, "imgid")
if (-not (Test-Path -LiteralPath $ImgIdPath)) { Write-Host 'FALHOU: sidecar nao criado'; exit 1 }
$lines = @(Get-Content -LiteralPath $ImgIdPath)
Write-Host ''
Write-Host '--- sidecar gerado ---'
$lines | ForEach-Object { Write-Host "  $_" }

# ARMADILHA (vista 3x): -match/-notmatch num ARRAY devolve os ELEMENTOS que
# casam, nao um booleano. Junto antes de casar.
$joined = $lines -join "`n"
$falhas = @()
foreach ($k in @('uefi_bytes=', 'uefi_epoch=', 'uefi_sha=', 'disk_bytes=', 'probe_na_fonte=', 'probe_na_imagem=',
                 'restore=', 'restore_motivo=', 'disk_lab_state_before=', 'pristine_bytes=')) {
    if ($joined -notmatch "(?m)^$k") { $falhas += "campo ausente: $k" }
}
$realUefi = (Get-Item -LiteralPath $Uefi).Length
$realDisk = (Get-Item -LiteralPath $Disk).Length
if ($joined -notmatch "(?m)^uefi_bytes=$realUefi`$") { $falhas += "uefi_bytes divergente do arquivo real ($realUefi)" }
if ($joined -notmatch "(?m)^disk_bytes=$realDisk`$") { $falhas += "disk_bytes divergente do arquivo real ($realDisk)" }
if ($joined -notmatch '(?m)^probe_na_imagem=True$') { $falhas += 'probe_na_imagem != True (imagem stale ou probe quebrado)' }
# restore=1 aqui seria o teste sobrescrevendo o disco do lab: tem de ser 0.
if ($joined -notmatch '(?m)^restore=0$') { $falhas += 'restore != 0 (o teste nao pode restaurar o disco compartilhado)' }
if ($joined -notmatch '(?m)^disk_lab_state_before=[01]$') { $falhas += 'disk_lab_state_before fora de {0,1}' }

# epoch do sidecar tem de bater com o arquivo (formula exata, sem ULP/arredond.)
$it = Get-Item -LiteralPath $Uefi
$ticks = [int64]$it.LastWriteTimeUtc.Ticks - 621355968000000000
$epoch = [int64](($ticks - ($ticks % 10000000)) / 10000000)
if ($joined -notmatch "(?m)^uefi_epoch=$epoch`$") { $falhas += "uefi_epoch divergente do arquivo real ($epoch)" }

# Ambiente compartido: se outro QEMU/build tem a imagem ABERTA, o launcher grava
# uefi_sha=ERRO:lido-em-uso (identidade nao estabelecida -> o parser reprova).
# Nao e falha do codigo deste teste: e o lab em uso. Sai 2 = inconclusivo.
$shaErr = ($joined -match '(?m)^uefi_sha=ERRO')

Remove-Item -Recurse -Force (Split-Path $LogPath -Parent) -ErrorAction SilentlyContinue
Write-Host ''
if ($shaErr) {
    Write-Host 'INCONCLUSIVO: target\uefi.img esta travado por outro processo (outro QEMU/build).'
    Write-Host 'O launcher registrou uefi_sha=ERRO:lido-em-uso -- o parser vai reprovar por identidade nao estabelecida.'
    Write-Host 'Rode de novo com o lab livre para fechar o gate.'
    if ($falhas.Count -eq 0) { exit 2 }
}
if ($falhas.Count -eq 0) { Write-Host 'bloco [7]+[14]: sidecar completo e coerente com os arquivos'; exit 0 }
Write-Host ("bloco [7]+[14]: {0} FALHA(S):" -f $falhas.Count)
foreach ($f in $falhas) { Write-Host "  - $f" }
exit 1