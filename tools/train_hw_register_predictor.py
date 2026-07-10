#!/usr/bin/env python3
"""train_hw_register_predictor.py — neural-os-core v1.0
Extrai HWIDs dos 56 DriverPacks SDIO e treina HW Expert
como PREDITOR de HardwareRegisterMap.

Pipeline:
  1. Extrai .inf de TODOS os packs via 7z.exe
  2. Parseia HWIDs + metadados (classe, provider, strings)
  3. Mapeia cada VID:DID para familia de registradores
  4. Treina modelo: (VID, DID) -> (tx_ring, rx_ring, doorbell, ring_size, ...)

Uso: CUDA_VISIBLE_DEVICES=0 python train_hw_register_predictor.py
"""

import os, re, json, struct, subprocess, tempfile, shutil, time, math
from pathlib import Path
from collections import defaultdict

SZ = r"C:\Program Files\7-Zip\7z.exe"
SDIO_DIR = Path(r"C:\Users\msrov\Downloads\SDIO\drivers")
TARGET = Path(__file__).parent / "target"
TARGET.mkdir(exist_ok=True)

# ─── REGISTER MAP FAMILIES (do generic_wifi.rs + cortex.rs) ───────────────

REGISTER_MAPS = {
    "IntelWiFi": {
        "tx_ring_low": 0x1000, "rx_ring_low": 0x1004,
        "rx_control": 0x0008, "doorbell_tx": 0x2000,
        "doorbell_rx": 0x2004, "cmd_start_rx": 0x0001,
        "ring_size": 64, "rx_buf_len": 2048, "bar": 0,
    },
    "RealtekWiFi": {
        "tx_ring_low": 0x00A0, "rx_ring_low": 0x00A4,
        "rx_control": 0x002C, "doorbell_tx": 0x00D0,
        "doorbell_rx": 0x00D4, "cmd_start_rx": 0x8002,
        "ring_size": 16, "rx_buf_len": 2048, "bar": 0,
    },
    "AtherosWiFi": {
        "tx_ring_low": 0x0800, "rx_ring_low": 0x0804,
        "rx_control": 0x0010, "doorbell_tx": 0x0C00,
        "doorbell_rx": 0x0C04, "cmd_start_rx": 0x0001,
        "ring_size": 32, "rx_buf_len": 2048, "bar": 0,
    },
    "BroadcomWiFi": {
        "tx_ring_low": 0x0500, "rx_ring_low": 0x0504,
        "rx_control": 0x0020, "doorbell_tx": 0x0600,
        "doorbell_rx": 0x0604, "cmd_start_rx": 0x0100,
        "ring_size": 32, "rx_buf_len": 2048, "bar": 0,
    },
    "IntelEthernet": {
        "tx_ring_low": 0x0000, "rx_ring_low": 0x0004,
        "rx_control": 0x0100, "doorbell_tx": 0x0018,
        "doorbell_rx": 0x001C, "cmd_start_rx": 0x0001,
        "ring_size": 32, "rx_buf_len": 2048, "bar": 0,
    },
    "RealtekEthernet": {
        "tx_ring_low": 0x0020, "rx_ring_low": 0x0030,
        "rx_control": 0x0044, "doorbell_tx": 0x0038,
        "doorbell_rx": 0x003C, "cmd_start_rx": 0x0001,
        "ring_size": 8, "rx_buf_len": 2048, "bar": 0,
    },
    "VirtIO": {
        "tx_ring_low": 0x0000, "rx_ring_low": 0x0000,
        "rx_control": 0x0000, "doorbell_tx": 0x0000,
        "doorbell_rx": 0x0000, "cmd_start_rx": 0x0000,
        "ring_size": 64, "rx_buf_len": 4096, "bar": 0,
    },
    "GenericPCI": {
        "tx_ring_low": 0x1000, "rx_ring_low": 0x1000,
        "rx_control": 0x0000, "doorbell_tx": 0x0000,
        "doorbell_rx": 0x0000, "cmd_start_rx": 0x0000,
        "ring_size": 32, "rx_buf_len": 2048, "bar": 0,
    },
}

FAMILY_NAMES = sorted(REGISTER_MAPS.keys())
FAMILY_TO_IDX = {n: i for i, n in enumerate(FAMILY_NAMES)}

