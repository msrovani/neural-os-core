#!/usr/bin/env python3
"""Treino HW Expert v3 com dataset completo: SDIO + pci.ids + usb.ids + kernel + firmware metadata.
Uso: python tools/train_hw_expert_v3.py --epochs 100 --hidden 128
"""
import os, sys, json, re, argparse, torch
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from train_gpu_full import BitNetLM, DEVICE, train, gen_pci_dataset, tokenize_pci

ROOT = Path(__file__).parent.parent
TARGET = ROOT / "tools" / "target"

# Try to force GPU
if torch.cuda.is_available():
    DEVICE = torch.device("cuda")
    print(f"[GPU] {torch.cuda.get_device_name(0)} ({torch.cuda.get_device_properties(0).total_memory/1e9:.1f}GB)")
else:
    DEVICE = torch.device("cpu")
    print("[GPU] CPU (CUDA indisponivel)")

HWID_RE = re.compile(
    r'(?:PCI\\|PCIVEN_)?VEN[_]?([0-9A-F]{4})'
    r'(?:&|_?)DEV[_]?([0-9A-F]{4})'
    r'|(?:USB\\|USBVID_)?VID[_]?([0-9A-F]{4})'
    r'(?:&|_?)PID[_]?([0-9A-F]{4})'
)

def load_sdio():
    p = TARGET / "sdio_hwids.json"
    if p.exists(): return json.load(open(p))
    return []

def load_pci_usb():
    p = TARGET / "pci_usb_hwids.json"
    if not p.exists(): return [], [], []
    d = json.load(open(p))
    return d.get("pci_ids", []), d.get("usb_ids", []), d.get("kernel_pci", [])

def extract_vid_did(hwid):
    m = HWID_RE.search(hwid.upper())
    if m:
        g = m.groups()
        if g[0]: return int(g[0], 16), int(g[1], 16)
        if g[2]: return int(g[2], 16), int(g[3], 16)
    return None, None

def build_dataset(sdio, pci_ids, usb_ids, kernel_pci):
    vocab = 64
    tokens, targets = [], []
    seen = set()

    def add(vid, did, cls_seed):
        if vid is None or did is None: return
        key = (vid, did)
        if key in seen: return
        seen.add(key)
        cls = hash(cls_seed) % vocab
        tok = [(vid>>8)%vocab, vid%vocab, (did>>8)%vocab, did%vocab]
        tokens.append(tok + [0]*12)
        targets.append([cls] + [0]*15)

    # 1. SDIO (top 100K)
    for entry in sdio[:100000]:
        hwid = entry.get("hwid", "") if isinstance(entry, dict) else entry
        vid, did = extract_vid_did(hwid)
        add(vid, did, f"sdio_{hwid}")

    # 2. pci.ids devices
    for e in pci_ids:
        if e.get("type") != "device": continue
        try:
            vid = int(e["vendor"]) if isinstance(e["vendor"], (int, str)) else 0
            did = int(str(e["device_id"]), 16)
        except: continue
        add(vid, did, f"pci_{e.get('name','?')}")

    # 3. usb.ids devices
    for e in usb_ids:
        if e.get("type") != "device": continue
        try:
            vid = int(e["vendor"]) if isinstance(e["vendor"], (int, str)) else 0
            did = int(str(e["device_id"]), 16)
        except: continue
        add(vid, did, f"usb_{e.get('name','?')}")

    # 4. Kernel PCI tables
    for e in kernel_pci:
        try:
            vid = int(e["vendor"], 16) if isinstance(e["vendor"], str) else int(e["vendor"])
            did = int(e["device"], 16) if isinstance(e["device"], str) else int(e["device"])
        except: continue
        add(vid, did, f"kernel_{vid:04x}")

    print(f"  Dataset: {len(tokens)} amostras unicas ({len(seen)} VID/DID unicos)")
    return list(zip(tokens, targets))

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--epochs", type=int, default=100)
    parser.add_argument("--hidden", type=int, default=128)
    parser.add_argument("--layers", type=int, default=6)
    parser.add_argument("--heads", type=int, default=8)
    parser.add_argument("--ff-dim", type=int, default=256)
    parser.add_argument("--batch", type=int, default=4096)
    parser.add_argument("--lr", type=float, default=3e-4)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    print("=" * 60)
    print("  HW Expert v3 — Dataset Completo (SDIO + pci/usb-ids + kernel)")
    print("=" * 60)

    sdio = load_sdio()
    pci_ids, usb_ids, kernel_pci = load_pci_usb()
    print(f"  SDIO: {len(sdio)} | pci.ids: {len(pci_ids)} | usb.ids: {len(usb_ids)} | kernel: {len(kernel_pci)}")

    combined = build_dataset(sdio, pci_ids, usb_ids, kernel_pci)

    if args.dry_run:
        print("  [DRY-RUN] OK")
        return

    if len(combined) == 0:
        print("  [ERRO] Dataset vazio!"); return

    model = BitNetLM(h=args.hidden, v=64, nl=args.layers,
                     nh=args.heads, ff=args.ff_dim).to(DEVICE)
    print(f"  Modelo: {sum(p.numel() for p in model.parameters()):,} params ({args.hidden}h, {args.layers}L)")
    print(f"  Device: {DEVICE}")

    train("HW Expert v3", model, combined, 64,
          ep=args.epochs, bs=args.batch, lr=args.lr,
          tok=b"hwexpert_v3_full")

if __name__ == "__main__":
    main()
