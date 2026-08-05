#!/usr/bin/env python3
"""HW Expert v4 — classificador HWID multi-head (BitNet ternário).

Treina um transformer ternário com 5 heads de saída estruturada:
  family_id (17), fw_id (8), agent_id (9), caps_bits (10), next_action (9)

Dataset: 59.905 amostras unificadas (WDM + SDIO + PCI.IDS + USB.IDS + kernel seed)

Uso:
  python tools/train_hw_expert_v4.py --epochs 100 --hidden 128
  python tools/train_hw_expert_v4.py --dry-run     # só valida dataset
"""
from __future__ import annotations

import argparse, json, os, struct, sys
from pathlib import Path
import numpy as np

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "tools" / "target"
DATASET = ROOT / "models" / "hw_expert" / "v4" / "dataset.json"

import torch
import torch.nn as nn

DEVICE = torch.device("cuda" if torch.cuda.is_available() else "cpu")
print(f"[HW Expert v4] Device: {DEVICE}")
if torch.cuda.is_available():
    print(f"  GPU: {torch.cuda.get_device_name(0)} VRAM: {torch.cuda.get_device_properties(0).total_memory/1e9:.1f}GB")

# ─── Vocab (espelha k_ai::hw_capability) ───────────────────────────────

FAMILY = [
    "unknown", "intel_e1000", "virtio_net", "realtek_eth", "intel_iwlwifi",
    "realtek_wifi", "atheros_wifi", "broadcom_wifi", "nvidia_gpu",
    "intel_i915", "amd_gpu", "qemu_vga", "virtio_gpu", "usb_xhci",
    "intel_hda", "storage_ata", "pci_bridge",
]
FW = ["-", "intel/iwlwifi", "rtlwifi", "ath9k", "brcmfmac", "nvidia/gp108", "i915", "amdgpu"]
AGENT = ["HwBridgeAgent", "NetAgent", "WifiAgent", "DisplayAgent", "GpuBackend",
         "UsbDriverAgent", "HdaAudioAgent", "DiskAgent", "PlatformAgent"]
NEXT = ["ready", "load_firmware", "bind_network", "bind_wifi_scan", "bind_gpu_compute",
        "bind_usb_host", "bind_audio", "bind_storage", "observe_only"]

N_FAMILY = len(FAMILY)
N_FW = len(FW)
N_AGENT = len(AGENT)
N_CAPS = 10
N_NEXT = len(NEXT)


# ─── Model: BitNet transformer + 5 heads ────────────────────────────────

class BitNetLMv4(nn.Module):
    """BitNet ternário com 5 heads de saída para classificação estruturada."""

    def __init__(self, hidden=128, vocab=64, num_layers=6, num_heads=4, ff_dim=256):
        super().__init__()
        self.h = hidden
        self.v = vocab
        self.nl = num_layers
        self.nh = num_heads
        self.ff = ff_dim

        # Transformer backbone (mesmo do v3)
        self.embed = nn.Embedding(vocab, hidden)
        self.q = nn.ModuleList([nn.Linear(hidden, hidden, 0) for _ in range(num_layers)])
        self.k = nn.ModuleList([nn.Linear(hidden, hidden, 0) for _ in range(num_layers)])
        self.v_ = nn.ModuleList([nn.Linear(hidden, hidden, 0) for _ in range(num_layers)])
        self.o = nn.ModuleList([nn.Linear(hidden, hidden, 0) for _ in range(num_layers)])
        self.g = nn.ModuleList([nn.Linear(hidden, ff_dim, 0) for _ in range(num_layers)])
        self.u = nn.ModuleList([nn.Linear(hidden, ff_dim, 0) for _ in range(num_layers)])
        self.d = nn.ModuleList([nn.Linear(ff_dim, hidden, 0) for _ in range(num_layers)])
        ra = [nn.Parameter(torch.ones(hidden)) for _ in range(num_layers)]
        rf = [nn.Parameter(torch.ones(hidden)) for _ in range(num_layers)]
        self.rms_a = nn.ParameterList(ra)
        self.rms_f = nn.ParameterList(rf)
        self.rms_o = nn.Parameter(torch.ones(hidden))

        # 5 heads multi-head (em vez do unembed do v3)
        self.family_head = nn.Linear(hidden, N_FAMILY)   # 17 classes
        self.fw_head = nn.Linear(hidden, N_FW)            # 8 classes
        self.agent_head = nn.Linear(hidden, N_AGENT)      # 9 classes
        self.caps_head = nn.Linear(hidden, N_CAPS)         # 10 binary
        self.next_head = nn.Linear(hidden, N_NEXT)         # 9 classes

    def forward(self, x):
        """x: (batch, seq) de token IDs."""
        h = self.embed(x)                     # (B, S, H)
        for i in range(self.nl):
            r = h
            h = h * self.rms_a[i]
            h = self.o[i](self.v_[i](h)) + r
            r = h
            h = h * self.rms_f[i]
            h = self.d[i](self.g[i](h) * self.u[i](h)) + r
        h = h * self.rms_o                    # (B, S, H)

        # Pooling: média sobre a sequência
        pooled = h.mean(dim=1)                 # (B, H)

        return {
            "family": self.family_head(pooled),    # (B, 17)
            "fw": self.fw_head(pooled),             # (B, 8)
            "agent": self.agent_head(pooled),       # (B, 9)
            "caps": self.caps_head(pooled),          # (B, 10) — logits BCE
            "next": self.next_head(pooled),          # (B, 9)
        }


