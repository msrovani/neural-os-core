# neural-os-core - QEMU KVM + VFIO passthrough GTX 1050 -> guest neural-os
# ADR-0105 B2.x device golden lab (gpu compute deixando de ser CpuOnly/AWAITING_HW)
#
# ============================================================================
# HONESTY (ADR-0105 "Distincoes obrigatorias"):
#   - Passthrough da GTX 1050 (10de:1c81, Pascal sm_61) entrega a GPU REAL ao
#     guest. Isso remove a barreira "QEMU nao emula NVIDIA" - mas NAO e o aceite
#     final: Ready exige pack verified + canario golden no guest (canary.rs).
#   - DEFAULT do script = CpuOnly honesto (sem -GpuReal). QEMU/stub = CpuOnly,
#     nunca vender como aceleracao (ADR-0105 tabela).
#   - NKP unsigned (tools/pack_nkp_lab.ps1 -IncludeSm61) = "D4 estrutural com
#     stub (nunca Ready)" mesmo com GPU real. Assinatura/verified e trabalho
#     separado (ADR-0105 item 3; ed25519-compact ja no workspace).
#   - UEFI GOP: este script NAO seta -vga (vfio-vga e o display do guest).
#     Console serial COM1 = log file (mesma convenccao do run-qemu-whpx.ps1).
#     Se o guest panico no boot sem GOP, use -SerialConsole e leia o log.
#
# ============================================================================
# PREFLIGHT VFIO (host LINUX - WHPX/Windows NAO suporta VFIO; SESSION s410):
#   1. BIOS/UEFI host: VT-d/AMD-Vi ON (SR-IOV/Above-4G se disponivel).
#   2. Kernel cmdline: intel_iommu=on iommu=pt (ou amd_iommu=on iommu=pt).
#      Verificar: dmesg | grep -e DMAR -e IOMMU  -> "IOMMU enabled"
#   3. Grupos IOMMU isolados:
#      for g in /sys/kernel/iommu_groups/*; do
#        echo "IOMMU $g:"; ls -l $g/devices/ | awk '{print $NF}'
#      done
#      A GTX 1050 (GPU 10de:1c81 + audio 10de:10f1) devem ficar num grupo
#      SEM outros devicespci (se PT migrar tudo p/ grupo proprio, ok).
#   4. Bind vfio-pci:
#      echo "10de 1c81" > /sys/bus/pci/drivers/vfio-pci/new_id   (via driverctl ou modprobe)
#      modprobe vfio_pci ids=10de:1c81,10de:10f1
#      (opcional, evita host driver segurar a GPU: blacklist nouveau/nvidia
#       + initramfs; ou `driverctl set-override 0000:01:00.0 vfio-pci`)
#   5. vGPU-NO: GTX 1050 nao tem vGPU; passthrough e all-or-nothing.
#      Host PERDE a GPU durante o guest. Ter display integrado ou SSH.
#   6. ROM BAR: Pascal notebook dGPU geralmente tem ROM legada quebrada.
#      Extrair: # echo 1 > /sys/bus/pci/devices/0000:01:00.0/rom
#               # cat /sys/bus/pci/devices/0000:01:00.0/rom > gtx1050.rom
#               # echo 0 > /sys/bus/pci/devices/0000:01:00.0/rom
#      Passar via -GpuRom gtx1050.rom. Sem ROM estavel, use -NoRom
#      (alguns hosts bootam sem ROM BAR com vfio raw; nem sempre funciona).
#
# ============================================================================
# USO (a partir de um host Linux com QEMU >= 6; roda o .sh ou chama este .ps1
#      via pwsh para gerar a linha de comando):
#   pwsh tools/run-qemu-kvm-vfio.ps1 -DryRun         # so imprime a cmdline
#   pwsh tools/run-qemu-kvm-vfio.ps1 -GpuReal        # VFIO passthrough ON
#   pwsh tools/run-qemu-kvm-vfio.ps1 -GpuReal -GpuRom gtx1050.rom
#   pwsh tools/run-qemu-kvm-vfio.ps1                 # KVM sem passthrough (sanity)
#
# GATES ADR-0105 que o guest aplica sozinho (nada aqui fura):
#   - k_hal::gpu::canary::run_vector_add_canary_nv  -> "vector_add PASS
#     isa=pascal -- has_compute=true" so com pack + golden OK.
#   - k_hal::gpu::kernel_pack::pack_present_on_fat  -> NKP_W2A8_SM61.BIN
#     (tools/pack_nkp_lab.ps1 -IncludeSm61 + mkfat32 NKP_* ja no FAT).
#   - Sem Ready -> CPU ladder (cortex::bitnet_w2a8) = CpuOnly honesto.
# ============================================================================
param(
    [switch]$GpuReal,          # VFIO passthrough da GTX 1050 (host precisa preflight OK)
    [switch]$DryRun,           # imprime a cmdline QEMU e sai
    [string]$GpuRom = "",      # caminho p/ gtx1050.rom (recomendado em notebook dGPU)
    [string]$GpuPci = "01:00.0",   # endereco PCI da GPU no host
    [string]$GpuAudioPci = "01:00.1", # endereco PCI do audio function da GPU
    [string]$GpuVidDid = "10de:1c81", # GTX 1050 (GP108); mudar p/ 1c82 = 1050 Ti
    [string]$GpuAudioVidDid = "10de:10f1",
    [int]$RamGB = 8,           # guest: 4GB VRAM GPU + falcon3 3B + heap
    [int]$Smp = 4,
    [string]$Disk = "",        # default target/disk_qemu.raw
    [switch]$Bridge,           # TAP net (internet real); default user/slirp
    [switch]$SerialConsole     # COM1 no terminal em vez de arquivo
)
$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$logfile = Join-Path $logDir "boot_kvm_vfio_$timestamp.txt"

