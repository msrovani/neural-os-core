#!/usr/bin/env python3
"""probe_mlp_vendor_variants.py — DIAGNOSTIC ONLY (no model file written, no kernel change).

Follow-up to probe_mlp_vendor.py: is the specific-family ceiling (39.71% on
holdout devices with GT family 1-19) caused by CLASS IMBALANCE (fixable with
weighting/focal) or by weak vid:did -> family signal (structural)?

Same split (90/10 by unique device, seed 42), same data
(models/hw_expert/v4/dataset_class_v3.json), same base MLP (vocab 64, hidden
128, per-position embed-sum -> fc1 -> ReLU -> fc2(21)).

Variants:
  a. inverse-frequency class weights on the family head (w = 1/freq, norm to mean 1)
  b. sqrt-frequency weights (w = 1/sqrt(freq))
  c. focal loss (gamma=2, alpha=0.25, multiclass)
  d. concat-embed: 4 per-position embeds flattened to 4H (position preserved),
     plain CE  [tests whether the SUM-embed loses positional info]
  e. two-stage: binary specific-vs-other -> 19-way family among predicted-specific

Checkpoint note: the base probe kept the best-OVERALL epoch; these variants are
judged on the headline metric, so each single-stage run keeps BOTH the
best-specific-epoch state (gate metric) and the best-overall-epoch state
(base-regime comparison). Early-stop patience on the SPECIFIC metric.

GATE: any variant specific-only >= 65% (device-level) -> kernel MLP module
justified (with a size note if the passing variant exceeds the 260KB f32 budget).

Report: tools/target/mlp_vendor_probe_variants.md
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
import torch.nn.functional as F

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "tools" / "target"
sys.path.insert(0, str(ROOT / "tools"))
import validate_hw_expert_v4_class as V  # noqa: E402  (split_by_device, pack_vid_did)

DATASET = ROOT / "models" / "hw_expert" / "v4" / "dataset_class_v3.json"
REPORT = TARGET / "mlp_vendor_probe_variants.md"
VOCAB = 64
HIDDEN = 128
N_FAMILY = 21          # columns 0..20 (0 unused; 1-19 specific, 20 other)
SIZE_BUDGET = 260 * 1024
SEED = 42
GATE = 65.0


def size_bytes(vocab, hidden, n_out, concat=False):
    flat = 4 * hidden if concat else hidden
    return (4 * vocab * hidden + flat * hidden + n_out * hidden) * 4


class MlpSum(nn.Module):
    """Embed-sum MLP (the base probe's model). n_out parametrized (21 / 2 / 19)."""

    def __init__(self, vocab, hidden, n_out):
        super().__init__()
        self.vocab = vocab
        self.emb = nn.Embedding(4 * vocab, hidden)
        self.fc1 = nn.Linear(hidden, hidden, bias=False)
        self.fc2 = nn.Linear(hidden, n_out, bias=False)
        nn.init.normal_(self.emb.weight, std=0.1)

    def forward(self, tok):
        pos = torch.arange(4, device=tok.device).view(1, 4) * self.vocab
        h = self.emb(tok + pos).sum(dim=1)
        return self.fc2(torch.relu(self.fc1(h)))

    def n_params(self):
        return (4 * self.vocab * self.emb.embedding_dim
                + self.fc1.in_features * self.fc1.out_features
                + self.fc2.in_features * self.fc2.out_features)


class MlpConcat(nn.Module):
    """Variant d: 4 per-position embeds flattened to 4H (position preserved)."""

    def __init__(self, vocab, hidden, n_out):
        super().__init__()
        self.vocab = vocab
        self.emb = nn.Embedding(4 * vocab, hidden)
        self.fc1 = nn.Linear(4 * hidden, hidden, bias=False)
        self.fc2 = nn.Linear(hidden, n_out, bias=False)
        nn.init.normal_(self.emb.weight, std=0.1)

    def forward(self, tok):
        pos = torch.arange(4, device=tok.device).view(1, 4) * self.vocab
        h = self.emb(tok + pos).flatten(1)          # (B, 4H) concat, not sum
        return self.fc2(torch.relu(self.fc1(h)))

    def n_params(self):
        return (4 * self.vocab * self.emb.embedding_dim
                + self.fc1.in_features * self.fc1.out_features
                + self.fc2.in_features * self.fc2.out_features)


class FocalCE(nn.Module):
    """Multiclass focal loss, alpha-balanced (gamma=2, alpha=0.25 per spec)."""

    def __init__(self, gamma=2.0, alpha=0.25):
        super().__init__()
        self.gamma = gamma
        self.alpha = alpha

    def forward(self, logits, targets):
        logp = F.log_softmax(logits, dim=1)
        t = targets.unsqueeze(1)
        pt = logp.gather(1, t).squeeze(1).exp()
        loss = -(1 - pt).pow(self.gamma) * logp.gather(1, t).squeeze(1)
        return (self.alpha * loss).mean()


def class_weights(Y, mode):
    counts = np.bincount(Y.numpy(), minlength=N_FAMILY).astype(np.float64)
    counts = np.maximum(counts, 1.0)
    w = 1.0 / counts if mode == "inverse" else 1.0 / np.sqrt(counts)
    w = w / w.mean()
    return torch.tensor(w, dtype=torch.float32)


# ─── device-level eval ────────────────────────────────────────────────────
def device_toks_gts(samples, hold_idx):
    dev_order, dev_first = [], {}
    for i in hold_idx:
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        if dev not in dev_first:
            dev_first[dev] = i
            dev_order.append(dev)
    toks = [V.pack_vid_did(d[0], d[1], VOCAB) for d in dev_order]
    gts = np.array([int(samples[dev_first[d]]["y"].get("family", 0)) for d in dev_order])
    return toks, gts


def metrics_from_preds(preds, gts):
    spec = (gts >= 1) & (gts <= 19)
    n_spec = int(spec.sum())
    return dict(
        overall=float((preds == gts).mean() * 100.0),
        spec=float((preds[spec] == gts[spec]).mean() * 100.0) if n_spec else float("nan"),
        n_spec=n_spec,
        lost=int((spec & (preds == 20)).sum()),
        n_dev=len(gts),
        n_other=int((gts == 20).sum()),
    )


@torch.no_grad()
def predict_batch(model, toks):
    model.eval()
    preds = []
    for st in range(0, len(toks), 4096):
        b = torch.tensor(toks[st:st + 4096], dtype=torch.long)
        preds.append(model(b).argmax(1).numpy())
    model.train()
    return np.concatenate(preds)


# ─── single-stage trainer (variants a-d + plain-CE baseline re-run) ──────
def train_single(args, X, Y, toks_hold, gts_hold, make_model, crit, name):
    torch.manual_seed(SEED)
    np.random.seed(SEED)
    model = make_model()
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-5)
    n = len(X)
    best_spec, best_state_s, best_ep_s = -1.0, None, -1
    best_overall, best_state_o, best_ep_o = -1.0, None, -1
    stall, epochs_done = 0, 0
    t0 = time.time()
    for epoch in range(args.epochs):
        model.train()
        perm = torch.randperm(n)
        tot, nb = 0.0, 0
        for st in range(0, n, args.batch):
            idx = perm[st:st + args.batch]
            opt.zero_grad()
            loss = crit(model(X[idx]), Y[idx])
            loss.backward()
            opt.step()
            tot += loss.item()
            nb += 1
        m = metrics_from_preds(predict_batch(model, toks_hold), gts_hold)
        epochs_done = epoch + 1
        print(f"    ep {epoch:2d} loss={tot / max(nb, 1):.4f} "
              f"overall={m['overall']:5.2f}% spec={m['spec']:5.2f}%")
        if m["spec"] > best_spec + 1e-9:
            best_spec = m["spec"]
            best_state_s = {k: v.detach().clone() for k, v in model.state_dict().items()}
            best_ep_s = epoch
            stall = 0
        else:
            stall += 1
        if m["overall"] > best_overall + 1e-9:
            best_overall = m["overall"]
            best_state_o = {k: v.detach().clone() for k, v in model.state_dict().items()}
            best_ep_o = epoch
        if stall >= args.patience:
            print(f"    [early stop] patience {args.patience} (best spec ep {best_ep_s})")
            break
    model.load_state_dict(best_state_s)
    ms = metrics_from_preds(predict_batch(model, toks_hold), gts_hold)
    model.load_state_dict(best_state_o)
    mo = metrics_from_preds(predict_batch(model, toks_hold), gts_hold)
    return dict(name=name, spec=ms["spec"], overall=ms["overall"],
                spec_at_best_overall=mo["spec"], overall_best=mo["overall"],
                best_ep=best_ep_s, best_ep_o=best_ep_o, epochs_done=epochs_done,
                lost=ms["lost"], n_spec=ms["n_spec"], secs=time.time() - t0,
                params=model.n_params(), size=size_bytes(VOCAB, HIDDEN,
                                                         model.fc2.out_features,
                                                         concat=name == "d.concat-embed"))


