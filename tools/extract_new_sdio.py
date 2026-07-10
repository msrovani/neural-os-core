#!/usr/bin/env python3
"""extract_new_sdio.py — neural-os-core v1.0
Extracao de 52GB de novos DriverPacks SDIO.
Processa .7z → .inf → HWIDs + metadados → merge → retreino HW Expert.

Uso: python tools/extract_new_sdio.py [--extract-only] [--train-only]

Fluxo:
  1. Varre SDIO_DIR atras de .7z
  2. Para cada pack: extrai .inf, parseia HWIDs, salva cache JSON
  3. APAGA .7z apos processar (libera espaco)
  4. Merge com dataset PCI+USB existente
  5. Retreina HW Expert Transformer na GPU
"""

import os, re, json, subprocess, tempfile, shutil, time, csv, sys
from pathlib import Path
from collections import defaultdict

SZ = r"C:\Program Files\7-Zip\7z.exe"
SDIO_DIR = Path(r"C:\Users\msrov\Downloads\sdio7z")
TARGET = Path(__file__).parent.parent / "target"
TARGET.mkdir(exist_ok=True)

sys.path.insert(0, str(Path(__file__).parent))

def parse_inf(text):
    data = {"hwids": [], "device_class": "", "provider": ""}
    for m in re.finditer(r'PCI\\VEN_(\w{4})&DEV_(\w{4})', text, re.I):
        data["hwids"].append({"type":"PCI", "vid":m.group(1).upper(), "did":m.group(2).upper()})
    for m in re.finditer(r'USB\\VID_(\w{4})&PID_(\w{4})', text, re.I):
        data["hwids"].append({"type":"USB", "vid":m.group(1).upper(), "did":m.group(2).upper()})
    for m in re.finditer(r'ACPI\\(\w{8})', text, re.I):
        data["hwids"].append({"type":"ACPI", "id":m.group(1).upper()})
    for m in re.finditer(r'Class\s*=\s*(\w+)', text, re.I):
        data["device_class"] = m.group(1)
    for m in re.finditer(r'Provider\s*=\s*%([^%]+)%', text, re.I):
        data["provider"] = m.group(1)
    return data if data["hwids"] else None

def extract_one_pack(pack):
    if not pack.exists():
        return 0, 0, False

    cache = TARGET / f"{pack.stem}_inf.json"
    if cache.exists():
        with open(cache) as f: r = json.load(f)
        nh = sum(len(e["hwids"]) for e in r)
        return len(r), nh, True

    if pack.stat().st_size == 0:
        pack.unlink(missing_ok=True)
        return 0, 0, False

    tmp = tempfile.mkdtemp()
    results = []
    inf_count = 0
    try:
        # Extrai TODOS os .inf recursivamente (-r) de uma vez
        r = subprocess.run([SZ, "x", str(pack), f"-o{tmp}", "-r", "*.inf", "-y"],
                          capture_output=True, text=True, timeout=300)
        out = r.stdout + r.stderr

        # Conta quantos arquivos foram extraidos
        for line in out.split("\n"):
            if "Extracting" in line and ".inf" in line.lower():
                inf_count += 1

        # Varre arvore de diretorios por .inf
        for root, dirs, files in os.walk(tmp):
            for fname in files:
                if not fname.lower().endswith(".inf"):
                    continue
                fpath = os.path.join(root, fname)
                try:
                    with open(fpath, "r", encoding="utf-8", errors="replace") as f:
                        p = parse_inf(f.read())
                    if p: results.append(p)
                except: pass

        # SAFETY: se 7z nao extraiu nada, NAO APAGA o .7z
        if inf_count == 0 and not results:
            print(f"  [ERRO] {pack.name}: 7z extraiu 0 .inf — preservando .7z!", flush=True)
            shutil.rmtree(tmp, ignore_errors=True)
            return 0, 0, False

        with open(cache, "w") as f: json.dump(results, f)
        nh = sum(len(e["hwids"]) for e in results)
        return len(results), nh, False
    except subprocess.TimeoutExpired:
        print(f"  [TIMEOUT] {pack.name} — preservando .7z!", flush=True)
        shutil.rmtree(tmp, ignore_errors=True)
        return 0, 0, False
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

def extract_all():
    packs = sorted(SDIO_DIR.glob("DP_*.7z"), key=lambda p: p.stat().st_size)  # menores primeiro
    if not packs:
        print("[ERRO] Nenhum .7z encontrado em", SDIO_DIR)
        print("  Downloads ainda em andamento?")
        return

    total_gb = sum(p.stat().st_size for p in packs) / (1024**3)
    print(f"\nProcessando {len(packs)} packs ({total_gb:.0f} GB)")
    print(f"Livre: {os.stat(SDIO_DIR).st_dev}")  # approximate
    import psutil
    try:
        free = psutil.disk_usage(str(SDIO_DIR)).free / (1024**3)
        print(f"Livre: {free:.0f} GB")
    except: pass

    total_inf = 0
    total_hwids = 0
    t0 = time.time()

    for i, pack in enumerate(packs):
        t1 = time.time()
        n_inf, n_hwids, cached = extract_one_pack(pack)
        total_inf += n_inf
        total_hwids += n_hwids
        tag = "[CACHE]" if cached else "[OK]"
        status = f"  {tag} [{i+1:2d}/{len(packs)}] {pack.name:45s} {n_inf:4d} .inf {n_hwids:6d} HWIDs"
        if not cached:
            if n_inf > 0 and n_hwids > 0:
                try:
                    sz = pack.stat().st_size
                    pack.unlink(missing_ok=True)
                    status += f"  APAGADO {sz//(1024*1024)}MB"
                except: pass
            else:
                status += "  PRESERVADO (sem dados extraidos)"
        print(status + f"  {time.time()-t1:.0f}s", flush=True)
        # Libera espaco a cada 5 packs
        if (i+1) % 5 == 0:
            import gc; gc.collect()
            free_after = psutil.disk_usage(str(SDIO_DIR)).free / (1024**3)
            print(f"  [ESPACO] {free_after:.0f} GB livre", flush=True)

    print(f"\nResumo: {total_inf} .inf, {total_hwids} HWIDs, {time.time()-t0:.0f}s")

