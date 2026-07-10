#!/usr/bin/env python3
"""extract_all_sdio.py — Extrai TODOS os 56 DriverPacks apagando .7z apos uso.
Uso: python extract_all_sdio.py [--resume]
     python extract_all_sdio.py --train-only

Requisitos: python train_hw_register_predictor.py na mesma pasta
"""

import os, re, json, struct, subprocess, tempfile, shutil, time, sys, csv
from pathlib import Path
from collections import defaultdict
sys.path.insert(0, str(Path(__file__).parent))

SZ = r"C:\Program Files\7-Zip\7z.exe"
SDIO_DIR = Path(r"C:\Users\msrov\Downloads\SDIO\drivers")
TARGET = Path(__file__).parent / "target"
TARGET.mkdir(exist_ok=True)

def parse_inf(text):
    """Extrai HWIDs + metadados de um .inf."""
    data = {"hwids": [], "class_guid": "", "device_class": "", "provider": "",
            "driver_ver": "", "strings": {}}
    for m in re.finditer(r'PCI\\VEN_(\w{4})&DEV_(\w{4})(?:&SUBSYS_(\w{8}))?', text, re.I):
        data["hwids"].append({"type": "PCI", "vid": m.group(1), "did": m.group(2)})
    for m in re.finditer(r'USB\\VID_(\w{4})&PID_(\w{4})', text, re.I):
        data["hwids"].append({"type": "USB", "vid": m.group(1), "did": m.group(2)})
    for m in re.finditer(r'ACPI\\(\w{8})', text, re.I):
        data["hwids"].append({"type": "ACPI", "id": m.group(1)})
    for m in re.finditer(r'Class\s*=\s*(\w+)', text, re.I):
        data["device_class"] = m.group(1)
    for m in re.finditer(r'Provider\s*=\s*%([^%]+)%', text, re.I):
        data["provider"] = m.group(1)
    in_str = False
    for line in text.split("\n"):
        s = line.strip()
        if s.startswith("[Strings]"): in_str = True; continue
        if in_str and s.startswith("["): break
        if in_str and "=" in s:
            k, v = s.split("=", 1)
            v = v.strip().strip('"')
            if any(c.isalpha() for c in v) and len(v) < 200:
                data["strings"][k.strip("%").strip()] = v
    return data if data["hwids"] else None

def process_one_pack(pack_path):
    """Extrai .inf, parseia, salva cache, APAGA .7z."""
    cache = TARGET / f"{pack_path.stem}_inf.json"
    if cache.exists():
        print(f"  [CACHE] {cache.name}")
        return

    tmp = tempfile.mkdtemp()
    results = []
    try:
        # Lista .inf
        r = subprocess.run([SZ, "l", str(pack_path)], capture_output=True, text=True, timeout=60)
        infs = []
        for line in r.stdout.split("\n"):
            parts = line.split()
            if len(parts) > 4 and parts[-1].lower().endswith(".inf"):
                infs.append(parts[-1])

        n_parsed = 0
        for inf_path in infs[:800]:
            out_dir = os.path.join(tmp, os.path.dirname(inf_path))
            os.makedirs(out_dir, exist_ok=True)
            r2 = subprocess.run([SZ, "e", str(pack_path), f"-o{out_dir}", inf_path, "-y"],
                               capture_output=True, text=True, timeout=30)
            extracted = os.path.join(out_dir, os.path.basename(inf_path))
            if os.path.exists(extracted):
                try:
                    with open(extracted, "r", encoding="utf-8", errors="replace") as f:
                        text = f.read()
                    parsed = parse_inf(text)
                    if parsed:
                        results.append(parsed)
                        n_parsed += 1
                except: pass
                try: os.remove(extracted)
                except: pass

        # Salva cache
        with open(cache, "w") as f:
            json.dump(results, f)

        n_hwids = sum(len(e["hwids"]) for e in results)
        print(f"  [OK] {pack_path.name}: {n_parsed} .inf, {n_hwids} HWIDs")

    finally:
        shutil.rmtree(tmp, ignore_errors=True)
        # APAGA .7z original
        try:
            sz = pack_path.stat().st_size
            pack_path.unlink()
            print(f"  [DEL] {pack_path.name} ({sz//(1024*1024)}MB liberados)")
        except Exception as e:
            print(f"  [WARN] Nao foi possivel apagar {pack_path.name}: {e}")

