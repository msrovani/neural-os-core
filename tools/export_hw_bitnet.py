#!/usr/bin/env python3
"""export_hw_bitnet.py — Exporta HW Expert Register Predictor como .bitnet v4.
Converte os pesos treinados do RegisterPredictor (MLP PyTorch)
para o formato .bitnet v4 que o kernel carrega via load_model().

O modelo no kernel e usado como:
   generate_via_hwexpert("PCI\\VEN_XXXX&DEV_XXXX") -> "IntelWiFi"

Estrategia: treinar um TransformerModel pequeno (hidden=32, 4L, 4H)
no dataset SDIO de 4251 PCI IDs, mapeando VID:DID -> familia de registradores.
"""

import os, sys, struct, json, time, math, re
from pathlib import Path
import numpy as np

TARGET = Path(__file__).parent / "target"
TARGET.mkdir(exist_ok=True)

import torch
import torch.nn as nn
import torch.nn.functional as F
import torch.optim as optim

DEVICE = torch.device("cuda" if torch.cuda.is_available() else "cpu")
print(f"[DEVICE] {DEVICE}")

# ─── Register Map Families (mesmo do generic_wifi.rs) ─────────────────────

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

# Heuristica vendor (mesma de generate_register_map em cortex.rs)
VENDOR_FAM = {
    0x8086: lambda d: "IntelWiFi" if (0x08B1<=d<=0x2726 or d in(0x3165,0x3166,0x06F0,0x02F0,0x2526,0x2527,0x2723,0x2725,0x2726,0x24F3,0x24F4,0x24F5,0x24F6,0x24FD) or (d>>8)==0x08) else ("IntelEthernet" if d in(0x100E,0x105E,0x10D3,0x10D5,0x10DE,0x10EA,0x10F5,0x10FB,0x10C9,0x1526,0x1527,0x154D,0x156F,0x1570,0x1533,0x1538,0x1539,0x1502,0x1503) else "GenericPCI"),
    0x10EC: lambda d: "RealtekWiFi" if d in(0x8176,0x8179,0x8812) else ("RealtekEthernet" if d in(0x8139,0x8168,0x8169) else "GenericPCI"),
    0x0BDA: lambda d: "RealtekWiFi",
    0x168C: lambda d: "AtherosWiFi",
    0x14E4: lambda d: "BroadcomWiFi",
    0x1AF4: lambda d: "VirtIO",
    0x1234: lambda d: "VirtIO",
}

def familia_para(vid, did):
    if vid in VENDOR_FAM: return VENDOR_FAM[vid](did)
    if vid==0x8086: return "IntelEthernet"
    if vid in(0x10EC,0x0BDA): return "RealtekWiFi"
    if vid in(0x168C,0x17CB,0x13D7): return "AtherosWiFi"
    if vid==0x14E4: return "BroadcomWiFi"
    return "GenericPCI"

# ─── Transformer Pequeno para HW Expert ──────────────────────────────────

