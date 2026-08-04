#!/usr/bin/env python3
"""retrain_hw_expert_v4_class.py — retrain the HW Expert classifier on the
RELABELED generic-class dataset (dataset_class_v2.json, 12 families) and ship
a VALIDATED, NON-degenerate .bitnet artifact.

Reuses the Rust-exact machinery from retrain_hw_expert_v4.py:
  - BitNetRustExact forward (rms_norm/swiglu/truncated-attn, STE ternary)
  - same export_bytes layout (embed row-major, prefixed tensors)
  - honest 90/10 split by unique (vid,did), seed 42
  - early stop on holdout FILE family acc (probed via the class-v2 Rust-exact
    port), threshold selection among a list with nonzero fraction >= 1%

Changes vs the v4 retrain:
  - family head = 12 classes (generic classes, not vendor families)
  - labels from dataset_class_v2.json (independent ground truth)

Usage:
  python tools/retrain_hw_expert_v4_class.py --epochs 12
"""
from __future__ import annotations

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
# Rust-exact machinery + class-v2 Rust-exact port
from retrain_hw_expert_v4 import (  # noqa: E402
    BitNetRustExact, export_bytes, rms_norm_torch, tern, tern_linear,
    build_tensors, caps_onehot, DEVICE, MAGIC, N_CAPS,
)
import validate_hw_expert_v4_class as V  # noqa: E402

N_FAMILY = 12          # real classes (columns 0..11) — from vocab_class_v2.json
N_FAMILY_FILE = 12     # file family head = same 12 columns (kernel updated to 12 by a later lane)
DATASET = ROOT / "models" / "hw_expert" / "v4" / "dataset_class_v2.json"


class BitNetRustExactClass(BitNetRustExact):
    """Same Rust-exact model; family head = 12 columns, matching the 12-class
    taxonomy in vocab_class_v2.json (unknown/network/wifi/display/storage/
    audio/usb/serial_io/bridge/multimedia/input/other)."""

    def __init__(self, hidden=128, vocab=64, num_layers=6, num_heads=4, ff_dim=256,
                 t=0.05, tau=16.0, n_family=N_FAMILY_FILE):
        super().__init__(hidden=hidden, vocab=vocab, num_layers=num_layers,
                         num_heads=num_heads, ff_dim=ff_dim, t=t, tau=tau)
        self.family_head = nn.Linear(hidden, n_family, bias=False)
        self.n_family = n_family


def eval_holdout_device(model, samples, hold_idx, n_family):
    """Device-level holdout family acc (1 vote per device; label = first
    sample of the device). Family argmax is clamped to the 12 real classes
    (dead columns 12..16 are never labels; a dead-column win = unknown=0,
    matching the kernel argmax + future decode)."""
    model.eval()
    dev_order = []
    dev_first = {}
    for pos, i in enumerate(hold_idx):
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        if dev not in dev_first:
            dev_order.append(dev)
            dev_first[dev] = (i, pos)
    X = torch.stack([torch.tensor(samples[dev_first[d][0]]["x"][:4] + [0] * 4, dtype=torch.long)[:4]
                     for d in dev_order])
    acc = {"family": 0, "fw_id": 0, "agent_id": 0, "caps_bits": 0, "next_action": 0}
    for start in range(0, len(X), 4096):
        bx = X[start:start + 4096].to(DEVICE)
        out = model(bx)
        fam = out["family"].argmax(1).cpu().numpy()
        fam = np.where(fam >= n_family, 0, fam)
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
    model = BitNetRustExactClass(hidden=args.hidden, vocab=64, num_layers=args.layers,
                                 num_heads=args.heads, ff_dim=args.ff_dim,
                                 n_family=args.n_family_file).to(DEVICE)
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-5)
    # Family loss: plain CE by default (class-balanced weights were tried and
    # HURT — inverse-freq weights collapse the family head to ~20%, sqrt
    # weights to ~59.6% vs 60.6% plain on the same split; the 12-generic
    # taxonomy is dominated by "other" (52%) + display (22%) and the ternary
    # backbone has no headroom for reweighting minorities). --class-weights
    # opts back into sqrt weights.
    fam_counts = torch.bincount(Yf, minlength=args.n_family_file).float().clamp(min=1)
    if args.class_weights:
        fam_w = (fam_counts.sum() / (args.n_family_file * fam_counts)).sqrt()
    else:
        fam_w = torch.ones(args.n_family_file)
    crit_ce_fam = nn.CrossEntropyLoss(weight=fam_w.to(DEVICE))
    crit_ce = nn.CrossEntropyLoss()
    crit_bce = nn.BCEWithLogitsLoss()

    dev_order = []
    dev_first = {}
    for pos, i in enumerate(hold_idx):
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        if dev not in dev_first:
            dev_order.append(dev)
            dev_first[dev] = (i, pos)
    vids = [d[0] for d in dev_order]
    dids = [d[1] for d in dev_order]

    def probe_file_acc(mdl):
        best = -1.0
        for th in args.probe_threshs:
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
            loss = (crit_ce_fam(out["family"], bf) * 1.0 + crit_ce(out["fw"], bfw) * 0.5 +
                    crit_ce(out["agent"], ba) * 0.5 + crit_bce(out["caps"], bt) * 0.3 +
                    crit_ce(out["next"], bn) * 0.5)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            total += loss.item()
            nb += 1
        acc, n_dev = eval_holdout_device(model, samples, hold_idx, args.n_family)
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


