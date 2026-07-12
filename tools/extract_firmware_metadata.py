#!/usr/bin/env python3
"""Extrai metadados de TODOS os diretorios de firmware do linux-firmware.
Nao so READMEs — headers .h, .info, configs, docs, etc.

Uso: python tools/extract_firmware_metadata.py
Saida: tools/target/firmware_metadata.json (~5000+ records)
"""
import os, re, json
from pathlib import Path

ROOT = Path(__file__).parent.parent
FW_DIR = ROOT / "target" / "firmware" / "linux-firmware"
TARGET = ROOT / "tools" / "target"
WHENCE = FW_DIR / "WHENCE"

# Extensoes de arquivo NAO binario que contem metadata
TEXT_EXTS = {".h", ".hpp", ".c", ".cfg", ".conf", ".txt", ".md", ".info",
             ".xml", ".json", ".dts", ".dtsi", ".py", ".sh", ".pl", ".ini",
             ".map", ".ld", ".s", ".S", ".inc", ".def", ".mak", ".mk",
             "README", "WHENCE", "LICENSE", "COPYING", "MANIFEST",
             ".asi", ".asl", ".hex", ".ver", ".version"}

BIN_EXTS = {".bin", ".fw", ".ucode", ".ko", ".o", ".elf", ".so", ".a",
            ".jpg", ".png", ".gif", ".bmp", ".ico"}

SKIP_DIRS = {".git"}

HWID_RE = re.compile(
    r'PCI\\VEN_\w{4}&DEV_\w{4}'
    r'|USB\\VID_\w{4}&PID_\w{4}'
    r'|ACPI\\\w{8}'
    r'|HDAUDIO\\\w+&\w+'
    r'|SD\\\w+'
    r'|VEN_\w{4}\s+DEV_\w{4}'
)

REGISTER_RE = re.compile(
    r'#define\s+(\w+)\s+0x([0-9a-fA-F]+)'
    r'|REG_\w+\s*=\s*0x[0-9a-fA-F]+'
    r'|0x[0-9a-fA-F]{4,8}\s*/\*.*?(?:register|reg|mmio|offset|bar).*?\*/'
)

VENDOR_RE = re.compile(r'VEN[_ ](\w{4})')
DEVICE_RE = re.compile(r'DEV[_ ](\w{4})')
SUBSYS_RE = re.compile(r'SUBSYS[_ ](\w{8})')


def is_text_file(name):
    ext = os.path.splitext(name)[1].lower()
    if ext in TEXT_EXTS:
        return True
    if ext in BIN_EXTS:
        return False
    # No extension or unknown — try reading first bytes
    return ext == "" or ext in (".", "")


def parse_text_metadata(text, fname, relpath):
    """Extract structured metadata from a text file."""
    meta = {
        "file": str(relpath),
        "size": len(text),
        "hwids": [],
        "registers": [],
        "vendors": [],
        "devices": [],
        "defines": {},
        "sections": {},
        "lines": len(text.split("\n")),
    }

    # HWIDs
    for m in HWID_RE.finditer(text):
        hwid = m.group(0).strip()
        if hwid not in meta["hwids"]:
            meta["hwids"].append(hwid)

    # Vendors/Devices from VEN_/DEV_ patterns
    for m in VENDOR_RE.finditer(text):
        v = m.group(1)
        if v not in meta["vendors"]:
            meta["vendors"].append(v)
    for m in DEVICE_RE.finditer(text):
        d = m.group(1)
        if d not in meta["devices"]:
            meta["devices"].append(d)

    # Registers
    for m in REGISTER_RE.finditer(text):
        r = m.group(0).strip()[:80]
        if r not in meta["registers"]:
            meta["registers"].append(r)

    return meta


