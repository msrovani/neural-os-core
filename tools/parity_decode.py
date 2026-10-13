#!/usr/bin/env python3
"""parity_decode.py — Paridade de LOGITS com teacher forcing: a referencia dita
os tokens, comparo meus logits a cada passo (cos + top1). Isola a matematica
do decode da divergencia de trajetoria greedy.

Uso:
  $env:TORCHDYNAMO_DISABLE="1"; python tools/parity_decode.py [--steps 12]
"""
from __future__ import annotations
import argparse
import sys
from pathlib import Path

import torch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from chat_v6 import V6  # noqa: E402

PROMPT = "Qual e o seu nome?"


def cos(a, b):
    af, bf = a.float().flatten(), b.float().flatten()
    return float((af @ bf) / (af.norm() * bf.norm() + 1e-12))


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("--steps", type=int, default=12)
    ap.add_argument("--model", default="target1/FALCON3_1B.V6")
    args = ap.parse_args()

    m = V6(ROOT / args.model)

    from transformers import AutoModelForCausalLM, AutoTokenizer
    tok = AutoTokenizer.from_pretrained(str(ROOT / "target" / "falcon3-1b"),
                                        trust_remote_code=False)
    res = tok.apply_chat_template([{"role": "user", "content": PROMPT}],
                                  add_generation_prompt=True)
    if hasattr(res, "__getitem__") and "input_ids" in res:
        res = res["input_ids"]
    ids = [int(i) for i in res if int(i) < m.vocab]
    print(f"[ids] {len(ids)} toks")

    ref = AutoModelForCausalLM.from_pretrained(
        str(ROOT / "target" / "falcon3-1b"), trust_remote_code=False)
    ref.eval()

    # passo 0: logits prefill dos dois
    U = m.unembed.float()
    kvc = [[torch.empty(1, 0, m.kv_heads, m.head_dim),
            torch.empty(1, 0, m.kv_heads, m.head_dim)] for _ in range(m.layers)]
    h = m.embed[:, ids].T.unsqueeze(0) * m.embed_scale
    for li, blk in enumerate(m.blocks):
        h = m.forward_layer(h, blk, kvc[li], 0)
    logits_my = ((m.rmsnorm(h[:, -1:, :], m.rms_final) @ U) * m.unembed_scale)[0, 0]
    tin = torch.tensor([ids])
    with torch.no_grad():
        logits_ref = ref(tin).logits[0, -1].float()
    t1my, t1rf = int(logits_my.argmax()), int(logits_ref.argmax())
    print(f"step0: cos={cos(logits_my, logits_ref):.4f} top1_my={t1my} top1_ref={t1rf} "
          f"{'IGUAL' if t1my == t1rf else 'DIFERE'}")

    # teacher = greedy da referencia; forca nos dois
    with torch.no_grad():
        gen = ref.generate(tin, max_new_tokens=args.steps, do_sample=False,
                           eos_token_id=tok.eos_token_id)
    teacher = [int(t) for t in gen[0][len(ids):]]
    print(f"teacher ({len(teacher)} toks): {tok.decode(teacher)!r}")

    past = None
    cur = tin
    agree = 0
    for s, tt in enumerate(teacher):
        with torch.no_grad():
            out = ref(cur, past_key_values=past, use_cache=True)
        past = out.past_key_values
        lr = out.logits[0, -1].float()
        # meu passo (cache proprio ja avancou no prefill)
        hh = m.embed[:, [tt]].T.unsqueeze(0) * m.embed_scale if s > 0 else None
        if s == 0:
            lm = logits_my
        else:
            for li, blk in enumerate(m.blocks):
                hh = m.forward_layer(hh, blk, kvc[li], len(ids) + s)
            lm = ((m.rmsnorm(hh[:, -1:, :], m.rms_final) @ U) * m.unembed_scale)[0, 0]
        a, b = int(lm.argmax()), int(lr.argmax())
        ok = a == b
        agree += ok
        print(f"step{s}: cos={cos(lm, lr):.4f} my={a} ref={b} {'=' if ok else 'X'}")
        cur = torch.tensor([[tt]])
    print(f"top1 acordo: {agree}/{len(teacher)} (step0 incluso: {int(t1my == t1rf) + agree}/{len(teacher) + 1})")


if __name__ == "__main__":
    main()
