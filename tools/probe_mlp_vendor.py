#!/usr/bin/env python3
"""probe_mlp_vendor.py — DIAGNOSTIC ONLY (no model file written, no kernel change).

Measures the honest ceiling of a small continuous f32 MLP on VENDOR-SPECIFIC
driver-family labels (dataset_class_v3.json, family 0-20). The future in-kernel
module's job is devices NEVER seen in training (the packed table covers known
devices at 100%), so the split is 90/10 by unique (vid,did), seed 42, and the
headline metric is holdout acc on devices whose GROUND-TRUTH family is specific
(1-19) — 'other' (20) is a valid but low-value prediction.

Model (plain continuous f32):
  pack (vid,did) -> 4 tokens in vocab N: [(vid>>8)%N, vid%N, (did>>8)%N, did%N]
  per-position embedding table E of shape (4N, hidden)  [flat idx = pos*N + tok]
  h = sum_pos E[pos*N + tok_pos]     (== nn.Linear(4N,H) on one-hot concat, no bias)
  h = relu(fc1(h))                   fc1: (hidden, hidden), bias=False
  logits = fc2(h)                    fc2: (hidden, 21), bias=False
  Params = 4*N*hidden + hidden*hidden + 21*hidden  (exact, no biases -> f32 size exact)

Training: plain CE, AdamW lr=1e-3, wd=1e-5, batch 4096, CPU, <=20 epochs,
early-stop patience 5 on device-level holdout family acc (OVERALL; the stable
metric — the gate needs both overall and specific >= 65%).

References on the same split: majority 'other' (always predict 20) and a
one-hot-linear (single linear 4N->21 on the same tokenizer, vocab 256).

GATE: overall >= 65% AND specific-only >= 65% on the best config that fits
<= 260KB (f32) -> kernel MLP module justified; below -> ship table+heuristic.

Report: tools/target/mlp_vendor_probe.md
Usage:
  python tools/probe_mlp_vendor.py
  python tools/probe_mlp_vendor.py --epochs 20 --patience 5
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

import numpy as np
import torch
import torch.nn as nn

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "tools" / "target"
sys.path.insert(0, str(ROOT / "tools"))
import validate_hw_expert_v4_class as V  # noqa: E402  (split_by_device, pack_vid_did)

DATASET = ROOT / "models" / "hw_expert" / "v4" / "dataset_class_v3.json"
VOCAB_FILE = ROOT / "models" / "hw_expert" / "v4" / "vocab_class_v3.json"
REPORT = TARGET / "mlp_vendor_probe.md"
N_FAMILY = 21          # columns 0..20 (0 unused as a label; 1-19 specific, 20 other)
SWEEP = [(N, H) for N in (64, 256) for H in (128, 256, 384)]
SIZE_BUDGET = 260 * 1024
MAJORITY_ACC = 65.60   # measured device-level 'other' fraction on v3 (recomputed at runtime)


class MlpVendor(nn.Module):
    """Embed-sum MLP; bias=False -> size is EXACTLY 4*N*H + H*H + 21*H f32."""

    def __init__(self, vocab, hidden):
        super().__init__()
        self.vocab = vocab
        self.emb = nn.Embedding(4 * vocab, hidden)   # flat idx = pos*vocab + token
        self.fc1 = nn.Linear(hidden, hidden, bias=False)
        self.fc2 = nn.Linear(hidden, N_FAMILY, bias=False)
        nn.init.normal_(self.emb.weight, std=0.1)

    def forward(self, tok):                          # tok: (B, 4) long
        pos = torch.arange(4, device=tok.device).view(1, 4) * self.vocab
        h = self.emb(tok + pos).sum(dim=1)           # (B, H)
        return self.fc2(torch.relu(self.fc1(h)))

    def n_params(self):
        return 4 * self.vocab * self.fc1.in_features + \
            self.fc1.in_features * self.fc1.out_features + \
            self.fc2.in_features * self.fc2.out_features


class OneHotLinear(nn.Module):
    """Reference: single linear 4N->21 on the same 4-token one-hot concat
    (embedding-sum trick is mathematically identical to Linear(4N,21), bias=False)."""

    def __init__(self, vocab):
        super().__init__()
        self.vocab = vocab
        self.emb = nn.Embedding(4 * vocab, N_FAMILY)
        nn.init.normal_(self.emb.weight, std=0.1)

    def forward(self, tok):
        pos = torch.arange(4, device=tok.device).view(1, 4) * self.vocab
        return self.emb(tok + pos).sum(dim=1)


def size_bytes(vocab, hidden):
    return (4 * vocab * hidden + hidden * hidden + 21 * hidden) * 4


@torch.no_grad()
def eval_devices(model, samples, hold_idx, vocab):
    """Device-level holdout family acc (1 vote per device; label = first sample
    of the device, like the reference probes). Returns overall / specific-only
    (GT in 1-19) / 'other'->specific confusion."""
    dev_order, dev_first = [], {}
    for i in hold_idx:
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        if dev not in dev_first:
            dev_first[dev] = i
            dev_order.append(dev)
    toks = [V.pack_vid_did(d[0], d[1], vocab) for d in dev_order]
    gts = np.array([int(samples[dev_first[d]]["y"].get("family", 0)) for d in dev_order])
    model.eval()
    preds = []
    for st in range(0, len(toks), 4096):
        b = torch.tensor(toks[st:st + 4096], dtype=torch.long)
        preds.append(model(b).argmax(1).numpy())
    preds = np.concatenate(preds)
    model.train()
    overall = float((preds == gts).mean() * 100.0)
    spec = (gts >= 1) & (gts <= 19)
    n_spec = int(spec.sum())
    spec_acc = float((preds[spec] == gts[spec]).mean() * 100.0) if n_spec else float("nan")
    spec_lost_other = int((spec & (preds == 20)).sum())
    return dict(overall=overall, spec=spec_acc, n_spec=n_spec,
                spec_lost_other=spec_lost_other, n_dev=len(dev_order),
                n_other=int((gts == 20).sum()))


def train_config(samples, train_idx, hold_idx, vocab, hidden, args, name):
    X = torch.tensor([V.pack_vid_did(samples[i]["meta"]["vid"],
                                     samples[i]["meta"]["did"], vocab)
                      for i in train_idx], dtype=torch.long)
    Y = torch.tensor([int(samples[i]["y"].get("family", 0)) for i in train_idx],
                     dtype=torch.long)
    model = MlpVendor(vocab, hidden)
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-5)
    crit = nn.CrossEntropyLoss()
    n = len(X)
    best_overall, best_state, best_epoch, stall = -1.0, None, -1, 0
    epochs_done = 0
    t0 = time.time()
    for epoch in range(args.epochs):
        model.train()
        perm = torch.randperm(n)
        tot, nb = 0.0, 0
        for st in range(0, n, args.batch):
            idx = perm[st:st + args.batch]
            bx, by = X[idx], Y[idx]
            opt.zero_grad()
            loss = crit(model(bx), by)
            loss.backward()
            opt.step()
            tot += loss.item()
            nb += 1
        ev = eval_devices(model, samples, hold_idx, vocab)
        epochs_done = epoch + 1
        print(f"    ep {epoch:2d} loss={tot / max(nb, 1):.4f} "
              f"overall={ev['overall']:5.2f}% spec={ev['spec']:5.2f}%")
        if ev["overall"] > best_overall + 1e-9:
            best_overall = ev["overall"]
            best_state = {k: v.detach().clone() for k, v in model.state_dict().items()}
            best_epoch = epoch
            stall = 0
        else:
            stall += 1
        if stall >= args.patience:
            print(f"    [early stop] patience {args.patience} (best ep {best_epoch}, "
                  f"overall={best_overall:.2f}%)")
            break
    model.load_state_dict(best_state)
    ev = eval_devices(model, samples, hold_idx, vocab)
    ev["best_epoch"] = best_epoch
    ev["epochs_done"] = epochs_done
    ev["secs"] = time.time() - t0
    ev["params"] = model.n_params()
    ev["size"] = size_bytes(vocab, hidden)
    ev["name"] = name
    return ev


def train_linear_ref(samples, train_idx, hold_idx, args):
    vocab = 256
    X = torch.tensor([V.pack_vid_did(samples[i]["meta"]["vid"],
                                     samples[i]["meta"]["did"], vocab)
                      for i in train_idx], dtype=torch.long)
    Y = torch.tensor([int(samples[i]["y"].get("family", 0)) for i in train_idx],
                     dtype=torch.long)
    model = OneHotLinear(vocab)
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-5)
    crit = nn.CrossEntropyLoss()
    n = len(X)
    best_overall, best_state, best_epoch, stall = -1.0, None, -1, 0
    for epoch in range(args.epochs):
        model.train()
        perm = torch.randperm(n)
        for st in range(0, n, args.batch):
            idx = perm[st:st + args.batch]
            opt.zero_grad()
            loss = crit(model(X[idx]), Y[idx])
            loss.backward()
            opt.step()
        ev = eval_devices(model, samples, hold_idx, vocab)
        if ev["overall"] > best_overall + 1e-9:
            best_overall = ev["overall"]
            best_state = {k: v.detach().clone() for k, v in model.state_dict().items()}
            best_epoch = epoch
            stall = 0
        else:
            stall += 1
        if stall >= args.patience:
            break
    model.load_state_dict(best_state)
    ev = eval_devices(model, samples, hold_idx, vocab)
    ev["best_epoch"] = best_epoch
    ev["vocab"] = vocab
    return ev


def write_report(results, lin_ref, data_stats, t_total):
    TARGET.mkdir(parents=True, exist_ok=True)
    fam_names = json.loads(VOCAB_FILE.read_text(encoding="utf-8"))["family"]
    md = []
    w = md.append
    w("# MLP Vendor Probe — honest ceiling on vendor-specific driver families")
    w("")
    w("Diagnostic lane (tools-only). No model file shipped, no kernel change.")
    w("")
    w("## Method")
    w("")
    w("- Data: `models/hw_expert/v4/dataset_class_v3.json` — "
      f"{data_stats['n_samples']:,} samples, {data_stats['n_devs']:,} unique devices.")
    w(f"- Split: 90/10 by unique (vid,did), seed 42 (holdout {data_stats['n_hold_dev']:,} "
      f"devices; {data_stats['n_hold_spec']:,} with specific GT 1-19, "
      f"{data_stats['n_hold_other']:,} GT 'other'/20). Family labels 1-20 "
      f"(0=unknown never used as label); family head = 21 columns.")
    w(f"- Model: continuous f32 embed-sum MLP — 4 tokens from (vid,did) via "
      f"`[(vid>>8)%N, vid%N, (did>>8)%N, did%N]`, per-position embedding table "
      f"(4N, hidden) summed, fc1(hidden,hidden) → ReLU → fc2(hidden,21). "
      f"No biases → params = 4·N·hidden + hidden² + 21·hidden, f32.")
    w("- Train: plain CE, AdamW lr=1e-3 wd=1e-5, batch 4096, CPU, ≤20 epochs, "
      "early-stop patience 5 on device-level holdout family acc (overall; "
      "checkpoint = best overall epoch). Eval = 1 vote per device, label = "
      "first sample of the device.")
    w("- Metrics: (a) OVERALL device acc, (b) SPECIFIC-ONLY device acc "
      "(GT family 1-19 — the headline: devices the packed table does not "
      "cover must be labeled specifically), (c) 'other'→specific confusion "
      "(specific GT devices predicted as 'other').")
    w("- References on the same split: majority 'other' (always predict 20) and "
      "one-hot linear (4N→21, vocab 256, same train regime).")
    w("- Gate: overall ≥ 65% **and** specific-only ≥ 65% on the best config "
      "≤ 260KB f32 → kernel MLP module justified.")
    w("")
    w("## Sweep results (best-overall-epoch checkpoint)")
    w("")
    w("| config (vocab/hidden) | epochs | overall acc | specific-only acc | "
      "spec lost to 'other' | size (f32) | ≤260KB |")
    w("|---|---|---|---|---|---|---|")
    for r in results:
        fit = "**✓**" if r["size"] <= SIZE_BUDGET else "✗"
        w(f"| {r['name']} | {r['epochs_done']} | {r['overall']:.2f}% | "
          f"{r['spec']:.2f}% | {r['spec_lost_other']}/{r['n_spec']} | "
          f"{r['size']:,} B ({r['size'] / 1024:.1f} KB) | {fit} |")
    w("")
    w(f"| reference | — | overall acc | specific-only acc |")
    w("|---|---|---|---|")
    w(f"| majority 'other' (predict 20) | — | {MAJORITY_ACC:.2f}% | 0.00% |")
    w(f"| one-hot linear (vocab 256) | — | {lin_ref['overall']:.2f}% | "
      f"{lin_ref['spec']:.2f}% |")
    w("")
    best = results[0]
    for r in results:
        if r["size"] <= SIZE_BUDGET and (best["size"] > SIZE_BUDGET or r["spec"] > best["spec"]):
            best = r
    w(f"## Best config ≤ 260KB: **{best['name']}**")
    w("")
    w(f"- overall acc: **{best['overall']:.2f}%**  (gate ≥65: "
      f"{'PASS' if best['overall'] >= 65 else 'FAIL'})")
    w(f"- specific-only acc: **{best['spec']:.2f}%**  (gate ≥65: "
      f"{'PASS' if best['spec'] >= 65 else 'FAIL'})")
    w(f"- size: **{best['size']:,} bytes ({best['size'] / 1024:.1f} KB)** "
      f"≤ 260KB budget")
    w(f"- epochs run: {best['epochs_done']} (best ep {best['best_epoch']}); "
      f"spec devices lost to 'other': {best['spec_lost_other']}/{best['n_spec']}")
    w("")
    gate_ok = best["overall"] >= 65 and best["spec"] >= 65
    w("## Verdict")
    w("")
    if gate_ok:
        w(f"**GATE PASS** — overall {best['overall']:.2f}% and specific-only "
          f"{best['spec']:.2f}% both ≥ 65% on unseen devices (90/10 device split, "
          f"seed 42). A small continuous f32 MLP classifies vendor-specific "
          f"driver families on never-seen devices; the in-kernel MLP module is "
          f"justified for the table-miss path (packed table keeps 100% on known "
          f"devices; MLP covers the tail).")
    else:
        best_spec = max(results, key=lambda r: r["spec"])
        extra = (f"Best raw sweep config {best_spec['name']} hits spec "
                 f"{best_spec['spec']:.2f}% but exceeds the 260KB budget "
                 f"({best_spec['size'] / 1024:.1f} KB) — only the config above is "
                 f"shippable at that size.") if best_spec["size"] > SIZE_BUDGET else ""
        w(f"**GATE FAIL** — best config ≤260KB reaches overall {best['overall']:.2f}% "
          f"and specific-only {best['spec']:.2f}%, below the 65%/65% bar. The MLP "
          f"cannot separate unseen vendor families better than the majority "
          f"('other' = {MAJORITY_ACC:.2f}%) + table; ship table+heuristic (known "
          f"device → table, unknown → heuristic family by vendor class). {extra}")
    w("")
    w("## Kernel MLP module spec (if gated in)")
    w("")
    w("### Tokenizer")
    w("```")
    w(f"fn tokens(vid: u16, did: u16, n: usize) -> [usize; 4] {{")
    w(f"    [(vid >> 8) as usize % n, vid as usize % n,")
    w(f"     (did >> 8) as usize % n, did as usize % n]")
    w(f"}}")
    w("```")
    w("")
    w("### Forward (f32, no biases, no batch)")
    w("```")
    w(f"// N = vocab ({best['vocab']}), H = hidden ({best['hidden']})")
    w(f"// E: [4*N, H]  W1: [H, H]  W2: [H, 21]   (flat embed idx = pos*N + tok)")
    w(f"h[j] = sum over pos of E[pos*N + tok[pos]][j]          // 4 lookups + add")
    w(f"h = relu(W1 @ h)                                        // (H) <- (H) matmul")
    w(f"logits = W2 @ h                                         // (21)")
    w(f"family = argmax(logits); if family == 0 {{ unknown }}   // col 0 unused")
    w("```")
    w("")
    w(f"Layers/dims: embed 4·N×H (no bias) → fc1 H×H (no bias) → ReLU → "
      f"fc2 H×21 (no bias). Activations: ReLU hidden, argmax head. "
      f"Params = 4·N·H + H² + 21·H; at N={best['vocab']}, H={best['hidden']} = "
      f"{best['params']:,} f32 = {best['size']:,} bytes (plus tokenizer, no tables).")
    w("")
    w(f"Total probe wall time: {t_total:.0f}s. "
      f"Diagnostic only — no model file written.")
    REPORT.write_text("\n".join(md) + "\n", encoding="utf-8")
    return best


def main():
    ap = argparse.ArgumentParser(description="MLP vendor-label ceiling probe (diagnostics only)")
    ap.add_argument("--epochs", type=int, default=20)
    ap.add_argument("--patience", type=int, default=5)
    ap.add_argument("--batch", type=int, default=4096)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--seed", type=int, default=42)
    args = ap.parse_args()

    torch.manual_seed(args.seed)
    np.random.seed(args.seed)
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:
        pass

    t_all = time.time()
    with open(DATASET, encoding="utf-8") as f:
        data = json.load(f)
    samples = data["samples"] if isinstance(data, dict) else data
    train_idx, hold_idx, hold_devs, n_devs = V.split_by_device(samples, 0.1, args.seed)

    # device-level stats on the holdout (for the report + majority baseline)
    dev_first = {}
    for i in hold_idx:
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        dev_first.setdefault(dev, i)
    n_hold_spec = sum(1 for d in dev_first if 1 <= int(samples[dev_first[d]]["y"]["family"]) <= 19)
    n_hold_other = sum(1 for d in dev_first if int(samples[dev_first[d]]["y"]["family"]) == 20)
    data_stats = dict(n_samples=len(samples), n_devs=n_devs,
                      n_hold_dev=len(dev_first), n_hold_spec=n_hold_spec,
                      n_hold_other=n_hold_other)
    global MAJORITY_ACC
    MAJORITY_ACC = n_hold_other / len(dev_first) * 100.0

    print("=" * 64)
    print("  MLP VENDOR PROBE — honest ceiling on vendor-specific families (v3)")
    print(f"  samples={len(samples)} devices={n_devs} holdout devices={len(dev_first)} "
          f"(specific {n_hold_spec}, other {n_hold_other})")
    print(f"  majority 'other' (device-level) = {MAJORITY_ACC:.2f}%")
    print("=" * 64)

    results = []
    for vocab, hidden in SWEEP:
        name = f"{vocab}/{hidden}"
        print(f"\n  CONFIG {name}  (size {size_bytes(vocab, hidden):,} B = "
              f"{size_bytes(vocab, hidden) / 1024:.1f} KB)")
        res = train_config(samples, train_idx, hold_idx, vocab, hidden, args, name)
        print(f"  -> {name}: overall={res['overall']:.2f}% spec={res['spec']:.2f}% "
              f"(ep {res['best_epoch']}, {res['epochs_done']} epochs, {res['secs']:.0f}s)")
        results.append(res)

    print("\n  LINEAR REFERENCE (one-hot, vocab 256, same split/regime)")
    lin_ref = train_linear_ref(samples, train_idx, hold_idx, args)
    print(f"  -> one-hot linear: overall={lin_ref['overall']:.2f}% "
          f"spec={lin_ref['spec']:.2f}%")

    # attach vocab/hidden to results for the kernel-spec section
    for (vocab, hidden), res in zip(SWEEP, results):
        res["vocab"], res["hidden"] = vocab, hidden

    t_total = time.time() - t_all
    best = write_report(results, lin_ref, data_stats, t_total)

    print("\n" + "=" * 64)
    print("  HEADLINE — MLP vendor probe")
    print(f"    best config (<=260KB)   : {best['name']}  ({best['size']:,} B = {best['size'] / 1024:.1f} KB)")
    print(f"    overall acc             : {best['overall']:.2f}%   (gate >=65: {'PASS' if best['overall'] >= 65 else 'FAIL'})")
    print(f"    SPECIFIC-only acc       : {best['spec']:.2f}%   (gate >=65: {'PASS' if best['spec'] >= 65 else 'FAIL'})")
    print(f"    spec lost to 'other'    : {best['spec_lost_other']}/{best['n_spec']}")
    print(f"    majority 'other'        : {MAJORITY_ACC:.2f}%")
    print(f"    one-hot linear          : {lin_ref['overall']:.2f}% overall / {lin_ref['spec']:.2f}% spec")
    verdict = "GATE PASS — kernel MLP justified" if (best["overall"] >= 65 and best["spec"] >= 65) else "GATE FAIL — ship table+heuristic"
    print(f"    VERDICT                 : {verdict}")
    print(f"    report                  : {REPORT}")
    print(f"    total time              : {t_total:.0f}s")
    print("=" * 64)


if __name__ == "__main__":
    main()
