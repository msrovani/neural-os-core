# Boot 1 no com o conjunto MAXIMO de HW emulado que o kernel tem driver.
# Video: virtio-gpu + VGA. Storage: ATA(ide) + AHCI + NVMe + virtio-blk.
# Rede: e1000 + virtio-net. USB: xHCI + tablet/kbd + usb-storage (MSC). Audio: HDA.
# Uso: powershell -NoProfile -ExecutionPolicy Bypass -File tools\run-qemu-fullhw.ps1 -TempoSeg 240
# Ficheiro ASCII-only (PS5.1 le UTF-8 sem BOM como CP1252).
param(
    [int]$TempoSeg = 240,
    [ValidateSet("whpx", "tcg")]
    [string]$Accel = "whpx",
    [string]$Mem = "7G",
    [switch]$Force,
    # Duas NICs fazem o net bootstrap prender no smoke de DNS (medido: 6/9 PHASE,
    # 0 ticks, MAC default 02:00:00:00:00:fe em vez da configurada). O caminho
    # canonico e NIC unica (e1000). -VirtioNet re-experimenta a 2a NIC.
    [switch]$VirtioNet
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
if (-not (Test-Path (Join-Path $Root "Cargo.toml"))) { $Root = $PSScriptRoot }
Set-Location $Root
. (Join-Path $Root "tools\qemu_mem_preflight.ps1")

$ts = Get-Date -Format "yyyyMMdd_HHmmss"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$log = Join-Path $logDir "boot_fullhw_$ts.txt"
$arglog = Join-Path $logDir "boot_fullhw_$ts.args.txt"

# Limpa QEMU anterior
Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemu)) { Write-Host "[ERRO] QEMU ausente" -ForegroundColor Red; exit 1 }
$ovmf = "C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmf)) { $ovmf = Join-Path $Root "target\ovmf.fd" }
$uefi = Join-Path $Root "target\uefi.img"
$disk = Join-Path $Root "target\disk_qemu.raw"
foreach ($f in @($ovmf, $uefi, $disk)) {
    if (-not (Test-Path $f)) { Write-Host "[ERRO] ausente: $f" -ForegroundColor Red; exit 1 }
}

$free = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1MB, 1)
Write-Host "freeGB=$free  mem=$Mem" -ForegroundColor Yellow
Assert-QemuMemFree -NeededGB ([double]($Mem -replace "G", "") + 1) -Label "fullhw $Mem" -Force:$Force

$cpu = if ($Accel -eq "whpx") { "Haswell,-rtm,-arat" } else { "qemu64" }
$accelArg = if ($Accel -eq "whpx") { "whpx" } else { "tcg,thread=multi" }

$qa = @(
    "-m", $Mem,
    "-smp", "4",
    "-cpu", $cpu,
    "-accel", $accelArg,
    "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
    "-drive", "format=raw,file=$uefi,if=ide,index=0",
    "-drive", "format=raw,file=$disk,if=ide,index=1",
    # --- STORAGE: AHCI + NVMe + virtio-blk (alem do ATA ja presente) ---
    "-drive", "if=none,id=satadrive,format=raw,file=target/sata.raw",
    "-device", "ahci,id=ahci0",
    "-device", "ide-hd,drive=satadrive,bus=ahci0.0",
    "-drive", "if=none,id=nvmeblk,format=raw,file=target/nvme.raw",
    "-device", "nvme,drive=nvmeblk,serial=neural0",
    "-drive", "if=none,id=virtioblk,format=raw,file=target/virtio.raw",
    "-device", "virtio-blk-pci,drive=virtioblk",
    # --- USB: xHCI + HID + MSC (usb-storage) ---
    "-drive", "if=none,id=usbstick,format=raw,file=target/usbstick.raw",
    "-device", "qemu-xhci,id=xhci",
    "-device", "usb-tablet,bus=xhci.0",
    "-device", "usb-kbd,bus=xhci.0",
    "-device", "usb-storage,bus=xhci.0,drive=usbstick",
    # --- NET: e1000 (canonico, NIC unica) ---
    "-netdev", "user,id=n0",
    "-device", "e1000,netdev=n0,mac=52:54:00:12:34:56",
    # --- VIDEO: virtio-gpu + VGA padrao (GOP) ---
    "-device", "virtio-gpu-pci,id=vgpu",
    "-vga", "std",
    # --- AUDIO: HDA + codec duplex ---
    "-audiodev", "none,id=snd0",
    "-device", "intel-hda,id=hda0",
    "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
    "-serial", "file:$log",
    "-no-reboot",
    "-display", "none",
    "-name", "neural-fullhw"
)

