#!/usr/bin/env python3
# extract_sdio_hw.py - neural-os-core v1.0
# Extrai HWIDs de DriverPacks .7z (SDIO) para treinar HW Expert.
# Uso: python extract_sdio_hw.py --dir SDIODRIVERS

import os, sys, re, json, struct, time, hashlib, argparse, csv, io
from pathlib import Path
from collections import defaultdict

SDIO_PATH = r"C:\Users\msrov\Downloads\SDIO\drivers"
TARGET = Path(__file__).parent / "target"

try:
    import py7zr
except ImportError:
    py7zr = None

HWID_RE = re.compile(
    r'(?:PCI|VEN|DEV|SUBSYS|REV|CC)'
    r'|USB\\VID_\w{4}&PID_\w{4}'
    r'|ACPI\\\w{8}'
    r'|HDAUDIO\\\w+&\w+'
    r'|SD\\\w+'
    r'|PCI\\VEN_\w{4}&DEV_\w{4}'
    r'|PCIVEN_\w{4}&DEV_\w{4}'
    r'|USB\\\w+'
    r'|PCI\_CC\_\w+'
)

CLASS_MAP = {
    "LAN":     "net", "WLAN": "net", "WiFi": "net", "Bluetooth": "net",
    "Video":   "gpu", "Videos": "gpu",
    "Sound":   "audio", "Audio": "audio", "HDA": "audio", "HDMI": "audio",
    "Chipset": "chipset", "MassStorage": "storage",
    "USB":     "usb", "xUSB": "usb", "USB3": "usb",
    "Camera":  "camera", "WebCam": "camera",
    "Touchpad":"touchpad", "Touch": "touchpad",
    "Printer": "printer",
    "Monitor": "display",
    "Biometric":"bio", "Fingerprint": "bio",
    "CardReader":"card",
    "Modem":   "modem",
    "xMobile": "mobile",
    "Vendor":  "vendor",
    "xVirtual":"virtual",
    "TV":      "tv",
    "Thermo":  "printer",
    "SDIO":    "sdio",
    "Misc":    "misc",
}

def cat_from_name(fname):
    for kw, cls in CLASS_MAP.items():
        if kw.lower() in fname.lower():
            return cls
    return "unknown"

def parse_inf_hwids(text):
    """Extrai HWIDs de um arquivo .inf."""
    hwids = set()
    for line in text.split("\n"):
        line = line.strip()
        # Linhas tipo: %DeviceDesc% = FOO_Install, PCI\VEN_8086&DEV_29C0
        for part in line.split(","):
            part = part.strip()
            if "VEN_" in part.upper() or "VID_" in part.upper() or "PCI\\" in part.upper():
                hwids.add(part)
            elif "USB\\" in part.upper() and "VID_" in part.upper():
                hwids.add(part)
            elif "ACPI\\" in part.upper():
                hwids.add(part)
            elif "HDAUDIO\\" in part.upper():
                hwids.add(part)
    return hwids

def parse_inf_sections(text):
    """Extrai secoes [Manufacturer], [ControlFlags], [Models]."""
    entries = []
    lines = text.split("\n")
    in_models = False
    for line in lines:
        if line.strip().startswith("[") and line.strip().endswith("]"):
            in_models = "%Manufacturer%" in line or "Models" in line.upper() or "Strings" in line.upper()
            continue
        if in_models and "=" in line and "\\" in line.upper():
            entries.append(line.strip())
    return entries

def extract_from_7z(path, max_files=500):
    """Extrai HWIDs de um arquivo .7z DriverPack."""
    if py7zr is None:
        return set(), 0

    all_hwids = set()
    count = 0
    try:
        with py7zr.SevenZipFile(path, mode='r', dereference=True) as z:
            names = list(z.getnames())[:max_files]
            inf_names = [n for n in names if n.lower().endswith('.inf')]

            for inf_name in inf_names[:200]:  # max 200 .inf por pack
                try:
                    data = z.read([inf_name])
                    if inf_name in data:
                        text = data[inf_name].read().decode("utf-8", errors="replace")
                        hwids = parse_inf_hwids(text)
                        all_hwids.update(hwids)
                        count += 1
                except:
                    pass
    except Exception as e:
        print(f"  [ERRO] {path.name}: {e}")

    return all_hwids, count

def parse_hwid_to_parts(hwid):
    """Converte HWID string em (vendor_id, device_id, classe)."""
    h = hwid.upper()
    vid = did = cls = 0

    # PCI\VEN_XXXX&DEV_XXXX
    m = re.search(r'VEN_(\w{4})', h)
    if m: vid = int(m.group(1), 16)
    m = re.search(r'DEV_(\w{4})', h)
    if m: did = int(m.group(1), 16)

    # USB\VID_XXXX&PID_XXXX
    m = re.search(r'VID_(\w{4})', h)
    if m and not vid: vid = int(m.group(1), 16)
    m = re.search(r'PID_(\w{4})', h)
    if m and not did: did = int(m.group(1), 16)

    # ACPI\XXXX
    m = re.search(r'ACPI\\(\w{8})', h)
    if m: vid = int(m.group(1)[:4], 16); did = int(m.group(1)[4:], 16)

    return vid, did

def generate_hw_labels(all_hwids):
    """Gera labels para cada HWID baseado no vendor."""
    vendors = defaultdict(list)
    for hwid in all_hwids:
        vid, did = parse_hwid_to_parts(hwid)
        if vid:
            vendors[vid].append(hwid)

    # Mapa vendor→label
    sorted_vids = sorted(vendors.keys())
    label_map = {v: i for i, v in enumerate(sorted_vids)}
    return label_map, vendors