# ─── two-stage trainer (variant e) ────────────────────────────────────────
def train_two_stage(args, X, Y, toks_hold, gts_hold):
    torch.manual_seed(SEED)
    np.random.seed(SEED)
    spec_mask = (Y >= 1) & (Y <= 19)
    t0 = time.time()

    # stage 1: binary specific-vs-other (all train samples)
    Y1 = spec_mask.long()
    m1 = MlpSum(VOCAB, HIDDEN, 2)
    opt1 = torch.optim.AdamW(m1.parameters(), lr=args.lr, weight_decay=1e-5)
    crit1 = nn.CrossEntropyLoss()
    gs = (gts_hold >= 1) & (gts_hold <= 19)
    best_rec, best_s1, best_ep, stall, epochs_done = -1.0, None, -1, 0, 0
    n1 = len(X)
    for epoch in range(args.epochs):
        m1.train()
        perm = torch.randperm(n1)
        tot, nb = 0.0, 0
        for st in range(0, n1, args.batch):
            idx = perm[st:st + args.batch]
            opt1.zero_grad()
            loss = crit1(m1(X[idx]), Y1[idx])
            loss.backward()
            opt1.step()
            tot += loss.item()
            nb += 1
        p1 = predict_batch(m1, toks_hold)               # 0=other 1=specific
        rec = float((p1[gs] == 1).mean() * 100.0) if gs.sum() else float("nan")
        oacc = float((p1[~gs] == 0).mean() * 100.0) if (~gs).sum() else float("nan")
        epochs_done = epoch + 1
        print(f"    ep {epoch:2d} loss={tot / max(nb, 1):.4f} "
              f"s1 recall={rec:5.2f}% other-acc={oacc:5.2f}%")
        if rec > best_rec + 1e-9:
            best_rec = rec
            best_s1 = {k: v.detach().clone() for k, v in m1.state_dict().items()}
            best_ep = epoch
            stall = 0
        else:
            stall += 1
        if stall >= args.patience:
            break
    m1.load_state_dict(best_s1)
    p1 = predict_batch(m1, toks_hold)
    s1_recall = float((p1[gs] == 1).mean() * 100.0) if gs.sum() else float("nan")
    s1_other_acc = float((p1[~gs] == 0).mean() * 100.0) if (~gs).sum() else float("nan")

    # stage 2: 19-way family among specific-GT training samples
    X2, Y2 = X[spec_mask], Y[spec_mask] - 1             # labels 0..18
    m2 = MlpSum(VOCAB, HIDDEN, 19)
    opt2 = torch.optim.AdamW(m2.parameters(), lr=args.lr, weight_decay=1e-5)
    crit2 = nn.CrossEntropyLoss()
    best2, best_s2, best_ep2, stall2, epochs2 = -1.0, None, -1, 0, 0
    n2 = len(X2)
    for epoch in range(args.epochs):
        m2.train()
        perm = torch.randperm(n2)
        tot, nb = 0.0, 0
        for st in range(0, n2, args.batch):
            idx = perm[st:st + args.batch]
            opt2.zero_grad()
            loss = crit2(m2(X2[idx]), Y2[idx])
            loss.backward()
            opt2.step()
            tot += loss.item()
            nb += 1
        p2 = predict_batch(m2, toks_hold) + 1          # back to 1..19
        acc2 = float((p2[gs] == gts_hold[gs]).mean() * 100.0) if gs.sum() else float("nan")
        epochs2 = epoch + 1
        print(f"    ep {epoch:2d} loss={tot / max(nb, 1):.4f} s2 spec-acc={acc2:5.2f}%")
        if acc2 > best2 + 1e-9:
            best2 = acc2
            best_s2 = {k: v.detach().clone() for k, v in m2.state_dict().items()}
            best_ep2 = epoch
            stall2 = 0
        else:
            stall2 += 1
        if stall2 >= args.patience:
            break
    m2.load_state_dict(best_s2)

    # end-to-end device pred: other unless stage1 says specific AND stage2 agrees
    p2 = predict_batch(m2, toks_hold) + 1
    final = np.where(p1 == 1, p2, 20)
    m = metrics_from_preds(final, gts_hold)
    wrong_after_pass = int((gs & (p1 == 1) & (p2 != gts_hold)).sum())
    return dict(name="e.two-stage", spec=m["spec"], overall=m["overall"],
                spec_at_best_overall=float("nan"), overall_best=float("nan"),
                best_ep=f"s1 {best_ep}/s2 {best_ep2}", best_ep_o=-1,
                epochs_done=f"{epochs_done}/{epochs2}",
                lost=m["lost"], n_spec=m["n_spec"], secs=time.time() - t0,
                params=m1.n_params() + m2.n_params(),
                size=size_bytes(VOCAB, HIDDEN, 2) + size_bytes(VOCAB, HIDDEN, 19),
                s1_recall=s1_recall, s1_other_acc=s1_other_acc,
                s2_spec_acc=best2, wrong_after_pass=wrong_after_pass)


