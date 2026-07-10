#!/usr/bin/env python3
"""download_hw_databases.py — Baixa PCI + USB ID databases e mescla com SDIO.
Uso: python download_hw_databases.py [--epochs N] [--batch N]
"""

import os, sys, json, csv, urllib.request, time, math
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))

TARGET = Path(__file__).parent / "target"
TARGET.mkdir(exist_ok=True)

# ─── 1. PCI IDs ────────────────────────────────────────────────────────────

PCI_URL = "https://pci-ids.ucw.cz/v2.2/pci.ids"
PCI_CACHE = TARGET / "pci_ids.json"

def download_pci():
    if PCI_CACHE.exists():
        with open(PCI_CACHE) as f: return json.load(f)
    print("[PCI] Baixando pci.ids...", flush=True)
    try:
        req = urllib.request.urlopen(PCI_URL, timeout=30)
        lines = req.read().decode("latin-1").split("\n")
        vendors = {}
        cv = None
        for line in lines:
            if not line or line[0] == "#": continue
            if line[0] != "\t":
                p = line.strip().split(" ", 1)
                if len(p) >= 2:
                    cv = p[0].upper()
                    vendors[cv] = {"name": p[1], "devices": {}}
            elif cv and line[0] == "\t":
                p = line.strip().split(" ", 1)
                if len(p) >= 2:
                    vendors[cv]["devices"][p[0].upper()] = p[1]
        # Converte para lista plana
        entries = []
        for vid, vi in vendors.items():
            try: int(vid, 16)
            except: continue
            for did, dn in vi["devices"].items():
                try: int(did, 16)
                except: continue
                entries.append({"vid": vid, "did": did, "name": dn, "vendor": vi["name"]})
        with open(PCI_CACHE, "w") as f: json.dump(entries, f)
        print(f"  [OK] {len(entries)} entradas PCI", flush=True)
        return entries
    except Exception as e:
        print(f"  [ERRO] PCI download: {e}", flush=True)
        return []

# ─── 2. USB IDs ────────────────────────────────────────────────────────────

USB_URL = "http://www.linux-usb.org/usb.ids"
USB_CACHE = TARGET / "usb_ids.json"

def download_usb():
    if USB_CACHE.exists():
        with open(USB_CACHE) as f: return json.load(f)
    print("[USB] Baixando usb.ids...", flush=True)
    try:
        req = urllib.request.urlopen(USB_URL, timeout=30)
        lines = req.read().decode("latin-1").split("\n")
        vendors = {}
        cv = None
        for line in lines:
            if not line or line[0] == "#": continue
            if line[0] != "\t":
                p = line.strip().split(" ", 1)
                if len(p) >= 2:
                    cv = p[0].upper()
                    vendors[cv] = {"name": p[1], "devices": {}}
            elif cv and line[0] == "\t":
                p = line.strip().split(" ", 1)
                if len(p) >= 2:
                    vendors[cv]["devices"][p[0].upper()] = p[1]
        entries = []
        for vid, vi in vendors.items():
            try: int(vid, 16)
            except: continue
            for did, dn in vi["devices"].items():
                try: int(did, 16)
                except: continue
                entries.append({"vid": vid, "did": did, "name": dn, "vendor": vi["name"]})
        with open(USB_CACHE, "w") as f: json.dump(entries, f)
        print(f"  [OK] {len(entries)} entradas USB", flush=True)
        return entries
    except Exception as e:
        print(f"  [ERRO] USB download: {e}", flush=True)
        return []

# ─── 3. SDIO data ─────────────────────────────────────────────────────────

def carregar_sdio():
    csv_path = TARGET / "sdio_hwids_all.csv"
    if not csv_path.exists():
        print("[SDIO] Cache CSV nao encontrado", flush=True)
        return []
    devices = []
    with open(csv_path, newline="", encoding="utf-8") as f:
        for row in csv.DictReader(f):
            devices.append(row)
    print(f"  [SDIO] {len(devices)} entradas", flush=True)
    return devices

# ─── 4. Mapa de familias ──────────────────────────────────────────────────