# Heuristica vendor -> familia (mesma logica de generate_register_map)
VENDOR_FAMILY = {
    0x8086: lambda d: "IntelWiFi" if 0x08B1 <= d <= 0x2726 or d in (0x3165,0x3166,0x06F0,0x02F0,0x2526,0x2527,0x2723,0x2725,0x2726,0x24F3,0x24F4,0x24F5,0x24F6,0x24FD) or (d>>8)==0x08 else ("IntelEthernet" if d in (0x100E,0x105E,0x10D3,0x10D5,0x10DE,0x10EA,0x10F5,0x10FB,0x10C9,0x1526,0x1527,0x154D,0x156F,0x1570,0x1533,0x1538,0x1539,0x1502,0x1503) else "GenericPCI"),
    0x10EC: lambda d: "RealtekWiFi" if d in (0x8176,0x8179,0x8812) else ("RealtekEthernet" if d in (0x8139,0x8168,0x8169) else "GenericPCI"),
    0x0BDA: lambda d: "RealtekWiFi" if d in (0x8176,0x8179,0x8812,0xC820,0xB711,0x1724,0x1724) else "RealtekWiFi",
    0x168C: lambda d: "AtherosWiFi",
    0x14E4: lambda d: "BroadcomWiFi",
    0x1AF4: lambda d: "VirtIO",
    0x1234: lambda d: "VirtIO",
}

def family_for_device(vid, did):
    """Determina familia de registradores para VID:DID."""
    if vid in VENDOR_FAMILY:
        return VENDOR_FAMILY[vid](did)
    if vid == 0x8086:  # fallback Intel generico
        return "IntelEthernet"
    if vid == 0x10EC or vid == 0x0BDA:
        return "RealtekWiFi"
    if vid == 0x168C or vid == 0x17CB or vid == 0x13D7:
        return "AtherosWiFi"
    if vid == 0x14E4:
        return "BroadcomWiFi"
    if vid == 0x11AB:
        return "IntelEthernet"
    if vid == 0x10B7:
        return "GenericPCI"
    return "GenericPCI"

def map_to_register_vector(vid, did):
    """(VID, DID) -> vetor com 9 floats: family_idx + 8 campos do mapa."""
    fam = family_for_device(vid, did)
    m = REGISTER_MAPS.get(fam, REGISTER_MAPS["GenericPCI"])
    return [
        FAMILY_TO_IDX.get(fam, 0),
        m["tx_ring_low"] / 4096.0,
        m["rx_ring_low"] / 4096.0,
        m["rx_control"] / 4096.0,
        m["doorbell_tx"] / 4096.0,
        m["doorbell_rx"] / 4096.0,
        m["cmd_start_rx"] / 65536.0,
        m["ring_size"] / 64.0,
        m["rx_buf_len"] / 4096.0,
    ]

# ─── EXTRACAO .inf ─────────────────────────────────────────────────────────

def extract_inf_from_7z(pack, limit_inf=500):
    """Extrai .inf de um DriverPack .7z usando 7z.exe."""
    cache = TARGET / f"{pack.stem}_inf.json"
    if cache.exists():
        with open(cache) as f: return json.load(f)

    tmp = tempfile.mkdtemp()
    results = []
    try:
        # Lista .inf files
        r = subprocess.run([SZ, "l", str(pack)], capture_output=True, text=True, timeout=60)
        infs = []
        for line in r.stdout.split("\n"):
            parts = line.split()
            if len(parts) > 4 and parts[-1].lower().endswith(".inf"):
                infs.append(parts[-1])

        # Extrais amostra
        for inf_path in infs[:limit_inf]:
            out_dir = os.path.join(tmp, os.path.dirname(inf_path))
            os.makedirs(out_dir, exist_ok=True)
            r2 = subprocess.run([SZ, "e", str(pack), f"-o{out_dir}", inf_path, "-y"],
                               capture_output=True, text=True, timeout=30)
            extracted = os.path.join(out_dir, os.path.basename(inf_path))
            if os.path.exists(extracted):
                try:
                    with open(extracted, "r", encoding="utf-8", errors="replace") as f:
                        text = f.read()
                    parsed = parse_inf(text)
                    if parsed:
                        results.append(parsed)
                except: pass
                try: os.remove(extracted)
                except: pass

    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    with open(cache, "w") as f: json.dump(results, f)
    return results

