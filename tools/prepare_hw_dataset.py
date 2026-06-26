#!/usr/bin/env python3
"""
Hardware Knowledge Dataset Generator
Creates training pairs from PCI ID database + kernel's pci.rs.
Output: JSON lines file for transformer training.

Usage:
    python prepare_hw_dataset.py
    python prepare_hw_dataset.py --pci-ids pci.ids --output hw_knowledge.jsonl
"""

import json
import re
import argparse
import urllib.request
import os
from pathlib import Path

PCI_IDS_URL = "https://raw.githubusercontent.com/pciutils/pciids/master/pci.ids"

VENDOR_EXAMPLES = [
    ("8086", "Intel Corporation — fabricante de processadores e chipsets"),
    ("10EC", "Realtek Semiconductor — fabricante de controladoras de rede e audio"),
    ("1022", "AMD — fabricante de processadores e GPUs"),
    ("1AF4", "Red Hat / QEMU — dispositivos VirtIO virtualizados"),
    ("1234", "QEMU — dispositivos graphic virtualizados"),
    ("8086 1237", "Intel 82441FX PMC — Host Bridge, class 06/00"),
    ("8086 7000", "Intel PIIX4 — ISA Bridge, class 06/01"),
    ("1234 1111", "QEMU Virtual VGA — VGA controller, class 03/00"),
    ("10EC 8139", "Realtek RTL8139 — Fast Ethernet controller, class 02/00"),
    ("1AF4 1041", "VirtIO-net — network device virtualizado"),
    ("1AF4 1050", "VirtIO-gpu — graphics device virtualizado"),
]

CLASS_EXAMPLES = [
    ("class 00", "Unclassified device"),
    ("class 01", "Mass storage controller"),
    ("class 02", "Network controller"),
    ("class 03", "Display controller"),
    ("class 0300", "VGA compatible controller"),
    ("class 06", "Bridge device"),
    ("class 0600", "Host bridge"),
    ("class 0601", "ISA bridge"),
    ("class 0C", "Serial bus controller"),
    ("class 0C03", "USB controller"),
    ("class 0C0330", "xHCI USB 3.0 controller"),
    ("class 04", "Audio device"),
    ("class 0108", "NVMe controller"),
]

SYSTEM_EXAMPLES = [
    ("hardware detectado", "PCI scan inicializado"),
    ("tem placa de video", "QEMU Virtual VGA detectado"),
    ("dispositivo rede", "Realtek RTL8139 — controladora Ethernet"),
    ("quantos pci", "4 dispositivos PCI encontrados no scan"),
    ("o que e 00:03.00", "Funcao 0 do dispositivo 3 no barramento 0 — geralmente placa de rede"),
    ("dispositivo 10EC:8139", "Realtek RTL8139 Fast Ethernet — classe 02/00 (rede)"),
    ("preciso de internet", "Ativar RTL8139 → smoltcp → DHCP → online"),
    ("como configurar rede", "init RTL8139 → smoltcp poll → DHCP em 10.0.2.3"),
    ("que cpu", "x86-64, QEMU virtual, 4 cores via SMP"),
    ("mostre hardware", "PCI scan: bridge ISA, bridge host, VGA, RTL8139"),
    ("tem usb", "USB controller pode estar presente via PCI class 0C03"),
    ("o que e BAR0", "Endereco base do dispositivo no espaco de memoria/IO PCI"),
]

def download_pci_ids(target: str):
    """Download pci.ids from the official repository."""
    print(f"[DOWNLOAD] Baixando PCI ID database de {PCI_IDS_URL}...")
    try:
        urllib.request.urlretrieve(PCI_IDS_URL, target)
        print(f"[DOWNLOAD] Salvo em {target}")
        return True
    except Exception as e:
        print(f"[DOWNLOAD] Falhou ({e}), usando exemplos embutidos")
        return False