def walk_firmware_tree():
    """Walk entire firmware tree, extract metadata from every text file."""
    records = []

    # --- 1. WHENCE (already done, 998 entries) ---
    if WHENCE.exists():
        whence = parse_whence()
        records.extend(whence)

    # --- 2. Every directory: walk text files ---
    for root, dirs, fnames in os.walk(FW_DIR):
        # Skip .git
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        rel = Path(root).relative_to(FW_DIR)
        if str(rel) == ".":
            rel = Path("")

        # Skip pure-binary directories
        text_files = [f for f in fnames if is_text_file(f)]
        if not text_files:
            continue

        for fname in text_files:
            fpath = Path(root) / fname
            relpath = rel / fname

            try:
                data = open(fpath, "rb").read()
                # Skip if binary (detect null bytes or high entropy)
                if data[:1024].count(b'\x00') > 50 or b'\xff\xfa' in data[:512]:
                    continue
                text = data.decode("utf-8", errors="replace")
            except:
                continue

            if len(text) < 20:
                continue

            meta = parse_text_metadata(text, fname, relpath)

            # Categorize by file type
            ext = fname.lower()
            if fname == "README" or fname.startswith("README"):
                meta["type"] = "readme"
            elif fname == "WHENCE":
                meta["type"] = "manifest"
            elif fname.startswith("LICENSE"):
                meta["type"] = "license"
            elif ext.endswith(".h") or ext.endswith(".hpp"):
                meta["type"] = "header"
            elif ext.endswith(".cfg") or ext.endswith(".conf"):
                meta["type"] = "config"
            elif ext.endswith(".dts") or ext.endswith(".dtsi"):
                meta["type"] = "devicetree"
            elif ext.endswith(".info"):
                meta["type"] = "board_info"
            elif ext.endswith(".json"):
                meta["type"] = "json"
            elif ext.endswith(".xml"):
                meta["type"] = "xml"
            elif ext.endswith(".map"):
                meta["type"] = "memory_map"
            elif ext.endswith(".py") or ext.endswith(".sh"):
                meta["type"] = "script"
            elif ext.endswith(".mak") or ext.endswith(".mk"):
                meta["type"] = "makefile"
            elif ext.endswith(".ver") or ext.endswith(".version"):
                meta["type"] = "version"
            else:
                meta["type"] = "doc"

            category = str(rel).split("\\")[0].split("/")[0] if str(rel) != "." else "root"
            meta["category"] = category
            records.append(meta)

    return records


def parse_whence():
    """Parse WHENCE file into structured records."""
    text = open(WHENCE, "r", errors="replace").read()
    entries = []
    current = {}
    for line in text.split("\n"):
        if line.strip() == "" or line.strip().startswith("---"):
            if current.get("File") or current.get("Driver"):
                current["type"] = "firmware"
                entries.append(current)
                current = {}
            continue
        m = re.match(r'^File:\s*(.*)', line)
        if m:
            current["File"] = m.group(1).strip()
            if current.get("File"):
                parts = current["File"].replace("\\", "/").split("/")
                current["category"] = parts[0] if len(parts) > 1 else "root"
            continue
        if line.startswith(" "):
            if "File" in current and not any(line.lower().startswith(k) for k in ["version:", "info:", "licen", "source:", "orig"]):
                current["File"] += " " + line.strip()
            continue
        for k, v in [("Version:", "Version"), ("Info:", "Info"),
                     ("Licen", "License"), ("Source:", "Source"),
                     ("Driver:", "Driver")]:
            if line.startswith(k):
                current[v] = line.split(":", 1)[1].strip() if ":" in line else ""
                break
    if current.get("File"):
        current["type"] = "firmware"
        entries.append(current)
    print(f"  [WHENCE] {len(entries)} entries")
    return entries


def build_summary(records):
    """Print summary of what was extracted."""
    by_type = {}
    by_cat = {}
    hwids_total = 0
    regs_total = 0
    for r in records:
        t = r.get("type", "?")
        by_type[t] = by_type.get(t, 0) + 1
        cat = r.get("category", "root")
        by_cat[cat] = by_cat.get(cat, 0) + 1
        hwids_total += len(r.get("hwids", []))
        regs_total += len(r.get("registers", []))
    print(f"\n  Tipos de arquivo:")
    for t, c in sorted(by_type.items(), key=lambda x: -x[1])[:15]:
        print(f"    {t}: {c}")
    print(f"\n  Categorias (top 15):")
    for c, n in sorted(by_cat.items(), key=lambda x: -x[1])[:15]:
        print(f"    {c}: {n}")
    print(f"\n  HWIDs extraidos: {hwids_total}")
    print(f"  Register definitions: {regs_total}")
    return by_type, by_cat


def main():
    print("=== Extraindo METADADOS de TODOS os firmwares ===")
    print(f"  Fonte: {FW_DIR}")
    TARGET.mkdir(exist_ok=True)

    records = walk_firmware_tree()
    print(f"\n  Total: {len(records)} records extraidos")

    out = TARGET / "firmware_metadata.json"
    # Limit registers/HWIDs per record to keep file manageable
    for r in records:
        if len(r.get("registers", [])) > 20:
            r["registers"] = r["registers"][:20]
        if len(r.get("hwids", [])) > 10:
            r["hwids"] = r["hwids"][:10]
        # Remove full text to keep JSON lean
        if "full_text" not in r:
            pass

    with open(out, "w", encoding="utf-8") as f:
        json.dump(records, f, indent=1, ensure_ascii=False)
    print(f"\n  [OK] {out} ({os.path.getsize(out) / 1024:.0f}KB)")

    build_summary(records)

if __name__ == "__main__":
    main()
