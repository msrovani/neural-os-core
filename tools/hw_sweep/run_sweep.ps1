<#
.SYNOPSIS
  HW Expert v4 runtime identification sweep (ADR-0041 HW-PnP).
  Boots QEMU 3x with ~15-20 PCI devices each (TCG, -NoDisk, model pinned via
  -device loader at 0x179000000), captures serial logs to logs/boot_sweep_N.txt.
  No kernel code is modified; only serial logs are produced.
#>

param(
    [switch]$OnlyBoot,   # e.g. -OnlyBoot 2 to run a single boot
    [int]$Boot = 0
)

$Root = "C:\DEV\neural-os-core"
$target = Join-Path $Root "target"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Path $logDir -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $Root "tools\hw_sweep") -Force | Out-Null

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemu)) { Write-Host "[ERRO] QEMU nao encontrado"; exit 1 }
$ovmf = "C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmf)) { Write-Host "[ERRO] OVMF nao encontrado em $ovmf"; exit 1 }
$uefi = Join-Path $target "uefi.img"
if (-not (Test-Path $uefi)) { Write-Host "[ERRO] uefi.img nao encontrado"; exit 1 }

# Sweep image: HEAD commit + 2 WIP fixes (v5 prefixed loader + SSE lanes clamp),
# built in a temp worktree - the dirty main-tree image triple-faults at boot.
$sweepUefi = "C:\Users\msrov\AppData\Local\Temp\opencode\bisect\target\uefi.img"
if (Test-Path $sweepUefi) { $uefi = $sweepUefi }

$model = Join-Path $Root "models\hw_expert\hw_expert_v4.bitnet"
if (-not (Test-Path $model)) { Write-Host "[ERRO] modelo v4 nao encontrado"; exit 1 }
$modelAddr = 0x179000000  # dentro do scan [0x129400000..0x180000000], alem do fim do LLAMA8B ~0x177B843C6

# Empty drive blob for virtio-blk / ide-hd
$nullImg = Join-Path $Root "tools\target\hw_sweep_null.img"
if (-not (Test-Path $nullImg)) { [System.IO.File]::WriteAllBytes($nullImg, (New-Object byte[] 1048576)) }

$baseArgs = @(
    "-m", "8G",
    "-smp", "2",
    "-cpu", "max",
    "-accel", "tcg",
    "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
    "-drive", "format=raw,file=$uefi,if=ide,index=0",
    "-vga", "std",
    "-display", "none",
    "-no-reboot",
    "-audiodev", "none,id=aud0",
    "-device", "loader,file=$model,addr=0x179000000"
)

# -- Boot 1: network + storage + bridges (bus-0 heavy, bridge traversal) --
$boot1 = @(
    # NICs (10)
    "-netdev", "user,id=n1",  "-device", "e1000,netdev=n1",
    "-netdev", "user,id=n2",  "-device", "e1000-82544gc,netdev=n2",
    "-netdev", "user,id=n3",  "-device", "e1000-82545em,netdev=n3",
    "-netdev", "user,id=n4",  "-device", "e1000e,netdev=n4",
    "-netdev", "user,id=n5",  "-device", "rtl8139,netdev=n5",
    "-netdev", "user,id=n6",  "-device", "ne2k_pci,netdev=n6",
    "-netdev", "user,id=n7",  "-device", "i82559er,netdev=n7",
    "-netdev", "user,id=n8",  "-device", "pcnet,netdev=n8",
    "-netdev", "user,id=n9",  "-device", "vmxnet3,netdev=n9",
    "-netdev", "user,id=n10", "-device", "virtio-net-pci,netdev=n10",
    # Bridges
    "-device", "pci-bridge,chassis_nr=1",
    "-device", "i82801b11-bridge",
    # Storage
    "-device", "ich9-ahci,id=ahci0",
    "-device", "nvme,id=nvme0,serial=hw-sweep-1",
    # Displays (secondary, 2 of 4 - display devices moved out of boot 2:
    # their allocations push SGDB bench-D past the 512MB mapped heap)
    "-device", "qxl",
    "-device", "bochs-display",
    # Behind bridges (traversal test)
    "-netdev", "user,id=n11", "-device", "i82559a,netdev=n11,bus=pci.1",
    "-netdev", "user,id=n12", "-device", "i82559c,netdev=n12,bus=pci.2"
)