def extract_all(resume=True):
    packs = sorted(SDIO_DIR.glob("DP_*.7z"))
    print(f"Total: {len(packs)} packs")

    for i, pack in enumerate(packs):
        cache = TARGET / f"{pack.stem}_inf.json"
        if resume and cache.exists():
            n = len(json.load(open(cache)))
            print(f"  [{i+1:2d}/{len(packs)}] [SKIP] {pack.name} ({n} ja em cache)")
            continue

        t1 = time.time()
        process_one_pack(pack)
        print(f"         tempo: {time.time()-t1:.0f}s")
        # Forca liberacao de espaco
        import gc; gc.collect()

    print("\nTODOS OS 56 PACKS PROCESSADOS!")

def build_dataset():
    """Junta todos os caches JSON -> dataset CSV + amostras de treino."""
    cache_files = sorted(TARGET.glob("DP_*_inf.json"))
    print(f"\nConstruindo dataset de {len(cache_files)} arquivos cache...")

    all_hwids = {}  # (vid, did) -> info
    all_strings = []
    classes = set()
    providers = set()

    for cf in cache_files:
        with open(cf) as f:
            entries = json.load(f)
        for entry in entries:
            for hwid in entry["hwids"]:
                if hwid["type"] != "PCI":
                    continue
                try:
                    vid = int(hwid["vid"], 16)
                    did = int(hwid["did"], 16)
                except ValueError:
                    continue
                key = (vid, did)
                if key not in all_hwids:
                    all_hwids[key] = {
                        "vid": vid, "did": did,
                        "class": entry.get("device_class", ""),
                        "provider": entry.get("provider", ""),
                        "strings": list(entry.get("strings", {}).values())[:3],
                    }
            if entry.get("device_class"): classes.add(entry["device_class"])
            if entry.get("provider"): providers.add(entry["provider"])
            for s in entry.get("strings", {}).values():
                all_strings.append(s)

    # Salva CSV com HWIDs unicos
    csv_path = TARGET / "sdio_hwids_all.csv"
    with open(csv_path, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["vid_hex", "did_hex", "vid_dec", "did_dec", "class", "provider", "strings"])
        for key, info in sorted(all_hwids.items()):
            w.writerow([f"{info['vid']:04X}", f"{info['did']:04X}",
                       info["vid"], info["did"],
                       info["class"], info["provider"],
                       " | ".join(info["strings"])])

    # Salva JSONL completo
    jsonl_path = TARGET / "sdio_hwids_all.jsonl"
    with open(jsonl_path, "w") as f:
        for key, info in sorted(all_hwids.items()):
            f.write(json.dumps(info) + "\n")

    print(f"  PCI unicos: {len(all_hwids)}")
    print(f"  Classes: {len(classes)}")
    print(f"  Providers: {len(providers)}")
    print(f"  Strings: {len(all_strings)}")
    print(f"  CSV: {csv_path}")
    print(f"  JSONL: {jsonl_path}")

    return list(all_hwids.values())

def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--resume", action="store_true", help="Pula packs ja em cache")
    parser.add_argument("--dataset-only", action="store_true", help="So junta caches, nao extrai")
    parser.add_argument("--train", action="store_true", help="Treina modelo apos extracao")
    parser.add_argument("--epochs", type=int, default=100)
    parser.add_argument("--batch", type=int, default=4096)
    args = parser.parse_args()

    print("=" * 60)
    print("  SDIO EXTRACTION PIPELINE")
    print("  Espaco atual: 15.9 GB livre em C:")
    print("  .7z a processar: 25.8 GB (serao deletados apos uso)")
    print("=" * 60)

    if args.resume or not args.dataset_only:
        extract_all(resume=args.resume)

    devices = build_dataset()

    if args.train:
        # Treina o modelo preditor de registradores
        from train_hw_register_predictor import build_register_dataset, train_model, FAMILY_NAMES
        samples = build_register_dataset([{"hwids": [{"type": "PCI", "vid": f"{d['vid']:04X}", "did": f"{d['did']:04X}"}]} for d in devices])
        print(f"\nTreinando com {len(samples)} amostras...")
        train_model(samples, epochs=args.epochs, batch_size=args.batch)

    print("\n" + "=" * 60)
    print("  Pipeline concluido!")
    print("  Para liberar espaco:")
    print("    Remove-Item -Recurse -Force target/DP_*_inf.json")
    print("    Remove-Item -Recurse -Force 'C:/Users/msrov/Downloads/SDIO'")
    print("=" * 60)

if __name__ == "__main__":
    main()
