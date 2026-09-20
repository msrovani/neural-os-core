# Dual mesh lab: 2x (8G/6c) + ALL QEMU-loader artifacts + HDA + video
# WHPX sandbox: models via -device loader (FAT PIO skipped). Window phys 4G..RamGB.
param(
    [int]$RamGB = 8,
    [int]$Smp = 6,
    [ValidateSet('whpx','tcg')][string]$Accel = 'whpx'
)
$ErrorActionPreference = 'Stop'
$Root = 'C:\DEV\neural-os-core-latest'
Set-Location $Root

$free = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 1)
Write-Host ("WARN freeGB={0} - guests 2x{1}G overcommit; models via loader" -f $free, $RamGB) -ForegroundColor Yellow

Get-Process qemu-system-x86_64 -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name='python.exe'" -EA SilentlyContinue |
    Where-Object { $_.CommandLine -match 'qemu_l2_hub' } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -EA SilentlyContinue }
Start-Sleep 2

$qemu = 'C:\Program Files\qemu\qemu-system-x86_64.exe'
if (-not (Test-Path $qemu)) { $qemu = (Get-Command qemu-system-x86_64).Source }
$ovmf = 'C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd'
$uefi = (Resolve-Path 'target\uefi.img').Path
$disk = if (Test-Path 'target1\disk_qemu.raw') { (Resolve-Path 'target1\disk_qemu.raw').Path }
        else { (Resolve-Path 'target\disk_qemu.raw').Path }

New-Item -ItemType Directory -Force -Path logs, target | Out-Null
[System.IO.File]::WriteAllBytes('target\netmode_a.flag', [byte[]]@([byte][char]'S', 10, 0, 3, 2))
[System.IO.File]::WriteAllBytes('target\netmode_b.flag', [byte[]]@([byte][char]'S', 10, 0, 3, 3))

# Fit in guest phys window [4GiB .. RamGB).
# FALCON3.BIN neste tree = ~2GB (7B) — gate v6 recusa (2001+256 > headroom~2024).
# Lab mesh: 1B primeiro (cabe + InferQ vivo); 7B so se sobrar janela.
# STT.BIN pinado em 0x163000000 (loader canônico jarbas::stt) — fora do pack sequencial.
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
    # FALCON3.BIN (~2GB/7B) omitido: gate v6 + OOM na janela bump; use 1B acima.
    # STT.BIN: ver pin abaixo @0x163000000
)

$modelGap = [uint64]0x100000
$modelAddr = [uint64]0x100000000
$modelEnd = $modelAddr
$windowEnd = [uint64]$RamGB * 1GB
$loaders = New-Object System.Collections.Generic.List[string]
$loaded = New-Object System.Collections.Generic.List[string]
foreach ($rel in $ordered) {
    $fp = Join-Path $Root $rel
    if (-not (Test-Path $fp)) {
        Write-Host ("skip missing {0}" -f $rel) -ForegroundColor DarkYellow
        continue
    }
    $len = [uint64](Get-Item $fp).Length
    if ($len -lt 10240) {
        Write-Host ("skip tiny {0} ({1})" -f $rel, $len) -ForegroundColor DarkYellow
        continue
    }
    $need = $modelAddr + $len
    if ($need -gt $windowEnd) {
        Write-Host ('skip OOB {0} needEnd=0x{1} windowEnd=0x{2}' -f $rel, ([uint64]$need).ToString('X'), $windowEnd.ToString('X')) -ForegroundColor Yellow
        continue
    }
    $hex = $modelAddr.ToString('X')
    $loaders.Add('-device') | Out-Null
    $loaders.Add(('loader,file={0},addr=0x{1}' -f $fp, $hex)) | Out-Null
    $mb = [math]::Round($len / 1MB, 1)
    Write-Host ('LOADER {0} ({1} MB) @0x{2}' -f $rel, $mb, $hex) -ForegroundColor Green
    $loaded.Add($rel) | Out-Null
    if ($need -gt $modelEnd) { $modelEnd = $need }
    # align to modelGap; keep uint64 (Ceiling returns Double — breaks :X format)
    $modelAddr = [uint64](([math]::Ceiling([double]($need + $modelGap) / [double]$modelGap)) * [double]$modelGap)
}

# STT CTC canônico @0x163000000 (scan+pin; evita colisão com pack sequencial 4G+)
$sttFp = Join-Path $Root 'target1\STT.BIN'
$sttAddr = [uint64]0x163000000
if (Test-Path $sttFp) {
    $sttLen = [uint64](Get-Item $sttFp).Length
    if ($sttAddr + $sttLen -le $windowEnd) {
        $loaders.Add('-device') | Out-Null
        $loaders.Add(('loader,file={0},addr=0x{1}' -f $sttFp, $sttAddr.ToString('X'))) | Out-Null
        $loaded.Add('target1\STT.BIN@0x163000000') | Out-Null
        Write-Host ('LOADER STT.BIN ({0} KB) @0x163000000' -f [math]::Round($sttLen/1KB)) -ForegroundColor Green
        if ($sttAddr + $sttLen -gt $modelEnd) { $modelEnd = $sttAddr + $sttLen }
    } else {
        Write-Host 'skip STT.BIN OOB @0x163000000' -ForegroundColor Yellow
    }
} else {
    Write-Host 'skip missing target1\STT.BIN' -ForegroundColor DarkYellow
}

