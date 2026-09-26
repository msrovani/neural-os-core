# Lab LLM: exige RESPOSTA real (decode/done), nao so InferQueue submit.
# Usa LINJ @0x2100000 + Falcon3_1B. Gate = cortex::llm_response_gate (host tests).
#
# Uso:
#   powershell -File tools\lab-llm-response.ps1
#   powershell -File tools\lab-llm-response.ps1 -Phrase "diga ok" -SkipBuild
#
# PASS: prefill_done + (done id= | decode_tok/s= | MSG_DELTA)
# FAIL: submit/prefill sem decode (caso app-clima mesh)

param(
    [int]$RamGB = 6,
    [int]$Smp = 4,
    [ValidateSet('whpx','tcg')][string]$Accel = 'whpx',
    [int]$WaitBootSec = 360,
    [int]$WaitRespSec = 900,
    [switch]$SkipBuild,
    [switch]$NoDisplay,
    [string]$Phrase = 'diga ok'
)

$ErrorActionPreference = 'Stop'
$Root = 'C:\DEV\neural-os-core-latest'
Set-Location $Root
New-Item -ItemType Directory -Force -Path logs, target | Out-Null

$Result = 'logs\lab_llm_response_result.txt'
function W([string]$m) { Add-Content $Result $m -Encoding utf8; Write-Host $m }
Set-Content $Result ("started {0}" -f (Get-Date -Format o)) -Encoding utf8

$freeGB = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 2)
W ("host freeGB={0} RamGB={1} Phrase={2}" -f $freeGB, $RamGB, $Phrase)
if ($RamGB -lt 5) { $RamGB = 5 }  # piso 5G: falcon1B+BPE+PIPER cabem abaixo de 0x163000000 (STT cai OOB — ok p/ prova de token); 6G estoura host com <5GB livres

if (-not $SkipBuild) {
    $env:CARGO_TARGET_DIR = 'target/agent-lab-clima'
    $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    cargo test -p cortex --lib llm_response_gate --release 2>&1 | Tee-Object logs\lab_llm_host_tests.txt | Out-Null
    $ec = $LASTEXITCODE
    cargo build --release -p boot 2>&1 | Tee-Object -Append logs\lab_llm_host_tests.txt | Out-Null
    $ErrorActionPreference = $prev
    if ($ec -ne 0) { throw 'cargo test llm_response_gate FAILED' }
    if ($LASTEXITCODE -ne 0) { throw 'cargo build -p boot FAILED' }
}

Get-Process qemu-system-x86_64 -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue
Start-Sleep 2

$qemu = 'C:\PROGRA~1\qemu\qemu-system-x86_64.exe'
$ovmf = 'C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd'
$uefi = (Resolve-Path 'target\uefi.img').Path
$disk = if (Test-Path 'target1\disk_qemu.raw') { (Resolve-Path 'target1\disk_qemu.raw').Path }
        else { (Resolve-Path 'target\disk_qemu.raw').Path }

# LINJ
$linjPath = 'target\lab_inject_llm_ping.bin'
$tb = [System.Text.Encoding]::ASCII.GetBytes($Phrase)
if ($tb.Length -gt 240) { $tb = $tb[0..239] }
$blob = New-Object byte[] (4 + $tb.Length + 1)
[System.Text.Encoding]::ASCII.GetBytes('LINJ').CopyTo($blob, 0)
$tb.CopyTo($blob, 4)
[System.IO.File]::WriteAllBytes((Join-Path $Root $linjPath), $blob)

