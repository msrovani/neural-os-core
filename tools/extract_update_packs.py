#!/usr/bin/env python3
"""Extract UPDATE DriverPacks (NVIDIA, AMD, Intel Video) e mescla com dataset existente."""
import os, sys, re, json, subprocess, tempfile, shutil, time, csv
from pathlib import Path
from collections import defaultdict
sys.path.insert(0, str(Path(__file__).parent))

SZ = r"C:\Program Files\7-Zip\7z.exe"
UPDATE_DIR = Path(r"C:\Users\msrov\Downloads\SDIO\update\SDIO_Update\drivers")
TARGET = Path("target")
TARGET.mkdir(exist_ok=True)

def parse_inf(text):
    data = {"hwids": [], "device_class": "", "provider": ""}
    for m in re.finditer(r'PCI\s*\\\s*VEN_(\w{4})&DEV_(\w{4})', text, re.I):
        data["hwids"].append({"type":"PCI", "vid":m.group(1), "did":m.group(2)})
    for m in re.finditer(r'USB\s*\\\s*VID_(\w{4})&PID_(\w{4})', text, re.I):
        data["hwids"].append({"type":"USB", "vid":m.group(1), "did":m.group(2)})
    for m in re.finditer(r'Class\s*=\s*(\w+)', text, re.I): data["device_class"] = m.group(1)
    for m in re.finditer(r'Provider\s*=\s*%([^%]+)%', text, re.I): data["provider"] = m.group(1)
    return data if data["hwids"] else None

def process_pack(pack):
    cache = TARGET / f"{pack.stem}_inf.json"
    if cache.exists():
        with open(cache) as f: results = json.load(f)
        nh = sum(len(e["hwids"]) for e in results)
        print(f"  [CACHE] {pack.name}: {len(results)} .inf, {nh} HWIDs")
        return

    if pack.stat().st_size == 0:
        print(f"  [VAZIO] {pack.name}: 0 bytes, pulando")
        return

    tmp = tempfile.mkdtemp()
    results = []
    try:
        r = subprocess.run([SZ, "l", str(pack)], capture_output=True, text=True, timeout=120)
        infs = [p.split()[-1] for p in r.stdout.split("\n") if len(p.split())>4 and p.split()[-1].lower().endswith(".inf")]

        for inf_path in infs[:1000]:
            od = os.path.join(tmp, os.path.dirname(inf_path))
            os.makedirs(od, exist_ok=True)
            subprocess.run([SZ, "e", str(pack), f"-o{od}", inf_path, "-y"], capture_output=True, timeout=120)
            extracted = os.path.join(od, os.path.basename(inf_path))
            if os.path.exists(extracted):
                try:
                    with open(extracted, "r", encoding="utf-8", errors="replace") as f:
                        p = parse_inf(f.read())
                    if p: results.append(p)
                except: pass
                try: os.remove(extracted)
                except: pass

        with open(cache, "w") as f: json.dump(results, f)
        nh = sum(len(e["hwids"]) for e in results)
        print(f"  [OK] {pack.name}: {len(results)} .inf, {nh} HWIDs")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
        try:
            sz = pack.stat().st_size
            pack.unlink()
            print(f"  [DEL] {sz//(1024*1024)}MB freed")
        except: pass

def merge_with_existing():
    """Mescla novos dados com dataset existente de 44K."""
    print("\n[MERGE] Lendo dataset existente...")
    exist_path = TARGET / "hw_all_unified.csv"
    existing = {}
    if exist_path.exists():
        with open(exist_path, newline="") as f:
            for row in csv.DictReader(f):
                key = (int(row["vid"]), int(row["did"]))
                existing[key] = row

    print(f"   Existentes: {len(existing)}")

    # Carrega novos caches
    cache_files = sorted(TARGET.glob("DP_Videos_*_inf.json")) + sorted(TARGET.glob("DP_Video_*_inf.json"))
    new_devices = defaultdict(list)

    for cf in cache_files:
        with open(cf) as f:
            entries = json.load(f)
        for entry in entries:
            for hwid in entry["hwids"]:
                if hwid["type"] != "PCI": continue
                try:
                    vid = int(hwid["vid"], 16)
                    did = int(hwid["did"], 16)
                except: continue
                key = (vid, did)
                if key not in existing:
                    new_devices[key].append({
                        "class": entry.get("device_class", ""),
                        "provider": entry.get("provider", ""),
                    })

    print(f"   Novos: {len(new_devices)}")
    total = len(existing) + len(new_devices)
    print(f"   Total: {total}")

    return existing, new_devices

