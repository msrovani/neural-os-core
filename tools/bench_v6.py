#!/usr/bin/env python3
"""bench_v6.py — Benchmark lado a lado (host): transformers-fonte (normal)
vs modelos .v6/.v7 (chat_v6). Cada lado no seu habitat (TII-template vs kernel
frame), greedy, mesmos 3 prompts do aceite. Modelos .v6/.v7 carregados em
SEQUENCIA (um por vez, RAM) com BPE proprio (embutido se houver).

Metricas por lado/prompt: TTFT, toks, tempo total, tok/s fim-a-fim, eos,
texto + veredito da suite. Nao e gate (exit 0 sempre).

Uso:
  $env:TORCHDYNAMO_DISABLE="1"; python tools/bench_v6.py --model target1/FALCON3_1B.V6 --models target1/FALCON3_1B.V6 target1/FALCON3_1B.V7
"""
from __future__ import annotations
import argparse
import sys
import time
from pathlib import Path

import torch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from chat_v6 import V6, BPE  # noqa: E402
from test_v6_suite import degenerate, looks_like_json  # noqa: E402

PROMPTS = [
    ("Q1", "Qual e o seu nome?", 48),
    ("Q2", "1+1=", 48),
    ("Q3", 'Retorne apenas JSON valido, sem texto extra: {"resultado": <quanto e 2+2>}', 48),
]