class HwExpertTransformer(nn.Module):
    """Transformer pequeno: embed + 4 camadas BitNet simplificadas + unembed.
    Entrada: tokens [vid_hi, vid_lo, did_hi, did_lo, cls_token]
    Saida: logits sobre vocab (family names como tokens)
    """
    def __init__(self, hidden=32, vocab=64, num_layers=4, num_heads=4, ffn_dim=64):
        super().__init__()
        self.hidden = hidden
        self.vocab = vocab
        self.num_layers = num_layers
        self.ffn_dim = ffn_dim

        self.embed = nn.Embedding(vocab, hidden)
        self.q = nn.ModuleList([nn.Linear(hidden, hidden, 0) for _ in range(num_layers)])
        self.k = nn.ModuleList([nn.Linear(hidden, hidden, 0) for _ in range(num_layers)])
        self.v = nn.ModuleList([nn.Linear(hidden, hidden, 0) for _ in range(num_layers)])
        self.o = nn.ModuleList([nn.Linear(hidden, hidden, 0) for _ in range(num_layers)])
        self.g = nn.ModuleList([nn.Linear(hidden, ffn_dim, 0) for _ in range(num_layers)])
        self.u = nn.ModuleList([nn.Linear(hidden, ffn_dim, 0) for _ in range(num_layers)])
        self.d = nn.ModuleList([nn.Linear(ffn_dim, hidden, 0) for _ in range(num_layers)])
        self.r1 = nn.ParameterList([nn.Parameter(torch.ones(hidden)) for _ in range(num_layers)])
        self.r2 = nn.ParameterList([nn.Parameter(torch.ones(hidden)) for _ in range(num_layers)])
        self.unembed = nn.Linear(hidden, vocab, 0)
        self.rf = nn.Parameter(torch.ones(hidden))

    def forward(self, x):
        h = self.embed(x)
        for i in range(self.num_layers):
            r = h
            h = h * self.r1[i]
            h = self.o[i](self.v[i](h)) + r
            r = h
            h = h * self.r2[i]
            h = self.d[i](self.g[i](h) * self.u[i](h)) + r
        h = h * self.rf
        return self.unembed(h)  # (batch, seq, vocab)

    def export_bitnet(self, path, tok_data=b""):
        """Exporta como .bitnet v4 para load_model() do kernel."""
        MAGIC = 0xBE11BE11
        hidden, vocab = self.hidden, self.vocab
        nl, nh = self.num_layers, 4
        ffn, kv, qd = self.ffn_dim, 4, hidden // 4

        with open(path, "wb") as f:
            # Header
            np_ = hidden*vocab + nl*(4*hidden*hidden + 3*hidden*ffn + 2*hidden) + hidden*vocab
            f.write(struct.pack("<I", MAGIC))
            f.write(struct.pack("<H", 4))  # version
            f.write(struct.pack("<I", np_))
            for v in (hidden, nl, nh, vocab, 64, ffn, kv, qd, 0):
                f.write(struct.pack("<H", v))
            f.write(b"\x00\x00\x00\x00")  # tie = no
            f.write(b"\x01")  # tok_type = char
            f.write(struct.pack("<I", len(tok_data)))
            f.write(tok_data)
            f.write(b"\x07")  # layer_features: inner_attn_ln|ffn_layernorm|RoPE

            def qp(arr):
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
                t2 = t.detach().cpu().to(torch.float32).numpy().reshape(-1)
                f.write(struct.pack("<I", len(t2)))
                f.write(struct.pack("<I", 0))
                f.write(qp(t2))

            def wv(t):
                a = t.detach().cpu().to(torch.float32).numpy().reshape(-1)
                f.write(struct.pack("<I", len(a)))
                for x in a: f.write(struct.pack("<f", float(x)))

            # embed
            wt(self.embed.weight.T)

            # Layers
            for i in range(nl):
                wv(self.r1[i]); wv(self.r2[i])
                wv(torch.ones(hidden)); wv(torch.ones(ffn))
                wt(self.q[i].weight.T); wt(self.k[i].weight.T)
                wt(self.v[i].weight.T); wt(self.o[i].weight.T)
                wt(self.g[i].weight.T); wt(self.u[i].weight.T)
                wt(self.d[i].weight.T)
                rope = torch.tensor([10000.**(-2.*j/32) for j in range(16)])
                wv(rope)

            # rms_final
            wv(self.rf)
            # unembed
            wt(self.unembed.weight.T)

        print(f"  [OK] Exportado: {path} ({path.stat().st_size//1024}KB)")

# ─── Dataset: PCI IDs -> familias ─────────────────────────────────────────