$uefi = Join-Path $Root "target/uefi.img"
$ovmfCode = Join-Path $Root "target/ovmf_code.fd"
$ovmfVars = Join-Path $Root "target/ovmf_vars.fd"
if ($Disk -eq "") { $Disk = Join-Path $Root "target/disk_qemu.raw" }
$qemu = if ($IsLinux -or $env:OSTYPE -match "linux") { "qemu-system-x86_64" } else { "C:\Program Files\qemu\qemu-system-x86_64.exe" }

foreach ($f in @($uefi, $ovmfCode, $ovmfVars)) {
    if (!(Test-Path $f)) { Write-Host "ERRO: $f ausente. cargo build --release + python tools/build_image.py --bios" -ForegroundColor Red; exit 1 }
}
if (!(Test-Path $Disk)) { Write-Host "AVISO: sem $Disk (FAT32 dos modelos/NKP)" -ForegroundColor Yellow }

## -- Preflight VFIO (apenas checagens passivas; bind e manual por root) ----
if ($GpuReal) {
    Write-Host "=== PREFLIGHT VFIO (host) ===" -ForegroundColor Cyan
    $isLinux = [System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Unix
    if ($isLinux) {
        $iommu = (dmesg 2>/dev/null | Select-String "IOMMU enabled|DMAR: IOMMU enabled").Count -gt 0
        if (-not $iommu) { Write-Host "WARN: IOMMU nao visivel no dmesg. intel_iommu=on iommu=pt?" -ForegroundColor Yellow }
        $vfioDev = "/sys/bus/pci/devices/0000:$GpuPci/driver"
        $bound = (Test-Path $vfioDev) -and ((Get-Item $vfioDev).Target -match "vfio")
        if (-not $bound) {
            Write-Host "WARN: 0000:$GpuPci nao esta bound a vfio-pci." -ForegroundColor Yellow
            Write-Host "  modprobe vfio_pci ids=${GpuVidDid},${GpuAudioVidDid}" -ForegroundColor Gray
            Write-Host "  (ou driverctl set-override 0000:$GpuPci vfio-pci)" -ForegroundColor Gray
        }
        if ($GpuRom -ne "" -and !(Test-Path $GpuRom)) {
            Write-Host "ERRO: -GpuRom '$GpuRom' nao existe (extrair via sysfs rom)" -ForegroundColor Red
            exit 1
        }
    } else {
        Write-Host "ERRO: VFIO e exclusivo de host Linux/KVM." -ForegroundColor Red
        Write-Host "  WHPX (Windows) NAO suporta passthrough de device PCI." -ForegroundColor Red
        Write-Host "  Rodar este script a partir de WSL2 com KVM nested TAMPOUCO funciona" -ForegroundColor Red
        Write-Host "  (WSL2 nao expoe IOMMU). Use host Linux bare-metal." -ForegroundColor Red
        exit 1
    }
    Write-Host "VFIO: 0000:$GpuPci ($GpuVidDid) + 0000:$GpuAudioPci ($GpuAudioVidDid)" -ForegroundColor Green
    if ($GpuRom -ne "") { Write-Host "ROM: $GpuRom" -ForegroundColor Green } else { Write-Host "ROM: none (-GpuRom recomendado p/ Pascal notebook)" -ForegroundColor Yellow }
}

# -- Monta cmdline -----------------------------------------------------------
$a = @(
    "-m", "${RamGB}G", "-smp", "$Smp",
    "-accel", "kvm", "-cpu", "host",
    "-machine", "q35,kernel-irqchip=on"   # q35 p/ PCIe clean no VFIO
)
# ADR-0105: host kernel precisa expor AVX2 ao guest (cpu=host = passthrough).
# bitnet_avx2 gateado por hypervisor = TCG so; com KVM, AVX2 nativo e esperado.
if ($Disk) { $a += @("-drive", "format=raw,file=$Disk,if=none,id=disk0", "-device", "nvme,serial=nvk1,drive=disk0") }
$a += @(
    "-drive", "format=raw,file=$uefi,if=ide,index=0",
    "-drive", "if=pflash,format=raw,file=$ovmfCode,readonly=on",
    "-drive", "if=pflash,format=raw,file=$ovmfVars",
    "-device", "intel-hda,id=hda0",
    "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
    "-audiodev", "none,id=snd0",
    "-device", "qemu-xhci,id=xhci",
    "-device", "usb-tablet,bus=xhci.0",
    "-device", "usb-kbd,bus=xhci.0"
)
# Net: user/slirp (static 10.0.2.15) ou TAP bridge (DHCP)
if ($Bridge) {
    $a += @("-netdev", "tap,id=n0,ifname=tap0,script=no,downscript=no", "-device", "e1000,netdev=n0")
} else {
    $a += @("-netdev", "user,id=n0,hostfwd=tcp::4445-:4445", "-device", "e1000,netdev=n0")
}
# Serial: log file (default) ou console
if ($SerialConsole) { $a += @("-serial", "stdio") } else { $a += @("-serial", "file:$logfile") }
# Display: passthrough deixa a GTX 1050 com o guest; host nao precisa display
if ($GpuReal) {
    $vfioArgs = @(
        "-device", "vfio-pci,host=0000:$GpuPci,id=gpu0,x-vga=on" + $(if ($GpuRom -ne "") { ",romfile=$GpuRom" } else { "" }),
        "-device", "vfio-pci,host=0000:$GpuAudioPci,id=gpu-audio0"
    )
    $a += $vfioArgs
    $a += @("-vga", "none", "-display", "none")   # sem emulacao VGA fantasma
} else {
    $a += @("-vga", "std", "-display", "none")    # sanity KVM sem passthrough
}

if ($DryRun) {
    Write-Host "=== DRY RUN (cmdline QEMU) ===" -ForegroundColor Cyan
    Write-Host ($qemu + " " + ($a -join " "))
    exit 0
}

Write-Host "=== NEURAL-OS-CORE (KVM + $(if ($GpuReal) {'VFIO passthrough GTX 1050'} else {'sem passthrough - CpuOnly sanity'})) ===" -ForegroundColor Cyan
Write-Host "RAM: ${RamGB}G | SMP: $Smp | Disk: $Disk"
if ($GpuReal) { Write-Host "GATE ADR-0105: guest decide Ready vs CPU ladder pelo canario golden. NKP unsigned = nunca Ready." -ForegroundColor Yellow }
Write-Host "Log: $logfile"

try {
    & $qemu @a
} catch {
    Write-Host "ERRO: QEMU falhou: $_" -ForegroundColor Red
    exit 1
}