def merge_and_train():
    """Merge caches novos com PCI+USB existente + retreino HW Expert."""
    print("\n[MERGE] Consolidando caches...", flush=True)
    from collections import defaultdict

    # Carrega caches novos
    cache_files = sorted(TARGET.glob("DP_*_inf.json"))
    seen = set()
    new_devices = []

    for cf in cache_files:
        with open(cf) as f: entries = json.load(f)
        for entry in entries:
            for hwid in entry["hwids"]:
                if hwid["type"] != "PCI": continue
                try:
                    vid = int(hwid["vid"], 16)
                    did = int(hwid["did"], 16)
                except: continue
                key = (vid, did)
                if key not in seen:
                    seen.add(key)
                    new_devices.append({"vid": vid, "did": did})

    print(f"  Novos PCI unicos: {len(new_devices)}", flush=True)

    # Treino
    from train_hw_final import BitNetTF, DEVICE, FAMILIAS, FAM2IDX, familia
    import torch, torch.nn as nn, torch.nn.functional as F, torch.optim as optim

    # Merge com dados PCI+USB existentes
    pci_path = TARGET / "pci_ids.json"
    usb_path = TARGET / "usb_ids.json"
    all_devices = list(new_devices)

    if pci_path.exists():
        with open(pci_path) as f: pci = json.load(f)
        for e in pci:
            try:
                vid = int(e["v"], 16); did = int(e["d"], 16)
                if (vid, did) not in seen: seen.add((vid, did)); all_devices.append({"vid": vid, "did": did})
            except: pass
        print(f"  +PCI: {len(pci)}", flush=True)

    if usb_path.exists():
        with open(usb_path) as f: usb = json.load(f)
        for e in usb:
            try:
                vid = int(e["v"], 16); did = int(e["d"], 16)
                if (vid, did) not in seen: seen.add((vid, did)); all_devices.append({"vid": vid, "did": did})
            except: pass
        print(f"  +USB: {len(usb)}", flush=True)

    print(f"  Total: {len(all_devices)} dispositivos", flush=True)

    vocab = 64
    inp = torch.tensor([[(d["vid"]>>8)%vocab, d["vid"]%vocab, (d["did"]>>8)%vocab, d["did"]%vocab] for d in all_devices], dtype=torch.long)
    tgt = torch.tensor([FAM2IDX.get(familia(d["vid"], d["did"]), 0) for d in all_devices], dtype=torch.long)

    for fn in FAMILIAS:
        cnt = sum(1 for d in all_devices if familia(d["vid"], d["did"]) == fn)
        print(f"    {fn:20s} {cnt:5d}", flush=True)

    model = BitNetTF().to(DEVICE)
    bs = min(1024, len(all_devices))
    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(inp, tgt), batch_size=bs, shuffle=True,
        drop_last=len(all_devices) > bs)

    opt = optim.AdamW(model.parameters(), lr=3e-3, weight_decay=1e-5)
    sched = optim.lr_scheduler.OneCycleLR(opt, 3e-3, steps_per_epoch=max(len(loader),1), epochs=200, last_epoch=-1)

    best = float("inf")
    t0 = time.time()
    for ep in range(200):
        model.train()
        tl = 0.0; nb = 0
        for x, y in loader:
            x, y = x.to(DEVICE), y.to(DEVICE)
            opt.zero_grad()
            loss = F.cross_entropy(model(x).mean(dim=1), y)
            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step(); sched.step()
            tl += loss.item(); nb += 1
        avg = tl / max(nb, 1)
        if avg < best:
            best = avg
            model.export(TARGET / "hw_expert_tf.bitnet")
        if (ep+1) % 50 == 0 or ep == 0:
            print(f"  Ep {ep+1:4d}/200 | loss={avg:.5f} | best={best:.5f}", flush=True)

    model.eval()
    with torch.no_grad():
        logits = model(inp.to(DEVICE))
        pred = logits.mean(dim=1).argmax(1)
        acc = (pred == tgt.to(DEVICE)).float().mean().item()
        print(f"\n  Acuracia: {acc*100:.1f}% | Tempo: {time.time()-t0:.0f}s", flush=True)
    model.export(TARGET / "hw_expert_tf.bitnet")

def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--extract-only", action="store_true", help="So extrai, nao treina")
    parser.add_argument("--train-only", action="store_true", help="So treina com caches existentes")
    args = parser.parse_args()

    print("=" * 60)
    print("  SDIO EXTRACAO 52GB — Pipeline Completo")
    print("=" * 60)

    packs = list(SDIO_DIR.glob("DP_*.7z"))
    caches = list(TARGET.glob("DP_*_inf.json"))

    if not args.train_only:
        if not packs:
            print("[AVISO] Nenhum .7z encontrado. Downloads ainda em andamento?")
            if not caches:
                print("[ERRO] Sem .7z e sem caches. Nada a processar.")
                return
            print("[INFO] Usando caches existentes para treino.")
        else:
            extract_all()

    if not args.extract_only and (caches or packs):
        merge_and_train()

    print("\n[OK] Pipeline concluido!")
    print(f"  Modelo: {TARGET / 'hw_expert_tf.bitnet'}")

if __name__ == "__main__":
    main()
