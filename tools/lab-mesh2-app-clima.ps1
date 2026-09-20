# Lab mesh 2x: video+HDA+artefatos + LINJ "crie uma app de clima e tempo"
# Master (A) recebe inject; mesh TOFU+ROLE; Worker (B) participa FRAG/matmul.
# Aceite: llm=LOADED A+B, TOFU/ROLE, LINJ USER_INTENT, InferQueue, mesh FRAG/role.
#
# Uso:
#   powershell -File tools\lab-mesh2-app-clima.ps1
#   powershell -File tools\lab-mesh2-app-clima.ps1 -SkipBuild -RamGB 5 -Smp 4

param(
    [int]$RamGB = 6,
    [int]$Smp = 4,
    [ValidateSet('whpx','tcg')][string]$Accel = 'whpx',
    [int]$WaitBootSec = 420,
    [int]$WaitMeshSec = 180,
    [int]$WaitInferSec = 420,
    [switch]$SkipBuild,
    [switch]$NoDisplay,
    [string]$Phrase = 'crie uma app de clima e tempo'
)

$ErrorActionPreference = 'Stop'
$Root = 'C:\DEV\neural-os-core-latest'
Set-Location $Root
New-Item -ItemType Directory -Force -Path logs, target | Out-Null

$ResultFile = 'logs\lab_mesh2_app_clima_result.txt'
$PromptFile = 'logs\lab_mesh2_app_clima_prompt.txt'
function Write-Result([string]$msg) {
    Add-Content -Path $ResultFile -Value $msg -Encoding utf8
    Write-Host $msg
}
Set-Content -Path $ResultFile -Value ("started {0}" -f (Get-Date -Format o)) -Encoding utf8

$freeGB = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 2)
Write-Host ("host freeGB={0}" -f $freeGB) -ForegroundColor Yellow
# 2 guests: precisa ~2*RamGB + 2GB; senao desce RamGB (STT@0x163 exige >=6)
$need = (2 * $RamGB) + 2.0
if ($freeGB -lt $need) {
    $alt = [math]::Max(4, [math]::Floor(($freeGB - 2.0) / 2))
    if ($alt -lt 5) {
        Write-Host ("WARN free={0} need~{1} - forcing RamGB=5 overcommit (pagefile)" -f $freeGB, $need) -ForegroundColor Yellow
        $RamGB = 5
    } else {
        Write-Host ("WARN shrink RamGB {0}->{1}" -f $RamGB, $alt) -ForegroundColor Yellow
        $RamGB = [int]$alt
    }
    Write-Result ("AUTO RamGB={0} freeGB={1}" -f $RamGB, $freeGB)
}
if ($Smp -gt 4 -and $freeGB -lt 14) { $Smp = 4 }

@"
LAB: mesh2 app clima/tempo
PHRASE: $Phrase
NODES: A(Master)+B(Worker) RamGB=$RamGB Smp=$Smp Accel=$Accel display=$(if($NoDisplay){'none'}else{'gtk'})
INJECT: LINJ @0x2100000 so no A (pos llm=LOADED)
SPLIT: mesh ROLE + FRAG matmul Worker->Master (ADR-0081)
HOST_FREE_GB: $freeGB
"@ | Set-Content -Path $PromptFile -Encoding utf8

if (-not $SkipBuild) {
    Write-Host 'BUILD boot...' -ForegroundColor Cyan
    $env:CARGO_TARGET_DIR = 'target/agent-lab-clima'
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    cargo build --release -p boot > logs\lab_mesh2_app_build.txt 2>&1
    $ec = $LASTEXITCODE
    $ErrorActionPreference = $prev
    if ($ec -ne 0) { throw 'cargo build -p boot FAILED' }
    $canon = Join-Path $Root 'target\uefi.img'
    $alt = Join-Path $Root 'target\agent-lab-clima\uefi.img'
    if ((Test-Path $alt)) {
        $af = (Resolve-Path $alt).Path
        $cf = if (Test-Path $canon) { (Resolve-Path $canon).Path } else { '' }
        if ($af -ne $cf) { Copy-Item $alt $canon -Force }
    }
}

Get-Process qemu-system-x86_64 -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name='python.exe'" -EA SilentlyContinue |
    Where-Object { $_.CommandLine -match 'qemu_l2_hub' } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -EA SilentlyContinue }