def retrain(existing, new_devices):
    """Retreina HW Expert com dataset completo."""
    from download_hw_databases import FAMILIAS, FAM2IDX, familia_para
    import torch, torch.nn as nn, torch.nn.functional as F, torch.optim as optim
    from export_hw_bitnet import HwExpertTransformer

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"\n[TREINO] Device: {device}")

    # Constrói dataset completo
    unified = []
    for key in existing:
        vid, did = key
        fam = familia_para(vid, did)
        unified.append({"vid": vid, "did": did, "family": fam})
    for key in new_devices:
        vid, did = key
        fam = familia_para(vid, did)
        unified.append({"vid": vid, "did": did, "family": fam})

    print(f"   Amostras: {len(unified)}")
    for fn in FAMILIAS:
        cnt = sum(1 for d in unified if d["family"] == fn)
        print(f"     {fn:20s} {cnt:5d}")

    vocab = 64
    inp = torch.tensor([[(d["vid"]>>8)%vocab, d["vid"]%vocab, (d["did"]>>8)%vocab, d["did"]%vocab] for d in unified], dtype=torch.long)
    tgt = torch.tensor([FAM2IDX.get(d["family"], 0) for d in unified], dtype=torch.long)

    model = HwExpertTransformer(hidden=32, vocab=64, num_layers=4, num_heads=4, ffn_dim=64).to(device)
    n = sum(p.numel() for p in model.parameters())
    print(f"   Parametros: {n:,}")

    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(inp, tgt),
        batch_size=128, shuffle=True, drop_last=True)

    opt = optim.AdamW(model.parameters(), lr=3e-3, weight_decay=1e-5)
    sched = optim.lr_scheduler.OneCycleLR(opt, 3e-3, steps_per_epoch=max(len(loader),1), epochs=300, last_epoch=-1)

    best = float("inf")
    t0 = time.time()
    for ep in range(300):
        model.train()
        tl = 0.0; nb = 0
        for x, y in loader:
            x, y = x.to(device), y.to(device)
            opt.zero_grad()
            logits = model(x)
            loss = F.cross_entropy(logits.mean(dim=1), y)
            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step(); sched.step()
            tl += loss.item(); nb += 1
        avg = tl / max(nb, 1)
        if avg < best:
            best = avg
            model.export_bitnet(TARGET / "hw_expert_tf.bitnet", tok_data=b"hwexpert_v2")
        if (ep+1) % 50 == 0 or ep == 0:
            print(f"  Ep {ep+1:4d}/300 | loss={avg:.5f} | best={best:.5f}")

    # Avaliação
    model.eval()
    with torch.no_grad():
        logits = model(inp.to(device))
        pred = logits.mean(dim=1).argmax(1)
        correct = (pred == tgt.to(device)).sum().item()
        acc = correct / len(unified)
        print(f"\n  Acuracia: {acc*100:.1f}% ({correct}/{len(unified)})")
        print(f"  Tempo: {time.time()-t0:.0f}s")

    model.export_bitnet(TARGET / "hw_expert_tf.bitnet", tok_data=b"hwexpert_v2")

def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--extract-only", action="store_true")
    parser.add_argument("--train-only", action="store_true")
    parser.add_argument("--skip-existing", action="store_true", help="Pula packs ja em cache")
    args = parser.parse_args()

    print("=" * 60)
    print("  UPDATE PACKS: NVIDIA + AMD + Intel Video")
    free = (os.stat("C:").st_dev) if hasattr(os, "stat") else 0
    print("=" * 60)

    if not args.train_only:
        packs = sorted(UPDATE_DIR.glob("DP_*.7z"))
        non_zero = [p for p in packs if p.stat().st_size > 0]
        print(f"Packs: {len(non_zero)} com dados, {len(packs)-len(non_zero)} ainda baixando (0 bytes)")
        print()

        for i, pack in enumerate(non_zero):
            if args.skip_existing and (TARGET / f"{pack.stem}_inf.json").exists():
                cache = TARGET / f"{pack.stem}_inf.json"
                with open(cache) as f: r = json.load(f)
                nh = sum(len(e["hwids"]) for e in r)
                print(f"  [{i+1:2d}/{len(non_zero)}] [SKIP] {pack.name} ({len(r)} .inf, {nh} HWIDs)")
                continue
            t1 = time.time()
            process_pack(pack)
            print(f"         {time.time()-t1:.0f}s")

    if not args.extract_only:
        existing, new_devices = merge_with_existing()
        retrain(existing, new_devices)

if __name__ == "__main__":
    main()
