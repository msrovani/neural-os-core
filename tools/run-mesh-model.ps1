# Lab mesh 4-node com modelo específico via QEMU loader
# Uso: .\tools\run-mesh-model.ps1 -Modelo <caminho> [-TempoSeg 120] [-Accel whpx|tcg] [-Force]
# Carrega o modelo em cada nó via -device loader e monitora logs por métricas de mesh.
param(
    [Parameter(Mandatory=$true)]
    [string]$Modelo,          # caminho do arquivo .BIN/.bitnet
    [int]$TempoSeg = 120,     # quanto tempo deixar o mesh rodando
    [ValidateSet("whpx", "tcg")]
    [string]$Accel = "whpx",
    [switch]$Force,
    [string]$Mem = "1G",      # RAM por nó (ajustado p/ caber na RAM do host)
    [int]$NeededGB = 6,       # preflight de RAM (ajustado p/ host com ~7.5 GB livres)
    [switch]$Single           # 1 nó só: valida boot+saúde com o modelo (sem P2P)
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
if (-not (Test-Path (Join-Path $Root "Cargo.toml"))) { $Root = $PSScriptRoot }
Set-Location $Root

. (Join-Path $Root "tools\qemu_mem_preflight.ps1")

$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$logDir = Join-Path $Root "logs"
$nmDir = Join-Path $Root "target"
New-Item -ItemType Directory -Force -Path $logDir, $nmDir | Out-Null

# Validar modelo
if (-not (Test-Path $Modelo)) { Write-Host "[ERRO] modelo não encontrado: $Modelo" -ForegroundColor Red; exit 1 }
$modelName = [System.IO.Path]::GetFileNameWithoutExtension($Modelo)
$modelSizeMB = [math]::Round((Get-Item $Modelo).Length / 1MB, 1)
Write-Host "=== Mesh 4-node com modelo: $modelName ($modelSizeMB MB) ===" -ForegroundColor Cyan

# limpa QEMUs e hub antigos
Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue |
    Stop-Process -Force -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name='python.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -match "qemu_l2_hub" } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep -Seconds 2

$free = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 1)
Write-Host "freeGB=$free (4x2G commit - overcommit esperado)" -ForegroundColor Yellow
Assert-QemuMemFree -NeededGB $NeededGB -Label "mesh-model $modelName" -Force:$Force

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemu)) {
    $alt = Get-Command qemu-system-x86_64 -ErrorAction SilentlyContinue
    if ($alt) { $qemu = $alt.Source } else { Write-Host "[ERRO] QEMU" -ForegroundColor Red; exit 1 }
}
$ovmf = "C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmf)) { $ovmf = Join-Path $Root "target\ovmf.fd" }
$uefi = Join-Path $Root "target\uefi.img"
if (-not (Test-Path $uefi)) { $uefi = Join-Path $Root "target1\uefi.img" }
if (-not (Test-Path $uefi)) { Write-Host "[ERRO] uefi.img ausente" -ForegroundColor Red; exit 1 }
$disk = Join-Path $Root "target\disk_qemu.raw"
if (-not (Test-Path $disk)) { Write-Host "[ERRO] disk_qemu.raw ausente" -ForegroundColor Red; exit 1 }

# 4 nós x 2G/4c - IPs 10.0.3.2..5
$nodes = @(
    @{ Name = "a"; Octet = 2; Mem = $Mem; Smp = 4; Port = 19001 },
    @{ Name = "b"; Octet = 3; Mem = $Mem; Smp = 4; Port = 19002 },
    @{ Name = "c"; Octet = 4; Mem = $Mem; Smp = 4; Port = 19003 },
    @{ Name = "d"; Octet = 5; Mem = $Mem; Smp = 4; Port = 19004 }
)
# -Single: 1 no. O modelo vive em 0x100000000 (4 GiB) e o kernel so varre esse
# endereco -> cada no precisa de >4 GiB de RAM. 4 nos nao cabem num host de
# 15,7 GB; 1 no x 7G cabe e valida boot+saude com o modelo carregado.
if ($Single) { $nodes = @($nodes[0]) }

foreach ($n in $nodes) {
    $flag = Join-Path $nmDir ("netmode_{0}.flag" -f $n.Name)
    [System.IO.File]::WriteAllBytes($flag, [byte[]]@([byte][char]"S", 10, 0, 3, [byte]$n.Octet))
}

