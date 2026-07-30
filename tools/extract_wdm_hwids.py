#!/usr/bin/env python3
"""Extract HWID entries from Windows Driver Store (.inf files).

Parses all .inf files in C:\Windows\System32\DriverStore\FileRepository
for PCI, USB, ACPI HWID patterns and saves structured JSON to models/WDM/.

Output:
    models/WDM/hwids.json        — all extracted HWIDs (PCI, USB, ACPI)
    models/WDM/stats.json         — extraction statistics
"""

import json
import os
import re
import sys
from collections import defaultdict
from pathlib import Path

DRIVER_STORE = Path(os.environ.get(
    "WINDIR", "C:\\Windows"
)) / "System32" / "DriverStore" / "FileRepository"

OUTPUT = Path(__file__).resolve().parent.parent / "models" / "WDM"

# ─── HWID regex patterns ────────────────────────────────────────────────

RE_PCI = re.compile(
    r'PCI\\VEN_([0-9A-Fa-f]{4})&DEV_([0-9A-Fa-f]{4})'
    r'(?:&SUBSYS_([0-9A-Fa-f]{8})|&REV_([0-9A-Fa-f]{2})|&CC_([0-9A-Fa-f]{4}))*'
)

RE_USB = re.compile(
    r'USB\\VID_([0-9A-Fa-f]{4})&PID_([0-9A-Fa-f]{4})'
    r'(?:&REV_([0-9A-Fa-f]{4})|&MI_([0-9A-Fa-f]{2}))*'
)

RE_ACPI = re.compile(r'ACPI\\([0-9A-Za-z*_]{4,16})')

RE_VEN_DEV = re.compile(
    r'VEN_([0-9A-Fa-f]{4})&DEV_([0-9A-Fa-f]{4})'
)

RE_VID_PID = re.compile(
    r'VID_([0-9A-Fa-f]{4})&PID_([0-9A-Fa-f]{4})'
)

# ─── Class inference ─────────────────────────────────────────────────────

def infer_class(category: str, inf_class: str) -> str:
    """Infer device class from INF Class or HWID category."""
    cls_map = {
        "Net": "network", "NetTrans": "network", "NetService": "network",
        "Display": "display", "MEDIA": "audio", "Audio": "audio",
        "HDAudio": "audio", "HDTAudio": "audio",
        "USB": "usb", "USBDevice": "usb", "USBXHCI": "usb",
        "System": "system", "HID": "hid", "Keyboard": "hid",
        "Mouse": "hid", "Point": "hid",
        "Storage": "storage", "SCSIAdapter": "storage",
        "DiskDrive": "storage", "HDC": "storage",
        "1394": "firewire", "SBP2": "firewire",
        "Bluetooth": "bluetooth", "BTH": "bluetooth",
        "Camera": "camera", "Image": "camera",
        "Printer": "printer", "SmartCard": "smartcard",
        "Extension": "extension",
        "Volume": "volume", "VolumeSnapshot": "volume",
        "SoftwareComponent": "software",
        "Security": "security",
    }
    if category == "pci":
        # Não sobreescreve PCI — heuristic_card() faz isso melhor
        return "pci"
    return cls_map.get(inf_class, category)


def parse_hwids_from_text(text: str, inf_class: str) -> list[dict]:
    """Parse all HWID entries from a single .inf file text."""
    results = []

    # PCI hardware IDs: PCI\VEN_XXXX&DEV_XXXX
    for m in RE_PCI.finditer(text):
        vid, did = int(m.group(1), 16), int(m.group(2), 16)
        subsys = int(m.group(3), 16) if m.group(3) else None
        entry = {
            "bus": "pci",
            "vid": vid,
            "did": did,
            "subsys": subsys,
            "class": infer_class("pci", inf_class),
        }
        # Check if it's a known vendor for more specific class
        results.append(entry)

    # USB hardware IDs: USB\VID_XXXX&PID_XXXX
    for m in RE_USB.finditer(text):
        vid, pid = int(m.group(1), 16), int(m.group(2), 16)
        entry = {
            "bus": "usb",
            "vid": vid,
            "did": pid,
            "subsys": None,
            "class": infer_class("usb", inf_class),
        }
        results.append(entry)

    # ACPI hardware IDs
    for m in RE_ACPI.finditer(text):
        entry = {
            "bus": "acpi",
            "vid": 0,
            "did": 0,
            "acpi_id": m.group(1),
            "class": infer_class("acpi", inf_class),
        }
        results.append(entry)

    # Generic VEN_XXXX&DEV_XXXX (sometimes without PCI\ prefix)
    for m in RE_VEN_DEV.finditer(text):
        vid, did = int(m.group(1), 16), int(m.group(2), 16)
        # Deduplicate with PCI results
        already = any(
            r["bus"] == "pci" and r["vid"] == vid and r["did"] == did
            for r in results
        )
        if not already:
            results.append({
                "bus": "pci_gen",
                "vid": vid,
                "did": did,
                "subsys": None,
                "class": infer_class("pci", inf_class),
            })

    # Generic VID_XXXX&PID_XXXX (USB without USB\ prefix)
    for m in RE_VID_PID.finditer(text):
        vid, pid = int(m.group(1), 16), int(m.group(2), 16)
        already = any(
            r["bus"] == "usb" and r["vid"] == vid and r["did"] == pid
            for r in results
        )
        if not already:
            results.append({
                "bus": "usb_gen",
                "vid": vid,
                "did": pid,
                "subsys": None,
                "class": infer_class("usb", inf_class),
            })

    return results