def pack_vid_did(vid: int, did: int, vocab: int = 64) -> list[int]:
    """Mesmo packing do v3 — entrada do classificador."""
    return [(vid >> 8) % vocab, vid % vocab, (did >> 8) % vocab, did % vocab]


# ─── Dataset ─────────────────────────────────────────────────────────────

def load_dataset(path: Path) -> tuple:
    """Carrega dataset unificado, retorna (inputs, targets)."""
    print(f"  Loading {path}...")
    with open(path) as f:
        data = json.load(f)

    samples = data["samples"] if isinstance(data, dict) else data
    print(f"  Samples: {len(samples)}")

    X, Y_family, Y_fw, Y_agent, Y_caps, Y_next = [], [], [], [], [], []

    for s in samples:
        x = s.get("x", s.get("input", []))
        y = s.get("y", s.get("target", {}))

        # Pad to 4 tokens (zero-padded)
        while len(x) < 4:
            x.append(0)
        X.append(x[:4])

        Y_family.append(y.get("family", 0))
        Y_fw.append(y.get("fw_id", 0))
        Y_agent.append(y.get("agent_id", 0))
        Y_caps.append(y.get("caps_bits", 0))
        Y_next.append(y.get("next_action", 8))

    return (
        torch.tensor(X, dtype=torch.long),
        torch.tensor(Y_family, dtype=torch.long),
        torch.tensor(Y_fw, dtype=torch.long),
        torch.tensor(Y_agent, dtype=torch.long),
        torch.tensor(Y_caps, dtype=torch.float),
        torch.tensor(Y_next, dtype=torch.long),
    )


# ─── Treino ──────────────────────────────────────────────────────────────

def train(model, opt, loader, epochs, batch_size):
    criterion_ce = nn.CrossEntropyLoss()
    criterion_bce = nn.BCEWithLogitsLoss()

    for epoch in range(epochs):
        model.train()
        total_loss = 0.0
        n_batches = 0

        # Shuffle
        perm = torch.randperm(len(loader[0]))
        X, Yf, Yfw, Ya, Yc, Yn = [t[perm] for t in loader]

        for i in range(0, len(X), batch_size):
            bx = X[i:i+batch_size].to(DEVICE)
            bf = Yf[i:i+batch_size].to(DEVICE)
            bfw = Yfw[i:i+batch_size].to(DEVICE)
            ba = Ya[i:i+batch_size].to(DEVICE)
            bc = Yc[i:i+batch_size].to(DEVICE)
            bn = Yn[i:i+batch_size].to(DEVICE)

            # One-hot caps
            caps_target = torch.zeros((len(bc), N_CAPS), device=DEVICE)
            for j, bits in enumerate(bc):
                for k in range(N_CAPS):
                    if int(bits.item()) & (1 << k):
                        caps_target[j, k] = 1.0

            opt.zero_grad()
            out = model(bx)

            loss = (
                criterion_ce(out["family"], bf) * 1.0 +
                criterion_ce(out["fw"], bfw) * 0.5 +
                criterion_ce(out["agent"], ba) * 0.5 +
                criterion_bce(out["caps"], caps_target) * 0.3 +
                criterion_ce(out["next"], bn) * 0.5
            )

            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()

            total_loss += loss.item()
            n_batches += 1

        avg_loss = total_loss / max(n_batches, 1)
        if epoch % 10 == 0 or epoch == epochs - 1:
            acc = quick_eval(model, X[:1024], Yf[:1024], Yfw[:1024], Ya[:1024], Yc[:1024], Yn[:1024])
            print(f"  Epoch {epoch:3d}  loss={avg_loss:.4f}  acc(family)={acc['family']:.1f}%  acc(fw)={acc['fw']:.1f}%  acc(agent)={acc['agent']:.1f}%  acc(caps)={acc['caps']:.1f}%  acc(next)={acc['next']:.1f}%")