$hubOut = Join-Path $logDir "l2_hub_$modelName.txt"
$hubErr = Join-Path $logDir "l2_hub_$modelName.err.txt"
$hub = Start-Process -FilePath "python" -ArgumentList @(
    (Join-Path $Root "tools\qemu_l2_hub.py"), "--port", "19000"
) -WorkingDirectory $Root -WindowStyle Minimized -PassThru `
    -RedirectStandardOutput $hubOut -RedirectStandardError $hubErr
Write-Host "hub pid=$($hub.Id) :19000" -ForegroundColor Green
Start-Sleep -Milliseconds 1500
if ($hub.HasExited) {
    Write-Host "[ERRO] hub L2 saiu exit=$($hub.ExitCode)" -ForegroundColor Red
    Get-Content $hubErr -ErrorAction SilentlyContinue | Select-Object -Last 5
    exit 1
}

$cpu = if ($Accel -eq "whpx") { "Haswell" } else { "qemu64" }
$accelArg = if ($Accel -eq "whpx") { "whpx" } else { "tcg,thread=multi" }

$macs = @{
    a = "52:54:00:AA:00:01"
    b = "52:54:00:BB:00:02"
    c = "52:54:00:CC:00:03"
    d = "52:54:00:DD:00:04"
}

$procs = @()
foreach ($n in $nodes) {
    $log = Join-Path $logDir ("boot_mesh_{0}_{1}.txt" -f $n.Name, $modelName)
    if (Test-Path $log) { Remove-Item $log -Force -ErrorAction SilentlyContinue }
    $flag = Join-Path $nmDir ("netmode_{0}.flag" -f $n.Name)
    $mac = $macs[$n.Name]
    $qa = @(
        "-m", $n.Mem,
        "-smp", "$($n.Smp)",
        "-cpu", $cpu,
        "-accel", $accelArg,
        "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
        "-drive", "format=raw,file=$uefi,if=ide,index=0,snapshot=on",
        "-drive", "format=raw,file=$disk,if=ide,index=1,snapshot=on",
        "-device", "loader,file=$flag,addr=0x2000000",
        "-device", "loader,file=$Modelo,addr=0x100000000",
        "-netdev", "socket,id=n0,udp=127.0.0.1:19000,localaddr=127.0.0.1:$($n.Port)",
        "-device", "e1000,netdev=n0,mac=$mac",
        "-audiodev", "none,id=snd0",
        "-device", "intel-hda,id=hda0",
        "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
        "-device", "qemu-xhci,id=xhci",
        "-device", "usb-tablet,bus=xhci.0",
        "-device", "usb-kbd,bus=xhci.0",
        "-serial", "file:$log",
        "-no-reboot",
        "-name", ("mesh4-{0}-$modelName" -f $n.Name.ToUpper()),
        "-display", "none"
    )
    $p = Start-Process -FilePath $qemu -ArgumentList $qa -PassThru
    Start-Sleep -Seconds 2
    $alive = -not $p.HasExited
    $procs += [pscustomobject]@{
        No  = $n.Name.ToUpper()
        IP    = ("10.0.3.{0}" -f $n.Octet)
        Mem   = $n.Mem
        Smp   = $n.Smp
        Pid   = $p.Id
        Alive = $alive
        Log   = ("boot_mesh_{0}_{1}.txt" -f $n.Name, $modelName)
        Proc  = $p
    }
    Write-Host ("{0} pid={1} {2} smp={3} alive={4} mac={5}" -f $n.Name.ToUpper(), $p.Id, $n.Mem, $n.Smp, $alive, $mac) `
        -ForegroundColor $(if ($alive) { "Green" } else { "Red" })
}

Start-Sleep -Seconds 4
Write-Host ""
Write-Host "=== mesh-4 $Accel com $modelName (4c cada) rodando por $TempoSeg s ===" -ForegroundColor Cyan
$procs | Format-Table -AutoSize -Property Name, IP, Mem, Smp, Pid, Alive

# Aguarda o tempo configurado
Write-Host "Aguardando $TempoSeg segundos..." -ForegroundColor Gray
Start-Sleep -Seconds $TempoSeg

