#!/usr/bin/env powershell
# Parser fail-closed do harness F1.5 (dev-2-run).
#
# Por que existe: tools/run-f15.ps1 e boot-only (nao tinha uma unica linha de assert),
# entao o PASS/FALSIFIED da OPCODE-0067 sairia de leitura humana. Este script le UM log por
# boot e grava <log>.parse com as linhas casadas + veredito + exit code. Nao toca no
# run-f15.ps1: e arquivo novo, para poder rodar sobre logs ja colhidos ou em cima do
# launcher depois.
#
#   powershell -File tools\f15_parse.ps1 -Log logs\f15_boot1.txt -Boot 1
#   powershell -File tools\f15_parse.ps1 -Log logs\f15_boot2.txt -Boot 2
#   powershell -File tools\f15_parse.ps1 -Compare logs\f15_boot1.txt,logs\f15_boot2.txt
#
# Regras que nao sao negociaveis (CURAIX-0046/0048):
#  - Select-Object -Last 1: o boot faz DUAS chamadas de reload (main.rs:2495 e :2760) e a
#    primeira, pre-mount, imprime durable=0. A ultima linha e a pos-mount.
#  - grupos NOMEADOS: a v1 usou Groups[4] num regex de 3 grupos e estourou.
#  - o hash logado e FNV-1a dos BYTES do wasm lidos de skill/wasm, nao do texto fonte.
#  - exit 1 no FALSIFIED: veredito que nao vira status de processo volta a ser opiniao (R10).
#  - TUDO do lab e escopado ao nome da skill (Get-LabLines/Get-LabMatch): o boot roda 147
#    agentes e o trafego alheio (FORJA de outra skill) reprovava o lab.
param(
    [string]$Log = "",
    [int]$Boot = 0,
    [string]$Compare = ""
)
$ErrorActionPreference = "Stop"

function Get-Lines($path) {
    if (-not (Test-Path $path)) { throw "log ausente: $path" }
    return (Get-Content -Raw $path)
}

function Get-Last($raw, $pattern) {
    $m = [regex]::Matches($raw, $pattern)
    if ($m.Count -eq 0) { return $null }
    return $m[$m.Count - 1]
}

# Recorta o log para as linhas que CITAM a skill do lab. Sem esse recorte o
# veredito passa a ser sobre o trafego de TODO mundo: no boot real, duas linhas
# de FORJA de OUTRA skill (hw_pnp_pci_bridge_observe_only_agent_platformage)
# reprovaram o boot 1 com o lab tendo gerado certinho. Contagem global de
# palavra-chave num boot que roda 147 agentes e falso positivo garantido.
function Get-LabLines($raw, $name) {
    $pat = [regex]::Escape($name)
    return ((($raw -split "`r?`n") | Where-Object { $_ -match $pat }) -join "`n")
}