REGISTER_MAPS = {
    "IntelWiFi":        (0x1000,0x1004,0x0008,0x2000,0x2004,0x0001,64,2048),
    "IntelEthernet":    (0x0000,0x0004,0x0100,0x0018,0x001C,0x0001,32,2048),
    "RealtekWiFi":      (0x00A0,0x00A4,0x002C,0x00D0,0x00D4,0x8002,16,2048),
    "RealtekEthernet":  (0x0020,0x0030,0x0044,0x0038,0x003C,0x0001,8,2048),
    "AtherosWiFi":      (0x0800,0x0804,0x0010,0x0C00,0x0C04,0x0001,32,2048),
    "BroadcomWiFi":     (0x0500,0x0504,0x0020,0x0600,0x0604,0x0100,32,2048),
    "VirtIO":           (0x0000,0x0000,0x0000,0x0000,0x0000,0x0000,64,4096),
    "GenericPCI":       (0x1000,0x1000,0x0000,0x0000,0x0000,0x0000,32,2048),
}
FAMILIAS = sorted(REGISTER_MAPS.keys())
FAM2IDX = {n:i for i,n in enumerate(FAMILIAS)}

# Heuristica de familia por vendor (mesma logica de generate_register_map)
def familia_para(vid, did):
    """Determina familia de registradores para (vendor_id, device_id)."""
    v = int(vid, 16) if isinstance(vid, str) else vid
    d = int(did, 16) if isinstance(did, str) else did

    if v == 0x8086:
        if (0x08B1 <= d <= 0x2726) or d in (0x3165,0x3166,0x06F0,0x02F0,0x2526,0x2527,0x2723,0x2725,0x2726,0x24F3,0x24F4,0x24F5,0x24F6,0x24FD) or (d>>8)==0x08:
            return "IntelWiFi"
        if d in (0x100E,0x105E,0x10D3,0x10D5,0x10DE,0x10EA,0x10F5,0x10FB,0x10C9,0x1526,0x1527,0x154D,0x156F,0x1570,0x1533,0x1538,0x1539,0x1502,0x1503):
            return "IntelEthernet"
        return "GenericPCI"
    if v == 0x10EC:
        return "RealtekWiFi" if d in (0x8176,0x8179,0x8812) else ("RealtekEthernet" if d in (0x8139,0x8168,0x8169) else "GenericPCI")
    if v == 0x0BDA: return "RealtekWiFi"
    if v == 0x168C: return "AtherosWiFi"
    if v == 0x14E4: return "BroadcomWiFi"
    if v in (0x1AF4, 0x1234): return "VirtIO"
    if v in (0x17CB, 0x13D7): return "AtherosWiFi"
    return "GenericPCI"

# ─── 5. Merge + Treino ────────────────────────────────────────────────────

def merge_datasets(pci_entries, usb_entries, sdio_devices):
    """Mescla PCI + USB + SDIO em dataset unificado."""
    print("\n[MERGE] Mesclando datasets...", flush=True)
    seen = set()
    unified = []

    # PCI IDs
    for e in pci_entries:
        try:
            vid = int(e["vid"], 16) if isinstance(e["vid"], str) else e["vid"]
            did = int(e["did"], 16) if isinstance(e["did"], str) else e["did"]
        except: continue
        key = (vid, did)
        if key not in seen:
            seen.add(key)
            fam = familia_para(vid, did)
            unified.append({"vid": vid, "did": did, "family": fam, "source": "pci_ids"})

    # USB IDs (mapeados como GenericPCI por serem USB, nao PCI)
    for e in usb_entries:
        try:
            vid = int(e["vid"], 16) if isinstance(e["vid"], str) else e["vid"]
            did = int(e["did"], 16) if isinstance(e["did"], str) else e["did"]
        except: continue
        key = (0x10000 + vid, did)  # namespace separado
        if key not in seen:
            seen.add(key)
            fam = familia_para(vid, did)
            unified.append({"vid": vid, "did": did, "family": fam, "source": "usb_ids"})

    # SDIO PCI data
    for row in sdio_devices:
        vid = int(row["vid_dec"])
        did = int(row["did_dec"])
        key = (vid, did)
        if key not in seen:
            seen.add(key)
            fam = familia_para(vid, did)
            unified.append({"vid": vid, "did": did, "family": fam, "source": "sdio"})

    print(f"  PCI IDs: {len(pci_entries):>7,} -> {sum(1 for u in unified if u['source']=='pci_ids'):,} unicos")
    print(f"  USB IDs: {len(usb_entries):>7,} -> {sum(1 for u in unified if u['source']=='usb_ids'):,} unicos")
    print(f"  SDIO:    {len(sdio_devices):>7,} -> {sum(1 for u in unified if u['source']=='sdio'):,} unicos")
    print(f"  TOTAL: {len(unified):,} dispositivos unicos")

    # Salva dataset unificado
    csv_path = TARGET / "hw_all_unified.csv"
    with open(csv_path, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["vid","did","vid_hex","did_hex","family","source"])
        for d in unified:
            w.writerow([d["vid"], d["did"], f"{d['vid']:04X}", f"{d['did']:04X}", d["family"], d["source"]])
    print(f"  CSV unificado: {csv_path}")

    return unified