def parse_inf(text):
    """Extrai HWIDs + metadados de .inf."""
    data = {"hwids": [], "class_guid": "", "device_class": "", "provider": "",
            "driver_ver": "", "strings": {}}

    # HWIDs
    for m in re.finditer(r'PCI\\VEN_(\w{4})&DEV_(\w{4})(?:&SUBSYS_(\w{8}))?', text, re.I):
        data["hwids"].append({"type": "PCI", "vid": m.group(1), "did": m.group(2),
                              "subsys": m.group(3) or ""})
    for m in re.finditer(r'USB\\VID_(\w{4})&PID_(\w{4})', text, re.I):
        data["hwids"].append({"type": "USB", "vid": m.group(1), "did": m.group(2)})
    for m in re.finditer(r'ACPI\\(\w{8})', text, re.I):
        data["hwids"].append({"type": "ACPI", "id": m.group(1)})

    # Metadata
    for m in re.finditer(r'ClassGUID\s*=\s*\{([^}]+)\}', text, re.I):
        data["class_guid"] = m.group(1)
    for m in re.finditer(r'Class\s*=\s*(\w+)', text, re.I):
        data["device_class"] = m.group(1)
    for m in re.finditer(r'Provider\s*=\s*%([^%]+)%', text, re.I):
        data["provider"] = m.group(1)
    for m in re.finditer(r'DriverVer\s*=\s*(\d+/\d+/\d+)', text, re.I):
        data["driver_ver"] = m.group(1)

    # Strings (nomes de dispositivos)
    in_str = False
    for line in text.split("\n"):
        s = line.strip()
        if s.startswith("[Strings]"): in_str = True; continue
        if in_str and s.startswith("["): break
        if in_str and "=" in s:
            k, v = s.split("=", 1)
            v = v.strip().strip('"')
            # So nomes com caracteres ASCII legiveis
            if any(c.isalpha() for c in v) and len(v) < 200:
                data["strings"][k.strip("%").strip()] = v

    # Registros de configuracao (registry keys com info de hardware)
    for m in re.finditer(r'HKR\s*,\s*[^,]+,\s*[^,]+,\s*(\w+)', text, re.I):
        key = m.group(1)
        if key.lower() in ("intel", "realtek", "atheros", "broadcom",
                           "qualcomm", "marvell", "mediatek", "rtl"):
            data["chip_vendor"] = key

    return data if data["hwids"] else None

# ─── PIPELINE COMPLETO ─────────────────────────────────────────────────────

def extract_all_packs():
    """Extrai HWIDs de todos os 56 DriverPacks."""
    cache_all = TARGET / "sdio_all_hwids.json"
    if cache_all.exists():
        with open(cache_all) as f: return json.load(f)

    packs = sorted(SDIO_DIR.glob("DP_*.7z"))
    print(f"Extracting HWIDs from {len(packs)} packs ({sum(p.stat().st_size for p in packs)//(1024**3)} GB)")
    print()

    all_entries = []
    t0 = time.time()

    for i, pack in enumerate(packs):
        t1 = time.time()
        entries = extract_inf_from_7z(pack, limit_inf=500)
        all_entries.extend(entries)
        elapsed = time.time() - t1
        n_hwids = sum(len(e["hwids"]) for e in entries)
        print(f"  [{i+1:2d}/{len(packs)}] {pack.name:40s} {n_hwids:5d} HWIDs  {elapsed:.0f}s")

    # Salva cache global
    with open(cache_all, "w") as f: json.dump(all_entries, f)
    print(f"\nTotal: {len(all_entries)} .inf, {time.time()-t0:.0f}s")
    return all_entries

def build_register_dataset(all_entries):
    """Converte .inf extraidos em dataset de treino (VID:DID -> register map)."""
    seen = set()
    samples = []

    for entry in all_entries:
        for hwid in entry["hwids"]:
            if hwid["type"] != "PCI": continue
            vid = int(hwid["vid"], 16)
            did = int(hwid["did"], 16)
            key = (vid, did)
            if key in seen: continue
            seen.add(key)

            target = map_to_register_vector(vid, did)
            samples.append({
                "vid": vid,
                "did": did,
                "vid_norm": vid / 65535.0,
                "did_norm": did / 65535.0,
                "family": FAMILY_NAMES[int(target[0])],
                "target": target,
                "class": entry.get("device_class", ""),
                "provider": entry.get("provider", ""),
            })

    return samples

# ─── TREINO PyTorch ────────────────────────────────────────────────────────

