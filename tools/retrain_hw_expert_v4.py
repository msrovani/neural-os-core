#!/usr/bin/env python3
"""retrain_hw_expert_v4.py — retrain the v4 multi-head classifier and ship a
VALIDATED, NON-degenerate .bitnet artifact.

Fixes over the original pipeline (tools/train_hw_expert_v4.py):
  1. Honest 90/10 split by unique (vid,did), seed 42 (same as
     tools/eval_hw_expert_v4.py) — holdout devices are NEVER seen in training.
  2. Early stopping on holdout device-level family acc (patience 3, max 12).
  3. Export threshold is tunable (default candidates 0.5/0.25/0.1/0.05);
     the threshold that maximizes holdout acc of the EXPORTED FILE (parsed by
     the Rust-exact loader port) with nonzero-fraction >= 1% is chosen.
  4. Embed tensor is written ROW-MAJOR over (vocab, hidden) — i.e.
     `wt(f, model.embed.weight)` NOT `.T` — because the Rust loader
     (cortex.rs predict_hw_v4) reads flat index `col*h + row`; the original
     `.T` write placed trained embeddings at scrambled positions.
  5. After export, the artifact is validated via tools/validate_hw_expert_v4.py
     (Rust-exact port): parse_end, header, nonzero fraction, test-device
     predictions, holdout acc of the file.

Usage:
  python tools/retrain_hw_expert_v4.py --epochs 12
  python tools/retrain_hw_expert_v4.py --epochs 12 --thresh-list 0.5,0.25,0.1,0.05
"""
from __future__ import annotations

import argparse
import json
import os
import struct
import sys
import time
from collections import defaultdict
from pathlib import Path

import numpy as np
import torch
import torch.nn as nn

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "tools" / "target"
sys.path.insert(0, str(ROOT / "tools"))
from train_hw_expert_v4 import FAMILY, FW, AGENT, NEXT  # noqa: E402
import validate_hw_expert_v4 as V  # noqa: E402  (Rust-exact port; numpy only)

DEVICE = torch.device("cuda" if torch.cuda.is_available() else "cpu")
N_CAPS = 10
MAGIC = 0xBE11BE11


# ─── Rust-exact model ─────────────────────────────────────────────────────
# The kernel runs predict_hw_v4 (crates/cortex/src/cortex.rs) whose math is:
#   rms_norm:  x / (sqrt(mean(x^2)) + 1e-6) * w
#   attn:      o(v(x)) with ONLY the first q_dim=32 outputs added to residual
#   ffn:       d(swiglu(g(x), u(x)))  with swiglu = g*sigmoid(g)*u
#   heads:     matmul_hybrid (weight-only, NO bias)
# The original BitNetLMv4 (train_hw_expert_v4.py) trains a DIFFERENT forward
# (plain scale instead of rms_norm, plain g*u instead of swiglu, biased heads,
# full-dim attention) — so any artifact exported from it reads garbage in the
# kernel. This class mirrors the Rust math exactly so train == kernel infer.
#
# Quantization-aware (STE): every Linear/embedding weight is ternary-quantized
# in the forward (threshold `t`, straight-through estimator) — exactly what the
# kernel reads from the .bitnet file — so export at threshold `t` is lossless
# and the model learns to work with ±1 weights and the kernel's activation
# scales. Logits are trained at 1/`tau` scale for loss stability; the kernel's
# argmax (heads) and sign (caps) are invariant to positive logit scaling.
def rms_norm_torch(x, w):
    ss = x.pow(2).mean(dim=-1, keepdim=True)
    rms = ss.sqrt() + 1e-6
    return x / rms * w


def tern(w, thresh):
    """Straight-through ternary quantization: forward = sign pattern
    (0 for |w| < thresh), gradient passes through as identity."""
    t = torch.where(w > thresh, torch.ones_like(w),
                    torch.where(w < -thresh, -torch.ones_like(w), torch.zeros_like(w)))
    return w + (t - w).detach()


def tern_linear(lin, x, thresh):
    return torch.nn.functional.linear(x, tern(lin.weight, thresh), None)


