#!/usr/bin/env python3
"""parse_sdio_bin.py — Tenta extrair HWIDs dos arquivos .bin de indice SDIO."""
import os, struct, re, json
from pathlib import Path

SDIO_INDEX = Path(r"C:\Users\msrov\Downloads\SDIO\indexes\SDIO")
TARGET = Path("target")
TARGET.mkdir(exist_ok=True)

def analyze_bin(path):
    """Analisa estrutura de um arquivo .bin SDIO."""
    with open(path, "rb") as f:
        data = f.read()

    result = {"file": path.name, "size": len(data)}

    # Magic + header
    magic = data[0:4]
    ver = struct.unpack("<I", data[4:8])[0]
    result["magic"] = magic.decode("latin-1")
    result["version"] = ver

    # Try to find PCI-style hardware IDs
    # Pattern: VEN_XXXX&DEV_XXXX or similar
    pci_strings = re.findall(rb'VEN_(\w{4})', data)
    result["pci_vendors"] = len(set(pci_strings))
    result["pci_vendor_samples"] = [s.decode() for s in sorted(set(pci_strings))[:10]]

    # Try to find text blocks (>4 readable chars)
    text_blocks = re.findall(rb'[\x20-\x7E]{8,}', data)
    readable = [t.decode(errors="replace") for t in text_blocks if not all(c in "0123456789ABCDEFabcdef" for c in t.decode(errors="replace"))]
    result["text_blocks"] = len(readable)
    result["text_samples"] = readable[:15]

    # Check if format has records with specific size
    # Try different record sizes
    for record_size in [16, 24, 32, 64, 128, 256]:
        header_size = 16  # Assume 16-byte header
        body = data[header_size:]
        n_records = len(body) // record_size
        remainder = len(body) % record_size
        if remainder == 0 and n_records > 1:
            result[f"records_{record_size}"] = n_records

    return result

def try_extract_hwids(path):
    """Tenta extrair HWIDs do formato binario SDIO."""
    with open(path, "rb") as f:
        data = f.read()

    hwids = set()

    # Method 1: Find PCI patterns in raw bytes
    for m in re.finditer(rb'VEN_(\w{4})', data):
        vid = m.group(1).decode()
        # Look for DEV_ nearby
        start = max(0, m.start() - 20)
        end = min(len(data), m.end() + 20)
        context = data[start:end]
        dev_m = re.search(rb'DEV_(\w{4})', context)
        if dev_m:
            did = dev_m.group(1).decode()
            hwids.add((vid, did))

    # Method 2: Try to decode as structured records
    # Header is probably 16 or 32 bytes
    for hdr_size in [16, 24, 32]:
        for record_size in [32, 48, 64, 80, 128, 256]:
            body = data[hdr_size:]
            if len(body) % record_size != 0:
                continue
            n_records = len(body) // record_size
            if n_records < 2 or n_records > 100000:
                continue
            # For each record, extract first 2 u16 as vid/did
            for i in range(min(n_records, 50000)):
                try:
                    rec = body[i * record_size : (i+1) * record_size]
                    if len(rec) >= 4:
                        vid = struct.unpack("<H", rec[0:2])[0]
                        did = struct.unpack("<H", rec[2:4])[0]
                        if (vid > 0 and vid < 0xFFFF and did >= 0 and did < 0xFFFF):
                            hwids.add((f"{vid:04X}", f"{did:04X}"))
                except: pass

    return hwids

def main():
    bins = sorted(SDIO_INDEX.glob("*.bin"))
    print(f"SDIO .bin files: {len(bins)}")

    all_hwids = set()
    by_vendor = {}

    for bin_file in bins:
        # Extract pack name from bin filename: _P_LAN_Intel_26040.bin -> LAN_Intel
        parts = bin_file.stem.split("_")
        if len(parts) >= 3:
            pack_name = "_".join(parts[2:-1])  # Skip _P_ prefix and _26040 suffix
        else:
            pack_name = bin_file.stem

        hwids = try_extract_hwids(bin_file)
        n_new = len(hwids - all_hwids)
        all_hwids.update(hwids)

        if hwids:
            by_vendor[pack_name] = len(hwids)
            print(f"  {pack_name:35s} {len(hwids):5d} HWIDs (+{n_new} novos)")

    print(f"\nTotal HWIDs unicos: {len(all_hwids)}")

    # Salva
    out_path = TARGET / "sdio_bin_hwids.json"
    with open(out_path, "w") as f:
        json.dump([{"vid": v, "did": d} for v,d in sorted(all_hwids)], f)
    print(f"Salvo: {out_path}")

    # Analise detalhada do primeiro arquivo
    print("\n--- Analise detalhada ---")
    result = analyze_bin(bins[0])
    print(json.dumps(result, indent=2, ensure_ascii=False)[:1000])

if __name__ == "__main__":
    main()