# -- Boot 2: virtio family only --
$boot2 = @(
    "-drive", "if=none,id=db,format=raw,file=$nullImg",
    "-device", "virtio-blk-pci,drive=db",
    "-device", "virtio-scsi-pci",
    "-device", "virtio-serial-pci",
    "-device", "virtio-balloon-pci",
    "-device", "virtio-rng-pci",
    "-device", "virtio-gpu-pci",
    "-device", "virtio-vga"
)

# -- Boot 3: audio + usb + misc + displays (2 of 4) --
$boot3 = @(
    "-device", "ich9-intel-hda",
    "-device", "hda-duplex,audiodev=aud0",
    "-device", "intel-hda",
    "-device", "AC97,audiodev=aud0",
    "-device", "ES1370,audiodev=aud0",
    "-device", "qemu-xhci",
    "-device", "nec-usb-xhci",
    "-device", "ich9-usb-ehci1",
    "-device", "ich9-usb-uhci1",
    "-device", "usb-ehci",
    "-device", "piix4-usb-uhci",
    "-netdev", "user,id=nm1", "-device", "i82557b,netdev=nm1",
    "-drive", "if=none,id=did,format=raw,file=$nullImg",
    "-device", "ide-hd,drive=did",
    "-device", "cirrus-vga",
    "-device", "ati-vga"
)

# -- Boot 4: devices that fell past the ~15-card EventBus publish ceiling --
$boot4 = @(
    "-device", "ich9-ahci,id=ahci1",
    "-device", "nvme,id=nvme1,serial=hw-sweep-4",
    "-device", "qxl",
    "-device", "pci-bridge,chassis_nr=4",
    "-device", "i82801b11-bridge",
    "-device", "usb-ehci",
    "-device", "piix4-usb-uhci",
    "-netdev", "user,id=n41", "-device", "i82557b,netdev=n41",
    "-device", "cirrus-vga",
    "-device", "ati-vga"
)

# -- Boot 5 (supplementary): remaining devices past the publish ceiling --
$boot5 = @(
    "-device", "ati-vga",
    "-device", "cirrus-vga",
    "-device", "usb-ehci",
    "-device", "piix4-usb-uhci",
    "-netdev", "user,id=n51", "-device", "i82557b,netdev=n51"
)

function Run-Boot {
    param([int]$N, [string[]]$Devices)
    $log = Join-Path $logDir "boot_sweep_$N.txt"
    if (Test-Path $log) { Remove-Item $log -Force }
    $args = $baseArgs + $Devices + @("-serial", "file:$log")
    Write-Host "[BOOT $N] lancando QEMU (TCG, 8G, $($Devices.Count / 2) -device args)..." -ForegroundColor Cyan
    $p = Start-Process -FilePath $qemu -ArgumentList $args -NoNewWindow -PassThru
    Write-Host "          PID: $($p.Id) log: $log" -ForegroundColor Gray

    $deadline = (Get-Date).AddSeconds(300)
    $cardsSeen = $false
    while ((Get-Date) -lt $deadline -and -not $p.HasExited) {
        Start-Sleep -Seconds 5
        if (Test-Path $log) {
            $c = Get-Content $log -Raw -ErrorAction SilentlyContinue
            if ($c) {
                if ($c -match "v4 multi-head LOADED" -and $c -match "\[HW-PnP\]" -and -not $cardsSeen) {
                    $cardsSeen = $true
                    Write-Host "          [HW-PnP] cards detectados - aguardando 30s de settle" -ForegroundColor Yellow
                }
                if ($cardsSeen) {
                    Start-Sleep -Seconds 25
                    break
                }
            }
        }
    }
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
    Start-Sleep -Seconds 2
    $logSize = 0
    if (Test-Path $log) { $logSize = (Get-Item $log).Length }
    Write-Host "[BOOT $N] encerrado. Log: $log ($logSize bytes)" -ForegroundColor Green
}

if ($OnlyBoot) {
    switch ($Boot) {
        1 { Run-Boot 1 $boot1 }
        2 { Run-Boot 2 $boot2 }
        3 { Run-Boot 3 $boot3 }
        4 { Run-Boot 4 $boot4 }
        5 { Run-Boot 5 $boot5 }
    }
} else {
    Run-Boot 1 $boot1
    Run-Boot 2 $boot2
    Run-Boot 3 $boot3
    Run-Boot 4 $boot4
    Run-Boot 5 $boot5
}
Write-Host "Sweep completo." -ForegroundColor Green
