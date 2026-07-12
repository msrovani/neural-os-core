#!/usr/bin/env python3
"""Treina HW Expert com dataset combinado: SDIO (171K HWIDs) + Firmware metadata
   (WHENCE 998 + headers + AMD ucode 64) + PCI hardcoded.
Uso: python tools/train_hw_expert_full.py [--epochs 50] [--hidden 64]
"""
import os, sys, json, re, argparse
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from train_gpu_full import BitNetLM, DEVICE, train, gen_pci_dataset, tokenize_pci

ROOT = Path(__file__).parent.parent
TARGET = ROOT / "tools" / "target"

HWID_RE = re.compile(
    r'(?:PCI\\|PCIVEN_)?VEN[_]?([0-9A-F]{4})'
    r'(?:&|_?)DEV[_]?([0-9A-F]{4})'
    r'|(?:USB\\|USBVID_)?VID[_]?([0-9A-F]{4})'
    r'(?:&|_?)PID[_]?([0-9A-F]{4})'
)

def load_sdio_hwids():
    path = TARGET / "sdio_hwids.json"
    if not path.exists():
        print(f"  [--] {path} nao encontrado (rode extract_sdio_hw.py primeiro)")
        return []
    data = json.load(open(path))
    print(f"  [SDIO] {len(data)} HWIDs carregados")
    return data

def load_firmware_metadata():
    path = TARGET / "firmware_metadata.json"
    if not path.exists():
        print(f"  [--] {path} nao encontrado")
        return []
    data = json.load(open(path))
    print(f"  [FW-META] {len(data)} records carregados")
    # Extract HWIDs from metadata
    hwids = set()
    for r in data:
        for h in r.get("hwids", []):
            hwids.add(h)
        # Also extract VEN/DEV from register info fields
        info = r.get("Info", "")
        for m in HWID_RE.finditer(info):
            g = m.groups()
            if g[0] and g[1]:
                hwids.add(f"PCI\\VEN_{g[0]}&DEV_{g[1]}")
    print(f"  [FW-META] {len(hwids)} HWIDs extraidos de metadata")
    return list(hwids)

def load_amd_ucode():
    # AMD microcode patches from firmware_metadata
    path = TARGET / "firmware_metadata.json"
    if not path.exists():
        return []
    data = json.load(open(path))
    ucodes = [(r["family"], r["model"], r.get("stepping", 0))
              for r in data if r.get("type") == "amd_ucode"]
    print(f"  [AMD-UCODE] {len(ucodes)} patches Family/Model/Stepping")
    return ucodes

def extract_hwid_vid_did(hwid_str):
    """Extract (vid, did) from any HWID format."""
    m = HWID_RE.search(hwid_str.upper())
    if m:
        g = m.groups()
        if g[0]:  # PCI VEN_XXXX&DEV_XXXX
            return int(g[0], 16), int(g[1], 16)
        if g[2]:  # USB VID_XXXX&PID_XXXX
            return int(g[2], 16), int(g[3], 16)
    return None, None

def build_dataset(sdio_hwids, fw_hwids, amd_ucodes):
    """Build unified training dataset."""
    vocab = 64
    tokens, targets = [], []

    # 1. SDIO HWIDs (up to 100K)
    for entry in sdio_hwids[:100000]:
        hwid = entry.get("hwid", "") if isinstance(entry, dict) else entry
        vid, did = extract_hwid_vid_did(hwid)
        if vid is None or did is None:
            continue
        # Class from HWID hash
        cls = hash(hwid) % vocab
        tok = [(vid>>8)%vocab, vid%vocab, (did>>8)%vocab, did%vocab]
        tokens.append(tok + [0]*12)
        targets.append([cls] + [0]*15)
    print(f"  [SDIO] {len(tokens)} tokens gerados")

    # 2. Firmware metadata HWIDs
    fw_count = 0
    for hwid in fw_hwids[:10000]:
        vid, did = extract_hwid_vid_did(hwid)
        if vid is None or did is None:
            continue
        cls = hash(f"fw_{hwid}") % vocab
        tok = [(vid>>8)%vocab, vid%vocab, (did>>8)%vocab, did%vocab]
        tokens.append(tok + [0]*12)
        targets.append([cls] + [0]*15)
        fw_count += 1
    print(f"  [FW-META] {fw_count} tokens gerados")

    # 3. AMD ucode (family/model as VID/DID)
    for fam, model, step in amd_ucodes:
        cls = hash(f"amd_f{fam:04x}") % vocab
        tok = [(fam>>8)%vocab, fam%vocab, (model>>8)%vocab, model%vocab]
        tokens.append(tok + [0]*12)
        targets.append([cls] + [0]*15)
    print(f"  [AMD-UCODE] {len(amd_ucodes)} tokens gerados")

    # 4. PCI hardcoded (from original train_hw_expert)
    pci_data = gen_pci_dataset()
    pci_tokens = tokenize_pci(pci_data, vcb=vocab, sl=16)
    for tok, tgt in pci_tokens:
        full_tok = tok[:4] + [0]*12
        full_tgt = [tgt[0]] + [0]*15
        tokens.append(full_tok)
        targets.append(full_tgt)
    print(f"  [PCI] {len(pci_data)} tokens gerados")

    return tokens, targets


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--epochs", type=int, default=50)
    parser.add_argument("--hidden", type=int, default=64)
    parser.add_argument("--layers", type=int, default=4)
    parser.add_argument("--heads", type=int, default=4)
    parser.add_argument("--ff-dim", type=int, default=128)
    parser.add_argument("--batch", type=int, default=2048)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--dry-run", action="store_true", help="So carrega dados, nao treina")
    args = parser.parse_args()

    print("=" * 60)
    print("  HW Expert v2 — Treino com Dataset Combinado")
    print("=" * 60)

    # Load all data sources
    sdio = load_sdio_hwids()
    fw_hwids = load_firmware_metadata()
    amd = load_amd_ucode()

    # Build dataset
    tokens, targets = build_dataset(sdio, fw_hwids, amd)
    combined = list(zip(tokens, targets))
    print(f"\n  Dataset total: {len(combined)} amostras ({len(tokens)} tokens)")

    if args.dry_run:
        print("  [DRY-RUN] Nao treinando")
        return

    if len(combined) == 0:
        print("  [ERRO] Dataset vazio!")
        return

    # Model
    model = BitNetLM(h=args.hidden, v=64, nl=args.layers,
                     nh=args.heads, ff=args.ff_dim).to(DEVICE)
    print(f"  Modelo: {sum(p.numel() for p in model.parameters()):,} params")

    # Train
    train("HW Expert v2", model, combined, 64,
          ep=args.epochs, bs=args.batch, lr=args.lr,
          tok=b"hwexpert_v2_sdio")

if __name__ == "__main__":
    main()