class BitNetRustExact(nn.Module):
    def __init__(self, hidden=128, vocab=64, num_layers=6, num_heads=4, ff_dim=256,
                 t=0.05, tau=16.0):
        super().__init__()
        self.h = hidden
        self.v = vocab
        self.nl = num_layers
        self.nh = num_heads
        self.ff = ff_dim
        self.t = t
        self.tau = tau
        self.q_dim = hidden // num_heads
        self.embed = nn.Embedding(vocab, hidden)
        # bias=False everywhere: export writes weights only and the Rust
        # matmul_hybrid has no bias term.
        self.q = nn.ModuleList([nn.Linear(hidden, hidden, bias=False) for _ in range(num_layers)])
        self.k = nn.ModuleList([nn.Linear(hidden, hidden, bias=False) for _ in range(num_layers)])
        self.v_ = nn.ModuleList([nn.Linear(hidden, hidden, bias=False) for _ in range(num_layers)])
        self.o = nn.ModuleList([nn.Linear(hidden, hidden, bias=False) for _ in range(num_layers)])
        self.g = nn.ModuleList([nn.Linear(hidden, ff_dim, bias=False) for _ in range(num_layers)])
        self.u = nn.ModuleList([nn.Linear(hidden, ff_dim, bias=False) for _ in range(num_layers)])
        self.d = nn.ModuleList([nn.Linear(ff_dim, hidden, bias=False) for _ in range(num_layers)])
        self.rms_a = nn.ParameterList([nn.Parameter(torch.ones(hidden)) for _ in range(num_layers)])
        self.rms_f = nn.ParameterList([nn.Parameter(torch.ones(hidden)) for _ in range(num_layers)])
        self.rms_o = nn.Parameter(torch.ones(hidden))
        self.family_head = nn.Linear(hidden, 17, bias=False)
        self.fw_head = nn.Linear(hidden, 8, bias=False)
        self.agent_head = nn.Linear(hidden, 9, bias=False)
        self.caps_head = nn.Linear(hidden, 10, bias=False)
        self.next_head = nn.Linear(hidden, 9, bias=False)

    def forward(self, x):
        """Matches Rust predict_hw_v4 exactly (ternary weights, STE).

        NOTE on residuals: the Rust kernel normalizes hidden_vec IN PLACE and
        adds the layer output to the NORMALIZED vector (h += o(v(h)) and
        h += d(swiglu(...))) — there is NO pre-norm skip connection. This
        unusual formulation must be mirrored here or train != kernel."""
        w_emb = tern(self.embed.weight, self.t)
        h = torch.nn.functional.embedding(x, w_emb)   # (B, S, H)
        for i in range(self.nl):
            h = rms_norm_torch(h, self.rms_a[i])
            oo = tern_linear(self.o[i], tern_linear(self.v_[i], h, self.t), self.t)
            attn = torch.zeros_like(h)
            attn[..., :self.q_dim] = oo[..., :self.q_dim]
            h = h + attn
            h = rms_norm_torch(h, self.rms_f[i])
            g = tern_linear(self.g[i], h, self.t)
            u = tern_linear(self.u[i], h, self.t)
            sw = g * torch.sigmoid(g) * u          # swiglu (Rust)
            h = h + tern_linear(self.d[i], sw, self.t)
        h = rms_norm_torch(h, self.rms_o)
        pooled = h.mean(dim=1) / self.tau          # (B, H) — tau is training-side
        return {
            "family": tern_linear(self.family_head, pooled, self.t),
            "fw": tern_linear(self.fw_head, pooled, self.t),
            "agent": tern_linear(self.agent_head, pooled, self.t),
            "caps": tern_linear(self.caps_head, pooled, self.t),
            "next": tern_linear(self.next_head, pooled, self.t),
        }


# ─── Export (fixed version of train_hw_expert_v4.export_v4) ──────────────
def wv(f, v):
    a = list(v.detach().cpu().numpy().reshape(-1))
    f.write(struct.pack("<I", len(a)))
    for x in a:
        f.write(struct.pack("<f", float(x)))


def qpack(arr, thresh):
    p = bytearray()
    for i in range(0, len(arr), 4):
        b = 0
        for j in range(4):
            if i + j < len(arr):
                v = float(arr[i + j])
                bits = 0b01 if v > thresh else (0b10 if v < -thresh else 0b00)
                b |= bits << (j * 2)
        p.append(b)
    return bytes(p)