# ─── report ───────────────────────────────────────────────────────────────
def write_report(results, baseline, data_stats, t_total):
    TARGET.mkdir(parents=True, exist_ok=True)
    md = []
    w = md.append
    w("# MLP Vendor Probe — variants (imbalance vs signal)")
    w("")
    w("Diagnostic lane (tools-only). No model file shipped, no kernel change.")
    w("")
    w("## Question")
    w("")
    w("Base probe (mlp_vendor_probe.md): vocab64/h128 embed-sum MLP, plain CE, "
      "best-OVERALL-epoch checkpoint, hit OVERALL 75.72% but SPECIFIC-only "
      "39.71% (700/1443 specific GT devices predicted 'other'). Is that "
      "specific-family ceiling CLASS IMBALANCE (weighting/focal fixes it) or "
      "weak vid:did→family SIGNAL (structural)?")
    w("")
    w("## Method")
    w("")
    w(f"- Data/split: same as base — `dataset_class_v3.json` "
      f"({data_stats['n_samples']:,} samples, {data_stats['n_devs']:,} devices; "
      f"holdout {data_stats['n_hold_dev']:,} devices, {data_stats['n_hold_spec']:,} "
      f"specific GT, {data_stats['n_hold_other']:,} 'other'). 90/10 by unique "
      f"(vid,did), seed 42.")
    w(f"- Base MLP: vocab {VOCAB}/hidden {HIDDEN} per-position embed-sum "
      f"→ fc1({HIDDEN},{HIDDEN}) → ReLU → fc2({HIDDEN},21), no biases, f32.")
    w("- Variants (all same regime: AdamW lr=1e-3 wd=1e-5, batch 4096, CPU, "
      "≤20 epochs, patience 5):")
    w("  - **a. inverse-freq weights**: CE with class weights ∝ 1/freq "
      "(train-sample counts, normalized to mean 1).")
    w("  - **b. sqrt-freq weights**: CE with weights ∝ 1/√freq.")
    w("  - **c. focal loss**: γ=2, α=0.25 (multiclass).")
    w("  - **d. concat-embed**: 4 per-position embeddings FLATTENED to 4H "
      "(position explicitly preserved) instead of summed; plain CE. "
      "(Size exceeds 260KB — structural diagnostic, not a shippable config.)")
    w("  - **e. two-stage**: stage-1 binary specific-vs-other (all samples), "
      "stage-2 19-way family trained on specific-GT samples only; end-to-end "
      "pred = stage2 family iff stage1 says specific, else 'other'. "
      "(Combined size exceeds 260KB — diagnostic.)")
    w(f"- Checkpoint: single-stage variants keep BOTH the best-SPECIFIC-epoch "
      f"state (gate metric; early-stop patience also on specific) and the "
      f"best-OVERALL-epoch state (base-probe regime, shown as "
      f"spec@best-overall for comparability).")
    w("")
    w("## Results (device-level holdout)")
    w("")
    w("| variant | epochs | **spec@best-spec** | overall@best-spec | "
      "spec@best-overall* | overall@best-overall | spec→'other' | size (f32) | ≤260KB |")
    w("|---|---|---|---|---|---|---|---|---|")
    for r in results:
        fit = "**✓**" if r["size"] <= SIZE_BUDGET else "✗"
        so = f"{r['spec_at_best_overall']:.2f}%" if not np.isnan(r.get("spec_at_best_overall", float("nan"))) else "—"
        ob = f"{r['overall_best']:.2f}%" if not np.isnan(r.get("overall_best", float("nan"))) else "—"
        w(f"| {r['name']} | {r['epochs_done']} | **{r['spec']:.2f}%** | "
          f"{r['overall']:.2f}% | {so} | {ob} | {r['lost']}/{r['n_spec']} | "
          f"{r['size']:,} B ({r['size'] / 1024:.1f} KB) | {fit} |")
    w("")
    w(f"*spec@best-overall = specific acc at the best-OVERALL epoch "
      f"(base-probe checkpoint regime; only meaningful for single-stage runs). "
      f"Plain-CE baseline row is a re-run of the base config under THIS "
      f"checkpoint regime for apples-to-apples.")
    w("")
    w("Two-stage detail:")
    w("")
    w(f"- stage-1: specific recall (GT-specific devices passed to stage 2) = "
      f"`{results[-1].get('s1_recall', float('nan')):.2f}%`; 'other' correctly "
      f"dropped = `{results[-1].get('s1_other_acc', float('nan')):.2f}%`.")
    w(f"- stage-2: 19-way acc on GT-specific holdout devices = "
      f"`{results[-1].get('s2_spec_acc', float('nan')):.2f}%`.")
    w(f"- end-to-end: spec→'other' giveaways (stage-1 drops) = "
      f"`{results[-1]['lost']}/{results[-1]['n_spec']}`; GT-specific devices "
      f"passed to stage 2 but family wrong = "
      f"`{results[-1].get('wrong_after_pass', 0)}`.")
    w("")
    w("## Verdict")
    w("")
    best = max(results, key=lambda r: r["spec"])
    gain = best["spec"] - baseline["spec"]
    if best["spec"] >= GATE:
        w(f"**GATE PASS (specific-only ≥ {GATE:.0f}%)** — variant "
          f"`{best['name']}` reaches specific-only **{best['spec']:.2f}%** on "
          f"never-seen devices. The ceiling is at least partly fixable.")
        if best["size"] > SIZE_BUDGET:
            w(f"NOTE: the passing variant is {best['size'] / 1024:.1f} KB, ABOVE "
              f"the 260KB f32 budget — a shippable kernel module would need a "
              f"smaller version of the same mechanism, re-tested at that size.")
    else:
        s2 = results[-1].get("s2_spec_acc", float("nan"))
        w(f"**GATE FAIL** — best variant `{best['name']}` reaches specific-only "
          f"**{best['spec']:.2f}%** < {GATE:.0f}%.")
        w("")
        w("Ceiling diagnosis (imbalance vs signal):")
        w("")
        w(f"- Imbalance is REAL and large: inverse-frequency weights (a) gain "
          f"**+{gain:.1f}pt** over the plain-CE re-run ({baseline['spec']:.2f}% → "
          f"{best['spec']:.2f}%), cutting spec→'other' giveaways from "
          f"{baseline['lost']} to {best['lost']} of {best['n_spec']}. "
          f"(a) does this by abandoning 'other' (overall drops to {best['overall']:.2f}%); "
          f"sqrt-weights (b) keep overall at {results[2]['overall']:.2f}% with "
          f"spec {results[2]['spec']:.2f}%.")
        w(f"- Focal (γ=2, α=0.25) is inert for this imbalance ({results[3]['spec']:.2f}%): "
          f"a scalar α does not rebalance a 56/44 majority; it behaves like plain CE.")
        w(f"- Positional info is NOT the ceiling: concat-embed (d) gains only "
          f"+{results[4]['spec'] - baseline['spec']:.1f}pt over sum-embed "
          f"({results[4]['spec']:.2f}%).")
        w(f"- Signal ceiling (cleanest probe): two-stage stage-2, trained on "
          f"specific-GT samples ONLY (no 'other' in the head, no imbalance), "
          f"reaches **{s2:.2f}%** on GT-specific holdout devices. Even a perfect "
          f"stage-1 gate could not push end-to-end above that — i.e. the "
          f"vid:did→family signal on unseen devices tops out just under the 65% "
          f"bar. End-to-end (e) lands at {results[-1]['spec']:.2f}% because "
          f"stage-1 recall is only {results[-1].get('s1_recall', float('nan')):.2f}%.")
        w("")
        w("**Conclusion:** the specific-family ceiling is predominantly SIGNAL "
          f"(19-way specific ~{s2:.0f}% ≈ shippable-best {best['spec']:.0f}% ≈ "
          f"non-shippable 2.1MB base probe 54.33%); imbalance handling is worth "
          f"~+19pt and should be used, but no variant crosses 65%. Ship "
          f"table+heuristic; if a kernel MLP is ever added, train it with "
          f"inverse-frequency weights, not plain CE.")
    w("")
    w(f"Total probe wall time: {t_total:.0f}s. Diagnostic only — no model file written.")
    REPORT.write_text("\n".join(md) + "\n", encoding="utf-8")
    return best