def judge(tag, text, ids, bpe, eos_id=None):
    """Mesma regua da suite, nos dois lados."""
    if tag == "Q1":
        d, _ = degenerate(ids, bpe)
        return (not d) and len(text.strip()) >= 3
    if tag == "Q2":
        d, _ = degenerate(ids, bpe)
        eos_hit = eos_id is not None and bool(ids) and ids[-1] == eos_id
        return ("2" in text) and (eos_hit or not d)
    s = text.strip()
    if s.startswith("```"):
        s = s[3:]
        if s.startswith("json"):
            s = s[4:]
        s = s.strip()
    if s.endswith("```"):
        s = s[:-3].strip()
    d, _ = degenerate(ids, bpe, use_piece=False)
    return looks_like_json(s) and not d and (("4" in s) or ("quatro" in s.lower()))


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("--max-tokens", type=int, default=48)
    ap.add_argument("--model", default="target1/FALCON3_1B.V6")
    ap.add_argument("--models", nargs="*", default=None,
                    help="lista .v6/.v7 (default: [--model])")
    args = ap.parse_args()
    torch.set_num_threads(max(1, __import__("os").cpu_count() or 4))

    import gc
    paths = args.models if args.models else [args.model]

    def vocab_of(rel):
        with open(ROOT / rel, "rb") as f:
            head = f.read(64)
        import struct as _st
        magic, ver = _st.unpack_from("<IH", head)
        assert magic == 0xBE11BE11 and ver in (6, 7), f"{rel} header"
        (vocab,) = _st.unpack_from("<I", head, 24)
        return vocab

    cap = min(vocab_of(p) for p in paths)
    print(f"[bench] vocab cap={cap} modelos={paths}")

    from transformers import AutoModelForCausalLM, AutoTokenizer

    tok = AutoTokenizer.from_pretrained(str(ROOT / "target" / "falcon3-1b"),
                                        trust_remote_code=False)
    ref = AutoModelForCausalLM.from_pretrained(
        str(ROOT / "target" / "falcon3-1b"), trust_remote_code=False)
    ref.eval()

    def tii_ids(prompt):
        res = tok.apply_chat_template([{"role": "user", "content": prompt}],
                                      add_generation_prompt=True)
        if hasattr(res, "__getitem__") and "input_ids" in res:
            res = res["input_ids"]
        return [int(i) for i in res if int(i) < cap]

    def kernel_ids(prompt, m, bpe):
        ids = [bpe.bos] + bpe.encode(f"<|user|>\n{prompt}\n<|assistant|>\n")
        return [i for i in ids if i < m.vocab] or [0]

    def load_bpe_for(m, mp):
        if getattr(m, "tok_type", 0) == 1 and getattr(m, "tok_blob", b""):
            b = BPE.from_bpb1(m.tok_blob)
            print(f"[tok] {Path(mp).name} embutido: vocab={len(b.vocab)}")
            return b
        return BPE(ROOT / "target" / "tokenizer_falcon3_1b.json")

    def run_side_v6(m, bpe, label):
        rec = {}
        for tag, prompt, mt in PROMPTS:
            mt = min(mt, args.max_tokens)
            bids = kernel_ids(prompt, m, bpe)
            first = {}
            t0 = time.time()

            def on_tok(t, _st=first, _t0=[t0]):
                if "t" not in _st:
                    _st["t"] = time.time() - _t0[0]

            out, _ = m.generate(bids, mt, bpe.eos, stream=on_tok)
            dtb = time.time() - t0
            b_text = bpe.decode(out)
            rec[tag] = (bids, out, b_text, first.get("t", dtb), dtb)
            print(f"{label}: {len(out)} toks TTFT={rec[tag][3]:.1f}s total={dtb:.1f}s "
                  f"{len(out) / dtb:.2f} tok/s :: {b_text[:160]!r}")
        return rec

    def mklabel(mp):
        p = Path(mp)
        return p.stem.replace("FALCON3_", "") + p.suffix.upper().replace(".", "")

    arec = {}
    for tag, prompt, mt in PROMPTS:
        mt = min(mt, args.max_tokens)
        aids = tii_ids(prompt)
        tin = torch.tensor([aids])
        t0 = time.time()
        with torch.no_grad():
            _ = ref(tin, use_cache=False)  # prefill puro ~= TTFT
        a_ttft = time.time() - t0
        t0 = time.time()
        with torch.no_grad():
            gout = ref.generate(tin, max_new_tokens=mt,
                                do_sample=False, eos_token_id=tok.eos_token_id,
                                use_cache=True)
        dt = time.time() - t0
        a_ids = [int(t) for t in gout[0][len(aids):]]
        a_text = tok.decode(a_ids)
        arec[tag] = (aids, a_ids, a_text, a_ttft, dt)
        print(f"--- {tag} ---")
        print(f"A-fonte: {len(a_ids)} toks TTFT={a_ttft:.1f}s total={dt:.1f}s "
              f"{len(a_ids) / dt:.2f} tok/s :: {a_text[:160]!r}")

    sides = []
    first_bpe = None
    for mp in paths:
        label = mklabel(mp)
        print(f"[load] {label} {mp} ...")
        m = V6(ROOT / mp)
        bpe = load_bpe_for(m, mp)
        if first_bpe is None:
            first_bpe = bpe
        rec = run_side_v6(m, bpe, label)
        sides.append((label, mp, bpe, rec))
        del m
        gc.collect()

    print("\n== BENCH fonte x " + " x ".join(s[0] for s in sides) + " ==")
    hdr = f"{'q':4} {'A toks':>7} {'A TTFT':>8} {'A tok/s':>8} {'A':3}"
    for label, _, _, _ in sides:
        hdr += f" | {label + ' toks':>7} {label + ' TTFT':>8} {label + ' tok/s':>8} {label[-1:]:>3}"
    print(hdr)
    for tag, _, _ in PROMPTS:
        aids, a_ids, a_text, a_ttft, dt = arec[tag]
        a_ok = judge(tag, a_text, a_ids, first_bpe, tok.eos_token_id)
        row = (f"{tag:4} {len(a_ids):7d} {a_ttft:8.1f} {len(a_ids) / dt:8.2f} "
               f"{'OK' if a_ok else '--':3}")
        for label, mp, bpe, rec in sides:
            bids, out, b_text, b_ttft, dtb = rec[tag]
            b_ok = judge(tag, b_text, out, bpe, bpe.eos)
            row += (f" | {len(out):7d} {b_ttft:8.1f} {len(out) / dtb:8.2f} "
                    f"{'OK' if b_ok else '--':3}")
        print(row)


if __name__ == "__main__":
    main()