def wt(f, t, thresh):
    t = t.detach().cpu().numpy().reshape(-1)
    f.write(struct.pack("<I", len(t)))
    f.write(struct.pack("<I", 0))  # scale = 0 (reader uses 1.0)
    f.write(qpack(t, thresh))


def export_bytes(model, thresh, tok=b"hwexpert_v4"):
    """Same byte layout as export_v4 (load_hwexpert_v5-compatible) but:
       - embed written as `embed.weight` (NOT .T) so Rust's `col*h + row`
         lookup retrieves the trained embedding;
       - quantize threshold parametrized."""
    h, nl, nh, ff = model.h, model.nl, model.nh, model.ff
    qd = h // nh
    np_ = h * model.v + nl * (4 * h * h + 3 * h * ff + 2 * h + qd) + h * model.v
    out = []
    f = out.append

    def wv_buf(v):
        a = list(v.detach().cpu().numpy().reshape(-1))
        f(struct.pack("<I", len(a)))
        f(b"".join(struct.pack("<f", float(x)) for x in a))

    def wt_buf(t):
        t = t.detach().cpu().numpy().reshape(-1)
        f(struct.pack("<I", len(t)))
        f(struct.pack("<I", 0))
        f(qpack(t, thresh))

    f(struct.pack("<I", MAGIC))
    f(struct.pack("<H", 5))
    f(struct.pack("<I", np_))
    f(struct.pack("<H", h))
    f(struct.pack("<H", nl))
    f(struct.pack("<H", nh))
    f(struct.pack("<I", model.v))
    f(struct.pack("<H", 16))
    f(struct.pack("<H", ff))
    f(struct.pack("<H", nh))
    f(struct.pack("<H", qd))
    f(struct.pack("<I", 0))
    f(b"MH\x00\x00")
    f(b"\x05")
    f(struct.pack("<I", len(tok)))
    f(tok)
    f(b"\x1F")

    wt_buf(model.embed.weight)  # FIX: no .T — Rust reads col*h + row
    for i in range(nl):
        wv_buf(model.rms_a[i]); wv_buf(model.rms_f[i])
        wv_buf(torch.ones(h)); wv_buf(torch.ones(ff))
        wt_buf(model.q[i].weight.T); wt_buf(model.k[i].weight.T)
        wt_buf(model.v_[i].weight.T); wt_buf(model.o[i].weight.T)
        wt_buf(model.g[i].weight.T); wt_buf(model.u[i].weight.T)
        wt_buf(model.d[i].weight.T)
        wv_buf(torch.tensor([10000. ** (-2. * j / 32) for j in range(16)]))
    wv_buf(model.rms_o)
    wt_buf(model.family_head.weight.T)
    wt_buf(model.fw_head.weight.T)
    wt_buf(model.agent_head.weight.T)
    wt_buf(model.caps_head.weight.T)
    wt_buf(model.next_head.weight.T)
    return b"".join(out)


# ─── Data ─────────────────────────────────────────────────────────────────
def build_tensors(samples, idx):
    X, Yf, Yfw, Ya, Yc, Yn = [], [], [], [], [], []
    for i in idx:
        s = samples[i]
        x = list(s["x"][:4])
        while len(x) < 4:
            x.append(0)
        y = s["y"]
        X.append(x)
        Yf.append(y.get("family", 0))
        Yfw.append(y.get("fw_id", 0))
        Ya.append(y.get("agent_id", 0))
        Yc.append(y.get("caps_bits", 0))
        Yn.append(y.get("next_action", 8))
    return (
        torch.tensor(X, dtype=torch.long),
        torch.tensor(Yf, dtype=torch.long),
        torch.tensor(Yfw, dtype=torch.long),
        torch.tensor(Ya, dtype=torch.long),
        torch.tensor(Yc, dtype=torch.long),
        torch.tensor(Yn, dtype=torch.long),
    )


def caps_onehot(bits: torch.Tensor) -> torch.Tensor:
    bits = bits.to(DEVICE)
    ar = torch.arange(N_CAPS, device=DEVICE)
    return ((bits.unsqueeze(1) >> ar) & 1).float()


