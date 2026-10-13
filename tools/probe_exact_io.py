#!/usr/bin/env python3
"""probe_exact_io.py — Mesmo forward .v6, I/O EXATO (embed BF16 + lm_head denso
do fonte, via monkeypatch). Diz se a fidelidade de I/O fecha Q2/Q3 com o
frame do kernel (o da suite de aceite).

Uso:
  python tools/probe_exact_io.py --model target1/FALCON3_1B.V6
"""
from __future__ import annotations
import sys
from pathlib import Path

import torch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from chat_v6 import V6, BPE  # noqa: E402
from test_v6_suite import degenerate, looks_like_json  # noqa: E402


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="target1/FALCON3_1B.V6")
    args = ap.parse_args()

    m = V6(ROOT / args.model)
    bpe = BPE(ROOT / "target" / "tokenizer_falcon3_1b.json")

    from safetensors import safe_open
    with safe_open(str(ROOT / "target" / "falcon3-1b" / "model.safetensors"),
                   framework="pt") as f:
        E = f.get_tensor("model.embed_tokens.weight").float()      # (V,H)
        U = f.get_tensor("lm_head.weight").float().T.contiguous()  # (H,V)
    m.embed = E.T.contiguous()
    m.embed_scale = 1.0
    print(f"[exact-io] embed {tuple(m.embed.shape)} lm_head {tuple(U.shape)}")

    def frame(prompt):
        ids = [bpe.bos] + bpe.encode(f"<|user|>\n{prompt}\n<|assistant|>\n")
        return [i for i in ids if i < m.vocab] or [0]

    cases = [
        ("Q1", "Qual e o seu nome?", 48, lambda t, o: (not degenerate(o, bpe)[0]) and len(t.strip()) >= 3),
        ("Q2", "1+1=", 24, lambda t, o: ("2" in t) and not degenerate(o, bpe)[0]),
        ("Q3", 'Retorne apenas JSON valido, sem texto extra: {"resultado": <quanto e 2+2>}',
         48, lambda t, o: looks_like_json(t.strip().removeprefix("```json").removeprefix("```").removesuffix("```").strip())
         and not degenerate(o, bpe)[0]),
    ]
    ok_all = True
    for tag, prompt, mt, judge in cases:
        out, tps = m.generate(frame(prompt), mt, bpe.eos, stream=None,
                              _unembed_override=(U, 1.0))
        text = bpe.decode(out)
        ok = judge(text, out)
        ok_all &= ok
        print(f"--- {tag} {'COERENTE' if ok else 'DEGENERADO'} ({len(out)} toks {tps:.2f} tok/s) ---")
        print(f"output: {text[:300]!r}")
    print("EXACT-IO VERDE" if ok_all else "EXACT-IO VERMELHO")


if __name__ == "__main__":
    main()