$falcon = (Resolve-Path 'target1\FALCON3_1B.V6').Path
$windowEnd = [uint64]$RamGB * 1GB
$loaders = [System.Collections.Generic.List[string]]::new()
function Add-Loader([string]$fp, [uint64]$addr) {
    if (-not (Test-Path $fp)) { return $false }
    $len = [uint64](Get-Item $fp).Length
    if ($addr + $len -gt $script:windowEnd) { Write-Host ("skip OOB {0}" -f $fp); return $false }
    $script:loaders.Add('-device') | Out-Null
    $script:loaders.Add(('loader,file={0},addr=0x{1}' -f $fp, $addr.ToString('X'))) | Out-Null
    Write-Host ('LOADER {0} @0x{1}' -f (Split-Path $fp -Leaf), $addr.ToString('X')) -ForegroundColor Green
    return $true
}
[void](Add-Loader $falcon ([uint64]0x100000000))
# BPE tokenizer (BPB1) — sem ele o decode cai no char-vocab 99 = ruído.
# 0x126000000: FALCON3_1B (570MB) termina em 0x12204A238 e PIPER (63MB) em
# ~0x125DD0000 — BPE vai DEPOIS deles, dentro do scan [0x100000000..0x180000000).
$bpe = Join-Path $Root 'target1\bpe_vocab.bin'
if (-not (Test-Path $bpe)) { $bpe = Join-Path $Root 'models\bpe_vocab.bin' }
if (Test-Path $bpe) { [void](Add-Loader $bpe ([uint64]0x126000000)) } else { Write-Host 'BPE AUSENTE — decode sera char-vocab (lixo)' -ForegroundColor Red }
$piper = Join-Path $Root 'target1\PIPER_PT_BR.BIN'
if (Test-Path $piper) {
    $addr = [uint64]0x122200000
    [void](Add-Loader $piper $addr)
}
$stt = Join-Path $Root 'target1\STT.BIN'
if (Test-Path $stt) { [void](Add-Loader $stt ([uint64]0x163000000)) }

$disp = if ($NoDisplay) { 'none' } else { 'gtk' }
$cpu = if ($Accel -eq 'whpx') { 'Haswell' } else { 'max' }
$accelArg = if ($Accel -eq 'whpx') { 'whpx' } else { 'tcg,thread=multi' }
$log = Join-Path $Root 'logs\lab_llm_response_boot.txt'
if (Test-Path $log) { Remove-Item $log -Force }

$qa = [System.Collections.Generic.List[string]]::new()
foreach ($x in @(
    '-m',("${RamGB}G"),'-smp',("$Smp"),'-cpu',$cpu,'-accel',$accelArg,
    '-drive',("if=pflash,format=raw,file={0},readonly=on" -f $ovmf),
    '-drive',("format=raw,file={0},if=ide,index=0,snapshot=on" -f $uefi),
    '-drive',("format=raw,file={0},if=ide,index=1,snapshot=on" -f $disk)
)) { $qa.Add($x) | Out-Null }
foreach ($x in $loaders) { $qa.Add($x) | Out-Null }
$linjFull = (Resolve-Path $linjPath).Path
foreach ($x in @(
    '-device',("loader,file={0},addr=0x2100000" -f $linjFull),
    '-netdev','user,id=n0','-device','e1000,netdev=n0',
    '-vga','std','-display',$disp,
    '-audiodev','dsound,id=snd0,out.mixing-engine=on,in.mixing-engine=on',
    '-device','intel-hda,id=hda0',
    '-device','hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0',
    '-serial',("file:{0}" -f $log),
    '-no-reboot','-name','lab-llm-response'
)) { $qa.Add($x) | Out-Null }

$argLine = ($qa | ForEach-Object { if ($_ -match '\s') { '"{0}"' -f $_ } else { $_ } }) -join ' '
$p = Start-Process -FilePath $qemu -ArgumentList $argLine -PassThru
Start-Sleep 8
if ($p.HasExited -and $Accel -eq 'whpx') {
    W 'WHPX exit - retry TCG'
    $argLine = $argLine -replace '-accel whpx','-accel tcg,thread=multi' -replace '-cpu Haswell','-cpu max'
    $p = Start-Process -FilePath $qemu -ArgumentList $argLine -PassThru
    Start-Sleep 10
}
if ($p.HasExited) { throw 'QEMU dead' }
W ("QEMU pid={0} alive" -f $p.Id)