@torch.no_grad()
def quick_eval(model, X, Yf, Yfw, Ya, Yc, Yn):
    model.eval()
    bx = X.to(DEVICE)
    out = model(bx)
    acc = {}
    acc["family"] = (out["family"].argmax(1).cpu() == Yf[:len(bx)]).float().mean().item() * 100
    acc["fw"] = (out["fw"].argmax(1).cpu() == Yfw[:len(bx)]).float().mean().item() * 100
    acc["agent"] = (out["agent"].argmax(1).cpu() == Ya[:len(bx)]).float().mean().item() * 100
    # Caps: binary accuracy per bit
    caps_pred = (out["caps"] > 0).cpu()
    caps_gt = torch.zeros((len(Yc), N_CAPS))
    for j, bits in enumerate(Yc):
        for k in range(N_CAPS):
            if int(bits.item()) & (1 << k):
                caps_gt[j, k] = 1.0
    acc["caps"] = (caps_pred[:len(bx)] == caps_gt[:len(bx)]).float().mean().item() * 100
    acc["next"] = (out["next"].argmax(1).cpu() == Yn[:len(bx)]).float().mean().item() * 100
    model.train()
    return acc


# ─── Export .bitnet v6 (multi-head, ADR-0085 model_type=1) ──────────────

import struct as _struct
from tools.bitnet_writer import (
    write_header_v6, write_embed, write_rms, write_ternary, compute_feat,
    MODEL_HWEXPERT, ACT_SILU, EMBED_TERNARY, pack_ternary,
)

MAGIC = 0xBE11BE11  # preserved for legacy readers


def export_v4(model, path, tok=b"hwexpert_v4"):
    """Exporta modelo v4 multi-head para .bitnet v6 (ADR-0085 model_type=1)."""
    import os as _os
    h, nl, nh, ff = model.h, model.nl, model.nh, model.ff
    qd = h // nh

    # num_params informativo
    num_params = h * model.v  # embed
    num_params += ff * nl     # rms_ffn_norm (intermediate_size)
    per_layer = (h * h * 4 + h * ff * 3)  # q,k,v,o,g,u,d (h,h) shapes
    num_params += per_layer * nl
    num_params += h * 17 + h * 8 + h * 9 + h * 10 + h * 9  # 5 heads

    # feat: 0 (rms_inner_attn and rms_ffn_norm are placeholder ones)
    feat = compute_feat(False, False, False)

    with open(path, "wb") as f:
        write_header_v6(
            f,
            model_type=MODEL_HWEXPERT,
            num_params=num_params,
            hidden=h,
            layers=nl,
            heads=nh,
            vocab=model.v,
            max_seq=16,
            intermediate=ff,
            kv_heads=nh,
            q_dim=qd,
            medusa=0,
            tie=False,
            act_type=ACT_SILU,
            embed_type=EMBED_TERNARY,
            feat=feat,
        )

        # Embed (ternary + scale)
        emb = model.embed.weight.detach().cpu().numpy().T  # (hidden, vocab) int8
        write_embed(f, emb.astype(np.int8), EMBED_TERNARY, scale=1.0)

        # Layers
        for i in range(nl):
            # rms_attn, rms_ffn (real values)
            write_rms(f, model.rms_a[i].detach().cpu().numpy())
            write_rms(f, model.rms_f[i].detach().cpu().numpy())
            # rms_inner_attn, rms_ffn_norm (placeholder ones → feat bit0/bit1 = 0)
            write_rms(f, torch.ones(h).detach().cpu().numpy())
            write_rms(f, torch.ones(ff).detach().cpu().numpy())

            # 7 weight tensors: q,k,v,o,gate,up,down
            tensors = [
                (model.q[i].weight, h, h),
                (model.k[i].weight, h, h),
                (model.v_[i].weight, h, h),
                (model.o[i].weight, h, h),
                (model.g[i].weight, h, ff),
                (model.u[i].weight, h, ff),
                (model.d[i].weight, ff, h),
            ]
            for wt_t, rows, cols in tensors:
                wt_np = wt_t.detach().cpu().numpy().T.ravel()
                write_ternary(f, wt_np.astype(np.int8), scale=1.0)

        # rms_final
        write_rms(f, model.rms_o.detach().cpu().numpy())

        # 5 heads (replacing unembed)
        for head in [model.family_head, model.fw_head, model.agent_head,
                      model.caps_head, model.next_head]:
            h_np = head.weight.detach().cpu().numpy().T.ravel()
            write_ternary(f, h_np.astype(np.int8), scale=1.0)

    size_kb = _os.path.getsize(path) // 1024
    print(f"  [OK] {path} ({size_kb} KB, v6 multi-head)")