def scan_driver_store() -> tuple[list[dict], dict]:
    """Scan all .inf files in the Windows Driver Store."""
    all_hwids = []
    stats = {
        "inf_files_scanned": 0,
        "inf_files_with_hwids": 0,
        "pci_count": 0,
        "usb_count": 0,
        "acpi_count": 0,
        "pci_gen_count": 0,
        "usb_gen_count": 0,
        "total_hwids": 0,
        "errors": [],
    }

    if not DRIVER_STORE.exists():
        stats["errors"].append(f"DriverStore not found: {DRIVER_STORE}")
        return [], stats

    print(f"[WDM] Scanning {DRIVER_STORE}...")

    for dir_entry in sorted(DRIVER_STORE.iterdir()):
        if not dir_entry.is_dir():
            continue
        inf_files = list(dir_entry.glob("*.inf"))
        if not inf_files:
            continue

        inf_path = inf_files[0]  # Usually 1 .inf per package
        stats["inf_files_scanned"] += 1

        try:
            text = inf_path.read_text(encoding="utf-8", errors="replace")

            # Extract the INF Class
            cls_match = re.search(r'^\s*Class\s*=\s*(\S+)', text, re.MULTILINE)
            inf_class = cls_match.group(1) if cls_match else "Unknown"

            hwids = parse_hwids_from_text(text, inf_class)
            if hwids:
                stats["inf_files_with_hwids"] += 1
                for hw in hwids:
                    hw["source"] = str(inf_path.relative_to(DRIVER_STORE))

                all_hwids.extend(hwids)

        except Exception as e:
            stats["errors"].append(f"{inf_path.name}: {e}")

    # Deduplicate by (bus, vid, did)
    seen = set()
    unique = []
    for hw in all_hwids:
        if hw["bus"] in ("acpi",):
            key = (hw["bus"], hw.get("acpi_id", ""))
        else:
            key = (hw["bus"], hw["vid"], hw["did"])
        if key not in seen:
            seen.add(key)
            unique.append(hw)

    # Update stats
    for hw in unique:
        if hw["bus"] == "pci":
            stats["pci_count"] += 1
        elif hw["bus"] == "usb":
            stats["usb_count"] += 1
        elif hw["bus"] == "acpi":
            stats["acpi_count"] += 1
        elif hw["bus"] == "pci_gen":
            stats["pci_gen_count"] += 1
        elif hw["bus"] == "usb_gen":
            stats["usb_gen_count"] += 1

    stats["total_hwids"] = len(unique)
    return unique, stats


def main():
    print("=" * 60)
    print("  Windows DriverStore HWID Extractor")
    print("  Target: models/WDM/")
    print("=" * 60)

    OUTPUT.mkdir(parents=True, exist_ok=True)

    hwids, stats = scan_driver_store()

    # Write HWIDs
    hwids_path = OUTPUT / "hwids.json"
    with open(hwids_path, "w", encoding="utf-8") as f:
        json.dump(hwids, f, indent=1)
    print(f"\n  Wrote {hwids_path} ({len(hwids)} unique HWIDs)")

    # Write stats
    stats_path = OUTPUT / "stats.json"
    with open(stats_path, "w", encoding="utf-8") as f:
        json.dump(stats, f, indent=2)
    print(f"  Wrote {stats_path}")

    # Summary
    print(f"\n  Summary:")
    print(f"    .inf files scanned:     {stats['inf_files_scanned']}")
    print(f"    .inf files with HWIDs:  {stats['inf_files_with_hwids']}")
    print(f"    PCI HWIDs:              {stats['pci_count']}")
    print(f"    USB HWIDs:              {stats['usb_count']}")
    print(f"    ACPI HWIDs:             {stats['acpi_count']}")
    print(f"    PCI generic:            {stats['pci_gen_count']}")
    print(f"    USB generic:            {stats['usb_gen_count']}")
    print(f"    ─────────────────────")
    print(f"    Total unique:           {stats['total_hwids']}")

    if stats["errors"]:
        print(f"\n  Errors ({len(stats['errors'])}):")
        for err in stats["errors"][:10]:
            print(f"    - {err}")


if __name__ == "__main__":
    main()