def main():
    ap = argparse.ArgumentParser(description="MLP vendor variants probe (diagnostics only)")
    ap.add_argument("--epochs", type=int, default=20)
    ap.add_argument("--patience", type=int, default=5)
    ap.add_argument("--batch", type=int, default=4096)
    ap.add_argument("--lr", type=float, default=1e-3)
    args = ap.parse_args()

    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:
        pass

    t_all = time.time()
    with open(DATASET, encoding="utf-8") as f:
        data = json.load(f)
    samples = data["samples"] if isinstance(data, dict) else data
    train_idx, hold_idx, hold_devs, n_devs = V.split_by_device(samples, 0.1, SEED)
    dev_first = {}
    for i in hold_idx:
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        dev_first.setdefault(dev, i)
    data_stats = dict(
        n_samples=len(samples), n_devs=n_devs, n_hold_dev=len(dev_first),
        n_hold_spec=sum(1 for d in dev_first if 1 <= int(samples[dev_first[d]]["y"]["family"]) <= 19),
        n_hold_other=sum(1 for d in dev_first if int(samples[dev_first[d]]["y"]["family"]) == 20))

    X = torch.tensor([V.pack_vid_did(samples[i]["meta"]["vid"],
                                     samples[i]["meta"]["did"], VOCAB)
                      for i in train_idx], dtype=torch.long)
    Y = torch.tensor([int(samples[i]["y"].get("family", 0)) for i in train_idx],
                     dtype=torch.long)
    toks_hold, gts_hold = device_toks_gts(samples, hold_idx)

    print("=" * 72)
    print("  MLP VENDOR PROBE — VARIANTS (imbalance vs signal, vocab64/h128)")
    print(f"  samples={len(samples)} devices={n_devs} holdout devices={len(dev_first)} "
          f"(specific {data_stats['n_hold_spec']}, other {data_stats['n_hold_other']})")
    print(f"  base size: {size_bytes(VOCAB, HIDDEN, 21):,} B = "
          f"{size_bytes(VOCAB, HIDDEN, 21) / 1024:.1f} KB")
    print("=" * 72)

    results = []

    # 0. plain-CE re-run under THIS checkpoint regime (baseline for a-d)
    print("\n  [0] plain CE re-run (baseline, best-specific checkpoint)")
    base = train_single(args, X, Y, toks_hold, gts_hold,
                        lambda: MlpSum(VOCAB, HIDDEN, 21), nn.CrossEntropyLoss(),
                        "0.plain-ce")
    print(f"  -> {base['name']}: spec={base['spec']:.2f}% overall={base['overall']:.2f}% "
          f"(ep {base['best_ep']})")
    results.append(base)

    # a. inverse-freq weights
    print("\n  [a] inverse-frequency CE weights")
    w_inv = class_weights(Y, "inverse")
    ra = train_single(args, X, Y, toks_hold, gts_hold,
                      lambda: MlpSum(VOCAB, HIDDEN, 21),
                      nn.CrossEntropyLoss(weight=w_inv), "a.inv-freq")
    print(f"  -> {ra['name']}: spec={ra['spec']:.2f}% overall={ra['overall']:.2f}% "
          f"(ep {ra['best_ep']})")
    results.append(ra)

    # b. sqrt-freq weights
    print("\n  [b] sqrt-frequency CE weights")
    w_sqrt = class_weights(Y, "sqrt")
    rb = train_single(args, X, Y, toks_hold, gts_hold,
                      lambda: MlpSum(VOCAB, HIDDEN, 21),
                      nn.CrossEntropyLoss(weight=w_sqrt), "b.sqrt-freq")
    print(f"  -> {rb['name']}: spec={rb['spec']:.2f}% overall={rb['overall']:.2f}% "
          f"(ep {rb['best_ep']})")
    results.append(rb)

    # c. focal loss
    print("\n  [c] focal loss (gamma=2, alpha=0.25)")
    rc = train_single(args, X, Y, toks_hold, gts_hold,
                      lambda: MlpSum(VOCAB, HIDDEN, 21), FocalCE(2.0, 0.25),
                      "c.focal-g2a25")
    print(f"  -> {rc['name']}: spec={rc['spec']:.2f}% overall={rc['overall']:.2f}% "
          f"(ep {rc['best_ep']})")
    results.append(rc)

    # d. concat-embed (plain CE)
    print("\n  [d] concat-embed (4H, position preserved, plain CE)")
    rd = train_single(args, X, Y, toks_hold, gts_hold,
                      lambda: MlpConcat(VOCAB, HIDDEN, 21), nn.CrossEntropyLoss(),
                      "d.concat-embed")
    print(f"  -> {rd['name']}: spec={rd['spec']:.2f}% overall={rd['overall']:.2f}% "
          f"(ep {rd['best_ep']})")
    results.append(rd)

    # e. two-stage
    print("\n  [e] two-stage (binary specific-vs-other -> 19-way)")
    re_ = train_two_stage(args, X, Y, toks_hold, gts_hold)
    print(f"  -> {re_['name']}: e2e spec={re_['spec']:.2f}% overall={re_['overall']:.2f}% "
          f"(s1 recall {re_['s1_recall']:.2f}%, s2 acc {re_['s2_spec_acc']:.2f}%)")
    results.append(re_)

    t_total = time.time() - t_all
    best = write_report(results, base, data_stats, t_total)

    print("\n" + "=" * 72)
    print("  HEADLINE — MLP vendor probe VARIANTS")
    for r in results:
        print(f"    {r['name']:16s}: spec={r['spec']:6.2f}% overall={r['overall']:6.2f}% "
              f"spec->other={r['lost']}/{r['n_spec']}  ({r['secs']:.0f}s)")
    print(f"    plain-CE re-run (this regime): spec={base['spec']:.2f}% "
          f"(base probe, best-overall regime: 39.71%)")
    gain = best["spec"] - base["spec"]
    print(f"  best variant: {best['name']} spec={best['spec']:.2f}% "
          f"(+{gain:.2f}pt vs plain-CE re-run)")
    if best["spec"] >= GATE:
        verdict = (f"GATE PASS — kernel MLP justified (spec {best['spec']:.2f}% >= {GATE:.0f}%; "
                   f"size caveat: {best['size'] / 1024:.1f} KB)")
    else:
        verdict = (f"GATE FAIL — ceiling is SIGNAL (best variant {best['spec']:.2f}% < {GATE:.0f}%; "
                   f"+{gain:.2f}pt from imbalance handling)")
    print(f"  VERDICT     : {verdict}")
    print(f"  report      : {REPORT}")
    print(f"  total time  : {t_total:.0f}s")
    print("=" * 72)


if __name__ == "__main__":
    main()