# Para todos os QEMUs
Write-Host "Encerrando QEMUs..." -ForegroundColor Yellow
foreach ($p in $procs) {
    try { if (-not $p.Proc.HasExited) { Stop-Process -Id $p.Pid -Force -ErrorAction SilentlyContinue } } catch {}
}
Stop-Process -Id $hub.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

# Extrai métricas dos logs
Write-Host ""
Write-Host "=== MÉTRICAS POR NÓ ($modelName) ===" -ForegroundColor Cyan
$results = @()
foreach ($p in $procs) {
    $logPath = Join-Path $logDir $p.Log
    $text = Get-Content $logPath -ErrorAction SilentlyContinue | Out-String
    $peers = "?"
    $health = "?"
    $pf = "?"
    $panic = "?"
    $tick = "?"
    $meshHealth = "?"
    if ($text) {
        # Última linha com peers=
        $mPeers = [regex]::Matches($text, "peers=(\d+)") | Select-Object -Last 1
        if ($mPeers) { $peers = $mPeers.Groups[1].Value }
        # Última linha com MESH_HEALTH
        $mHealth = [regex]::Matches($text, "MESH_HEALTH.*?(\{.*?\})") | Select-Object -Last 1
        if ($mHealth) { $health = $mHealth.Groups[1].Value.Substring(0, [math]::Min(80, $mHealth.Groups[1].Value.Length)) }
        # #PF: o veredito e o marcador que o kernel JA emite ("#PF storm"), nunca
        # "#PF > 0" e nunca a contagem crua. Dois motivos medidos:
        #  - 5 de 12 ocorrencias de "#PF" no log sao TEXTO ("patch_idt: ... #PF hook",
        #    "avoid #PF"), nao faults;
        #  - a demo Ring3 P6 emite 1 fault em 0x000070000030000a por design.
        $pf = ([regex]::Matches($text, "\[EXC\] #PF")).Count
        $mStorm = [regex]::Matches($text, "#PF storm") | Measure-Object
        $storm = $mStorm.Count
        $mHalt = [regex]::Matches($text, "RECOVER-HALT") | Measure-Object
        $halt = $mHalt.Count
        # panic: literal [PANIC]. "panic=true" em boot_failure e o registro
        # cross-boot do k_ai SELF lendo o ramlog do boot ANTERIOR (falso positivo).
        $mPanic = [regex]::Matches($text, "\[PANIC\]") | Measure-Object
        $panic = $mPanic.Count
        # tick
        $mTick = [regex]::Matches($text, "tick=(\d+)") | Select-Object -Last 1
        if ($mTick) { $tick = $mTick.Groups[1].Value }
    }
    $results += [pscustomobject]@{
        No      = $p.No
        IP      = $p.IP
        Peers   = $peers
        Tick    = $tick
        PF      = $pf
        Storm   = $storm
        Panic   = $panic
        Health  = $health
        Log     = $p.Log
    }
}
$results | Format-Table -AutoSize -Property No, IP, Peers, Tick, PF, Panic, Health

# Salva resumo
$summaryPath = Join-Path $logDir "mesh_summary_$modelName`_$timestamp.txt"
"=== Mesh 4-node com modelo: $modelName ($modelSizeMB MB) ===" | Out-File $summaryPath -Encoding UTF8
"Data: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")" | Out-File $summaryPath -Append -Encoding UTF8
"Accel: $Accel | Tempo: $TempoSeg s" | Out-File $summaryPath -Append -Encoding UTF8
"" | Out-File $summaryPath -Append -Encoding UTF8
$results | Format-Table -AutoSize | Out-File $summaryPath -Append -Encoding UTF8
Write-Host ""
Write-Host "Resumo salvo em: $summaryPath" -ForegroundColor Green

# Veredito geral
$allOk = $true
foreach ($r in $results) {
    # O kernel emite "#PF storm" / "RECOVER-HALT" quando realmente falha.
    if ($r.Storm -gt 0 -or $r.Panic -gt 0) { $allOk = $false }
}
if ($allOk) {
    Write-Host "VEREDIto: TODOS OS NOS SAUDAVEIS (sem #PF storm, sem [PANIC]) - modelo $modelName OK no mesh" -ForegroundColor Green
} else {
    Write-Host "VEREDIto: ALGUM NO COM #PF STORM OU [PANIC] - revisar logs" -ForegroundColor Red
}
