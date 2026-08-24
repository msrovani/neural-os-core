#!/usr/bin/env python3
"""validate_router_v6.py — validate the EXPORTED ROUTER.BITNET artifact.

Mirrors the Rust loader `crates/cortex/src/trinity.rs::load_router_from_file` (v6,
model_type=2) byte-exact:

Checks:
  1. parse_end == file size; header (magic 0xBE11BE11 ver 6 model_type 2 vocab 99 hidden 64 n_exp 7)
  2. nonzero fraction of weight i8 (GATE >= 1%)
  3. holdout accuracy of the EXPORTED FILE (same stratified 90/10 seed 7 split as train_router.py)
     gate >= 0.80
  4. round-trip prediction consistency (file vs in-memory not needed — file is truth)

Usage:
    python tools/validate_router_v6.py target/ROUTER.BITNET
    python tools/validate_router_v6.py --all   # searches target1/target/tools/target
"""
from __future__ import annotations
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import numpy as np

VOCAB = 99
HIDDEN = 64
N_EXPERTS = 7

def load_router_v6(path: Path):
    d = path.read_bytes()
    if len(d) < 26:
        return None, f"too small {len(d)}"
    magic, = struct.unpack_from("<I", d, 0)
    ver, = struct.unpack_from("<H", d, 4)
    mt, = struct.unpack_from("<B", d, 14)
    if magic != 0xBE11BE11:
        return None, f"magic {magic:#x}"
    if ver != 6:
        return None, f"version {ver}"
    if mt != 2:
        return None, f"model_type {mt} want 2"
    vocab, hidden, n_exp = struct.unpack_from("<IHH", d, 18)
    if vocab != VOCAB or hidden != HIDDEN or n_exp != N_EXPERTS:
        return None, f"dims {vocab}x{hidden}x{n_exp} want {VOCAB}x{HIDDEN}x{N_EXPERTS}"
    pos = 26
    embed_bytes = vocab * hidden * 4
    wbytes = hidden * n_exp
    if pos + embed_bytes + wbytes != len(d):
        return None, f"trailing {len(d)-(pos+embed_bytes+wbytes)} parse_end={pos+embed_bytes+wbytes} size={len(d)}"
    embed = np.frombuffer(d[pos:pos+embed_bytes], dtype="<f4").reshape(VOCAB, HIDDEN).copy()
    pos += embed_bytes
    Wq = np.frombuffer(d[pos:pos+wbytes], dtype=np.int8).reshape(HIDDEN, N_EXPERTS).copy()
    return dict(embed=embed, Wq=Wq, size=len(d), parse_end=pos+wbytes), None

def nonzero_gate(Wq):
    nz = int((Wq != 0).sum())
    tot = Wq.size
    frac = nz / tot if tot else 0
    return frac, nz, tot

def encode(text: str):
    BOS, EOS, CHAR_OFFSET = 0, 1, 3
    MAX_TOKENS = 32
    toks = [BOS]
    for b in text.encode("utf-8"):
        if 32 <= b <= 126:
            toks.append((b - 32) + CHAR_OFFSET)
    toks.append(EOS)
    toks = toks[:MAX_TOKENS]
    counts = np.zeros(VOCAB, dtype=np.float32)
    for t in toks:
        counts[min(t, VOCAB-1)] += 1.0
    return counts

def forward(X, embed, Wq):
    # X: (N, VOCAB), embed: (VOCAB,HIDDEN), Wq: (HIDDEN,N_EXPERTS)
    h = X @ embed
    norms = np.linalg.norm(h, axis=1, keepdims=True) + 1e-8
    h = h / norms
    logits = h @ Wq.astype(np.float32)
    logits -= logits.max(axis=1, keepdims=True)
    ex = np.exp(logits)
    probs = ex / ex.sum(axis=1, keepdims=True)
    return probs