Start-Sleep 3
$freeGB = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 2)
Write-Host ("host freeGB after kill={0}" -f $freeGB) -ForegroundColor Yellow

$qemu = 'C:\PROGRA~1\qemu\qemu-system-x86_64.exe'
if (-not (Test-Path $qemu)) { $qemu = (Get-Command qemu-system-x86_64).Source }
$ovmf = 'C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd'
$uefi = (Resolve-Path 'target\uefi.img').Path
$disk = if (Test-Path 'target1\disk_qemu.raw') { (Resolve-Path 'target1\disk_qemu.raw').Path }
        else { (Resolve-Path 'target\disk_qemu.raw').Path }

[System.IO.File]::WriteAllBytes('target\netmode_a.flag', [byte[]]@([byte][char]'S', 10, 0, 3, 2))
[System.IO.File]::WriteAllBytes('target\netmode_b.flag', [byte[]]@([byte][char]'S', 10, 0, 3, 3))

# LINJ so no Master A
$linjPath = Join-Path $Root 'target\lab_inject_app_clima.bin'
$linjText = [System.Text.Encoding]::ASCII.GetBytes($Phrase)
if ($linjText.Length -gt 240) { $linjText = $linjText[0..239] }
$linjBytes = New-Object byte[] (4 + $linjText.Length + 1)
[System.Text.Encoding]::ASCII.GetBytes('LINJ').CopyTo($linjBytes, 0)
$linjText.CopyTo($linjBytes, 4)
$linjBytes[4 + $linjText.Length] = 0
[System.IO.File]::WriteAllBytes($linjPath, $linjBytes)
Write-Host ("LINJ {0}B: {1}" -f $linjBytes.Length, $Phrase) -ForegroundColor Magenta

# Full artifacts (mesmo pack mesh2-artifacts; OOB skip por RamGB)
$ordered = @(
    'target1\FALCON3_1B.V6',
    'target1\AGENT.v6',
    'target1\RUSTCDR3.BIN',
    'target1\BGE_M3.BIN',
    'target1\LEARNER.v6',
    'target1\PIPER_PT_BR.BIN',
    'target1\VISION.BIN',
    'target1\E5_MULTI.BIN',
    'target1\hw_expert_v6.bitnet',
    'target1\bpe_vocab.bin'
)
$modelGap = [uint64]0x100000
$modelAddr = [uint64]0x100000000
$windowEnd = [uint64]$RamGB * 1GB
$loaders = New-Object System.Collections.Generic.List[string]
foreach ($rel in $ordered) {
    $fp = Join-Path $Root $rel
    if (-not (Test-Path $fp)) { continue }
    $len = [uint64](Get-Item $fp).Length
    if ($len -lt 10240) { continue }
    $needEnd = $modelAddr + $len
    if ($needEnd -gt $windowEnd) {
        Write-Host ("skip OOB {0}" -f $rel) -ForegroundColor Yellow
        continue
    }
    $hex = $modelAddr.ToString('X')
    $loaders.Add('-device') | Out-Null
    $loaders.Add(('loader,file={0},addr=0x{1}' -f $fp, $hex)) | Out-Null
    Write-Host ('LOADER {0} @0x{1}' -f $rel, $hex) -ForegroundColor Green
    $modelAddr = [uint64](([math]::Ceiling([double]($needEnd + $modelGap) / [double]$modelGap)) * [double]$modelGap)
}
$sttFp = Join-Path $Root 'target1\STT.BIN'
$sttAddr = [uint64]0x163000000
if ((Test-Path $sttFp) -and ($sttAddr + (Get-Item $sttFp).Length -le $windowEnd)) {
    $loaders.Add('-device') | Out-Null
    $loaders.Add(('loader,file={0},addr=0x{1}' -f $sttFp, $sttAddr.ToString('X'))) | Out-Null
    Write-Host 'LOADER STT.BIN @0x163000000' -ForegroundColor Green
} else {
    Write-Host 'skip STT (precisa RamGB>=6)' -ForegroundColor Yellow
}