def build_training_dataset(all_hwids, cat):
    """Converte HWIDs em pares (input, target) para treino."""
    label_map, vendors = generate_hw_labels(all_hwids)
    dataset = []
    for hwid in all_hwids:
        vid, did = parse_hwid_to_parts(hwid)
        if vid and did:
            label = label_map.get(vid, 0)
            dataset.append({
                "hwid": hwid,
                "vid": f"{vid:04X}",
                "did": f"{did:04X}",
                "class": cat,
                "label": label
            })
    return dataset, len(label_map)

def write_jsonl(dataset, path):
    with open(path, "w") as f:
        for entry in dataset:
            f.write(json.dumps(entry) + "\n")
    print(f"  [OK] {len(dataset)} entries -> {path}")

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", default=IMPORT_PATH)
    parser.add_argument("--dry-run", action="store_true", help="So conta, nao extrai")
    parser.add_argument("--train-only", action="store_true", help="So treina com cache existente")
    parser.add_argument("--max-packs", type=int, default=999)
    parser.add_argument("--epochs", type=int, default=50)
    args = parser.parse_args()

    sdio_dir = Path(args.dir)
    if not sdio_dir.exists():
        print(f"[FATAL] {sdio_dir} nao encontrado")
        sys.exit(1)

    cache_file = TARGET / "sdio_hwids.json"
    TARGET.mkdir(exist_ok=True)

    if not args.train_only:
        print("=" * 65)
        print(f"  SDIO HW Extraction Pipeline")
        print(f"  Fonte: {sdio_dir}")
        print("=" * 65)

        packs = sorted(sdio_dir.glob("DP_*.7z"))[:args.max_packs]
        print(f"  Encontrados {len(packs)} DriverPacks")

        if args.dry_run:
            total_gb = sum(f.stat().st_size for f in packs) / (1024**3)
            print(f"  Tamanho total: {total_gb:.1f} GB")
            return

        all_hwids = set()
        total_inf = 0
        t0 = time.time()

        for i, pack in enumerate(packs):
            print(f"  [{i+1}/{len(packs)}] {pack.name}...", end="", flush=True)
            hwids, n_inf = extract_from_7z(pack)
            cat = cat_from_name(pack.name)
            all_hwids.update(hwids)
            total_inf += n_inf
            print(f"  {len(hwids)} HWIDs, {n_inf} .inf files", flush=True)

        # Salva cache
        hwids_list = [{"hwid": h, "class": cat_from_name("")} for h in all_hwids]
        # Melhor: salva com categoria
        hwids_by_pack = {}
        for pack in packs:
            cat = cat_from_name(pack.name)
            hwids_by_pack[pack.name] = cat

        structured = []
        for hwid in all_hwids:
            structured.append({
                "hwid": hwid,
                "class": "unknown"
            })

        with open(cache_file, "w") as f:
            json.dump(structured, f)

        elapsed = time.time() - t0
        print(f"\n  [OK] {len(all_hwids)} HWIDs unicos de {total_inf} .inf files em {elapsed:.0f}s")
    else:
        print(f"[TRAIN] Usando cache existente: {cache_file}")

    # Treino
    print("\n" + "=" * 65)
    print("  Treinando HW Expert com dados SDIO + PCI")
    print("=" * 65)

    # Merge SDIO + PCI data
    sdio_data = []
    if cache_file.exists():
        with open(cache_file) as f:
            sdio_data = json.load(f)
        print(f"  SDIO: {len(sdio_data)} HWIDs carregados")

    # Treina modelo
    sys.path.insert(0, str(Path(__file__).parent))
    from train_gpu_full import BitNetLM, DEVICE, train, gen_pci_dataset, tokenize_pci

    # Prepara dataset SDIO
    sdio_tokens = []
    for entry in sdio_data[:50000]:
        hwid = entry.get("hwid", "")
        vid, did = 0, 0
        m = re.search(r'VEN_(\w{4})', hwid)
        if m: vid = int(m.group(1), 16)
        m = re.search(r'DEV_(\w{4})', hwid)
        if m: did = int(m.group(1), 16)
        if not vid: m = re.search(r'VID_(\w{4})', hwid)
        if m: vid = int(m.group(1), 16)
        if not did: m = re.search(r'PID_(\w{4})', hwid)
        if m: did = int(m.group(1), 16)
        if not did: did = (vid + len(hwid)) & 0xFFFF

        vocab = 64
        inp = [(vid>>8)%vocab, vid%vocab, (did>>8)%vocab, did%vocab]
        inp = (inp + [0]*16)[:15]
        cls = hash(entry.get("class", "unknown")) % vocab
        tgt = [cls] + [0]*14
        sdio_tokens.append((inp[:15], tgt[:15]))

    # PCI dataset
    pci_entries = gen_pci_dataset()
    pci_tokens = tokenize_pci(pci_entries, vcb=64, sl=16)

    # Merge
    combined = (sdio_tokens + pci_tokens)[:100000]
    print(f"  Dataset combinado: {len(combined)} amostras")

    model = BitNetLM(h=64, v=64, nl=4, nh=4, ff=128).to(DEVICE)
    train("HW Expert SDIO", model, combined, 64,
          ep=args.epochs, bs=2048, lr=1e-3, tok=b"hwexpert_sdio_v1")

if __name__ == "__main__":
    main()
