#!/usr/bin/env python3
"""parity_layers.py — Paridade hidden-state por layer: meu forward vs referencia.

Acha a PRIMEIRA camada onde meu .v6-forward diverge do transformers no fonte.
Rel-err pequeno (<0.2) = matematica igual (ruido de quant ede ativacao);
rel-err >> 1 ou NaN = bug.

Uso (dynamo off p/ evitar inductor sem cl.exe):
  $env:TORCHDYNAMO_DISABLE="1"; python tools/parity_layers.py
"""
from __future__ import annotations
import sys
from pathlib import Path

import torch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from chat_v6 import V6  # noqa: E402

PROMPT = "Qual e o seu nome?"


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    m = V6(ROOT / "target1" / "FALCON3_1B.V6")

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
    tin = torch.tensor([ids])
    with torch.no_grad():
        out = ref.model(tin, output_hidden_states=True, use_cache=False)
    rhs = [h.float() for h in out.hidden_states]
    print(f"[ref] {len(rhs)} states, last rms={rhs[-1].pow(2).mean().sqrt():.4f}")

    mine = m.prefill_collect(ids)
    print(f"[my]  {len(mine)} states, last rms={mine[-1].pow(2).mean().sqrt():.4f}")

    print(f"{'layer':>5} {'rel-err medio':>13} {'rel-err max':>11}  veredito")
    for li, (a, b) in enumerate(zip(mine, rhs)):
        d = (a - b).abs()
        rel = d / (b.abs() + 1e-6)
        mu, mx = float(rel.mean()), float(rel.max())
        flag = "OK " if mu < 0.2 else ("DIVERGIU" if mu > 1.0 else "suspeito")
        tag = "embed" if li == 0 else f"L{li - 1:02d}"
        print(f"{tag:>5} {mu:13.4f} {mx:11.4f}  {flag}")
        if not (d.isfinite().all()):
            print("  !! NaN/Inf a partir daqui — parei")
            break


if __name__ == "__main__":
    main()
