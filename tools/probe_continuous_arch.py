#!/usr/bin/env python3
"""probe_continuous_arch.py — DIAGNOSTIC ONLY (no model file written).

Decides whether the HW Expert v4 12-class ternary plateau
(60.67% holdout family acc ~= majority 60.58%, loss drops 4.12→2.42 while acc
stays flat) is a QUANTIZATION failure or an ARCHITECTURE failure.

Control (decisive): SAME BitNetRustExactClass forward, same split (90/10 by
unique device, seed 42), same dataset (dataset_class_v2.json), same multi-task
loss — but tern/tern_linear replaced by plain fp32 matmul (weights stay fp32,
no STE clamp to {-1,0,1}). Keeps RMS norm, truncated attention q_dim=32,
SwiGLU, mean pool, 12-col family head. (The 71.2% MLP probe is a DIFFERENT
arch — not this control.)

Variants (--variant):
  cont      — fully continuous same-arch (backbone + all heads), vocab 64
  head      — backbone ternary as-is (STE, t=0.05) + CONTINUOUS fp32 family
              head (other heads stay ternary) — isolates the coarse {-1,0,1}
              family head
  vocab256  — fully continuous same-arch with vocab-256 tokenizer
              (pack_vid_did with vocab=256 — the vocab-64 tokenizer discards
              2 bits per byte; embed grows 64x128 -> 256x128)

Report: appends a section to tools/target/hwexp_continuous_control.md.
Usage:
  python tools/probe_continuous_arch.py --variant cont
  python tools/probe_continuous_arch.py --variant head
  python tools/probe_continuous_arch.py --variant vocab256
  python tools/probe_continuous_arch.py --all
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
from retrain_hw_expert_v4 import (  # noqa: E402
    BitNetRustExact, rms_norm_torch, tern, tern_linear, build_tensors,
    caps_onehot, DEVICE, N_CAPS,
)
from retrain_hw_expert_v4_class import (  # noqa: E402
    BitNetRustExactClass, N_FAMILY_FILE,
)
import validate_hw_expert_v4_class as V  # noqa: E402

DATASET = ROOT / "models" / "hw_expert" / "v4" / "dataset_class_v2.json"
REPORT = TARGET / "hwexp_continuous_control.md"
MAJORITY_ACC = 60.58  # 'other' baseline from prior probes


class ProbeModel(BitNetRustExactClass):
    """Same Rust-exact arch; quantization is selectable per part.

    backbone: "continuous" (fp32 matmul, no STE) | "ternary" (as-is, STE)
    fam_head_fp32: family head computed in fp32 even when backbone is ternary
                   (other heads follow the backbone mode).
    """

    def __init__(self, backbone="continuous", fam_head_fp32=True, vocab=64,
                 hidden=128, num_layers=6, num_heads=4, ff_dim=256, t=0.05,
                 tau=16.0, n_family=N_FAMILY_FILE):
        super().__init__(hidden=hidden, vocab=vocab, num_layers=num_layers,
                         num_heads=num_heads, ff_dim=ff_dim, t=t, tau=tau,
                         n_family=n_family)
        self.backbone = backbone
        self.fam_head_fp32 = fam_head_fp32

    def _lin(self, layer, x):
        if self.backbone == "ternary":
            return tern_linear(layer, x, self.t)
        return F.linear(x, layer.weight, None)

    def forward(self, x):
        w_emb = self.embed.weight if self.backbone == "continuous" else tern(self.embed.weight, self.t)
        h = F.embedding(x, w_emb)  # (B, S, H)
        for i in range(self.nl):
            h = rms_norm_torch(h, self.rms_a[i])
            oo = self._lin(self.o[i], self._lin(self.v_[i], h))
            attn = torch.zeros_like(h)
            attn[..., :self.q_dim] = oo[..., :self.q_dim]
            h = h + attn
            h = rms_norm_torch(h, self.rms_f[i])
            g = self._lin(self.g[i], h)
            u = self._lin(self.u[i], h)
            sw = g * torch.sigmoid(g) * u  # swiglu (Rust)
            h = h + self._lin(self.d[i], sw)
        h = rms_norm_torch(h, self.rms_o)
        pooled = h.mean(dim=1) / self.tau  # (B, H)
        if self.backbone == "continuous" or self.fam_head_fp32:
            fam = F.linear(pooled, self.family_head.weight, None)
        else:
            fam = tern_linear(self.family_head, pooled, self.t)
        return {
            "family": fam,
            "fw": self._lin(self.fw_head, pooled),
            "agent": self._lin(self.agent_head, pooled),
            "caps": self._lin(self.caps_head, pooled),
            "next": self._lin(self.next_head, pooled),
        }


def tokens_for(sample, vocab):
    if vocab == 64:
        x = list(sample["x"][:4])
        while len(x) < 4:
            x.append(0)
        return x
    return V.pack_vid_did(sample["meta"]["vid"], sample["meta"]["did"], vocab)


def build_tokens(samples, idx, vocab):
    X, Yf = [], []
    for i in idx:
        s = samples[i]
        X.append(tokens_for(s, vocab))
        Yf.append(s["y"].get("family", 0))
    return torch.tensor(X, dtype=torch.long), torch.tensor(Yf, dtype=torch.long)


@torch.no_grad()
def eval_holdout_device(model, samples, hold_idx, vocab, n_family):
    """Device-level holdout family acc (1 vote per device, label = first
    sample of the device), in-memory (no export — continuous models are not
    shippable artifacts, this lane is diagnostics only)."""
    model.eval()
    dev_order = []
    dev_first = {}
    for pos, i in enumerate(hold_idx):
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        if dev not in dev_first:
            dev_order.append(dev)
            dev_first[dev] = i
    X = torch.tensor([tokens_for(samples[dev_first[d]], vocab) for d in dev_order],
                     dtype=torch.long)
    hits = 0
    for start in range(0, len(X), 4096):
        bx = X[start:start + 4096].to(DEVICE)
        out = model(bx)
        fam = out["family"].argmax(1).cpu().numpy()
        fam = np.where(fam >= n_family, 0, fam)
        for k in range(fam.shape[0]):
            dev = dev_order[start + k]
            y = samples[dev_first[dev]]["y"]
            hits += int(fam[k] == int(y.get("family", 0)))
    n = len(dev_order)
    model.train()
    return hits / n * 100.0, n


def run_variant(variant, args):
    torch.manual_seed(args.seed)
    np.random.seed(args.seed)

    with open(DATASET, encoding="utf-8") as f:
        data = json.load(f)
    samples = data["samples"] if isinstance(data, dict) else data
    train_idx, hold_idx, hold_devs, n_devs = V.split_by_device(samples, 0.1, args.seed)

    if variant == "cont":
        backbone, vocab, fam_fp32, label = "continuous", 64, True, "CONTINUOUS same-arch (vocab 64)"
    elif variant == "head":
        backbone, vocab, fam_fp32, label = "ternary", 64, True, "TERNARY backbone + CONTINUOUS family head (vocab 64)"
    else:  # vocab256
        backbone, vocab, fam_fp32, label = "continuous", 256, True, "CONTINUOUS same-arch (vocab 256)"

    # tokens honor the variant's vocab (vocab-64 samples["x"] vs pack_vid_did(v, 256))
    X, Yf = build_tokens(samples, train_idx, vocab)
    # non-family labels (fw/agent/caps/next) — SAME multi-task loss as the retrain
    _, _, Yff, Yfa, Yfc, Yfn = build_tensors(samples, train_idx)
    model = ProbeModel(backbone=backbone, fam_head_fp32=fam_fp32, vocab=vocab,
                       hidden=args.hidden, num_layers=args.layers,
                       num_heads=args.heads, ff_dim=args.ff_dim).to(DEVICE)
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-5)
    crit_ce_fam = nn.CrossEntropyLoss()          # plain CE — same as retrain default
    crit_ce = nn.CrossEntropyLoss()
    crit_bce = nn.BCEWithLogitsLoss()

    n = len(X)
    best_acc = -1.0
    best_state = None
    best_epoch = -1
    stall = 0
    rows = []
    epochs_done = 0
    t_start = time.time()
    for epoch in range(args.epochs):
        t0 = time.time()
        model.train()
        perm = torch.randperm(n)
        Xp, Yfp, Yfwp, Yap, Ycp, Ynp = [t[perm] for t in (X, Yf, Yff, Yfa, Yfc, Yfn)]
        total, nb = 0.0, 0
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
        acc, n_dev = eval_holdout_device(model, samples, hold_idx, vocab, args.n_family)
        secs = time.time() - t0
        epochs_done = epoch + 1
        line = f"epoch {epoch:3d}  loss={total / max(nb, 1):.4f}  {secs:6.1f}s  dev family (in-mem)={acc:6.2f}%"
        print("  " + line)
        rows.append((epoch, total / max(nb, 1), secs, acc))
        if acc > best_acc + 1e-9:
            best_acc = acc
            best_state = {k: v.detach().clone() for k, v in model.state_dict().items()}
            best_epoch = epoch
            stall = 0
        else:
            stall += 1
        if stall >= args.patience:
            print(f"  [early stop] no dev family improvement for {args.patience} epochs "
                  f"(best ep {best_epoch}, dev family={best_acc:.2f}%)")
            break
    model.load_state_dict(best_state)
    t_elapsed = time.time() - t_start
    return dict(variant=variant, label=label, backbone=backbone, vocab=vocab,
                best_epoch=best_epoch, best_acc=best_acc, epochs_done=epochs_done,
                rows=rows, secs=t_elapsed, n_dev_holdout=n_dev,
                n_train=len(train_idx))


def write_section(res):
    TARGET.mkdir(parents=True, exist_ok=True)
    md = REPORT.read_text(encoding="utf-8") if REPORT.exists() else ""
    lines = []
    w = lines.append
    w(f"## Variant: {res['variant']} — {res['label']}")
    w("")
    w(f"- backbone: **{res['backbone']}** | vocab: **{res['vocab']}** | "
      f"family head: **{'continuous fp32' if res['variant'] == 'head' or res['backbone'] == 'continuous' else 'ternary'}**")
    w(f"- config: hidden=128 layers=6 heads=4 (q_dim=32) ff=256 batch=4096 lr=3e-4 "
      f"AdamW wd=1e-5 clip=1.0 plain-CE family, multi-task loss (same as retrain)")
    w(f"- data: dataset_class_v2.json, 90/10 by unique device seed 42 "
      f"(train {res['n_train']}, holdout {res['n_dev_holdout']} devices); eval = in-memory device-level family acc")
    w(f"- ran {res['epochs_done']} epochs, early-stop patience 6; best epoch {res['best_epoch']}")
    w("")
    w("| epoch | loss | secs | dev family (in-mem) |")
    w("|-------|------|------|---------------------|")
    for ep, loss, secs, acc in res["rows"]:
        w(f"| {ep} | {loss:.4f} | {secs:.1f} | {acc:.2f}% |")
    w("")
    w(f"**BEST holdout dev family: {res['best_acc']:.2f}%** (vs majority 60.58%, "
      f"ternary retrain plateau 60.67%, one-hot linear 63.3%, continuous MLP 71.2%)")
    w("")
    md += "\n".join(lines) + "\n"
    REPORT.write_text(md, encoding="utf-8")
    print(f"  section appended -> {REPORT}")


VERDICT_TEMPLATE = """
## Verdict