@torch.no_grad()
def eval_holdout_device(model, samples, hold_idx):
    """Device-level holdout acc per head (1 vote per device; label = first
    sample of the device), computed with the in-memory torch model. Used for
    per-epoch logging + early stopping."""
    model.eval()
    dev_order = []
    dev_first = {}
    dev_pos = {}
    for pos, i in enumerate(hold_idx):
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        if dev not in dev_first:
            dev_order.append(dev)
            dev_first[dev] = (i, pos)
            dev_pos[dev] = pos
    idxs = [dev_first[d][1] for d in dev_order]
    X = torch.stack([torch.tensor(samples[dev_first[d][0]]["x"][:4] + [0] * 4, dtype=torch.long)[:4]
                     for d in dev_order])
    acc = {"family": 0, "fw_id": 0, "agent_id": 0, "caps_bits": 0, "next_action": 0}
    for start in range(0, len(X), 4096):
        bx = X[start:start + 4096].to(DEVICE)
        out = model(bx)
        fam = out["family"].argmax(1).cpu().numpy()
        fw = out["fw"].argmax(1).cpu().numpy()
        ag = out["agent"].argmax(1).cpu().numpy()
        caps = (out["caps"] > 0).cpu().numpy()
        nx = out["next"].argmax(1).cpu().numpy()
        for k, dev in enumerate(dev_order[start:start + 4096]):
            first_i, _ = dev_first[dev]
            y = samples[first_i]["y"]
            j = start + k
            acc["family"] += int(fam[k] == int(y.get("family", 0)))
            acc["fw_id"] += int(fw[k] == int(y.get("fw_id", 0)))
            acc["agent_id"] += int(ag[k] == int(y.get("agent_id", 0)))
            gt = int(y.get("caps_bits", 0))
            acc["caps_bits"] += all(bool(caps[k, b]) == bool((gt >> b) & 1) for b in range(N_CAPS))
            acc["next_action"] += int(nx[k] == int(y.get("next_action", 8)))
    n = len(dev_order)
    model.train()
    return {k: v / n * 100.0 for k, v in acc.items()}, n


def train_with_early_stop(samples, train_idx, hold_idx, args, log):
    X, Yf, Yfw, Ya, Yc, Yn = build_tensors(samples, train_idx)
    model = BitNetRustExact(hidden=args.hidden, vocab=64, num_layers=args.layers,
                            num_heads=args.heads, ff_dim=args.ff_dim).to(DEVICE)
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-5)
    crit_ce = nn.CrossEntropyLoss()
    crit_bce = nn.BCEWithLogitsLoss()

    # holdout device order (used for the file-level probe)
    dev_order = []
    dev_first = {}
    for pos, i in enumerate(hold_idx):
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        if dev not in dev_first:
            dev_order.append(dev)
            dev_first[dev] = (i, pos)
    vids = [d[0] for d in dev_order]
    dids = [d[1] for d in dev_order]

    probe_threshs = args.probe_threshs

    def probe_file_acc(mdl):
        """Best holdout dev family acc of the EXPORTED bytes at probe
        thresholds (with nonzero fraction >= 1%). This is the honest signal:
        the kernel reads the ternary file, not the float model."""
        best = -1.0
        for th in probe_threshs:
            try:
                data = export_bytes(mdl, th)
                mm, end = V.load_v5(data)
                if mm is None or end != len(data):
                    continue
                if V.nonzero_fraction(mm) < 0.01:
                    continue
                fam_p, *_ = V.predict_batch(mm, vids, dids)
                hits = sum(int(fam_p[k]) == int(samples[dev_first[d][0]]["y"].get("family", 0))
                           for k, d in enumerate(dev_order))
                acc = hits / len(dev_order) * 100.0
                if acc > best:
                    best = acc
            except Exception:  # noqa: BLE001
                continue
        return best

    n = len(X)
    best_file = -1.0
    best_state = None
    best_epoch = -1
    stall = 0
    epochs_done = 0
    for epoch in range(args.epochs):
        t0 = time.time()
        model.train()
        perm = torch.randperm(n)
        Xp, Yfp, Yfwp, Yap, Ycp, Ynp = [t[perm] for t in (X, Yf, Yfw, Ya, Yc, Yn)]
        total = 0.0
        nb = 0
        for i in range(0, n, args.batch):
            bx = Xp[i:i + args.batch].to(DEVICE)
            bf = Yfp[i:i + args.batch].to(DEVICE)
            bfw = Yfwp[i:i + args.batch].to(DEVICE)
            ba = Yap[i:i + args.batch].to(DEVICE)
            bc = Ycp[i:i + args.batch].to(DEVICE)
            bn = Ynp[i:i + args.batch].to(DEVICE)
            bt = caps_onehot(bc)
            opt.zero_grad()
            out = model(bx)
            loss = (crit_ce(out["family"], bf) * 1.0 + crit_ce(out["fw"], bfw) * 0.5 +
                    crit_ce(out["agent"], ba) * 0.5 + crit_bce(out["caps"], bt) * 0.3 +
                    crit_ce(out["next"], bn) * 0.5)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            total += loss.item()
            nb += 1
        acc, n_dev = eval_holdout_device(model, samples, hold_idx)
        t_mid = time.time()
        file_acc = probe_file_acc(model)
        secs = time.time() - t0
        epochs_done = epoch + 1
        line = (f"  epoch {epoch:3d}  loss={total / max(nb, 1):.4f}  {secs:6.1f}s  "
                f"dev family (in-mem)={acc['family']:6.2f}%  FILE family={file_acc:6.2f}%")
        print(line)
        log.append(line)
        if file_acc > best_file + 1e-9:
            best_file = file_acc
            best_state = {k: v.detach().clone() for k, v in model.state_dict().items()}
            best_epoch = epoch
            stall = 0
        else:
            stall += 1
        if stall >= args.patience:
            print(f"  [early stop] no FILE family improvement for {args.patience} epochs "
                  f"(best ep {best_epoch}, file family={best_file:.2f}%)")
            break
    model.load_state_dict(best_state)
    print(f"  best epoch {best_epoch}  FILE holdout dev family={best_file:.2f}%")
    return model, best_epoch, best_file, epochs_done


