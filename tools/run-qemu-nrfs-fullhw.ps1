# Lab QEMU: video + disco NeuralFS + maximo HW emulado (s493).
# ESP = target\uefi.img | DATA = target\disk_qemu.neural.raw (MBR 0x7F)
# Uso:
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\run-qemu-nrfs-fullhw.ps1 -Window
#   powershell -File tools\run-qemu-nrfs-fullhw.ps1 -BuildDisk -Window -AudioBridge
# ASCII-only (PS 5.1).
param(
    [switch]$Window,
    [switch]$BuildDisk,
    [switch]$BuildBoot,
    [switch]$AudioBridge,
    [switch]$Force,
    [switch]$NoLoader,
    [ValidateSet("whpx", "tcg")]
    [string]$Accel = "whpx",
    # FALCON3.V7 ~1210MB @0x100000000 precisa high-RAM >1GB (e820). 6G capava
    # em 1GiB e truncava o parse. 8G = high ~4GB.
    [string]$Mem = "8G",
    [int]$Smp = 4,
    [int]$TempoSeg = 0,
    [int]$DiskSizeMB = 4096,
    [string]$PackLlm = "falcon3"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
if (-not (Test-Path (Join-Path $Root "Cargo.toml"))) { $Root = $PSScriptRoot }
Set-Location $Root
. (Join-Path $Root "tools\qemu_mem_preflight.ps1")

$ts = Get-Date -Format "yyyyMMdd_HHmmss"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$log = Join-Path $logDir "boot_nrfs_fullhw_$ts.txt"
$arglog = Join-Path $logDir "boot_nrfs_fullhw_$ts.args.txt"

Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemu)) { Write-Host "[ERRO] QEMU ausente" -ForegroundColor Red; exit 1 }

$target = Join-Path $Root "target"
New-Item -ItemType Directory -Force -Path $target | Out-Null
$uefi = Join-Path $target "uefi.img"
$nrfs = Join-Path $target "disk_qemu.neural.raw"
$ovmfCode = Join-Path $target "ovmf_code.fd"
$ovmfVars = Join-Path $target "ovmf_vars.fd"
$qemuShare = "C:\Program Files\qemu\share"
if (-not (Test-Path $ovmfCode)) {
    Copy-Item (Join-Path $qemuShare "edk2-x86_64-code.fd") $ovmfCode -Force
}
Copy-Item (Join-Path $qemuShare "edk2-i386-vars.fd") $ovmfVars -Force

if ($BuildBoot -or -not (Test-Path $uefi)) {
    Write-Host "=== cargo build --release -p boot ===" -ForegroundColor Cyan
    $env:CARGO_TARGET_DIR = Join-Path $Root "target"
    cargo build --release -p boot
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $esp = Join-Path $target "limine-esp.img"
    if (Test-Path $esp) { Copy-Item $esp $uefi -Force }
}

function Ensure-StubRaw([string]$Path, [int]$Mb) {
    if (Test-Path $Path) { return }
    $bytes = [long]$Mb * 1024L * 1024L
    $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
    try { $fs.SetLength($bytes) } finally { $fs.Close() }
    Write-Host "[stub] $Path ${Mb}MB" -ForegroundColor DarkGray
}

# Aux disks (AHCI/NVMe/virtio/USB) — vazios; enumera drivers sem payload
Ensure-StubRaw (Join-Path $target "sata.raw") 64
Ensure-StubRaw (Join-Path $target "nvme.raw") 64
Ensure-StubRaw (Join-Path $target "virtio.raw") 64
Ensure-StubRaw (Join-Path $target "usbstick.raw") 64