# --- um boot: veredito proprio -------------------------------------------------
function Invoke-Boot($path, $boot) {
    $raw  = Get-Lines $path
    $fail = @()
    $evid = @()
    $name = "oracle_rt_expr_v1"

    $lab = Get-LabLines $raw $name

    # o que NAO pode acontecer, por boot
    $forja = [regex]::Matches($lab, "skill_gen_request|FORJA|escalate=(gen|reuse)")
    $genAll = [regex]::Matches($lab, "act=gen name=")

    # A ULTIMA linha de reload e a pos-mount (main.rs:2760). E o campo
    # `durable_unknown` decide o significado do `durable=0`:
    #   durable_unknown=true  -> o pass NAO rodou (Tickv ausente) = UNKNOWN
    #   durable_unknown=false -> o pass rodou e achou ZERO chaves = numero legitimo
    # Sem isso o parser acusava "Tickv ausente" em cima de um zero verdadeiro  -  o
    # mesmo erro que o reload Ticketv acabou de corrigir, replicado na ferramenta.
    $unknown = $null
    $reload = Get-Last $raw "durable=(?<d>\d+) durable_unknown=(?<u>true|false)"
    if ($reload) { $unknown = $reload.Groups['u'].Value }
    else { $reload = Get-Last $raw "durable=(?<d>\d+)" }   # log anterior ao campo
    $evid += "ultima linha reload: durable=$($reload.Groups['d'].Value) durable_unknown=$unknown"
    if ($reload -and $unknown -eq "true") {
        $fail += "FALSIFIED boot$boot : pass duravel NAO executou (Tickv ausente)  -  durable=0 e UNKNOWN, nao 'zero skills'"
    }

    if ($boot -eq 1) {
        $gen = Get-LabMatch $lab "act=gen name=(?<n>\S+) prov=(?<prov>\S+) bytes=(?<b>\d+) hash=(?<h>0x[0-9a-f]+)"
        if (-not $gen) {
            $fail += "FALSIFIED boot1 : nenhuma linha act=gen (o hook nao gerou a skill)"
        } else {
            $evid += $gen.Value
            if ($gen.Groups['n'].Value -ne $name) { $fail += "FALSIFIED boot1 : nome $($gen.Groups['n'].Value) != $name" }
            if ($gen.Groups['prov'].Value -ne "model-born") { $fail += "FALSIFIED boot1 : prov=$($gen.Groups['prov'].Value) != model-born" }
            $run = Get-LabMatch $lab "run name=(?<n>\S+) a=(?<a>\d+) b=(?<b>\d+) result=(?<r>-?\d+)"
            if (-not $run) { $fail += "FALSIFIED boot1 : nenhuma linha run ... result=" }
            else {
                $evid += $run.Value
                if ($run.Groups['a'].Value -ne "6" -or $run.Groups['b'].Value -ne "7") { $fail += "FALSIFIED boot1 : run com a/b inesperados ($($run.Groups['a'].Value),$($run.Groups['b'].Value)) esperado 6,7" }
                if ($run.Groups['r'].Value -ne "52") { $fail += "FALSIFIED boot1 : result=$($run.Groups['r'].Value) != 52" }
            }
        }
        if ($forja.Count -gt 0) { $fail += "FALSIFIED boot1 : $($forja.Count) linha(s) de escalate/FORJA (promote deveria ter sido direto)" }
    }
    elseif ($boot -eq 2) {
        $nf = Get-LabMatch $lab "escalate=reuse name=\S+ reason=(?<r>[a-z_]+)"
        $reuse = Get-LabMatch $lab "reuse name=(?<n>\S+) has_skill=(?<h>\d) hash=(?<hash>0x[0-9a-f]+)"
        if (-not $reuse) {
            if ($nf) {
                $evid += $nf.Value
                $fail += "FALSIFIED boot2 : escalate=reuse reason=$($nf.Groups['r'].Value) - a skill nao sobreviveu ao reboot (o Tickv nao devolveu o blob)"
            } else {
                $fail += "FALSIFIED boot2 : nenhuma linha reuse (a skill nao sobreviveu ao reboot)"
            }
        } else {
            $evid += $reuse.Value
            if ($reuse.Groups['h'].Value -ne "1") { $fail += "FALSIFIED boot2 : has_skill=0 (o registro nao veio do Tickv)" }
            if ($reuse.Groups['n'].Value -ne $name) { $fail += "FALSIFIED boot2 : nome $($reuse.Groups['n'].Value) != $name" }
        }
        $run = Get-LabMatch $lab "act=reuse name=(?<n>\S+) a=(?<a>\d+) b=(?<b>\d+) result=(?<r>-?\d+)"
        if (-not $run) { $fail += "FALSIFIED boot2 : nenhuma linha act=reuse ... result=" }
        else {
            $evid += $run.Value
            if ($run.Groups['a'].Value -ne "9" -or $run.Groups['b'].Value -ne "2") { $fail += "FALSIFIED boot2 : run com a/b inesperados ($($run.Groups['a'].Value),$($run.Groups['b'].Value)) esperado 9,2" }
            if ($run.Groups['r'].Value -ne "82") { $fail += "FALSIFIED boot2 : result=$($run.Groups['r'].Value) != 82" }
        }
        if ($genAll.Count -gt 0) { $fail += "FALSIFIED boot2 : $($genAll.Count) act=gen no boot 2 (regenerou em vez de reusar)" }
        if ($forja.Count -gt 0) { $fail += "FALSIFIED boot2 : $($forja.Count) linha(s) de escalate/FORJA/skill_gen_request do LAB (as causas estao acima)" }
    }
    else { throw "-Boot tem de ser 1 ou 2" }

    # --- [14] o veredito so vale se o artefato bootado for identificavel ------
    # Regra: nenhuma conclusao de runtime vale sem demonstrar qual imagem bootou.
    # Ausencia do sidecar NAO e zero: e prova faltando -> FALSIFIED.
    $imgid = [System.IO.Path]::ChangeExtension($path, "imgid")
    if (Test-Path -LiteralPath $imgid) {
        $ident = @(Get-Content -LiteralPath $imgid)
        $evid += "artefato bootado: " + (($ident | Where-Object { $_ -match '^(uefi_bytes|disk_bytes|probe_na_imagem)=' }) -join " ")
        if (($ident -join "`n") -notmatch "probe_na_imagem=True") {
            $fail += "FALSIFIED boot$boot : a imagem bootada NAO contem o literal da fonte (probe_na_imagem!=True) - veredito sobre artefato stale"
        }
        # --- [14b] o artefato de AGORA tem de SER o artefato que bootou -------
        # O probe acima e lido NO PARSE: se a imagem foi reconstruida depois do
        # boot, ele mede a imagem nova e "prova" o codigo novo num log antigo.
        # O sidecar guarda bytes+mtime+sha do instante do boot; se o arquivo
        # divergir agora, o veredito e sobre uma imagem que nao bootou.
        $stamp = @{}
        foreach ($l in $ident) { if ($l -match '^([^=]+)=(.*)$') { $stamp[$matches[1]] = $matches[2] } }
        $uefiPath = $stamp["uefi"]
        if (-not $uefiPath -or -not (Test-Path -LiteralPath $uefiPath)) {
            $fail += "FALSIFIED boot$boot : a imagem do sidecar nao existe mais ($uefiPath) - veredito sobre artefato ausente"
        } else {
            $it = Get-Item -LiteralPath $uefiPath
            if ($stamp["uefi_bytes"] -ne ("{0}" -f $it.Length)) {
                $fail += "FALSIFIED boot$boot : a imagem mudou DEPOIS do boot (bytes sidecar=$($stamp['uefi_bytes']) agora=$($it.Length)) - o probe mediu a imagem nova"
            }
            $epochTicks = [int64]$it.LastWriteTimeUtc.Ticks - 621355968000000000
            $epochNow = [int64](($epochTicks - ($epochTicks % 10000000)) / 10000000)
            if ($stamp["uefi_epoch"]) {
                # epoch inteiro: comparacao exata, sem a perda de 1 ULP da string
                if ([int64]$stamp["uefi_epoch"] -ne $epochNow) {
                    $fail += "FALSIFIED boot$boot : a imagem mudou DEPOIS do boot (epoch sidecar=$($stamp['uefi_epoch']) agora=$epochNow)"
                }
            } elseif ($stamp["uefi_mtime"] -and $stamp["uefi_mtime"] -ne $it.LastWriteTimeUtc.ToString("o")) {
                # sidecar antigo, sem epoch: compara a string (fail-closed se divergir)
                $fail += "FALSIFIED boot$boot : a imagem mudou DEPOIS do boot (mtime sidecar=$($stamp['uefi_mtime']) agora=$($it.LastWriteTimeUtc.ToString('o')))"
            }
            if ($stamp["uefi_sha"]) {
                $sha = (Get-FileHash -LiteralPath $uefiPath -Algorithm SHA256).Hash.Substring(0, 16)
                if ($stamp["uefi_sha"] -ne $sha) {
                    $why = if ($stamp["uefi_sha"].StartsWith("ERRO")) { "a identidade NAO foi estabelecida no boot (sidecar=$($stamp['uefi_sha'])); agora=$sha" } else { "sha divergente (sidecar=$($stamp['uefi_sha']) agora=$sha) - conteudo diferente com o mesmo tamanho/mtime" }
                    $fail += "FALSIFIED boot$boot : $why"
                }
            }
            # --- [7] braco de ABLACAO: o disco de partida e uma variavel
            # declarada. Sem estes campos no sidecar nao se sabe de que disco o
            # boot partiu -- e `act=gen` num disco que JA tinha a skill nao
            # prova geracao.
            if (-not $stamp["restore"] -or -not $stamp["disk_lab_state_before"]) {
                $fail += "FALSIFIED boot$boot : sidecar sem o registro de ablacao (restore=/disk_lab_state_before=) - nao se sabe de que disco o boot partiu"
            } else {
                $evid += "ablacao: restore=$($stamp['restore']) disk_lab_state_before=$($stamp['disk_lab_state_before']) ($($stamp['restore_motivo']))"
                if ($boot -eq 1 -and $stamp["disk_lab_state_before"] -eq "1") {
                    $fail += "FALSIFIED boot1 : o disco JA continha a skill do lab antes do boot (disk_lab_state_before=1) - act=gen nao prova geracao"
                }
                if ($boot -eq 2 -and $stamp["restore"] -eq "1") {
                    $fail += "FALSIFIED boot2 : restore=1 no boot 2 apaga o estado que o boot 1 deveria ter persistido - experimento rigged"
                }
            }
        }
    } else {
        $fail += "FALSIFIED boot$boot : sem identidade do artefato bootado (sidecar $imgid ausente) - secao 14 nao permite concluir sobre imagem desconhecida"
    }

    if ($fail.Count -eq 0) { $fail += "PASS boot$boot" }
    # nome do artefato vem do NOME DO LOG, nao do -Boot: se viesse do boot, parsear outro
    # log sobrescreveria a evidencia anterior (achado no proprio teste).
    $base = [System.IO.Path]::GetFileNameWithoutExtension($path)
    $out = Join-Path (Split-Path $path -Parent) ("$base.parse")
    $txt = @("=== evidencia ===") + $evid + @("=== veredito ===") + $fail
    # WriteAllLines e UTF-8 SEM BOM: Set-Content -Encoding UTF8 no PS 5.1 grava BOM, e BOM
    # no inicio de arquivo de evidencia e a origem classica de mojibake depois.
    [System.IO.File]::WriteAllLines((Resolve-Path (Split-Path $path -Parent)).Path + "\" + "$base.parse", $txt)
    foreach ($l in $evid) { Write-Host "[parse] $l" }
    foreach ($l in $fail) { Write-Host "[parse] $l" }
    Write-Host "[parse] -> $out"
    return @($fail, $out)
}

# Ultima LINHA do recorte do lab que casa o padrao. Casa linha a linha em vez de
# regex global: (a) um padrao fraco nao atravessa duas linhas e casa pedaco de
# outra skill, (b) o padrao so cobre um TRECHO da linha (o ^...$ que eu tinha
# ancorado exigia a linha inteira e nunca casava). Devolve o Match, para os
# Groups do veredito continuarem acessiveis.
function Get-LabMatch($lab, $pattern) {
    $rx = [regex]::new($pattern)
    $last = $null
    foreach ($ln in ($lab -split "`n")) {
        $m = $rx.Match($ln)
        if ($m.Success) { $last = $m }
    }
    return $last
}

# --- comparacao entre os boots ------------------------------------------------
function Invoke-Compare($two) {
    $paths = $two -split ","
    if ($paths.Count -ne 2) { throw "-Compare espera dois logs separados por virgula" }
    $h = @()
    foreach ($p in $paths) {
        $raw = Get-Lines $p.Trim()
        $lab = Get-LabLines $raw "oracle_rt_expr_v1"
        $nf  = Get-LabMatch $lab "escalate=reuse name=\S+ reason=(?<r>[a-z_]+)"
        $m = Get-LabMatch $lab "hash=(?<h>0x[0-9a-f]+)"
        if (-not $m) {
            if ($nf) { Write-Host "[parse] FALSIFIED compare: $p - escalate=reuse reason=$($nf.Groups['r'].Value) (a skill nao sobreviveu; nao ha hash para comparar)" }
            else      { Write-Host "[parse] FALSIFIED compare: sem hash da skill do lab em $p" }
            exit 1
        }
        $h += $m.Groups['h'].Value
    }
    if ($h[0] -eq $h[1]) { Write-Host "[parse] PASS compare: hash identico $($h[0]) (bytes do wasm persistido batem)"; exit 0 }
    Write-Host "[parse] FALSIFIED compare: hash divergente $($h[0]) vs $($h[1])"
    exit 1
}

if ($Compare -ne "") { Invoke-Compare $Compare }
if ($Log -eq "") { throw "informe -Log <arquivo> -Boot <1|2>  ou  -Compare <log1,log2>" }
$r = Invoke-Boot $Log $Boot
exit $(if ($r[0] -match "FALSIFIED") { 1 } else { 0 })