# wait llm
$deadline = (Get-Date).AddSeconds($WaitBootSec)
$ready = $false
while ((Get-Date) -lt $deadline) {
    Start-Sleep 6
    if (-not (Get-Process -Id $p.Id -EA SilentlyContinue)) { W 'FAIL: QEMU morreu'; exit 4 }
    if (-not (Test-Path $log)) { continue }
    $sz = [math]::Round((Get-Item $log).Length/1KB)
    $hit = Select-String -Path $log -Pattern 'llm=LOADED' -EA SilentlyContinue | Select-Object -Last 1
    Write-Host ("... boot {0}KB loaded={1}" -f $sz, [bool]$hit)
    if ($hit) { $ready = $true; break }
}
if (-not $ready) { W 'FAIL: llm=LOADED timeout'; exit 2 }

# wait response evidence (full log scan — not Last-N)
W ("wait {0}s for decode/done..." -f $WaitRespSec)
$d2 = (Get-Date).AddSeconds($WaitRespSec)
$ev = @{
    intent=$false; submit=$false; prefill_begin=$false; prefill_done=$false
    decode=$false; done=$false; stream=$false; tts=$false
}
while ((Get-Date) -lt $d2) {
    Start-Sleep 12
    if (-not (Get-Process -Id $p.Id -EA SilentlyContinue)) { W 'WARN: QEMU saiu durante wait'; break }
    $txt = Get-Content $log -Raw -EA SilentlyContinue
    if (-not $txt) { continue }
    if ($txt -match 'inject USER_INTENT \(LINJ\)|Texto:') { $ev.intent = $true }
    if ($txt -match 'InferQueue submit') { $ev.submit = $true }
    if ($txt -match 'prefill_begin') { $ev.prefill_begin = $true }
    if ($txt -match 'prefill_done') { $ev.prefill_done = $true }
    if ($txt -match 'decode_tok/s=') { $ev.decode = $true }
    if ($txt -match 'InferQ.*done id=') { $ev.done = $true }
    if ($txt -match 'MSG_DELTA\|') { $ev.stream = $true }
    if ($txt -match 'TTS Piper|TTS Formant') { $ev.tts = $true }
    $resp = $ev.done -or $ev.decode -or $ev.stream
    Write-Host ("intent={0} submit={1} prefill_done={2} decode={3} done={4} stream={5}" -f `
        $ev.intent,$ev.submit,$ev.prefill_done,$ev.decode,$ev.done,$ev.stream)
    if ($resp -and $ev.prefill_done) { break }
}

W '=== EVIDENCE ==='
Select-String -Path $log -Pattern 'LINJ|Texto:|InferQueue submit|prefill_|decode_tok|InferQ.*done id=|MSG_DELTA|TTS Piper' -EA SilentlyContinue |
    Select-Object -First 30 |
    ForEach-Object { W $_.Line.Substring(0, [Math]::Min(160, $_.Line.Length)) }

$hasResp = $ev.done -or $ev.decode -or $ev.stream
$verdict = if (-not $ev.intent -and -not $ev.submit) {
    'FAIL: sem intent/submit'
} elseif ($ev.submit -and -not $ev.prefill_begin -and -not $hasResp) {
    'FAIL: submit sem prefill'
} elseif ($ev.prefill_begin -and -not $ev.prefill_done -and -not $hasResp) {
    'FAIL: prefill incompleto'
} elseif ($ev.prefill_done -and -not $hasResp) {
    'FAIL: prefill_done sem decode/done (resposta ausente)'
} elseif ($hasResp -and $ev.tts) {
    'PASS: resposta LLM + TTS'
} elseif ($hasResp) {
    'PASS: resposta LLM (decode/done/stream)'
} else {
    'FAIL: submit sem resposta (PASS falso evitavel)'
}
W ("VERDICT: {0}" -f $verdict)
Write-Host $verdict -ForegroundColor $(if ($verdict -match '^PASS') { 'Green' } else { 'Red' })
exit $(if ($verdict -match '^PASS') { 0 } else { 1 })
