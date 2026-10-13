#!/usr/bin/env python3
"""ref_transformers.py — Arbitro de referencia p/ o pipeline .v6 (host).

T1 (instantaneo): meu unpack numpy vs transformers.unpack_weights nos mesmos
    bytes (paridade da implementacao).
T2 (minutos): generate de referencia no fonte tiiuae/Falcon3-1B-Instruct-1.58bit
    com o chat template TII (greedy). Se coerente, o fonte e bom e o bug esta
    no meu pipeline; se salada, o checkpoint pede template/amostragem exata.

Uso:
  python tools/ref_transformers.py [--t2] [--prompt "Qual e o seu nome?"]
"""
from __future__ import annotations
import argparse
import sys
from pathlib import Path

import numpy as np
import torch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from convert_falcon3_to_v6 import unpack_hf_oi  # noqa: E402

SRC = ROOT / "target" / "falcon3-1b"


def t1():
    # Oraculo: exemplo worked da docstring de unpack_weights (HF-autorado),
    # sem chamar a funcao (ela carrega @torch.compile -> exige cl.exe).
    packed = np.array([[0b10100001, 0b00011000],
                       [0b10010000, 0b00001010]], dtype=np.uint8)
    want = np.array([[0, -1], [-1, 1], [-1, 1], [-1, 1],
                     [1, 0], [0, -1], [1, -1], [1, -1]], dtype=np.int8)
    got = unpack_hf_oi(packed)
    print(f"T1 shapes: got={got.shape} want={want.shape}")
    print(f"T1 elementwise igual ao exemplo HF: {(got == want).all()}")
    if not (got == want).all():
        print(f"T1 got:\n{got}\nwant:\n{want}")
    # distribuicao dos pesos reais layer0 q (sanidade)
    from safetensors import safe_open
    with safe_open(str(SRC / "model.safetensors"), framework="pt") as f:
        raw = f.get_tensor("model.layers.0.self_attn.q_proj.weight").numpy()
    w = unpack_hf_oi(raw)
    uq, ct = np.unique(w, return_counts=True)
    print(f"T1 q0 distrib: {dict(zip(uq.tolist(), (ct / ct.sum()).round(3).tolist()))}")


def t2(prompt):
    from transformers import AutoModelForCausalLM, AutoTokenizer
    tok = AutoTokenizer.from_pretrained(str(SRC), trust_remote_code=False)
    print(f"[ref] template TII: {getattr(tok, 'chat_template', None)!r}"[:400])
    model = AutoModelForCausalLM.from_pretrained(
        str(SRC), trust_remote_code=False)
    model.eval()
    msgs = [{"role": "user", "content": prompt}]
    res = tok.apply_chat_template(msgs, add_generation_prompt=True)
    if hasattr(res, "ids"):
        res = res.ids
    elif hasattr(res, "__getitem__") and "input_ids" in res:
        res = res["input_ids"]
    ids = torch.tensor([res])
    print(f"[ref] prompt toks={ids.shape[1]}")
    import time
    t0 = time.time()
    with torch.no_grad():
        out = model.generate(ids, max_new_tokens=40, do_sample=False,
                             eos_token_id=tok.eos_token_id)
    dt = time.time() - t0
    gen = out[0][ids.shape[1]:]
    print(f"[ref] OUTPUT: {tok.decode(gen)!r}")
    print(f"[ref] {len(gen)} toks {len(gen) / dt:.2f} tok/s")


def main():
    global SRC
    ap = argparse.ArgumentParser()
    ap.add_argument("--t2", action="store_true")
    ap.add_argument("--prompt", default="Qual e o seu nome?")
    ap.add_argument("--src", default="target/falcon3-1b")
    args = ap.parse_args()
    SRC = ROOT / args.src
    t1()
    if args.t2:
        t2(args.prompt)


if __name__ == "__main__":
    main()