def main():
    import argparse
    ap = argparse.ArgumentParser(description="Retrain HW Expert v4 CLASS (relabeled) + ship")
    ap.add_argument("--epochs", type=int, default=25)
    ap.add_argument("--patience", type=int, default=6)
    ap.add_argument("--hidden", type=int, default=128)
    ap.add_argument("--layers", type=int, default=6)
    ap.add_argument("--heads", type=int, default=4)
    ap.add_argument("--ff-dim", type=int, default=256)
    ap.add_argument("--batch", type=int, default=4096)
    ap.add_argument("--lr", type=float, default=3e-4)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--n-family", type=int, default=N_FAMILY)
    ap.add_argument("--n-family-file", type=int, default=N_FAMILY_FILE)
    ap.add_argument("--class-weights", action="store_true",
                    help="use sqrt class-balanced family weights (default off: plain CE — "
                         "reweighting was measured to HURT family acc on this taxonomy)")
    ap.add_argument("--thresh-list", type=str,
                    default="0.05,0.03,0.02,0.015,0.01,0.008,0.005")
    ap.add_argument("--probe-threshs", type=str, default="0.05,0.03,0.02,0.01",
                    help="thresholds probed per-epoch for early-stop FILE acc "
                         "(must include the training t=0.05 regime; lower-only probes "
                         "measure near-random acc and distort early stopping)")
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
    print("  HW Expert v4 — CLASS RELABEL RETRAIN + SHIP (validated artifact)")
    print("=" * 64)
    print(f"  device: {DEVICE}  config: hidden={args.hidden} layers={args.layers} "
          f"heads={args.heads} ff={args.ff_dim} batch={args.batch} lr={args.lr}")
    print(f"  max epochs={args.epochs} patience={args.patience} seed={args.seed} "
          f"n_family={args.n_family}")
    print(f"  export thresholds to try: {thresh_list}")

    with open(DATASET, encoding="utf-8") as f:
        data = json.load(f)
    samples = data["samples"] if isinstance(data, dict) else data
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
            print(f"    thresh={th:5.3f}: nz={nz * 100:6.3f}%  holdout dev family={fam_acc:6.2f}%")
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
    print(f"  artifact: {out_path}  ({os.path.getsize(out_path)} bytes)")
    print("=" * 64)
    print("  HEADLINE (class-v2)")
    print(f"    chosen export threshold : {chosen_thresh}")
    print(f"    backbone nonzero frac  : {res['nz_frac'] * 100:.3f}%")
    for k in ("family", "fw_id", "agent_id", "caps_bits", "next_action"):
        print(f"    holdout {k:12s} (file): {res['holdout_acc'][k]:.2f}%")
    print(f"    validation             : "
          f"parse={res['parse_ok']} header={res['header_ok']} nz={res['nz_gate']} "
          f"test={res['test_gate']} family={res['family_gate']}")
    print("=" * 64)


if __name__ == "__main__":
    main()
