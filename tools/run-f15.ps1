#!/usr/bin/env pwsh
# F1.5 lab launcher (ORACLE-0051 / OPCODE-0063) - two-boot ablation for durable skills.
#   Boot 1 (G): target\lab_skill_boot1.bin -> LSK1 'G' generate + run.
#   Boot 2 (E): target\lab_skill_boot2.bin -> LSK1 'E' reuse + run (durable Tickv).
# Not run by CI; invoke manually. The pristine copy enables ablation from a clean disk.
#
# VEREDITO: depois do run, use tools\f15_parse.ps1 (le 1 log por boot, grava <log>.parse,
# exit 1 no FALSIFIED). Este launcher nao tira veredito: ele so entrega log + status.
#   powershell -File tools\run-f15.ps1 -Boot 1 -TimeoutSec 900
#   powershell -File tools\f15_parse.ps1 -Log logs\f15_boot1.txt -Boot 1
#   powershell -File tools\f15_parse.ps1 -Compare "logs\f15_boot1.txt,logs\f15_boot2.txt"
# (o -Compare e SOZINHO: um unico argumento com virgula, e sem -Log)
param(
    [int]$Boot = 1,
    [int]$Cores = 4,
    [string]$LogPath = "",
    [switch]$PreparePristine,
    [switch]$Whpx,
    [int]$TimeoutSec = 900
)
$ErrorActionPreference = "Stop"
$Root  = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Qemu  = "C:\Program Files\qemu\qemu-system-x86_64.exe"
# OVMF dual pflash (code ro + vars rw). Antes apontava para target\ovmf.bin, que
# NAO existe (so ha ovmf_code.fd/ovmf_vars.fd) -> throw antes de abrir o log e
# serial de 0 bytes (diagnostico OPKIMI-0004 / FREEBU-0053).
$OvmfCode = Join-Path $Root "target\ovmf_code.fd"
# Template de NVRAM VIRGEM. target\ovmf_vars.fd acumula estado a cada boot e um run
# morto no meio da escrita corrompe -> OVMF nao acha o ESP e o serial sai 0 bytes
# (medido 03/10: vars corrompido = 0B; vars virgem = 98KB em 60s, Limine handoff OK).
# Preferir o template do QEMU; cair no do projeto se ausente.
$OvmfVars = "C:\Program Files\qemu\share\edk2-i386-vars.fd"
if (-not (Test-Path $OvmfVars)) { $OvmfVars = Join-Path $Root "target\ovmf_vars.fd" }
# Copia fresca das vars por boot: vars e RW, e reusar o mesmo arquivo entre os
# dois boots contamina o NVRAM (nao afeta o disco, mas quebra reprodutibilidade).
$OvmfVarsRun = Join-Path $Root ("target\ovmf_vars_boot{0}.fd" -f $Boot)
$Uefi  = Join-Path $Root "target\uefi.img"
$Disk  = Join-Path $Root "target\disk_qemu.raw"
$Prist = Join-Path $Root "target\disk_qemu.pristine.raw"
$Blob  = Join-Path $Root ("target\lab_skill_boot{0}.bin" -f $Boot)
if ($LogPath -eq "") { $LogPath = Join-Path $Root ("logs\f15_boot{0}.txt" -f $Boot) }

# Pristine-copy step (ablation): snapshot the clean disk once, restore before a run.
if ($PreparePristine) {
    if (-not (Test-Path $Prist)) { Copy-Item $Disk $Prist -Force }
    Write-Host "[f15] pristine: $Prist"
    exit 0
}

# Generate the LSK1 lab blobs (must exist before launch; layout in skill_lab.rs).
& python (Join-Path $Root "tools\gen_lsk1.py")
if ($LASTEXITCODE -ne 0) { throw "gen_lsk1.py failed ($LASTEXITCODE)" }