def treinar(unified, epochs=300, batch=128):
    """Treina HW Expert Transformer com dataset unificado."""
    print(f"\n[TREINO] {len(unified)} amostras, {epochs} epocas, batch={batch}", flush=True)

    import torch
    import torch.nn as nn
    import torch.nn.functional as F
    import torch.optim as optim
    import numpy as np

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"  Device: {device}", flush=True)

    # Contagem por familia
    for fn in FAMILIAS:
        cnt = sum(1 for d in unified if d["family"] == fn)
        print(f"    {fn:20s} {cnt:5d} devices")

    # Prepara dataset
    vocab = 64
    inp = torch.tensor([[(d["vid"]>>8)%vocab, d["vid"]%vocab, (d["did"]>>8)%vocab, d["did"]%vocab] for d in unified], dtype=torch.long)
    fam_idx = torch.tensor([FAM2IDX.get(d["family"], 0) for d in unified], dtype=torch.long)
    tgt = fam_idx

    # Modelo
    from export_hw_bitnet import HwExpertTransformer
    model = HwExpertTransformer(hidden=32, vocab=64, num_layers=4, num_heads=4, ffn_dim=64).to(device)
    n = sum(p.numel() for p in model.parameters())
    print(f"  Parametros: {n:,}", flush=True)

    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(inp, tgt),
        batch_size=batch, shuffle=True, drop_last=True)

    opt = optim.AdamW(model.parameters(), lr=3e-3, weight_decay=1e-5)
    sched = optim.lr_scheduler.OneCycleLR(opt, 3e-3, steps_per_epoch=max(len(loader),1), epochs=epochs, last_epoch=-1)

    best = float("inf")
    t0 = time.time()
    for ep in range(epochs):
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
            print(f"  Ep {ep+1:4d}/{epochs} | loss={avg:.5f} | best={best:.5f}", flush=True)

    # Avaliacao
    model.eval()
    with torch.no_grad():
        logits = model(inp.to(device))
        pred = logits.mean(dim=1).argmax(1)
        correct = (pred == tgt.to(device)).sum().item()
        acc = correct / len(unified)
        print(f"\n  Acuracia: {acc*100:.1f}% ({correct}/{len(unified)})", flush=True)
        print(f"  Tempo: {time.time()-t0:.0f}s", flush=True)

    model.export_bitnet(TARGET / "hw_expert_tf.bitnet", tok_data=b"hwexpert_v2")
    return model, acc

# ─── Main ──────────────────────────────────────────────────────────────────

def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--epochs", type=int, default=300)
    parser.add_argument("--batch", type=int, default=128)
    parser.add_argument("--download-only", action="store_true")
    parser.add_argument("--train-only", action="store_true")
    args = parser.parse_args()

    print("=" * 60)
    print("  HW DATABASES: PCI + USB + SDIO")
    print("=" * 60)

    if not args.train_only:
        pci = download_pci()
        usb = download_usb()
        sdio = carregar_sdio()
    else:
        # Carrega caches existentes
        pci = json.load(open(PCI_CACHE)) if PCI_CACHE.exists() else []
        usb = json.load(open(USB_CACHE)) if USB_CACHE.exists() else []
        sdio = carregar_sdio()
        print(f"  PCI cache: {len(pci)} entradas")
        print(f"  USB cache: {len(usb)} entradas")
        print(f"  SDIO cache: {len(sdio)} entradas")

    if not args.download_only:
        unified = merge_datasets(pci, usb, sdio)
        treinar(unified, epochs=args.epochs, batch=args.batch)

    print("\n[OK] Pipeline concluido!")
    print(f"  Modelo: target/hw_expert_tf.bitnet")
    print(f"  Dataset unificado: target/hw_all_unified.csv")

if __name__ == "__main__":
    main()
