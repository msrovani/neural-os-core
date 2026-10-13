#!/usr/bin/env python3
"""gen_compare.py — Mesmo modelo .v6, dois framings: template TII (referencia)
vs frame do kernel. Decide se o gap e template.

Uso:
  python tools/gen_compare.py --model target1/FALCON3_1B.V6 --prompt "Qual e o seu nome?" [--max-tokens 40]
"""
from __future__ import annotations
import argparse
import sys
from pathlib import Path

import torch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from chat_v6 import V6, BPE  # noqa: E402


def tii_ids(prompt):
    from transformers import AutoTokenizer
    tok = AutoTokenizer.from_pretrained(str(ROOT / "target" / "falcon3-1b"),
                                        trust_remote_code=False)
    res = tok.apply_chat_template([{"role": "user", "content": prompt}],
                                  add_generation_prompt=True)
    if hasattr(res, "__getitem__") and "input_ids" in res:
        res = res["input_ids"]
    elif isinstance(res, list) and len(res) == 1 and hasattr(res[0], "ids"):
        res = res[0].ids
    elif hasattr(res, "ids"):
        res = res.ids
    print(f"[tii] {len(res)} toks, head={res[:8]}")
    return list(res)


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True)
    ap.add_argument("--prompt", default="Qual e o seu nome?")
    ap.add_argument("--max-tokens", type=int, default=40)
    args = ap.parse_args()

    m = V6(ROOT / args.model)
    bpe = BPE(ROOT / "target" / "tokenizer_falcon3_1b.json")
    print(f"[tok] vocab={len(bpe.vocab)} bos={bpe.bos} eos={bpe.eos}")

    frames = {
        "TII": [i for i in tii_ids(args.prompt) if i < m.vocab],
        "KERNEL": [i for i in ([bpe.bos] + bpe.encode(
            f"<|user|>\n{args.prompt}\n<|assistant|>\n")) if i < m.vocab],
    }
    for name, ids in frames.items():
        print(f"\n===== framing={name} ({len(ids)} toks) =====")
        out, tps = m.generate(ids, args.max_tokens, bpe.eos,
                              stream=lambda t: (sys.stdout.write(bpe.decode([t])),
                                                sys.stdout.flush()))
        print(f"\n[{name}] {len(out)} toks {tps:.2f} tok/s eos={out and out[-1] == bpe.eos}")


if __name__ == "__main__":
    main()