# -VirtioNet: 2a NIC (opt-in; ver o comentário do param). Fora do caminho
# canonico porque prende o net bootstrap no smoke de DNS.
if ($VirtioNet) {
    $qa += @("-netdev", "user,id=n1", "-device", "virtio-net-pci,netdev=n1,mac=52:54:00:12:34:57")
}

# Grava a linha de comando por ciclo (licao SESSION_465: erro de parse nomeia
# a spec rejeitada, nao o que veio junto).
$dump = @("timestamp=" + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), "accel=$Accel", "mem=$Mem", "--- argv ---")
$dump += $qa
Set-Content -LiteralPath $arglog -Value $dump -Encoding ASCII
Write-Host "args -> $arglog" -ForegroundColor DarkGray
Write-Host "log   -> $log" -ForegroundColor DarkGray
Write-Host "Dispositivos: VGA+virtio-gpu, AHCI, NVMe, virtio-blk, e1000, virtio-net, xHCI+tablet/kbd/storage, HDA" -ForegroundColor Cyan
Write-Host "Rodando por $TempoSeg s..." -ForegroundColor Gray

$p = Start-Process -FilePath $qemu -ArgumentList $qa -PassThru
Start-Sleep -Seconds $TempoSeg
if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
Start-Sleep -Seconds 2

Write-Host ""
Write-Host "=== SAUDE ===" -ForegroundColor Cyan
$t = Get-Content $log -Raw -ErrorAction SilentlyContinue
if (-not $t) { Write-Host "[ERRO] log vazio - QEMU nao subiu?" -ForegroundColor Red; exit 1 }
$phases = ([regex]::Matches($t, "=== PHASE")).Count
# Null-safe: um boot que morre antes do SCHED nao tem nenhum tick, e
# `(... | Select-Object -Last 1).Groups[1]` em array vazio mata o script
# (o proprio script virando a evidencia = bug de harness, nao do kernel).
$lastTick = "nenhum"
$mt = [regex]::Matches($t, "tick=(\d+)")
if ($mt.Count -gt 0) { $lastTick = $mt[$mt.Count - 1].Groups[1].Value }
$storm = ([regex]::Matches($t, "#PF storm")).Count
$panic = ([regex]::Matches($t, "\[PANIC\]")).Count
$halt = ([regex]::Matches($t, "RECOVER-HALT")).Count
Write-Host ("PHASE banners : {0}/9" -f $phases)
Write-Host ("ultimo tick   : {0}" -f $lastTick)
Write-Host ("#PF storm     : {0}" -f $storm)
Write-Host ("PANIC         : {0}" -f $panic)
Write-Host ("RECOVER-HALT  : {0}" -f $halt)

Write-Host ""
Write-Host "=== EVIDENCIA POR DEVICE ===" -ForegroundColor Cyan
$probes = [ordered]@{
    "VGA/GOP"     = "VGA|GOP|framebuffer|FB "
    "virtio-gpu"  = "virtio.?gpu|VIRTIO_GPU|VirtIOGPU"
    "AHCI"        = "AHCI|ahci"
    "NVMe"        = "NVMe|nvme"
    "virtio-blk"  = "virtio.?blk|VIRTIO_BLK|VirtIOBlk"
    "e1000"       = "e1000|E1000"
    "virtio-net"  = "virtio.?net|VIRTIO_NET|VirtIONet"
    "xHCI"        = "xHCI|xhci|qemu-xhci"
    "usb-storage" = "usb-storage|MSC|UAS|BOT"
    "HDA audio"   = "HDA|hda-duplex|Intel HDA"
    "ATA/IDE"     = "ATA|slave|IDE"
}
foreach ($k in $probes.Keys) {
    $n = ([regex]::Matches($t, $probes[$k])).Count
    $mark = if ($n -gt 0) { "OK  " } else { "--  " }
    Write-Host ("{0} {1,-12} {2} linhas" -f $mark, $k, $n)
}

Write-Host ""
Write-Host "=== LINHAS DE ENUMERACAO ===" -ForegroundColor Cyan
Select-String -Path $log -Pattern 'NVMe|nvme|AHCI|ahci|virtio.?blk|virtio.?net|virtio.?gpu|xHCI|MSC|usb-storage|HDA|e1000|VGA|GOP' |
    Select-Object -First 30 | ForEach-Object { "  " + $_.Line.Trim() }