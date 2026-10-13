#!/usr/bin/env python3
"""probe_dense_unembed.py — Meu forward + lm_head DENSO do fonte: o unembed
ternario-absmean e o gap? Gera com os dois e compara com a referencia T2
('O nome do personagem de "Harry Potter" ...').

Uso:
  python tools/probe_dense_unembed.py --prompt "Qual e o seu nome?" [--max-tokens 40]
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
        res = res.ids if hasattr(res, "ids") else res["input_ids"]
    elif hasattr(res, "ids"):
        res = res.ids
    return [int(i) for i in res]


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("--prompt", default="Qual e o seu nome?")
    ap.add_argument("--max-tokens", type=int, default=40)
    ap.add_argument("--model", default="target1/FALCON3_1B.V6")
    args = ap.parse_args()

    m = V6(ROOT / args.model)
    bpe = BPE(ROOT / "target" / "tokenizer_falcon3_1b.json")
    ids = [i for i in tii_ids(args.prompt) if i < m.vocab]
    print(f"[ids] {len(ids)} toks TII")

    from safetensors import safe_open
    with safe_open(str(ROOT / "target" / "falcon3-1b" / "model.safetensors"),
                   framework="pt") as f:
        dense = f.get_tensor("lm_head.weight").float()  # (vocab, hidden) BF16->f32
    # lm_head HF e (vocab, hidden); meu U e (hidden, vocab)
    dense_U = dense.T.contiguous()
    print(f"[dense] U={tuple(dense_U.shape)} rms={float(dense_U.pow(2).mean().sqrt()):.4f} "
          f"(ternario: zero%=27.77)")
    # via do meio: Q6K (encode->decode, como o embed do .v6)
    import sys as _sys
    _sys.path.insert(0, str(ROOT / "tools"))
    from bitnet_writer import _encode_q6k
    from chat_v6 import decode_q6k_fast
    import numpy as _np
    q6k_bytes = _encode_q6k(dense_U.numpy())
    q6k_U = torch.from_numpy(
        decode_q6k_fast(q6k_bytes, *dense_U.shape)).to(torch.float32)
    dq = (dense_U - q6k_U).abs()
    print(f"[q6k] bytes={len(q6k_bytes) // 1024 // 1024}MB "
          f"err medio={float(dq.mean()):.5f} (ternario err seria ~0.02)")

    for tag, U, s in [("TERNARIO-ARQUIVO", m.unembed.float(), m.unembed_scale),
                      ("DENSO-FONTE", dense_U, 1.0),
                      ("Q6K", q6k_U, 1.0)]:
        out, tps = m.generate(ids, args.max_tokens, bpe.eos,
                              stream=None, _unembed_override=(U, s))
        print(f"\n===== unembed={tag} =====")
        print(f"texto: {bpe.decode(out)!r}")
        print(f"[{tag}] {len(out)} toks {tps:.2f} tok/s eos={out and out[-1] == bpe.eos}")


if __name__ == "__main__":
    main()