def parse_pci_ids(path: str) -> list:
    """Parse pci.ids into (vendor, device, description) tuples."""
    entries = []
    current_vendor = None
    try:
        with open(path, 'r', encoding='utf-8', errors='ignore') as f:
            for line in f:
                if line.startswith('#'):
                    continue
                line = line.rstrip()
                if not line:
                    continue
                m = re.match(r'^([0-9a-fA-F]{4})\s+(.+)$', line)
                if m:
                    current_vendor = m.group(1).lower()
                    desc = m.group(2)
                    entries.append((f"{current_vendor}", "", desc))
                    continue
                m = re.match(r'^\t([0-9a-fA-F]{4})\s+(.+)$', line)
                if m and current_vendor:
                    device = m.group(1).lower()
                    desc = m.group(2)
                    entries.append((current_vendor, device, desc))
    except FileNotFoundError:
        pass
    return entries

def generate_jsonl(entries: list, output: str):
    """Generate training pairs in JSONL format."""
    count = 0
    with open(output, 'w', encoding='utf-8') as f:
        # Vendor-level knowledge
        for vid, device, desc in entries:
            if not device:
                inp = f"vendor {vid}"
                out = desc
            else:
                inp = f"{vid} {device}"
                out = desc
            f.write(json.dumps({"input": inp, "output": out}) + '\n')
            count += 1

        # Class knowledge
        for inp, out in CLASS_EXAMPLES:
            f.write(json.dumps({"input": inp, "output": out}) + '\n')
            count += 1

        # System knowledge
        for inp, out in SYSTEM_EXAMPLES:
            f.write(json.dumps({"input": inp, "output": out}) + '\n')
            count += 1

        # Hardware query patterns
        for vid, device, desc in entries:
            if not device:
                for prefix in ["o que e", "identifique", "fale sobre"]:
                    inp = f"{prefix} vendor {vid}"
                    f.write(json.dumps({"input": inp, "output": f"Vendor {vid}: {desc}"}) + '\n')
                    count += 1

    print(f"[DATASET] Gerados {count} pares de treino em {output}")
    return count

def parse_usb_ids(path: str) -> list:
    """Parse usb.ids into (vendor, device, description) tuples."""
    entries = []
    current_vendor = None
    try:
        with open(path, 'r', encoding='utf-8', errors='ignore') as f:
            for line in f:
                if line.startswith('#'):
                    continue
                line = line.rstrip()
                if not line:
                    continue
                m = re.match(r'^([0-9a-fA-F]{4})\s+(.+)$', line)
                if m:
                    current_vendor = m.group(1).lower()
                    desc = m.group(2)
                    entries.append((f"{current_vendor}", "", f"USB: {desc}"))
                    continue
                m = re.match(r'^\t([0-9a-fA-F]{4})\s+(.+)$', line)
                if m and current_vendor:
                    device = m.group(1).lower()
                    desc = m.group(2)
                    entries.append((current_vendor, device, f"USB: {desc}"))
    except FileNotFoundError:
        pass
    return entries

USB_SPEED_DATA = [
    ("usb speed 1", "Low Speed (1.5 Mbps) — teclados, mouses, dispositivos simples"),
    ("usb speed 2", "Full Speed (12 Mbps) — joysticks, hubs, audio USB 1.x"),
    ("usb speed 3", "High Speed (480 Mbps) — pendrives USB 2.0, cameras, HDs externos"),
    ("usb speed 4", "Super Speed (5 Gbps) — USB 3.0: pendrives rapidos, SSDs, hubs"),
    ("usb speed 5", "Super Speed+ (10 Gbps) — USB 3.1 Gen 2: dispositivos de alto desempenho"),
    ("xHCI controller", "eXtensible Host Controller Interface — controlador USB 3.0 padrao"),
    ("xHCI 1033:0194", "NEC uPD720200 — controlador xHCI USB 3.0, 2 portas, 64 slots"),
    ("quantas portas usb", "xHCI reporta ate 64 portas virtuais, 2-4 fisicas tipicamente"),
    ("que velocidades usb", "USB suporta Low (1.5M), Full (12M), High (480M), Super (5G), Super+ (10G)"),
    ("o que e um hub usb", "Hub USB — expansor de portas, repete o sinal para varios dispositivos"),
    ("como conectar usb", "USB e plug-and-play: o controlador xHCI detecta automaticamente"),
    ("dispositivo velocidade baixa", "USB Low Speed — teclado ou mouse tipicamente"),
    ("armazenamento usb", "USB Mass Storage — pendrives, HDs externos, SSDs portateis"),
    ("USB identificado", "Dispositivo USB conectado — LLM pode identifica-lo por VID/PID"),
]