function Start-QemuDirect([string]$qemuExe, [System.Collections.Generic.List[string]]$qa) {
    $argLine = ($qa | ForEach-Object {
        $a = [string]$_
        if ($a -match '\s') { '"{0}"' -f ($a -replace '"','\"') } else { $a }
    }) -join ' '
    return (Start-Process -FilePath $qemuExe -ArgumentList $argLine -PassThru)
}

$hub = Start-Process python -ArgumentList @('tools\qemu_l2_hub.py', '--port', '19000') `
    -WorkingDirectory $Root -WindowStyle Minimized -PassThru `
    -RedirectStandardOutput 'logs\l2_hub.txt' -RedirectStandardError 'logs\l2_hub.err.txt'
Start-Sleep 1
if ($hub.HasExited) { throw 'hub L2 failed' }

$cpu = if ($Accel -eq 'whpx') { 'Haswell' } else { 'max' }
$accelArg = if ($Accel -eq 'whpx') { 'whpx' } else { 'tcg,thread=multi' }
$disp = if ($NoDisplay) { 'none' } else { 'gtk' }

$nodes = @(
    @{ Name = 'a'; Port = 19001; Mac = '52:54:00:AA:00:01'; Flag = 'target\netmode_a.flag'; Mon = 45501; Linj = $true },
    @{ Name = 'b'; Port = 19002; Mac = '52:54:00:BB:00:02'; Flag = 'target\netmode_b.flag'; Mon = 45502; Linj = $false }
)