def carregar_pci_ids():
    """Carrega todos os PCI IDs do cache SDIO ou CSV."""
    csv_path = TARGET / "sdio_hwids_all.csv"
    if not csv_path.exists():
        print("[ERRO] Execute build_and_train.py primeiro")
        return []

    import csv
    devices = []
    with open(csv_path, newline="", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for row in reader:
            vid = int(row["vid_dec"])
            did = int(row["did_dec"])
            fam = familia_para(vid, did)
            devices.append({"vid": vid, "did": did, "family": fam})

    return devices

def build_dataset(devices):
    """Cria pares (tokens_input, token_target)."""
    # Tokenizador: cada byte do VID/DID vira um token (0-255 mapeado para 0-vocab)
    vocab = 64
    samples = []
    for d in devices:
        # Input: 4 tokens [vid_hi, vid_lo, did_hi, did_lo]
        vid, did = d["vid"], d["did"]
        inp = [(vid>>8)%vocab, vid%vocab, (did>>8)%vocab, did%vocab]
        # Target: token da familia (mapeada para vocab)
        fam_idx = FAM2IDX.get(d["family"], FAM2IDX["GenericPCI"])
        tgt = (fam_idx % (vocab-2)) + 2  # evita 0 (pad) e 1 (eos)
        samples.append((inp, tgt))
    return samples

# ─── Treino ────────────────────────────────────────────────────────────────

def treinar(devices, epochs=500, batch=64):
    print(f"\n=== Treinando HW Expert Transformer ===")
    samples = build_dataset(devices)
    print(f"  Amostras: {len(samples)} | Familias: {len(FAMILIAS)}")

    # Contagem por familia
    for fn in FAMILIAS:
        cnt = sum(1 for d in devices if d["family"] == fn)
        print(f"    {fn:20s} {cnt:4d} devices")

    model = HwExpertTransformer(hidden=32, vocab=64, num_layers=4,
                                num_heads=4, ffn_dim=64).to(DEVICE)
    n = sum(p.numel() for p in model.parameters())
    print(f"  Parametros: {n:,}")

    inp = torch.tensor([s[0] for s in samples], dtype=torch.long)
    tgt = torch.tensor([s[1] for s in samples], dtype=torch.long)
    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(inp, tgt),
        batch_size=batch, shuffle=True, drop_last=True)

    opt = optim.AdamW(model.parameters(), lr=3e-3, weight_decay=1e-5)
    sched = optim.lr_scheduler.OneCycleLR(opt, 3e-3,
                steps_per_epoch=max(len(loader),1), epochs=epochs, last_epoch=-1)

    best = float("inf")
    t0 = time.time()
    for ep in range(epochs):
        model.train()
        tl = 0.0; nb = 0
        for x, y in loader:
            x, y = x.to(DEVICE), y.to(DEVICE)
            opt.zero_grad()
            logits = model(x)  # (B, 4, V)
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
            sps = (ep+1)*len(samples)/(time.time()-t0+0.001)
            print(f"  Ep {ep+1:4d}/{epochs} | loss={avg:.5f} | best={best:.5f} | {sps:.0f}s/s")

    # Avaliacao
    model.eval()
    with torch.no_grad():
        logits = model(inp.to(DEVICE))  # (N, 4, V)
        pred = logits.mean(dim=1).argmax(1)  # (N,)
        expected = (fam_idx % 62) + 2
        correct = (pred == tgt.to(DEVICE)).sum().item()
        acc = correct / len(samples)
        print(f"\n  Acurácia final: {acc*100:.1f}% ({correct}/{len(samples)})")
        print(f"  Tempo: {time.time()-t0:.0f}s")

    model.export_bitnet(TARGET / "hw_expert_tf.bitnet", tok_data=b"hwexpert_v2")
    return model, acc

# ─── Teste de inferencia ──────────────────────────────────────────────────

def testar(model):
    """Testa o modelo com dispositivos conhecidos."""
    test_cases = [
        (0x8086, 0x24FD, "IntelWiFi"),       # Intel AX200 WiFi
        (0x8086, 0x100E, "IntelEthernet"),    # Intel PRO/1000
        (0x10EC, 0x8139, "RealtekEthernet"),  # Realtek RTL8139
        (0x10EC, 0x8179, "RealtekWiFi"),      # Realtek RTL8188
        (0x168C, 0x0030, "AtherosWiFi"),      # Atheros AR9380
        (0x14E4, 0x43A0, "BroadcomWiFi"),     # Broadcom BCM4360
        (0x1AF4, 0x1041, "VirtIO"),           # VirtIO-net
        (0x10DE, 0x1C82, "GenericPCI"),       # GTX 1050 (nao-rede)
    ]
    print("\n=== Teste de Inferencia ===")
    model.eval()
    with torch.no_grad():
        for vid, did, expected in test_cases:
            inp = torch.tensor([[(vid>>8)%64, vid%64, (did>>8)%64, did%64]], device=DEVICE)
            logits = model(inp)
            pred_token = logits.mean(dim=1).argmax(1).item()
            pred_fam_idx = pred_token - 2
            pred_fam = FAMILIAS[pred_fam_idx] if 0 <= pred_fam_idx < len(FAMILIAS) else "???"
            ok = "OK" if pred_fam == expected else "X"
            print(f"  [{ok}] {expected:20s} <- {pred_fam:20s} (PCI:{vid:04X}:{did:04X})")

# ─── Main ──────────────────────────────────────────────────────────────────

def main():
    devices = carregar_pci_ids()
    if not devices:
        # Fallback: dados sinteticos baseados na heuristica
        print("[FALLBACK] Gerando dados sinteticos...")
        devices = []
        for vid in range(0x0000, 0xFFFF, 0x17):
            for did in range(0x0000, 0xFFFF, 0x1F3):
                if len(devices) >= 5000: break
                fam = familia_para(vid, did)
                devices.append({"vid": vid, "did": did, "family": fam})
            if len(devices) >= 5000: break
        print(f"  Gerados {len(devices)} dispositivos")

    model, acc = treinar(devices, epochs=300, batch=64)
    testar(model)

    print(f"\nModelo final: target/hw_expert_tf.bitnet")

if __name__ == "__main__":
    main()