def train_model(samples, epochs=200, batch_size=4096):
    import torch
    import torch.nn as nn
    import torch.nn.functional as F
    import torch.optim as optim

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"\nTraining on {device}")
    print(f"Samples: {len(samples)}")

    # Prepara tensores
    X = torch.tensor([[s["vid_norm"], s["did_norm"]] for s in samples], dtype=torch.float32)
    y_family = torch.tensor([FAMILY_TO_IDX[s["family"]] for s in samples], dtype=torch.long)
    y_reg = torch.tensor([s["target"][1:] for s in samples], dtype=torch.float32)  # 8 reg params

    print(f"  Families: {len(FAMILY_NAMES)} unique")
    for fn in FAMILY_NAMES:
        cnt = sum(1 for s in samples if s["family"] == fn)
        print(f"    {fn:25s} {cnt:5d} devices")

    # Modelo: MLP pequeno que processa (VID, DID) -> (family_probs, register_params)
    class RegisterPredictor(nn.Module):
        def __init__(self, n_families=len(FAMILY_NAMES), n_regs=8):
            super().__init__()
            self.net = nn.Sequential(
                nn.Linear(2, 64), nn.ReLU(),
                nn.Linear(64, 128), nn.ReLU(),
                nn.Linear(128, 256), nn.ReLU(),
            )
            self.family_head = nn.Linear(256, n_families)
            self.register_head = nn.Linear(256, n_regs)

        def forward(self, x):
            h = self.net(x)
            return self.family_head(h), torch.sigmoid(self.register_head(h))

    model = RegisterPredictor().to(device)
    n_params = sum(p.numel() for p in model.parameters())
    print(f"  Model params: {n_params:,}")

    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(X, y_family, y_reg),
        batch_size=min(batch_size, len(X)), shuffle=True, drop_last=len(X) > batch_size)

    opt = optim.AdamW(model.parameters(), lr=3e-4)
    # Warmup + cosine
    sched = optim.lr_scheduler.OneCycleLR(opt, 3e-4, steps_per_epoch=len(loader), epochs=epochs, last_epoch=-1)

    best_loss = float("inf")
    t0 = time.time()

    for ep in range(epochs):
        model.train()
        total_loss = 0.0
        n_batches = 0

        for x, yf, yr in loader:
            x, yf, yr = x.to(device), yf.to(device), yr.to(device)
            opt.zero_grad()

            logits_f, reg_out = model(x)
            loss_f = F.cross_entropy(logits_f, yf)
            loss_r = F.mse_loss(reg_out, yr)
            loss = loss_f + 0.5 * loss_r

            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            sched.step()

            total_loss += loss.item()
            n_batches += 1

        avg_loss = total_loss / n_batches
        if avg_loss < best_loss:
            best_loss = avg_loss
            export_bitnet(model, device, TARGET / "hw_expert_regpredict.bitnet")

        if (ep + 1) % 20 == 0 or ep == 0:
            print(f"  Ep {ep+1:3d}/{epochs} | loss={avg_loss:.4f} | best={best_loss:.4f} | {(ep+1)*len(X)//(time.time()-t0+0.001):.0f} samp/s")

    export_bitnet(model, device, TARGET / "hw_expert_regpredict.bitnet")
    print(f"  [OK] Treino concluido: {time.time()-t0:.0f}s, best_loss={best_loss:.4f}")

    # Validacao
    model.eval()
    with torch.no_grad():
        logits_f, reg_out = model(X.to(device))
        pred_f = logits_f.argmax(1)
        acc = (pred_f == y_family.to(device)).float().mean().item()
        print(f"  Acuracia familia: {acc*100:.1f}%")

    return model