| Variant | holdout dev family |
|---------|--------------------|
| {cont_label} | {cont:.2f}% |
| {head_label} | {head:.2f}% |
| {vocab256_label} | {vocab256:.2f}% |

References: majority 'other' = 60.58% · ternary retrain (t=0.05) = 60.67% ·
one-hot linear = 63.3% · continuous MLP (different arch) = 71.2%.

Verdict: **{verdict}** — {reason}

{extra}
"""


def main():
    ap = argparse.ArgumentParser(description="Continuous same-arch control probe (diagnostics only)")
    ap.add_argument("--variant", choices=["cont", "head", "vocab256"], default=None)
    ap.add_argument("--all", action="store_true", help="run the three variants")
    ap.add_argument("--epochs", type=int, default=20)
    ap.add_argument("--patience", type=int, default=6)
    ap.add_argument("--hidden", type=int, default=128)
    ap.add_argument("--layers", type=int, default=6)
    ap.add_argument("--heads", type=int, default=4)
    ap.add_argument("--ff-dim", type=int, default=256)
    ap.add_argument("--batch", type=int, default=4096)
    ap.add_argument("--lr", type=float, default=3e-4)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--n-family", type=int, default=N_FAMILY_FILE)
    args = ap.parse_args()

    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:
        pass

    variants = ["cont", "head", "vocab256"] if args.all else [args.variant]
    if variants == [None]:
        ap.error("need --variant X or --all")
    if args.all:
        args.variant = None

    t_all = time.time()
    results = {}
    for v in variants:
        print("=" * 64)
        print(f"  VARIANT: {v}")
        print("=" * 64)
        res = run_variant(v, args)
        results[v] = res
        write_section(res)

    if args.all:
        cont = results["cont"]["best_acc"]
        head = results["head"]["best_acc"]
        v256 = results["vocab256"]["best_acc"]
        majority = MAJORITY_ACC
        if cont >= 65.0:
            verdict = "QUANTIZATION is the killer"
            reason = (f"continuous same-arch ({cont:.2f}%) clears the ≥65% bar by "
                      f"{cont - majority:.2f} pts over majority — the fp32 version of the "
                      "SAME transformer learns the taxonomy; the ternary STE forward is what "
                      "caps it at ~60.6%. Fix path: ADR-0084 QAT (soft-expectation STE, "
                      "tanh logit scale 30*tanh(x/30), per-param LR).")
        elif cont <= 62.0:
            verdict = "ARCHITECTURE is the killer"
            reason = (f"continuous same-arch ({cont:.2f}%) ≈ ternary plateau (60.67%) and "
                      "majority (60.58%) — removing quantization changes nothing, so the "
                      "truncated-attention q_dim=32 + mean-pool transformer cannot separate "
                      "these 12 classes. Ship table+MLP instead (continuous MLP hit 71.2%).")
        else:
            verdict = "AMBIGUOUS (62–65%)"
            reason = (f"continuous same-arch {cont:.2f}% is above plateau but below the 65% "
                      "bar — quantization is a contributor but not clearly the only one. "
                      "Recommended: run ADR-0084 QAT on this continuous checkpoint and see if "
                      "QAT training recovers ≥65% post-quantization.")
        head_note = (f"continuous family head on ternary backbone = {head:.2f}% "
                     f"({'head is NOT the main blocker' if head >= cont - 2.0 else 'head is a blocker'})")
        if head >= cont - 2.0:
            head_note += " — the coarse {-1,0,1} family head is not what pins family acc (the ternary backbone dominates)"
        else:
            head_note += " — the {-1,0,1} family head loses ground vs fp32, but only a few pts"
        v256_note = (f"vocab-256 tokenizer on continuous same-arch = {v256:.2f}% "
                     f"({'gains ' + f'{v256 - cont:.2f} pts over vocab 64' if v256 > cont else 'no gain over vocab 64'}) — "
                     f"the 2 discarded bits per byte {'matter' if v256 - cont >= 1.5 else 'do not move the needle'}")

        md = REPORT.read_text(encoding="utf-8")
        md += VERDICT_TEMPLATE.format(
            cont_label=results["cont"]["label"], cont=cont,
            head_label=results["head"]["label"], head=head,
            vocab256_label=results["vocab256"]["label"], vocab256=v256,
            verdict=verdict, reason=reason,
            extra=head_note + "\n\n" + v256_note,
        )
        REPORT.write_text(md, encoding="utf-8")

        print("\n" + "=" * 64)
        print("  HEADLINE — continuous same-arch control")
        print(f"    continuous same-arch (vocab 64) : {cont:.2f}%")
        print(f"    ternary backbone + cont. head    : {head:.2f}%")
        print(f"    continuous same-arch (vocab 256): {v256:.2f}%")
        print(f"    reference majority              : {majority:.2f}%")
        print(f"    reference ternary retrain       : 60.67%")
        print(f"    reference one-hot linear        : 63.30%")
        print(f"    reference continuous MLP        : 71.20%")
        print(f"    VERDICT                         : {verdict}")
        print(f"    report                          : {REPORT}")
        print(f"    total time                      : {time.time() - t_all:.1f}s")
        print("=" * 64)


if __name__ == "__main__":
    main()