foreach ($p in @($Qemu, $OvmfCode, $OvmfVars, $Uefi, $Disk)) {
    if (-not (Test-Path $p)) { throw "missing: $p" }
}
# --- [14] IDENTIDADE DO ARTEFATO BOOTADO -------------------------------------
# Regra: nenhuma conclusao de runtime vale sem dizer qual imagem bootou. Gravo
# um sidecar ao lado do log com tamanho + mtime das duas imagens e a presenca de
# um literal do codigo-fonte dentro da uefi.img. Sem isso, um veredito mede um
# artefato desconhecido (foi assim que uma imagem em reconstrucao me deu um
# serial de 0 bytes que eu culpei no launcher).
$ImgIdPath = [System.IO.Path]::ChangeExtension($LogPath, "imgid")
$probe = "SKILL_LAB"
try {
    $srcHit = Select-String -Path (Join-Path $Root "crates\hermes\src\skill_lab.rs") `
                          -Pattern $probe -SimpleMatch -Quiet
} catch { $srcHit = $false }
$uefiHit = (Select-String -Path $Uefi -Pattern $probe -SimpleMatch -Quiet -ErrorAction SilentlyContinue)
$imgid = @(
    "uefi=$Uefi"
    ("uefi_bytes={0}" -f (Get-Item $Uefi).Length)
    ("uefi_mtime={0}" -f (Get-Item $Uefi).LastWriteTimeUtc.ToString("o"))
    # epoch em SEGUNDOS INTEIROS: a string ISO perde 1 ULP entre o PowerShell e o
    # Python e reprovava casos legitimos (medido: ...651Z vs ...652Z).
    ("uefi_epoch={0}" -f ([int64](([int64](Get-Item $Uefi).LastWriteTimeUtc.Ticks - 621355968000000000) / 10000000)))
    # sha dos 16 primeiros hex: o parser compara com o arquivo atual; sem isso,
    # "probe_na_imagem=True" lido no parse prova a imagem de AGORA, nao a que
    # bootou (medido s447: sidecar dizia mtime 00:36, o arquivo era de 00:40).
    ("uefi_sha={0}" -f (Get-FileHash -LiteralPath $Uefi -Algorithm SHA256).Hash.Substring(0, 16))
    "disk=$Disk"
    ("disk_bytes={0}" -f (Get-Item $Disk).Length)
    ("disk_mtime={0}" -f (Get-Item $Disk).LastWriteTimeUtc.ToString("o"))
    "probe=$probe"
    "probe_na_fonte=$srcHit"
    "probe_na_imagem=$uefiHit"
)
[System.IO.File]::WriteAllLines($ImgIdPath, $imgid)
Write-Host ("[f15] imgid: {0} (probe na imagem={1})" -f $ImgIdPath, $uefiHit)

New-Item -ItemType Directory -Force -Path (Split-Path $LogPath -Parent) | Out-Null
Remove-Item -Force $LogPath -ErrorAction SilentlyContinue
Copy-Item -Force $OvmfVars $OvmfVarsRun

# CPU/accel espelham o launcher mantido (run-qemu-whpx.ps1:175-181): WHPX usa
# Haswell porque host+APX/MPX em QEMU 11 da #GP no PlatformPei do OVMF; em TCG o
# mantido usa max.
if ($Whpx) { $accel = "whpx"; $cpu = "Haswell" } else { $accel = "tcg"; $cpu = "max" }

# Caminhos em aspas: Start-Process -ArgumentList junta o array com espacos e nao
# escapa nada (ao contrario do & $Qemu @qargs).
$qargs = @(
    "-m", "4G", "-smp", $Cores, "-accel", $accel, "-cpu", $cpu, "-net", "none", "-no-reboot",
    "-drive", ('format=raw,file="{0}",if=ide,index=0' -f $Uefi),
    "-drive", ('format=raw,file="{0}",if=virtio,cache=writethrough' -f $Disk),
    "-drive", ('if=pflash,format=raw,file="{0}",readonly=on' -f $OvmfCode),
    "-drive", ('if=pflash,format=raw,file="{0}"' -f $OvmfVarsRun),
    "-serial", ('file:{0}' -f $LogPath), "-serial", "null", "-display", "none"
)
if (Test-Path $Blob) {
    $qargs += @("-device", ('loader,file="{0}",addr=0x2110000' -f $Blob))
    Write-Host "[f15] loader: $Blob @0x2110000"
} else {
    Write-Host "[f15] WARN blob ausente: $Blob (boot sem LSK1)" -ForegroundColor Yellow
}
Write-Host "[f15] boot=$Boot accel=$accel cpu=$cpu log=$LogPath timeout=${TimeoutSec}s"

# ORCAMENTO DE TEMPO + EXIT CODE: o guest nao encerra o QEMU, entao sem isto o
# run e interativo e 'QEMU morreu na hora' e 'guest nao escreveu nada' ficam
# indistinguiveis (um serial de 0 bytes nos dois casos - FREEBU-0053).
$proc = Start-Process -FilePath $Qemu -ArgumentList $qargs -PassThru -NoNewWindow
# CACHEIA O HANDLE: sem esta linha o .ExitCode volta NULL depois da espera (o
# .NET solta o handle do processo que ele nao abriu) e `$code -ne 0` com $code
# null e TRUE - o script acusava falha ate num QEMU que saiu 0. Medido em
# target/test_exitcode.ps1; os 3 .run.log anteriores mostram o sintoma
# ("[f15] qemu exit= log=..." com o codigo vazio, seguido de throw).
$null = $proc.Handle
Write-Host ("[f15] qemu pid={0}" -f $proc.Id)
$timedOut = $false
if (-not $proc.WaitForExit($TimeoutSec * 1000)) {
    $timedOut = $true
    Write-Host ("[f15] orcamento esgotado: parando pid {0}" -f $proc.Id) -ForegroundColor Yellow
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    $proc.WaitForExit()
}
try { $proc.Refresh() } catch { }
$code = $proc.ExitCode
$codeTxt = if ($null -eq $code) { "<desconhecido>" } else { "$code" }
$size = 0
if (Test-Path $LogPath) { $size = (Get-Item $LogPath).Length }
Write-Host ("[f15] qemu exit={0} log={1} bytes -> {2}" -f $codeTxt, $size, $LogPath)

if ($timedOut) {
    # codigo proprio: distingue "matei eu" de "QEMU falhou" - que era a ambiguidade
    Write-Host "[f15] TIMEOUT: log parcial; parse com f15_parse.ps1 ou aumente -TimeoutSec" -ForegroundColor Yellow
    exit 2
}
# NUNCA tratar "nao li" como "falhou" (nem como "passou"): o veredito do run fica
# desconhecido e o run falha, para nao virar um PASS mudo (R10).
if ($null -eq $code) { throw "nao consegui ler o exit code do QEMU (handle sem cache) - veredito do run DESCONHECIDO, nao um PASS" }
if ($code -ne 0) { throw "qemu saiu $code (ver console para o erro do QEMU)" }
if ($size -eq 0) {
    Write-Host "[f15] AVISO: log de 0 bytes - o guest nao escreveu na COM1" -ForegroundColor Red
    exit 3
}
exit 0