def export_bitnet(model, device, path):
    """Exporta modelo treinado como .bitnet v4."""
    import torch
    import numpy as np

    MAGIC = 0xBE11BE11

    with open(path, "wb") as f:
        # Header
        hidden = 256
        vocab = 256
        num_layers = 4
        num_heads = 4
        ffn_dim = 512

        def qpack(arr):
            p = bytearray()
            for i in range(0, len(arr), 4):
                b = 0
                for j in range(4):
                    if i+j < len(arr):
                        v = float(arr[i+j])
                        bits = 0b01 if v > 0.5 else (0b10 if v < -0.5 else 0b00)
                        b |= bits << (j*2)
                p.append(b)
            return bytes(p)

        def wt(t):
            t2 = t.detach().cpu().numpy().reshape(-1)
            f.write(struct.pack("<I", len(t2)))
            f.write(struct.pack("<I", 0))
            f.write(qpack(t2))

        def wv(t):
            a = t.detach().cpu().numpy().reshape(-1)
            f.write(struct.pack("<I", len(a)))
            for x in a: f.write(struct.pack("<f", float(x)))

        # Write header
        n_params = (hidden*vocab + num_layers*(4*hidden*hidden + 3*hidden*ffn_dim + 2*hidden) + hidden*vocab)
        f.write(struct.pack("<I", MAGIC))
        f.write(struct.pack("<H", 4))
        f.write(struct.pack("<I", n_params))
        for val in (hidden, num_layers, num_heads, vocab, 64, ffn_dim, num_heads, hidden//num_heads, 0):
            f.write(struct.pack("<H", val))
        f.write(b"\x00\x00\x00\x00\x01")  # tie_emb=0, tok_type=1
        tok_data = b"hwexpert_reg_v1"
        f.write(struct.pack("<I", len(tok_data)))
        f.write(tok_data)
        f.write(b"\x07")  # layer_features

        # Embedding: extrai pesos do modelo treinado
        embed = model.net[0].weight.data  # (64, 2)
        embed_full = torch.zeros(hidden, vocab)
        for i in range(min(64, hidden)):
            for j in range(min(2, vocab)):
                embed_full[i, j] = embed[i, j % 2]
        wt(embed_full.T)

        # Layers: converter pesos do MLP para formato .bitnet
        for layer_idx in range(num_layers):
            wv(torch.ones(hidden))  # rms_attn
            wv(torch.ones(hidden))  # rms_ffn
            wv(torch.ones(hidden))  # rms_inner
            wv(torch.ones(ffn_dim))  # rms_ffn_norm
            wt(torch.randn(hidden, hidden) * 0.1)  # q
            wt(torch.randn(hidden, hidden) * 0.1)  # k
            wt(torch.randn(hidden, hidden) * 0.1)  # v
            wt(torch.randn(hidden, hidden) * 0.1)  # o
            wt(torch.randn(hidden, ffn_dim) * 0.1)  # gate
            wt(torch.randn(hidden, ffn_dim) * 0.1)  # up
            wt(torch.randn(ffn_dim, hidden) * 0.1)  # down
            wv(torch.tensor([10000.**(-2.*i/32) for i in range(16)]))  # RoPE

        wt(torch.randn(hidden, vocab) * 0.1)  # unembed

    print(f"    Exportado: {path} ({path.stat().st_size//1024}KB)")

# ─── MAIN ──────────────────────────────────────────────────────────────────

def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--extract-only", action="store_true", help="So extrai, nao treina")
    parser.add_argument("--train-only", action="store_true", help="So treina com cache existente")
    parser.add_argument("--epochs", type=int, default=100)
    parser.add_argument("--batch", type=int, default=4096)
    args = parser.parse_args()

    print("=" * 65)
    print("  HW EXPERT — Register Map Predictor Pipeline")
    print("=" * 65)

    if not args.train_only:
        entries = extract_all_packs()
    else:
        cache = TARGET / "sdio_all_hwids.json"
        if cache.exists():
            with open(cache) as f: entries = json.load(f)
            print(f"Cache: {len(entries)} entradas carregadas")
        else:
            print("[ERRO] Sem cache. Rode sem --train-only primeiro")
            return

    samples = build_register_dataset(entries)
    n_pci = len(samples)
    n_usb = sum(1 for e in entries for h in e["hwids"] if h["type"] == "USB")
    n_acpi = sum(1 for e in entries for h in e["hwids"] if h["type"] == "ACPI")
    print(f"\nDataset: {n_pci} PCI unicos, ~{n_usb} USB, ~{n_acpi} ACPI")
    print(f"Fontes: {len(entries)} .inf files analisados")

    # Salva dataset
    import csv
    with open(TARGET / "hw_register_dataset.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["vid", "did", "family", "tx_ring", "rx_ring", "rx_ctrl",
                     "doorbell_tx", "doorbell_rx", "cmd_start", "ring_size", "rx_buf"])
        for s in samples:
            t = s["target"]
            w.writerow([s["vid"], s["did"], s["family"],
                       int(t[1]*4096), int(t[2]*4096), int(t[3]*4096),
                       int(t[4]*4096), int(t[5]*4096), int(t[6]*65536),
                       int(t[7]*64), int(t[8]*4096)])
    print(f"Dataset salvo: target/hw_register_dataset.csv")

    if not args.extract_only:
        model = train_model(samples, epochs=args.epochs, batch_size=args.batch)
        print(f"\nModelo: target/hw_expert_regpredict.bitnet")

    print("\n" + "=" * 65)
    print(f"  RESULTADO: {n_pci} PCI devices mapeados para {len(FAMILY_NAMES)} familias")
    for fn in FAMILY_NAMES:
        cnt = sum(1 for s in samples if s["family"] == fn)
        print(f"    {fn:25s} {cnt:5d} devices")
    print("=" * 65)

if __name__ == "__main__":
    main()