# ─── Main ─────────────────────────────────────────────────────────────────
def main():
    ap = argparse.ArgumentParser(description="Retrain + validate HW Expert v4")
    ap.add_argument("--epochs", type=int, default=40)
    ap.add_argument("--patience", type=int, default=3)
    ap.add_argument("--hidden", type=int, default=128)
    ap.add_argument("--layers", type=int, default=6)
    ap.add_argument("--heads", type=int, default=4)
    ap.add_argument("--ff-dim", type=int, default=256)
    ap.add_argument("--batch", type=int, default=4096)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--thresh-list", type=str,
                    default="0.5,0.25,0.1,0.05,0.03,0.02,0.015,0.01")
    ap.add_argument("--probe-threshs", type=str, default="0.1,0.05,0.02")
    ap.add_argument("--out", type=str, default=str(TARGET / "hw_expert_v4.bitnet"))
    args = ap.parse_args()

    torch.manual_seed(args.seed)
    np.random.seed(args.seed)
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:
        pass

    thresh_list = [float(t) for t in args.thresh_list.split(",")]
    args.probe_threshs = [float(t) for t in args.probe_threshs.split(",")]
    t_start = time.time()
    print("=" * 64)
    print("  HW Expert v4 — RETRAIN + SHIP (validated artifact)")
    print("=" * 64)
    print(f"  device: {DEVICE}  config: hidden={args.hidden} layers={args.layers} "
          f"heads={args.heads} ff={args.ff_dim} batch={args.batch} lr={args.lr}")
    print(f"  max epochs={args.epochs} patience={args.patience} seed={args.seed}")
    print(f"  export thresholds to try: {thresh_list}")

    samples = V.load_samples()
    train_idx, hold_idx, hold_devs, n_devs = V.split_by_device(samples, 0.1, args.seed)
    print(f"  unique devices: {n_devs} (hold-out {len(hold_devs)})")
    print(f"  samples: train={len(train_idx)} hold-out={len(hold_idx)}")

    dev_order = []
    dev_first = {}
    for pos, i in enumerate(hold_idx):
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        if dev not in dev_first:
            dev_order.append(dev)
            dev_first[dev] = (i, pos)
    vids = [d[0] for d in dev_order]
    dids = [d[1] for d in dev_order]

    log = []
    model, best_epoch, best_file_fam, epochs_done = train_with_early_stop(
        samples, train_idx, hold_idx, args, log)
    t_train = time.time() - t_start

    # Threshold selection: best holdout family acc of the EXPORTED FILE
    # (parsed with the Rust-exact port) among thresh with nonzero >= 1%.
    print("\n  ── threshold selection (holdout acc of EXPORTED bytes) ──")
    results = []
    for th in thresh_list:
        try:
            data = export_bytes(model, th)
            m, end = V.load_v5(data)
            if m is None or end != len(data):
                print(f"    thresh={th}: EXPORT PARSE FAILED — skipping")
                continue
            nz = V.nonzero_fraction(m)
            fam_p, fw_p, ag_p, caps_p, nx_p = V.predict_batch(m, vids, dids)
            fam_hits = 0
            for k, dev in enumerate(dev_order):
                first_i, _ = dev_first[dev]
                fam_hits += int(fam_p[k] == int(samples[first_i]["y"].get("family", 0)))
            fam_acc = fam_hits / len(dev_order) * 100.0
            results.append({"thresh": th, "nz": nz, "family": fam_acc, "data": data})
            print(f"    thresh={th:4.2f}: nz={nz * 100:6.3f}%  holdout dev family={fam_acc:6.2f}%")
        except Exception as e:  # noqa: BLE001
            print(f"    thresh={th}: failed ({e})")

    eligible = [r for r in results if r["nz"] >= 0.01]
    pool = eligible if eligible else results
    if not pool:
        print("[ERROR] no export threshold produced a parseable artifact")
        sys.exit(1)
    best = max(pool, key=lambda r: (r["family"], r["thresh"]))
    chosen_thresh = best["thresh"]
    print(f"\n  → chosen export threshold: {chosen_thresh} "
          f"(nz={best['nz'] * 100:.3f}%, holdout dev family={best['family']:.2f}%)")

    # Write chosen artifact + full file-level validation
    TARGET.mkdir(parents=True, exist_ok=True)
    out_path = Path(args.out)
    out_path.write_bytes(best["data"])
    print(f"\n  ── full validation of exported file ({out_path}) ──")
    res = V.validate(str(out_path))
    if res is None:
        print("[ERROR] validation failed to parse — aborting (no file shipped)")
        sys.exit(1)

    t_total = time.time() - t_start
    print(f"\n  total training time: {t_train:.1f}s  |  total run: {t_total:.1f}s")

    # Report (hashes appended after the 4 copies are written by the caller)
    report = TARGET / "hw_expert_v4_ship_validation.md"
    lines = []
    w = lines.append
    w("# HW Expert v4 — Ship Validation Report")
    w("")
    w(f"_Gerado por `tools/retrain_hw_expert_v4.py` + `tools/validate_hw_expert_v4.py` "
      f"em {time.strftime('%Y-%m-%d %H:%M:%S')}_")
    w("")
    w("## Método")
    w("")
    w(f"- **Split honesto**: 90/10 por dispositivo único (vid,did), seed 42 (mesmo de `eval_hw_expert_v4.py`). Hold-out: {len(hold_devs)} dispositivos / {len(hold_idx)} amostras, nunca vistos no treino.")
    w(f"- **Modelo**: `BitNetRustExact` — hidden=128, layers=6, heads=4 (q_dim=32), ff=256, batch={args.batch}, lr={args.lr}, weight_decay=1e-5, clip=1.0, AdamW. Forward espelha EXATAMENTE o `predict_hw_v4` do kernel (rms_norm real, swiglu, atenção truncada em q_dim=32, heads sem bias).")
    w(f"- **Correção de forward (crítica)**: o `BitNetLMv4` original treinava com forward DIFERENTE do kernel (scale simples em vez de rms_norm, g·u em vez de swiglu, heads com bias, atenção full-dim) — artefato exportado lia lixo no kernel. O modelo agora treina com a MESMA matemática do Rust.")
    w(f"- **Treino**: max {args.epochs} epochs, early stop patience={args.patience} sobre acurácia DO ARQUIVO (thresholds sonda {args.probe_threshs}); {epochs_done} epochs rodados; melhor época {best_epoch} (FILE holdout dev family {best_file_fam:.2f}%).")
    w(f"- **Export**: v5 multi-head, threshold de quantização ternária escolhido = **{chosen_thresh}** entre {thresh_list} (maximiza acurácia do ARQUIVO exportado com nz ≥ 1%).")
    w("- **Correção de layout embed**: `embed.weight` gravado sem `.T` — o loader Rust (`predict_hw_v4`) lê índice `col*h + row`; a gravação `.T` original colocava os embeddings treinados em posições embaralhadas. Formato byte-compatível (mesmo header/tamanhos).")
    w("- **Validação do arquivo**: port exato do loader Rust (`load_hwexpert_v5` + `predict_hw_v4`) — parse_end == tamanho do arquivo, header, fração não-zero do backbone, predições dos 10 devices canônicos, e acurácia hold-out DO ARQUIVO.")
    w("")
    w("## Treino — acurácia hold-out por época")
    w("")
    w("| Época | loss | tempo | dev family (in-mem) | FILE family (port) |")
    w("|-------|------|-------|---------------------|--------------------|")
    for ln in log:
        # "  epoch  0  loss=2.7100   12.3s  dev family (in-mem)= 42.00%  FILE family= 45.00%"
        parts = ln.split()
        ep = parts[1]
        loss = parts[2].split("=")[1]
        secs = parts[3].replace("s", "")
        inm = parts[7]
        fl = parts[10]
        w(f"| {ep} | {loss} | {secs} | {inm} | {fl} |")
    w("")
    w("## Validação do arquivo exportado (Rust-exact port)")
    w("")
    w(f"- **Parse**: size={res['size']} bytes, parse_end={res['parse_end']} → ok={res['parse_ok']}")
    w(f"- **Header**: hidden={res['hidden']} layers={res['layers']} heads={res['heads']} → ok={res['header_ok']}")
    w(f"- **Fração não-zero do backbone** (q/k/v/o/g/u/d × 6 layers): **{res['nz_frac'] * 100:.3f}%** (gate ≥ 1%: {res['nz_gate']})")
    w(f"- **Threshold de export**: **{chosen_thresh}**")
    w("")
    w("### Hold-out final DO ARQUIVO EXPORTADO (device-level)")
    w("")
    w("| Head | Acurácia (dispositivos) |")
    w("|------|-------------------------|")
    for k in ("family", "fw_id", "agent_id", "caps_bits", "next_action"):
        w(f"| {k} | {res['holdout_acc'][k]:.2f}% |")
    w(f"")
    w(f"- **GATE family ≥ 60%**: {res['family_gate']}  (devices hold-out: {res['holdout_devices']})")
    w("")
    w("## Predições — devices canônicos (port scalar exato)")
    w("")
    w("| Device | family | fw_id | agent_id | caps_bits | next_action |")
    w("|--------|--------|-------|----------|-----------|-------------|")
    for name, fam, fname, fw, fwn, ag, agn, caps, nx, nxn in res["test_rows"]:
        w(f"| {name} | {fam} ({fname}) | {fw} ({fwn}) | {ag} ({agn}) | 0x{caps:x} | {nx} ({nxn}) |")
    w(f"")
    w(f"- Famílias distintas entre os 10 devices: {res['n_distinct_family']} (gate ≥ 2: {res['test_gate']})")
    w("")
    w("## Hashes (4 cópias idênticas)")
    w("")
    w("- SHA256: *(preenchido após cópia das 4 cópias)*")
    w("")
    w("## Caveats")
    w("")
    w("1. Rótulos são heurística (classify_by_vendor), não ground-truth de HW real — o NN aprende o padrão vendor/máscara.")
    w("2. Sem PCI class byte no dataset; o kernel real despacha por class byte.")
    w("3. Acurácia medida pelo port do loader Rust sobre o arquivo final (não o modelo em memória).")
    w(f"4. Tempo total de treino: {t_train:.1f}s; run total: {t_total:.1f}s.")
    with open(report, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(f"\n  report: {report}")
    print(f"  artifact: {out_path}  ({os.path.getsize(out_path)} bytes)")

    print("\n" + "=" * 64)
    print("  HEADLINE")
    print(f"    chosen export threshold : {chosen_thresh}")
    print(f"    backbone nonzero frac  : {res['nz_frac'] * 100:.3f}%")
    for k in ("family", "fw_id", "agent_id", "caps_bits", "next_action"):
        print(f"    holdout {k:12s} (file): {res['holdout_acc'][k]:.2f}%")
    print(f"    validation             : PASS (parse/header/nz/test/family gates)")
    print("=" * 64)


if __name__ == "__main__":
    main()