$needBuild = $BuildDisk -or -not (Test-Path $nrfs) -or ((Get-Item $nrfs).Length -lt (512L * 1024L * 1024L))
if ($needBuild) {
    Write-Host "=== mkneuralfs PACK_LLM=$PackLlm size=${DiskSizeMB}MB ===" -ForegroundColor Cyan
    $env:PACK_LLM = $PackLlm
    $env:CARGO_TARGET_DIR = Join-Path $Root "target\mkneuralfs"
    python (Join-Path $Root "tools\mkneuralfs.py") --size $DiskSizeMB --output $nrfs
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[ERRO] mkneuralfs falhou" -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

foreach ($f in @($uefi, $nrfs, $ovmfCode)) {
    if (-not (Test-Path $f)) { Write-Host "[ERRO] ausente: $f" -ForegroundColor Red; exit 1 }
}

$memGb = [double]($Mem -replace "G", "")
Assert-QemuMemFree -NeededGB ($memGb + 1.5) -Label "nrfs-fullhw $Mem" -Force:$Force

$cpu = if ($Accel -eq "whpx") { "Haswell" } else { "qemu64" }
$accelArg = if ($Accel -eq "whpx") { "whpx" } else { "tcg,thread=multi" }
$audio = if ($AudioBridge) {
    Write-Host "AudioBridge: dsound duplex" -ForegroundColor Green
    "dsound,id=snd0,out.mixing-engine=on,in.mixing-engine=on"
} else {
    "none,id=snd0"
}
$display = if ($Window) { @("-vga", "std", "-display", "gtk") } else { @("-vga", "std", "-display", "none") }

# Active ~1.2GB via ATA PIO sob WHPX aborta (s493). Lab perto de metal =
# QEMU-loader @0x100000000 + NRFS para AlwaysTiny/posture. -NoLoader = so NRFS.
$falconCandidates = @(
    (Join-Path $Root "target1\FALCON3.V7"),
    (Join-Path $Root "target1\FALCON3_BASE.V7"),
    (Join-Path $Root "target\FALCON3.V7")
)
$falcon = $null
foreach ($c in $falconCandidates) {
    if (Test-Path $c) { $falcon = $c; break }
}

$qa = @(
    "-m", $Mem,
    "-smp", "$Smp",
    "-cpu", $cpu,
    "-accel", $accelArg,
    "-drive", "if=pflash,format=raw,file=$ovmfCode,readonly=on",
    "-drive", "if=pflash,format=raw,file=$ovmfVars",
    # Boot ESP + data NeuralFS (IDE master/slave — kernel prefer 0x7F)
    "-drive", "format=raw,file=$uefi,if=ide,index=0",
    "-drive", "format=raw,file=$nrfs,if=ide,index=1",
    # STORAGE extra
    "-drive", "if=none,id=satadrive,format=raw,file=target/sata.raw",
    "-device", "ahci,id=ahci0",
    "-device", "ide-hd,drive=satadrive,bus=ahci0.0",
    "-drive", "if=none,id=nvmeblk,format=raw,file=target/nvme.raw",
    "-device", "nvme,drive=nvmeblk,serial=neural0",
    "-drive", "if=none,id=virtioblk,format=raw,file=target/virtio.raw",
    "-device", "virtio-blk-pci,drive=virtioblk",
    # USB
    "-drive", "if=none,id=usbstick,format=raw,file=target/usbstick.raw",
    "-device", "qemu-xhci,id=xhci",
    "-device", "usb-tablet,bus=xhci.0",
    "-device", "usb-kbd,bus=xhci.0",
    "-device", "usb-storage,bus=xhci.0,drive=usbstick",
    # NET (unica e1000)
    "-netdev", "user,id=n0,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446",
    "-device", "e1000,netdev=n0,mac=52:54:00:12:34:56",
    # VIDEO
    "-device", "virtio-gpu-pci,id=vgpu",
    # AUDIO
    "-audiodev", $audio,
    "-device", "intel-hda,id=hda0",
    "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
    "-serial", "file:$log",
    "-serial", "null",
    "-monitor", "tcp:127.0.0.1:5555,server,nowait",
    "-name", "neural-nrfs-fullhw"
) + $display

if (-not $NoLoader -and $falcon) {
    $qa += @("-device", "loader,file=$falcon,addr=0x100000000")
    $mb = [math]::Round((Get-Item $falcon).Length / 1MB, 0)
    Write-Host ("Active loader: {0} ({1}MB) @0x100000000" -f $falcon, $mb) -ForegroundColor Green
} elseif ($NoLoader) {
    Write-Host "Active loader: OFF (-NoLoader) - NRFS-only niches" -ForegroundColor Yellow
} else {
    Write-Host "Active loader: MISSING FALCON3.V7 in target1/" -ForegroundColor Red
}

Set-Content -LiteralPath $arglog -Value (@(
    "timestamp=" + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"),
    "accel=$Accel mem=$Mem smp=$Smp nrfs=$nrfs",
    "--- argv ---"
) + $qa) -Encoding ASCII

$nrfsMb = [math]::Round((Get-Item $nrfs).Length / 1MB, 0)
Write-Host "=== NEURAL NRFS FULL-HW ===" -ForegroundColor Cyan
Write-Host "ESP : $uefi"
Write-Host ("NRFS: {0} ({1} MB)" -f $nrfs, $nrfsMb)
Write-Host "HW  : VGA+virtio-gpu, IDE+AHCI+NVMe+virtio-blk, e1000, xHCI+HID+MSC, HDA"
Write-Host "log : $log"
Write-Host "args: $arglog"

if ($TempoSeg -gt 0) {
    $p = Start-Process -FilePath $qemu -ArgumentList $qa -PassThru
    Start-Sleep -Seconds $TempoSeg
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    Write-Host "stopped after ${TempoSeg}s"
    if (Test-Path $log) {
        Select-String -Path $log -Pattern "ATA mount LBA|posture data_fs|BOOT SCORE|NeuralFS LOADED|nrfs metal-lab|LLM LOADED|hub status|active=" |
            Select-Object -First 30 | ForEach-Object { $_.Line.Substring(0, [Math]::Min(180, $_.Line.Length)) }
    }
} else {
    & $qemu @qa
    Write-Host "Serial log: $log"
}