def holdout_accuracy(embed, Wq):
    # Reuse train_router's dataset + stratified split (seed 7) to compute test acc of the FILE.
    from tools.train_router import CURATED, TEMPLATE_SPEC, build_templates, stratified_split
    # exact same split as train() (CURATED only for test/val; templates are train-only)
    test, rest = stratified_split(CURATED, 0.28, seed=7)
    val, train_cur = stratified_split(rest, 0.30, seed=8)
    # test is pure CURATED holdout never seen in templates+train_cur — same 31 samples
    Xte = np.stack([encode(t) for t,_ in test])
    yte = np.array([l for _,l in test], dtype=np.int64)
    probs = forward(Xte, embed, Wq)
    pred = probs.argmax(1)
    acc = float((pred == yte).mean())
    return acc, len(test), int((pred==yte).sum())

def validate(path: Path, quiet=False):
    res, err = load_router_v6(path)
    if res is None:
        print(f"[FAIL] {path}: {err}")
        return None
    embed, Wq = res["embed"], res["Wq"]
    size, parse_end = res["size"], res["parse_end"]
    parse_ok = parse_end == size
    nz_frac, nz, tot = nonzero_gate(Wq)
    nz_gate = nz_frac >= 0.01
    acc, n_test, n_ok = holdout_accuracy(embed, Wq)
    acc_gate = acc >= 0.80

    if not quiet:
        print(f"  file: {path}")
        print(f"  size={size} parse_end={parse_end} parse_ok={parse_ok}")
        print(f"  header: vocab={VOCAB} hidden={HIDDEN} n_exp={N_EXPERTS} model_type=2 ver=6 ok=True")
        print(f"  weight nonzero: {nz}/{tot} {nz_frac*100:.2f}% (GATE >=1%: {nz_gate})")
        print(f"  holdout CURATED 31 (seed 7 stratified): {n_ok}/{n_test} {acc:.3f} (GATE >=0.80: {acc_gate})")

    out = dict(file=str(path), size=size, parse_end=parse_end, parse_ok=parse_ok,
               nz_frac=nz_frac, nz_gate=nz_gate, acc=acc, acc_gate=acc_gate,
               n_test=n_test)
    return out

def find_candidates():
    cands = []
    for p in [ROOT/"target1"/"ROUTER.BITNET", ROOT/"target"/"ROUTER.BITNET", ROOT/"tools"/"target"/"ROUTER.BITNET", ROOT/"ROUTER.BITNET"]:
        if p.exists():
            cands.append(p)
    return cands

def main():
    if "--all" in sys.argv:
        cands = find_candidates()
        if not cands:
            print("[FAIL] no ROUTER.BITNET found in target1/target/tools/target")
            sys.exit(1)
        ok_all = True
        for p in cands:
            out = validate(p)
            if out is None:
                ok_all = False
                continue
            ok = out["parse_ok"] and out["nz_gate"] and out["acc_gate"]
            print(f"  VALIDATION {'PASS' if ok else 'FAIL'} (parse={out['parse_ok']} nz={out['nz_gate']} acc={out['acc_gate']})\n")
            ok_all = ok_all and ok
        sys.exit(0 if ok_all else 2)

    if len(sys.argv) < 2:
        print(__doc__)
        # try auto-find
        cands = find_candidates()
        if cands:
            print(f"auto-found: {cands[0]}")
            out = validate(cands[0])
            if out is None:
                sys.exit(1)
            ok = out["parse_ok"] and out["nz_gate"] and out["acc_gate"]
            print(f"\n  VALIDATION {'PASS' if ok else 'FAIL'}")
            sys.exit(0 if ok else 2)
        sys.exit(1)
    path = Path(sys.argv[1])
    # allow --all mixed
    if path.name == "--all":
        return main()
    out = validate(path)
    if out is None:
        sys.exit(1)
    ok = out["parse_ok"] and out["nz_gate"] and out["acc_gate"]
    print(f"\n  VALIDATION {'PASS' if ok else 'FAIL'} (parse={out['parse_ok']} nz={out['nz_gate']} acc={out['acc_gate']})")
    sys.exit(0 if ok else 2)

if __name__ == "__main__":
    main()