CAPABILITIES_DATA = [
    ("USB class 08", "Mass Storage: armazenamento de dados. Capabilities: leitura, escrita, backup. MHI: HDD ou NVMe. Driver: padrao USB."),
    ("USB class 0E", "Video: captura de imagem. Capabilities: camera, video_capture. MHI: Dram. Driver: padrao UVC."),
    ("USB class 01", "Audio: reproducao/gravacao. Capabilities: speaker, mic. MHI: Dram. Driver: padrao UAC."),
    ("USB class 03", "HID: interface humana. Capabilities: input, keystrokes. MHI: Dram. Driver: padrao HID."),
    ("USB class 02", "Communications: modem/rede. Capabilities: network, serial. MHI: Dram."),
    ("USB class 09", "Hub: expansao de portas. Capabilities: port_multiplier. MHI: pass-through."),
    ("USB class E0", "Wireless: Bluetooth/WiFi. Capabilities: wireless, ble. MHI: Dram."),
    ("PCI class 02", "Rede: Ethernet. Capabilities: network_tx, network_rx, dma. Skills: smoltcp, http_get. MHI: Dram."),
    ("PCI class 01", "Armazenamento: controladora de disco. Capabilities: block_io, dma. MHI: NVMe ou HDD."),
    ("PCI class 0108", "NVMe: SSD ultra-rapido. Capabilities: fast_io. MHI: Nvme tier 2."),
    ("PCI class 03", "Video: GPU/VGA. Capabilities: framebuffer, compute. MHI: Vram tier 0."),
    ("PCI class 0C03", "USB: controladora USB. Capabilities: usb_host. MHI: Dram."),
    ("PCI class 04", "Audio: placa de som. Capabilities: playback, capture. MHI: Dram."),
    ("PCI class 06", "Bridge: conexao barramentos. Capabilities: pci_bridge. MHI: infraestrutura."),
    ("o que fazer com usb storage", "Montar volume, MHI HDD, file_manager backup logs."),
    ("o que fazer com camera usb", "Streaming UVC, deteccao objetos via MLP."),
    ("o que fazer com gpu", "Framebuffer + gpu_compute para inferencia, Vram no MHI."),
    ("o que fazer com placa de rede", "smoltcp TCP/IP, HTTP, network_agent health check."),
    ("o que fazer com nvme", "MHI Nvme tier, swap transformer, modelos .bitnet."),
    ("driver rtl8139", "Proprietario documentado. rtl8139.rs implementado."),
    ("driver usb storage", "Padrao USB class 08. Sem fabricante."),
    ("driver uvc", "Padrao USB Video Class. Sem fabricante."),
    ("onde alocar nvme", "MHI Nvme tier (alta prioridade)."),
    ("onde alocar gpu", "MHI Vram tier (mais rapido)."),
    ("onde alocar ethernet", "MHI Dram tier (buffers rede)."),
]

SMBIOS_DATA = [
    ("smbios system manufacturer", "QEMU"),
    ("smbios system product", "Standard PC (Q35 + ICH9, 2009)"),
    ("smbios bios vendor", "SeaBIOS"),
    ("smbios bios version", "rel-1.16.3-0"),
    ("smbios baseboard", "QEMU Q35"),
    ("smbios processor", "QEMU virtual CPU"),
    ("smbios memory", "2048 MB DDR4"),
    ("fabricante sistema", "QEMU — maquina virtual"),
    ("qual bios", "SeaBIOS — firmware open source padrao do QEMU"),
    ("versao bios", "rel-1.16.3 — SeaBIOS"),
    ("qual placa mae", "QEMU Q35 — chipset ICH9"),
    ("memoria instalada", "2048 MB (2 GB) — configurada via -m 2G"),
    ("virtualizacao", "QEMU System Emulator — virtualizacao full-system"),
    ("processador virtual", "QEMU Virtual CPU versao 11.0.50 — x86-64"),
    ("tipo de placa", "QEMU Q35 — chipset Intel ICH9"),
    ("hardware plataforma", "QEMU q35 — virtualizacao x86-64 completa"),
    ("qual chipset", "Intel 82441FX + ICH9 (PIIX4) — emulacao QEMU"),
    ("placa de rede onboard", "Realtek RTL8139 — Fast Ethernet, emulada pelo QEMU"),
    ("video onboard", "QEMU Virtual VGA — framebuffer simples, 16 MB VRAM"),
    ("controladora usb", "xHCI — USB 3.0, emulada pelo QEMU"),
    ("tempo de atividade", "LAPIC timer periodico ~100 Hz, contador de ticks"),
]

