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

def main():
    parser = argparse.ArgumentParser(description='Generate HW knowledge dataset')
    parser.add_argument('--pci-ids', default='pci.ids', help='PCI ID database file')
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

    if not entries:
        print("[WARN] Nenhuma entrada PCI encontrada — usando exemplos embutidos")
        entries = [("8086", "1237", "Intel 82441FX PMC")]

    generate_jsonl(entries, args.output)
    print("[DONE] Dataset pronto para treino")

if __name__ == '__main__':
    main()