$pids = @{}
foreach ($n in $nodes) {
    $log = Join-Path $Root ("logs\boot_mesh_{0}.txt" -f $n.Name)
    if (Test-Path $log) { Remove-Item $log -Force }
    $flag = (Resolve-Path $n.Flag).Path
    $qa = New-Object System.Collections.Generic.List[string]
    foreach ($x in @(
        '-m', ("{0}G" -f $RamGB),
        '-smp', ("{0}" -f $Smp),
        '-cpu', $cpu,
        '-accel', $accelArg,
        '-drive', ("if=pflash,format=raw,file={0},readonly=on" -f $ovmf),
        '-drive', ("format=raw,file={0},if=ide,index=0,snapshot=on" -f $uefi),
        '-drive', ("format=raw,file={0},if=ide,index=1,snapshot=on" -f $disk)
    )) { $qa.Add($x) | Out-Null }
    foreach ($x in $loaders) { $qa.Add($x) | Out-Null }
    $qa.Add('-device') | Out-Null
    $qa.Add(('loader,file={0},addr=0x2000000' -f $flag)) | Out-Null
    if ($n.Linj) {
        $qa.Add('-device') | Out-Null
        $qa.Add(('loader,file={0},addr=0x2100000' -f $linjPath)) | Out-Null
    }
    foreach ($x in @(
        '-netdev', ("socket,id=n0,udp=127.0.0.1:19000,localaddr=127.0.0.1:{0}" -f $n.Port),
        '-device', ("e1000,netdev=n0,mac={0}" -f $n.Mac),
        '-vga', 'std',
        '-display', $disp,
        '-audiodev', 'dsound,id=snd0,out.mixing-engine=on,in.mixing-engine=on',
        '-device', 'intel-hda,id=hda0',
        '-device', 'hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0',
        '-device', 'qemu-xhci,id=xhci',
        '-device', 'usb-tablet,bus=xhci.0',
        '-device', 'usb-kbd,bus=xhci.0',
        '-serial', ("file:{0}" -f $log),
        '-monitor', ("tcp:127.0.0.1:{0},server,nowait" -f $n.Mon),
        '-no-reboot',
        '-name', ("mesh-appclima-{0}" -f $n.Name.ToUpper())
    )) { $qa.Add($x) | Out-Null }

    $p = Start-QemuDirect $qemu $qa
    Start-Sleep 8
    if ($p.HasExited -and $Accel -eq 'whpx') {
        Write-Host ("{0} WHPX exit - retry TCG" -f $n.Name) -ForegroundColor Yellow
        for ($i = 0; $i -lt $qa.Count; $i++) {
            if ($qa[$i] -eq '-accel') { $qa[$i + 1] = 'tcg,thread=multi' }
            if ($qa[$i] -eq '-cpu') { $qa[$i + 1] = 'max' }
        }
        $p = Start-QemuDirect $qemu $qa
        Start-Sleep 10
    }
    $alive = -not $p.HasExited
    $pids[$n.Name] = $p.Id
    Write-Host ("{0} pid={1} mon={2} alive={3} linj={4}" -f $n.Name.ToUpper(), $p.Id, $n.Mon, $alive, $n.Linj) `
        -ForegroundColor $(if ($alive) { 'Green' } else { 'Red' })
    if (-not $alive) { throw ("QEMU {0} dead" -f $n.Name) }
}

# --- wait llm on both ---
Write-Host ("wait {0}s llm=LOADED A+B..." -f $WaitBootSec) -ForegroundColor Cyan
$deadline = (Get-Date).AddSeconds($WaitBootSec)
$readyA = $false; $readyB = $false
while ((Get-Date) -lt $deadline) {
    Start-Sleep 8
    foreach ($nm in @('a','b')) {
        if (-not (Get-Process -Id $pids[$nm] -EA SilentlyContinue)) {
            Write-Result ("FAIL: QEMU {0} morreu" -f $nm)
            exit 4
        }
    }
    $logA = 'logs\boot_mesh_a.txt'; $logB = 'logs\boot_mesh_b.txt'
    $szA = if (Test-Path $logA) { [math]::Round((Get-Item $logA).Length/1KB) } else { 0 }
    $szB = if (Test-Path $logB) { [math]::Round((Get-Item $logB).Length/1KB) } else { 0 }
    if (-not $readyA -and (Test-Path $logA)) {
        $h = Select-String -Path $logA -Pattern 'llm=LOADED' -EA SilentlyContinue | Select-Object -Last 1
        $s = Select-String -Path $logA -Pattern 'SCHEDULER|Runtime tick|PHASE 7' -EA SilentlyContinue | Select-Object -Last 1
        if ($h -and $s) { $readyA = $true; Write-Host 'READY A llm=LOADED' -ForegroundColor Green }
    }
    if (-not $readyB -and (Test-Path $logB)) {
        $h = Select-String -Path $logB -Pattern 'llm=LOADED' -EA SilentlyContinue | Select-Object -Last 1
        $s = Select-String -Path $logB -Pattern 'SCHEDULER|Runtime tick|PHASE 7' -EA SilentlyContinue | Select-Object -Last 1
        if ($h -and $s) { $readyB = $true; Write-Host 'READY B llm=LOADED' -ForegroundColor Green }
    }
    Write-Host ("... A={0}KB readyA={1} B={2}KB readyB={3}" -f $szA, $readyA, $szB, $readyB)
    if ($readyA -and $readyB) { break }
}
if (-not $readyA) { Write-Result 'FAIL: A sem llm=LOADED'; exit 2 }
if (-not $readyB) { Write-Result 'WARN: B sem llm=LOADED - mesh split pode degradar' }

# --- mesh TOFU/ROLE ---
Write-Host ("wait {0}s mesh TOFU/ROLE..." -f $WaitMeshSec) -ForegroundColor Cyan
$dMesh = (Get-Date).AddSeconds($WaitMeshSec)
$okTofu = $false; $okRole = $false; $okMaster = $false; $okWorker = $false
while ((Get-Date) -lt $dMesh) {
    Start-Sleep 10
    $pat = 'TOFU|role=|role-assign|Master|Worker|MESH|FRAG|heartbeat|SkillSync'
    $la = @(Select-String -Path 'logs\boot_mesh_a.txt' -Pattern $pat -EA SilentlyContinue | Select-Object -Last 15)
    $lb = @(Select-String -Path 'logs\boot_mesh_b.txt' -Pattern $pat -EA SilentlyContinue | Select-Object -Last 15)
    foreach ($l in ($la + $lb)) {
        $s = $l.Line
        if ($s -match 'TOFU') { $okTofu = $true }
        if ($s -match 'role-assign|role=') { $okRole = $true }
        if ($s -match 'Master') { $okMaster = $true }
        if ($s -match 'Worker|Memory|Compute') { $okWorker = $true }
    }
    Write-Host ("mesh tofu={0} role={1} master={2} workerish={3}" -f $okTofu, $okRole, $okMaster, $okWorker)
    if ($okTofu -and $okRole) { break }
}
Write-Result ("MESH tofu={0} role={1} master={2} workerish={3}" -f $okTofu, $okRole, $okMaster, $okWorker)

# settle pos-LOADED para LINJ (Hermes so consome apos model_is_loaded)
Start-Sleep 25
Write-Result ("ARMED LINJ phrase: {0}" -f $Phrase)

# --- wait intent + infer + mesh FRAG ---
Write-Host ("wait {0}s LINJ/Infer/FRAG..." -f $WaitInferSec) -ForegroundColor Cyan
$d2 = (Get-Date).AddSeconds($WaitInferSec)
$okIntent = $false; $okInfer = $false; $okApp = $false; $okFrag = $false; $okTts = $false
while ((Get-Date) -lt $d2) {
    Start-Sleep 10
    # Scan FULL logs (LINJ ocorre cedo ~T+200; Last-N mentia FAIL)
    $hitIntent = Select-String -Path 'logs\boot_mesh_a.txt' -Pattern 'inject USER_INTENT \(LINJ\).*clima|Texto:.*clima|ENTER — USER_INTENT.*clima' -EA SilentlyContinue | Select-Object -First 1
    $hitInfer = Select-String -Path 'logs\boot_mesh_a.txt' -Pattern 'InferQueue submit' -EA SilentlyContinue | Select-Object -First 1
    $hitApp = Select-String -Path 'logs\boot_mesh_a.txt' -Pattern 'plan for .*clima|REACT.*clima|Generating for:.*clima|Enviando:.*clima' -EA SilentlyContinue | Select-Object -First 1
    $hitFrag = Select-String -Path 'logs\boot_mesh_a.txt','logs\boot_mesh_b.txt' -Pattern 'frag (TX|RX)|matmul (request|resposta)|self-test.*mesh FRAG' -EA SilentlyContinue | Select-Object -First 1
    $hitTts = Select-String -Path 'logs\boot_mesh_a.txt' -Pattern 'TTS Piper|TTS Formant' -EA SilentlyContinue | Select-Object -First 1
    if ($hitIntent) { $okIntent = $true }
    if ($hitInfer) { $okInfer = $true }
    if ($hitApp) { $okApp = $true }
    if ($hitFrag) { $okFrag = $true }
    if ($hitTts) { $okTts = $true }
    Write-Host ("intent={0} infer={1} appish={2} frag={3} tts={4}" -f $okIntent, $okInfer, $okApp, $okFrag, $okTts)
    if ($okIntent -and $okInfer -and $okApp) { break }
}

Write-Result '=== EVIDENCE A (app clima) ==='
Select-String -Path 'logs\boot_mesh_a.txt' -Pattern 'LINJ|Texto:.*clima|Enviando:.*clima|plan for .*clima|REACT.*clima|Generating for:.*clima|InferQueue submit' -EA SilentlyContinue |
    Select-Object -First 15 | ForEach-Object { Write-Result ('A: ' + $_.Line.Substring(0, [Math]::Min(160, $_.Line.Length))) }
Write-Result '=== EVIDENCE B (mesh FRAG) ==='
Select-String -Path 'logs\boot_mesh_b.txt' -Pattern 'frag |matmul |mesh role=|TOFU|role aplicado' -EA SilentlyContinue |
    Select-Object -First 15 | ForEach-Object { Write-Result ('B: ' + $_.Line.Substring(0, [Math]::Min(160, $_.Line.Length))) }

$verdict = if (-not $okIntent) {
    'FAIL: sem USER_INTENT LINJ no Master'
} elseif (-not $okInfer) {
    'FAIL: intent ok mas sem InferQueue'
} elseif ($okFrag) {
    'PASS: app-clima intent + InferQ + mesh FRAG (split Memory/Master)'
} elseif ($okTofu -or $okRole) {
    'PASS_PARTIAL: intent+InferQ+mesh up; FRAG matmul nao observado neste run'
} else {
    'PASS_WEAK: intent+InferQ sem evidência mesh split'
}
Write-Result ("VERDICT: {0}" -f $verdict)
Write-Host $verdict -ForegroundColor $(if ($verdict -match 'PASS') { 'Green' } else { 'Red' })
Write-Host 'QEMU A+B rodando (gtk+HDA). Stop-Process qemu* para parar.' -ForegroundColor Cyan
exit $(if ($verdict -match 'FAIL') { 1 } else { 0 })
