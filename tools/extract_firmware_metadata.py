#!/usr/bin/env python3
"""Extrai dados estruturados do linux-firmware WHENCE + READMEs para treino.
Uso: python tools/extract_firmware_metadata.py

Saida: tools/target/firmware_metadata.json (dataset de treino)
"""
import os, re, json, sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
FW_DIR = ROOT / "target" / "firmware" / "linux-firmware"
TARGET = ROOT / "tools" / "target"
WHENCE = FW_DIR / "WHENCE"
README_MD = FW_DIR / "README.md"
AMD_UCODE_README = FW_DIR / "amd-ucode" / "README"

def parse_whence():
    """Parse WHENCE file into list of firmware records."""
    if not WHENCE.exists():
        print(f"[--] WHENCE nao encontrado em {WHENCE}")
        return []
    text = open(WHENCE, "r", errors="replace").read()
    entries = []
    current = {}
    field_order = []

    for line in text.split("\n"):
        # Entry separator: blank line or "---"
        if line.strip() == "" or line.strip().startswith("---"):
            if current.get("File") or current.get("Driver"):
                entries.append(current)
                current = {}
            continue

        # "File:" field (can be multi-line)
        m = re.match(r'^File:\s*(.*)', line)
        if m:
            current["File"] = m.group(1).strip()
            continue

        # Continuation of File (indented)
        if line.startswith(" ") and "File" in current and not any(line.startswith(k.lower()) for k in ["version", "info", "licen", "source", "orig"]):
            current["File"] += " " + line.strip()
            continue

        # Other fields
        m = re.match(r'^Version:\s*(.*)', line)
        if m: current["Version"] = m.group(1).strip(); continue
        m = re.match(r'^Info:\s*(.*)', line)
        if m: current["Info"] = m.group(1).strip(); continue
        m = re.match(r'^Licen[cs]e:\s*(.*)', line)
        if m: current["License"] = m.group(1).strip(); continue
        m = re.match(r'^Source:\s*(.*)', line)
        if m: current["Source"] = m.group(1).strip(); continue
        m = re.match(r'^Original.*Source:\s*(.*)', line)
        if m: current["Source"] = m.group(1).strip(); continue
        m = re.match(r'^Driver:\s*(.*)', line)
        if m: current["Driver"] = m.group(1).strip(); continue

    # Last entry
    if current.get("File"):
        entries.append(current)

    print(f"  [OK] {len(entries)} entradas no WHENCE")
    return entries


def parse_amd_ucode():
    """Parse AMD microcode README for Family/Model/Stepping data."""
    if not AMD_UCODE_README.exists():
        return []
    text = open(AMD_UCODE_README, "r", errors="replace").read()
    patches = []
    for line in text.split("\n"):
        m = re.search(r'Family=0x(\w+)\s+Model=0x(\w+)\s+Stepping=0x(\w+):\s+Patch=0x(\w+)\s+Length=(\d+)', line)
        if m:
            patches.append({
                "family": int(m.group(1), 16),
                "model": int(m.group(2), 16),
                "stepping": int(m.group(3), 16),
                "patch": m.group(4),
                "length": int(m.group(5)),
            })
        m2 = re.search(r'Family=0x(\w+)\s+Model=0x(\w+)\s+Stepping=0x(\w+):\s+Patch=0x(\w+)', line)
        if m2 and not m:
            patches.append({
                "family": int(m2.group(1), 16),
                "model": int(m2.group(2), 16),
                "stepping": int(m2.group(3), 16),
                "patch": m2.group(4),
            })
    print(f"  [OK] {len(patches)} entradas AMD microcode")
    return patches


def parse_firmware_readmes():
    """Extract metadata from all README files."""
    readmes = []
    for p in FW_DIR.rglob("README*"):
        rel = p.relative_to(FW_DIR)
        text = open(p, "r", errors="replace").read()
        readmes.append({
            "path": str(rel),
            "size": len(text),
            "content": text[:500],  # first 500 chars
        })
    print(f"  [OK] {len(readmes)} READMEs processados")
    return readmes


def build_dataset(whence_entries, amd_patches, readmes):
    """Build structured training dataset."""
    dataset = []

    # 1. Flatten firmware files from WHENCE
    for entry in whence_entries:
        fw_file = entry.get("File", "")
        # Extract category from path
        parts = fw_file.replace("\\", "/").split("/")
        category = parts[0] if len(parts) > 1 else "root"
        dataset.append({
            "type": "firmware",
            "path": fw_file,
            "category": category,
            "version": entry.get("Version", ""),
            "license": entry.get("License", ""),
            "info": entry.get("Info", ""),
            "driver": entry.get("Driver", ""),
            "source": entry.get("Source", ""),
        })

    # 2. AMD microcode patches
    for p in amd_patches:
        dataset.append({
            "type": "amd_ucode",
            "family": p["family"],
            "model": p["model"],
            "stepping": p["stepping"],
            "patch": p["patch"],
            "length": p.get("length", 0),
            "hwid": f"AMD\\Family_{p['family']:04X}&Model_{p['model']:04X}&Step_{p['stepping']:02X}",
        })

    # 3. README metadata
    for r in readmes:
        dataset.append({
            "type": "readme",
            "path": r["path"],
            "size": r["size"],
            "preview": r["content"],
        })

    return dataset


def main():
    print("=== Firmware Metadata Extraction ===")
    TARGET.mkdir(exist_ok=True)

    print("\n--- WHENCE ---")
    whence_entries = parse_whence()

    print("\n--- AMD microcode ---")
    amd_patches = parse_amd_ucode()

    print("\n--- READMEs ---")
    readmes = parse_firmware_readmes()

    print("\n--- Building dataset ---")
    dataset = build_dataset(whence_entries, amd_patches, readmes)
    out = TARGET / "firmware_metadata.json"
    with open(out, "w") as f:
        json.dump(dataset, f, indent=1)
    print(f"  [OK] {len(dataset)} records -> {out}")

    # Summary
    types = {}
    for d in dataset:
        t = d.get("type", "?")
        types[t] = types.get(t, 0) + 1
    for t, c in sorted(types.items()):
        print(f"  {t}: {c}")

if __name__ == "__main__":
    main()
