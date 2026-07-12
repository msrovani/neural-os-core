#!/usr/bin/env python3
"""Download pci.ids and usb.ids, then extract PCI tables from linux kernel."""
import os, sys, re, json
from pathlib import Path

TARGET = Path(r"C:\DEV\neural-os-core\tools\target")
KERNEL = TARGET / "linux"
TARGET.mkdir(exist_ok=True)

# ─── 1. Download pci.ids ───────────────────────────────────────────────
pci_ids_path = TARGET / "pci.ids"
if not pci_ids_path.exists():
    print("[PCI-IDS] Baixando...")
    import urllib.request
    urllib.request.urlretrieve("https://pci-ids.ucw.cz/v2.2/pci.ids", str(pci_ids_path))
pci_size = pci_ids_path.stat().st_size
print(f"[PCI-IDS] {pci_size//1024}KB")

# Parse pci.ids
pci_entries = []
current_vendor = None
for line in open(pci_ids_path, "r", errors="replace"):
    line = line.rstrip()
    if line.startswith("#") or not line:
        continue
    # Vendor line: XXXX  VendorName
    m = re.match(r'^([0-9A-Fa-f]{4})\s+(.+)$', line)
    if m and not line.startswith("\t") and not line.startswith(" "):
        current_vendor = (int(m.group(1), 16), m.group(2).strip())
        pci_entries.append({"type": "vendor", "id": m.group(1), "name": m.group(2).strip()})
        continue
    # Device line: \tXXXX  DeviceName
    m = re.match(r'^\t(\w{4})\s+(.+)$', line)
    if m and current_vendor:
        pci_entries.append({
            "type": "device", "vendor": current_vendor[0],
            "vendor_name": current_vendor[1],
            "device_id": m.group(1), "name": m.group(2).strip(),
        })
# Subsystem lines (optional) could also be parsed
print(f"[PCI-IDS] {len(pci_entries)} entries ({sum(1 for e in pci_entries if e['type']=='vendor')} vendors, {sum(1 for e in pci_entries if e['type']=='device')} devices)")

# ─── 2. Download usb.ids ───────────────────────────────────────────────
usb_ids_path = TARGET / "usb.ids"
if not usb_ids_path.exists():
    print("[USB-IDS] Baixando...")
    import urllib.request
    urllib.request.urlretrieve("http://www.linux-usb.org/usb.ids", str(usb_ids_path))
usb_size = usb_ids_path.stat().st_size
print(f"[USB-IDS] {usb_size//1024}KB")

usb_entries = []
current_vendor = None
for line in open(usb_ids_path, "r", errors="replace"):
    line = line.rstrip()
    if line.startswith("#") or not line:
        continue
    m = re.match(r'^([0-9A-Fa-f]{4})\s+(.+)$', line)
    if m and not line.startswith("\t") and not line.startswith(" "):
        current_vendor = (int(m.group(1), 16), m.group(2).strip())
        usb_entries.append({"type": "vendor", "id": m.group(1), "name": m.group(2).strip()})
        continue
    m = re.match(r'^\t(\w{4})\s+(.+)$', line)
    if m and current_vendor:
        usb_entries.append({
            "type": "device", "vendor": current_vendor[0],
            "vendor_name": current_vendor[1],
            "device_id": m.group(1), "name": m.group(2).strip(),
        })
print(f"[USB-IDS] {len(usb_entries)} entries ({sum(1 for e in usb_entries if e['type']=='vendor')} vendors, {sum(1 for e in usb_entries if e['type']=='device')} devices)")

# ─── 3. Linux kernel PCI device tables ─────────────────────────────────
print("\n[KERNEL] Extraindo PCI ID tables...")
if not KERNEL.exists():
    print("[KERNEL] Nao clonado, baixando...")
    import subprocess
    subprocess.run(["git", "clone", "--depth", "1", "--filter=blob:none", "--sparse",
                   "https://github.com/torvalds/linux.git", str(KERNEL)], check=True)
    subprocess.run(["git", "-C", str(KERNEL), "sparse-checkout", "set",
                   "drivers/pci", "drivers/net/ethernet", "drivers/gpu/drm"], check=True)
    subprocess.run(["git", "-C", str(KERNEL), "checkout"], check=True)

# Regex for PCI device tables in C files
PCI_TABLE_RE = re.compile(
    r'PCI_VDEVICE\(\s*(\w+)\s*,\s*(0x\w+)\s*\)'
    r'|{0x(\w{4})\s*,\s*0x(\w{4})\s*,\s*[^}]*}'  # { vendor, device, ... }
    r'|{PCI_DEVICE\(\s*0x(\w{4})\s*,\s*0x(\w{4})\s*\)}'
)
kernel_entries = set()
for root, dirs, files in os.walk(KERNEL / "drivers"):
    for fname in files:
        if not fname.endswith(".c") and not fname.endswith(".h"):
            continue
        fpath = os.path.join(root, fname)
        try:
            text = open(fpath, "r", errors="replace").read()
        except:
            continue
        for m in PCI_TABLE_RE.finditer(text):
            g = m.groups()
            if g[0] and g[1]:  # PCI_VDEVICE
                # PCI_VDEVICE(name, device_id) -> vendor from name
                vendor_name = g[0]
                device_id = int(g[1], 16) if g[1].startswith("0x") else int(g[1], 16)
                kernel_entries.add(("PCI_VDEVICE", vendor_name, f"{device_id:04X}"))
            elif g[2] and g[3]:  # { vendor, device }
                kernel_entries.add(("PAIR", g[2], g[3]))
            elif g[4] and g[5]:  # PCI_DEVICE(vendor, device)
                kernel_entries.add(("PCI_DEVICE", g[4], g[5]))

print(f"[KERNEL] {len(kernel_entries)} PCI entries de drivers do kernel")

# ─── 4. Salvar datasets como JSON ──────────────────────────────────────
dataset = {
    "pci_ids": pci_entries,
    "usb_ids": usb_entries,
    "kernel_pci": [{"vendor": v, "device": d} for _, v, d in kernel_entries],
}

out = TARGET / "pci_usb_hwids.json"
json.dump(dataset, open(out, "w"), indent=1)
print(f"\n[OK] {out} ({os.path.getsize(out)//1024}KB)")
total = len(pci_entries) + len(usb_entries) + len(kernel_entries)
print(f"Total combinado: {total:,} registros de HWID")