# ─── Main ────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(description="HW Expert v4 — multi-head HWID classifier")
    ap.add_argument("--epochs", type=int, default=100)
    ap.add_argument("--hidden", type=int, default=128)
    ap.add_argument("--layers", type=int, default=6)
    ap.add_argument("--heads", type=int, default=4)
    ap.add_argument("--ff-dim", type=int, default=256)
    ap.add_argument("--batch", type=int, default=4096)
    ap.add_argument("--lr", type=float, default=3e-4)
    ap.add_argument("--dry-run", action="store_true", help="Valida dataset sem treinar")
    args = ap.parse_args()

    print("=" * 60)
    print("  HW Expert v4 — Multi-Head HWID Classifier")
    print("  BitNet ternário | 5 heads | 44K unique devices | 60K samples")
    print("=" * 60)
    print(f"  hidden={args.hidden} layers={args.layers} heads={args.heads} ff={args.ff_dim}")
    print(f"  epochs={args.epochs} batch={args.batch} lr={args.lr}")
    print(f"  dataset: {DATASET}")

    # Load dataset
    if not DATASET.exists():
        print(f"\n  [ERRO] Dataset não encontrado. Rode tools/unify_hwids_v4.py primeiro.")
        sys.exit(1)

    loader = load_dataset(DATASET)
    X, Yf, Yfw, Ya, Yc, Yn = loader
    print(f"  Input shape: {X.shape}")
    print(f"  Targets: family={Yf.unique().numel()} classes, fw={Yfw.unique().numel()}, "
          f"agent={Ya.unique().numel()}, caps=10-bit, next={Yn.unique().numel()}")

    if args.dry_run:
        print("\n  [DRY-RUN] Dataset OK — pronto para treino.")
        print(f"  Device: {DEVICE}")
        print(f"  Model params: ~1M (backbone) + 5 heads (~5K)")
        print(f"  Export: .bitnet v5 multi-head (~260 KB)")
        return

    # Model
    model = BitNetLMv4(
        hidden=args.hidden,
        vocab=64,
        num_layers=args.layers,
        num_heads=args.heads,
        ff_dim=args.ff_dim,
    ).to(DEVICE)

    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-5)

    n_params = sum(p.numel() for p in model.parameters())
    print(f"\n  Model params: {n_params:,} (~{n_params // 1_000_000}M)")

    # Train
    print(f"\n  Training on {DEVICE}...")
    train(model, opt, loader, args.epochs, args.batch)

    # Export
    TARGET.mkdir(parents=True, exist_ok=True)
    out = TARGET / "hw_expert_v4.bitnet"
    export_v4(model, out)
    print(f"\n  Done — v4 model ready for Rust integration.")


if __name__ == "__main__":
    main()