Write-Host ('loaded_count={0} end=0x{1}' -f $loaded.Count, $modelEnd.ToString('X')) -ForegroundColor Cyan

$hub = Start-Process python -ArgumentList @('tools\qemu_l2_hub.py', '--port', '19000') `
    -WorkingDirectory $Root -WindowStyle Minimized -PassThru `
    -RedirectStandardOutput 'logs\l2_hub.txt' -RedirectStandardError 'logs\l2_hub.err.txt'
Start-Sleep 1
if ($hub.HasExited) { throw 'hub L2 failed - see logs/l2_hub.err.txt' }
Write-Host ("hub pid={0}" -f $hub.Id) -ForegroundColor Green

$cpu = if ($Accel -eq 'whpx') { 'Haswell' } else { 'max' }
$accelArg = if ($Accel -eq 'whpx') { 'whpx' } else { 'tcg,thread=multi' }

$nodes = @(
    @{ Name = 'a'; Port = 19001; Mac = '52:54:00:AA:00:01'; Flag = 'target\netmode_a.flag' },
    @{ Name = 'b'; Port = 19002; Mac = '52:54:00:BB:00:02'; Flag = 'target\netmode_b.flag' }
)

foreach ($n in $nodes) {
    $log = Join-Path $Root ("logs\boot_mesh_{0}.txt" -f $n.Name)
    if (Test-Path $log) { Remove-Item $log -Force }
    $flag = (Resolve-Path $n.Flag).Path
    # Canonical netmode addr (hermes::detect_qemu_net_mode). Nao colocar apos
    # modelos em 4G+ — candidatos 0x110..0x160 leem bytes do .v6 e viram BRIDGE.
    $nmHex = '2000000'
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
    foreach ($x in @(
        '-device', ('loader,file={0},addr=0x{1}' -f $flag, $nmHex),
        '-netdev', ("socket,id=n0,udp=127.0.0.1:19000,localaddr=127.0.0.1:{0}" -f $n.Port),
        '-device', ("e1000,netdev=n0,mac={0}" -f $n.Mac),
        '-vga', 'std',
        '-display', 'gtk',
        '-audiodev', 'dsound,id=snd0,out.mixing-engine=on,in.mixing-engine=on',
        '-device', 'intel-hda,id=hda0',
        '-device', 'hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0',
        '-device', 'qemu-xhci,id=xhci',
        '-device', 'usb-tablet,bus=xhci.0',
        '-device', 'usb-kbd,bus=xhci.0',
        '-serial', ("file:{0}" -f $log),
        '-no-reboot',
        '-name', ("mesh2-art-{0}" -f $n.Name.ToUpper())
    )) { $qa.Add($x) | Out-Null }

    $p = Start-Process $qemu -ArgumentList $qa.ToArray() -PassThru
    Start-Sleep 5
    if ($p.HasExited -and $Accel -eq 'whpx') {
        Write-Host ("{0} WHPX exit={1} - retry TCG" -f $n.Name, $p.ExitCode) -ForegroundColor Yellow
        $arr = $qa.ToArray()
        for ($i = 0; $i -lt $arr.Length; $i++) {
            if ($arr[$i] -eq '-accel') { $arr[$i + 1] = 'tcg,thread=multi' }
            if ($arr[$i] -eq '-cpu') { $arr[$i + 1] = 'max' }
        }
        $p = Start-Process $qemu -ArgumentList $arr -PassThru
        Start-Sleep 5
    }
    $alive = -not $p.HasExited
    Write-Host ("{0} pid={1} alive={2}" -f $n.Name.ToUpper(), $p.Id, $alive) `
        -ForegroundColor $(if ($alive) { 'Green' } else { 'Red' })
}

Write-Host 'waiting 180s for boot+loader scan...' -ForegroundColor Cyan
Start-Sleep 180
$qc = @(Get-Process qemu-system-x86_64 -EA SilentlyContinue).Count
$free2 = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 1)
Write-Host ("qemu={0} freeGB={1}" -f $qc, $free2)

foreach ($name in @('a', 'b')) {
    $log = Join-Path $Root ("logs\boot_mesh_{0}.txt" -f $name)
    if (-not (Test-Path $log)) {
        Write-Host ("{0}: no log" -f $name)
        continue
    }
    $len = (Get-Item $log).Length
    Write-Host ("=== {0} {1}B ===" -f $name, $len) -ForegroundColor Cyan
    Select-String -Path $log -Pattern 'LLM magic|model OK|llm=|ABSENT|BGE |PIPER|Piper|HWEXPERT|RUSTCODER|ROUTER|AirLLM|Asset|Status|AIOS adapt|skip Infer|LOADED|layers=' |
        Select-Object -Last 30 |
        ForEach-Object { $_.Line.Substring(0, [Math]::Min(170, $_.Line.Length)) }
}