def main():
    parser = argparse.ArgumentParser(description='Generate HW knowledge dataset')
    parser.add_argument('--pci-ids', default='pci.ids', help='PCI ID database file')
    parser.add_argument('--usb-ids', default='usb.ids', help='USB ID database file')
    parser.add_argument('--output', default='hw_knowledge.jsonl', help='Output JSONL file')
    parser.add_argument('--download', action='store_true', help='Download pci.ids')
    args = parser.parse_args()

    entries = []
    if args.download or not os.path.exists(args.pci_ids):
        if download_pci_ids(args.pci_ids):
            entries = parse_pci_ids(args.pci_ids)
            print(f"[PARSE] Extraidas {len(entries)} entradas do {args.pci_ids}")
    else:
        entries = parse_pci_ids(args.pci_ids)
        print(f"[PARSE] Extraidas {len(entries)} entradas do {args.pci_ids}")

    # Always add our embedded examples
    for item in VENDOR_EXAMPLES:
        if len(item) == 2:
            vid, desc = item
            entries.append((vid, "", desc))
        else:
            vid, device, desc = item
            parts = vid.split()
            if len(parts) == 1:
                entries.append((parts[0], device, desc))
            else:
                entries.append((parts[0], parts[1], desc))

    # Parse USB IDs
    if os.path.exists(args.usb_ids):
        usb_entries = parse_usb_ids(args.usb_ids)
        print(f"[PARSE] Extraidas {len(usb_entries)} entradas do {args.usb_ids}")
        entries.extend(usb_entries)

    # Add USB speed data
    for inp, out in USB_SPEED_DATA:
        entries.append((inp, "", out))

    # Add SMBIOS data
    smbios_count = 0
    for inp, out in SMBIOS_DATA:
        entries.append((inp, "", out))
        smbios_count += 1
    print(f"[SMBIOS] {smbios_count} entradas adicionadas")

    # === HARDWARE CAPABILITIES ===
    cap_count = 0
    for inp, out in CAPABILITIES_DATA:
        entries.append((inp, "", out))
        cap_count += 1
    print(f"[CAPABILITIES] {cap_count} entradas de capabilities adicionadas")

    # === KERNEL CODE KNOWLEDGE ===
    kernel_knowledge = [
        ("o que e neural os", "Neural OS Hermes — sistema operacional IA-native, bare-metal Rust, sem Linux"),
        ("arquitetura kernel", "Microkernel com 3 rings: Reflex (Cortex), LLM (BitNet), Skills (WASM)"),
        ("o que e o executor", "NeuralExecutor — scheduler cooperativo com 8 tasks async"),
        ("o que e EVENT_BUS", "EventBus — pub/sub com CapabilityToken, IPC entre daemons"),
        ("o que e SKILL_REGISTRY", "SkillRegistry — registro central de skills com zero-trust policy"),
        ("o que e TRUST_CACHE", "TrustCache — cache de tokens de capacidade com TTL e denylist"),
        ("o que e o CORTEX", "Cortex — roteador neural de intencoes, 12 categorias, dispatch para skills"),
        ("o que e TRANSFORMER", "TransformerModel — 4 camadas BitNet, 272K params ternarios, generate_text()"),
        ("o que e GLOBAL_ALLOCATOR", "BitmapFrameAllocator — 128KB bitmap, 4GB fisico, allocate_contiguous()"),
        ("o que e PHYS_MEM_OFFSET", "Offset de mapeamento da memoria fisica no espaco virtual"),
        ("o que e rtl8139", "RTL8139 — driver de rede via I/O ports, 4 TX desc, RX ring buffer"),
        ("o que e smoltcp", "smoltcp 0.13.1 — pilha TCP/IP no_std, Device trait para RTL8139"),
        ("o que e PciDevice", "Dispositivo PCI com vendor_id, device_id, class, bar0-5"),
        ("o que e OffsetPageTable", "Mapper de paginacao 4 niveis, suporta huge pages 2MB/1GB"),
        ("o que e APIC", "Advanced Programmable Interrupt Controller — LAPIC + IOAPIC"),
        ("o que e LAPIC timer", "Timer periodico do LAPIC, ~100 Hz, vetor 32, substitui PIT"),
        ("o que e SMP", "Symmetric Multi-Processing — INIT-SIPI-SIPI, trampoline, PerCpu GS.base"),
        ("o que e MHI", "Memory Hierarchy Index — gerenciamento de memoria por tiers (Dram/Vram/Nvme/Hdd)"),
        ("o que e MLP", "Multilayer Perceptron — rede neural feedforward para classificacao de intencoes"),
        ("o que e PackedTernaryTensor", "Tensor ternario 2-bit — 4 pesos por byte, matmul ADD/SUB sem multiplicacao"),
        ("o que e BitNet", "Arquitetura BitNet 1.58-bit — pesos ternarios {-1,0,+1}, inferencia eficiente"),
        ("o que e HwIdentifySkill", "Skill que executa PCI scan e envia para o LLM identificar dispositivos"),
        ("o que e input_daemon", "Daemon que le scancodes do teclado, monta buffer ASCII, publica USER_INTENT"),
        ("o que e intent_router", "Daemon que recebe USER_INTENT, classifica via Cortex, executa skills"),
        ("o que e cortex_llm", "Daemon LLM que recebe LLM_REQUEST, gera texto via transformer, publica resposta"),
        ("o que e TIMER_TICKS", "Contador atomico de ticks do LAPIC timer, usado para timeouts"),
        ("boot sequence", "bootloader → VGA → IDT → heap → SIMD → PCI → ACPI → APIC → SMP → NET → executor"),
        ("explique o boot", "11 fases do boot: firmware → kernel → hardware discovery → 8 agents → console"),
        ("quantas tasks", "8 tasks: system, monitor, hw_bridge, network, input, cortex_llm, router, console"),
        ("o que faz o system_daemon", "Publica SYSTEM_READY e morre — sinaliza que o sistema iniciou"),
        ("o que faz o hermes_console", "Escuta HERMES_RESPONSE e exibe [Hermes] no VGA + serial"),
        ("que linguagem", "Rust puro, no_std, nightly, x86_64-unknown-none"),
        ("versao kernel", "v0.28.0 — HW-Aware Cortex LLM"),
        ("qual bootloader", "bootloader crate v0.9.34 com map_physical_memory"),
        ("target hardware", "QEMU q35 → AMD APU com memoria unificada"),
    ]
    for inp, out in kernel_knowledge:
        entries.append((inp, "", out))

    # === GIT HISTORY ===
    import subprocess
    try:
        result = subprocess.run(['git', 'log', '--oneline', '--format=%h %s', '-100'],
                              capture_output=True, cwd=os.path.dirname(__file__) + '/..')
        raw = result.stdout.decode('utf-8', errors='replace')
        for line in raw.strip().split('\n'):
            if not line.strip():
                continue
            parts = line.strip().split(' ', 1)
            if len(parts) == 2:
                hash_id, message = parts
                entries.append((f"commit {hash_id}", "", f"Git: {message}"))
                entries.append((f"o que fez o commit {hash_id}", "", message))
    except:
        print("[GIT] Aviso: git log falhou, pulando historico")

    if not entries:
        print("[WARN] Nenhuma entrada encontrada — usando exemplos embutidos")
        entries = [("8086", "1237", "Intel 82441FX PMC")]

    generate_jsonl(entries, args.output)
    print("[DONE] Dataset pronto para treino")

if __name__ == '__main__':
    main()
