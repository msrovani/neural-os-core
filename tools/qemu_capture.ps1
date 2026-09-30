# tools/qemu_capture.ps1 - Boot QEMU (UEFI/OVMF) com captura:
#   serial -> logs\capture_<ts>.txt   (para ler o log)
#   screendump -> frames\frame_NNNN.png  (para montar o video/GIF)
#
# Uso:
#   .\tools\qemu_capture.ps1 -RamGB 4 -Smp 8 -Seconds 150 -Fps 2
#   .\tools\qemu_capture.ps1 -Tcg -RamGB 4 -Smp 8
#
# Hardware "detectavel" (default): e1000 + xhci(usb-tablet/kbd) + intel-hda +
# virtio-gpu-pci + disco ATA FAT32 (disk_qemu.raw). Nada de modelo loader
# (nao precisamos de LLM para validar card/HUD).
param(
    [int]$RamGB = 4,
    [int]$Smp = 8,
    [int]$Seconds = 150,
    [double]$Fps = 2.0,
    [switch]$Tcg,
    [switch]$VirtioGpu,
    [switch]$Window,
    [int]$MonitorPort = 5555,
    [switch]$NoDisk
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$ts = Get-Date -Format "yyyyMMdd_HHmmss"
$logDir = Join-Path $Root "logs"
$framesDir = Join-Path $logDir "capture_$ts"
New-Item -ItemType Directory -Force -Path $logDir, $framesDir | Out-Null
$logfile = Join-Path $logDir "capture_$ts.txt"

$uefi = Join-Path $Root "target\uefi.img"
$disk = Join-Path $Root "target\disk_qemu.raw"
$ovmfCode = Join-Path $Root "target\ovmf_code.fd"
$ovmfVars = Join-Path $Root "target\ovmf_vars.fd"
foreach ($f in @($uefi, $ovmfCode, $ovmfVars, $qemu)) {
    if (!(Test-Path $f)) { Write-Host "ERRO: ausente $f" -ForegroundColor Red; exit 1 }
}

$acc = if ($Tcg) { "tcg" } else { "whpx" }
$cpu = if ($Tcg) { "max" } else { "host" }

$a = @(
    "-m", "${RamGB}G", "-smp", "$Smp",
    "-accel", $acc, "-cpu", $cpu,
    "-drive", "format=raw,file=$uefi,if=ide,index=0"
)
if (!$NoDisk -and (Test-Path $disk)) {
    $a += @("-drive", "format=raw,file=$disk,if=ide,index=1")
}
$a += @(
    "-drive", "if=pflash,format=raw,file=$ovmfCode,readonly=on",
    "-drive", "if=pflash,format=raw,file=$ovmfVars",
    "-serial", "file:$logfile",
    "-serial", "null",
    "-netdev", "user,id=n0",
    "-device", "e1000,netdev=n0",
    "-audiodev", "none,id=snd0",
    "-device", "intel-hda,id=hda0",
    "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
    "-device", "qemu-xhci,id=xhci",
    "-device", "usb-tablet,bus=xhci.0",
    "-device", "usb-kbd,bus=xhci.0",
    "-device", "virtio-gpu-pci,id=vgpu",
    "-vga", "std",
    "-monitor", "tcp:127.0.0.1:$MonitorPort,server,nowait",
    "-pidfile", (Join-Path $framesDir "qemu.pid")
)
if ($Window) { $a += @("-display", "gtk") } else { $a += @("-display", "none") }

Write-Host "QEMU: -m ${RamGB}G -smp $Smp accel=$acc cpu=$cpu disk=$(!$NoDisk)" -ForegroundColor Gray
Write-Host "log   : $logfile" -ForegroundColor Gray
Write-Host "frames: $framesDir" -ForegroundColor Gray

$proc = Start-Process -FilePath $qemu -ArgumentList $a -PassThru -WindowStyle Hidden

# Espera o monitor subir (QEMU abre o socket no boot).
$client = $null
for ($i = 0; $i -lt 40; $i++) {
    Start-Sleep -Milliseconds 250
    try {
        $client = New-Object System.Net.Sockets.TcpClient
        $client.Connect("127.0.0.1", $MonitorPort)
        break
    } catch {
        $client = $null
        if ($proc.HasExited) { Write-Host "QEMU saiu (exit=$($proc.ExitCode))" -ForegroundColor Red; break }
    }
}
if ($null -eq $client) {
    Write-Host "ERRO: monitor nao conectou em :$MonitorPort" -ForegroundColor Red
    if (!$proc.HasExited) { Stop-Process -Id $proc.Id -Force }
    Get-Content $logfile -ErrorAction SilentlyContinue | Select-Object -Last 30
    exit 2
}

$writer = New-Object System.IO.StreamWriter($client.GetStream())
$writer.AutoFlush = $true
Start-Sleep -Milliseconds 400

$n = 0
$deadline = (Get-Date).AddSeconds($Seconds)
while ((Get-Date) -lt $deadline) {
    if ($proc.HasExited) { Write-Host "QEMU saiu durante a captura (exit=$($proc.ExitCode))" -ForegroundColor Yellow; break }
    $n++
    # QEMU monitor: usar '/' (backslash vira escape e o arquivo nao e escrito).
    $frame = (Join-Path $framesDir ("frame_{0:d4}.png" -f $n)).Replace('\', '/')
    try { $writer.WriteLine("screendump $frame") } catch { break }
    Start-Sleep -Milliseconds ([int](1000 / $Fps))
}
try { $client.Close() } catch {}

if (!$proc.HasExited) { Stop-Process -Id $proc.Id -Force; Start-Sleep -Milliseconds 500 }

$frames = @(Get-ChildItem $framesDir -Filter "frame_*.png" -ErrorAction SilentlyContinue)
Write-Host ""
Write-Host "OK: $($frames.Count) frames em $framesDir" -ForegroundColor Green
Write-Host "log: $logfile" -ForegroundColor Green
Write-Host "framesDir=$framesDir"
Write-Host "logfile=$logfile"
