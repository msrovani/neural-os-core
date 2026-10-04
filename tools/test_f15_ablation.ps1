# Testa o braco de ABLACAO ([7]) do run-f15.ps1 EXECUTANDO O BLOCO REAL extraido
# do arquivo (nao uma copia redigitada) sobre discos de mentira pequenos: o
# caminho de 3 GB nao pode ser o teste, e o bloco real e o que precisa ser testado.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\test_f15_ablation.ps1
#
# O que precisa ser provado (e o que a suite de fixtures do parser NAO cobre):
#   1. restore=1 realmente devolve o pristine ao disco (e o disco perde a skill);
#   2. a verificacao byte a byte roda e reprova se o destino divergir;
#   3. sem restore, o scan ve a skill (lab_state=1) -- o falsificador do boot 1;
#   4. o `ensure` recusa criar um "pristine" a partir de um disco ja sujo.
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$T = Join-Path $Root 'target\abl_test'
$AblPy = Join-Path $Root 'tools\f15_pristine.py'

if (Test-Path $T) { Remove-Item -Recurse -Force $T }
New-Item -ItemType Directory -Force -Path $T | Out-Null
$Disk = Join-Path $T 'disk.raw'
$Prist = Join-Path $T 'pristine.raw'

# discos de mentira: 4 MB, um limpo e um sujo (com o caminho do lab dentro)
$clean = New-Object byte[] (4 * 1024 * 1024)
[System.IO.File]::WriteAllBytes($Prist, $clean)
$dirty = New-Object byte[] (4 * 1024 * 1024)
$needle = [System.Text.Encoding]::ASCII.GetBytes('skill/wasm/oracle_rt_expr_v1')
[System.Array]::Copy($needle, 0, $dirty, 1 * 1024 * 1024, $needle.Length)
[System.IO.File]::WriteAllBytes($Disk, $dirty)

# --- extrai o bloco [7] por LINHAS (o bloco real, nao uma copia) -------------
$src = Get-Content -LiteralPath (Join-Path $Root 'tools\run-f15.ps1')
$ini = -1; $fim = -1
for ($i = 0; $i -lt $src.Count; $i++) {
    if ($src[$i].StartsWith('# --- [7]')) { $ini = $i; break }
}
if ($ini -lt 0) { Write-Host 'FALHOU: marcador [7] nao encontrado no launcher'; exit 1 }
for ($i = $ini + 1; $i -lt $src.Count; $i++) {
    if ($src[$i].StartsWith('# --- [14]')) { $fim = $i - 1; break }
}
if ($fim -lt $ini) { Write-Host 'FALHOU: marcador [14] (fim do bloco) nao encontrado'; exit 1 }
$bloco = ($src[$ini..$fim]) -join "`n"
Write-Host ("bloco [7]: linhas {0}..{1} ({2} linhas)" -f $ini, $fim, ($fim - $ini + 1))

$falhas = @()
function Testa($nome, $cond, $detalhe) {
    if ($cond) { Write-Host ("ok      {0}" -f $nome) }
    else { $script:falhas += $nome; Write-Host ("FALHOU  {0} -- {1}" -f $nome, $detalhe) }
}
function Scan-Disk($path) {
    $out = @(& python $AblPy scan --disk $path)
    if ($LASTEXITCODE -ne 0) { throw "scan falhou ($LASTEXITCODE)" }
    $h = @{}
    foreach ($l in $out) { if ($l -match '^([^=]+)=(.*)$') { $h[$matches[1]] = $matches[2] } }
    return $h
}

# --- 0. o disco sujo e o pristine limpo sao o que a variavel precisa ----------
$h = Scan-Disk $Disk
Testa 'disco de partida tem a skill do lab' ($h['lab_state'] -eq '1') ("lab_state=" + $h['lab_state'])
$h = Scan-Disk $Prist
Testa 'pristine esta limpo' ($h['lab_state'] -eq '0') ("lab_state=" + $h['lab_state'])

# --- 1+3. SEM restore: o launcher ve lab_state=1 e avisa ------------------
$RestorePristine = $false
$Boot = 1
Invoke-Expression $bloco
Testa 'sem restore => restore=0' ($restore -eq 0) ("restore=$restore")
Testa 'sem restore => lab_state=1' ($labState -eq 1) ("labState=$labState")
Testa 'sem restore => motivo declarado' ($restoreMotivo -like '*nao solicitado*') ("motivo=$restoreMotivo")
Testa 'sem restore => disco intacto (dirty)' ((Scan-Disk $Disk)['lab_state'] -eq '1') 'o disco perdeu a skill sem restore'

# --- 2. COM restore: volta ao pristine e verifica byte a byte -------------
$RestorePristine = $true
$Boot = 1
Invoke-Expression $bloco
Testa 'com restore => restore=1' ($restore -eq 1) ("restore=$restore")
Testa 'com restore => lab_state=0' ($labState -eq 0) ("labState=$labState")
$h = Scan-Disk $Disk
Testa 'restore apagou a skill do disco' ($h['lab_state'] -eq '0') ("lab_state=" + $h['lab_state'])
Testa 'restaurado == pristine (bytes)' ((Get-Item $Disk).Length -eq (Get-Item $Prist).Length) 'tamanho divergente'

# --- 4. ensure recusa fonte suja (um "pristine" sujo nao e pristine) ------
$out = @(& python $AblPy ensure --disk $Disk --pristine (Join-Path $T 'p2.raw') --force)
Testa 'ensure --force aceita fonte suja explicitamente' ($LASTEXITCODE -eq 0) ("exit=$LASTEXITCODE out=$($out -join ' | ')")
# agora suja a fonte e pede sem --force: tem de recusar
[System.IO.File]::WriteAllBytes($Disk, $dirty)
$out = @(& python $AblPy ensure --disk $Disk --pristine (Join-Path $T 'p3.raw'))
Testa 'ensure recusa fonte com a skill do lab' ($LASTEXITCODE -eq 3) ("exit=$LASTEXITCODE out=$($out -join ' | ')")
Testa 'ensure nao criou snapshot recusado' (-not (Test-Path (Join-Path $T 'p3.raw'))) 'p3.raw foi criado apesar da recusa'

# --- 5. -RestorePristine no boot 2 tem de ser recusado pelo launcher ------
$RestorePristine = $true
$Boot = 2
$threw = $false
try { Invoke-Expression $bloco } catch { $threw = $true }
Testa 'restore no boot 2 e recusado' $threw 'o launcher aceitou restore no boot 2 (apagaria o estado a persistir)'

Remove-Item -Recurse -Force $T -ErrorAction SilentlyContinue
Write-Host ''
if ($falhas.Count -eq 0) { Write-Host 'bloco [7]: todas as assercoes OK'; exit 0 }
Write-Host ("bloco [7]: {0} FALHA(S): {1}" -f $falhas.Count, ($falhas -join ', '))
exit 1