#!/usr/bin/env python3
"""parity_ops.py — Localiza o op divergente no layer 0: projecoes, rope,
atencao e MLP do meu forward vs hooks na referencia (mesmos ids).

Uso:
  $env:TORCHDYNAMO_DISABLE="1"; python tools/parity_ops.py
"""
from __future__ import annotations
import sys
from pathlib import Path

import torch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from chat_v6 import V6  # noqa: E402


def rel(a, b):
    af, bf = a.float().flatten(), b.float().flatten()
    d = (af - bf).abs()
    r = d / (bf.abs() + 1e-6)
    cos = float((af @ bf) / (af.norm() * bf.norm() + 1e-12))
    return float(r.mean()), float(r.max()), cos, float(af.pow(2).mean().sqrt()), float(bf.pow(2).mean().sqrt())


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    m = V6(ROOT / "target1" / "FALCON3_1B.V6")

    from transformers import AutoModelForCausalLM, AutoTokenizer
    tok = AutoTokenizer.from_pretrained(str(ROOT / "target" / "falcon3-1b"),
                                        trust_remote_code=False)
    res = tok.apply_chat_template([{"role": "user", "content": "Qual e o seu nome?"}],
                                  add_generation_prompt=True)
    if hasattr(res, "__getitem__") and "input_ids" in res:
        res = res["input_ids"]
    ids = [int(i) for i in res if int(i) < m.vocab]
    print(f"[ids] {len(ids)} toks")

    ref = AutoModelForCausalLM.from_pretrained(
        str(ROOT / "target" / "falcon3-1b"), trust_remote_code=False)
    ref.eval()
    L0 = ref.model.layers[0]
    cap = {}

    def hook(name):
        def fn(mod, inp, out):
            o = out[0] if isinstance(out, tuple) else out
            cap[name] = o.detach().float()
        return fn

    L0.input_layernorm.register_forward_hook(hook("norm_attn"))
    L0.self_attn.q_proj.register_forward_hook(hook("q"))
    L0.self_attn.k_proj.register_forward_hook(hook("k"))
    L0.self_attn.v_proj.register_forward_hook(hook("v"))
    L0.self_attn.o_proj.register_forward_hook(hook("o"))
    L0.post_attention_layernorm.register_forward_hook(hook("norm_ffn"))
    L0.mlp.gate_proj.register_forward_hook(hook("gate"))
    L0.mlp.up_proj.register_forward_hook(hook("up"))
    L0.mlp.down_proj.register_forward_hook(hook("down"))
    tin = torch.tensor([ids])
    with torch.no_grad():
        ref.model(tin, use_cache=False)
    print("[ref] capturas:", sorted(cap.keys()),
          "q rms=", float(cap["q"].pow(2).mean().sqrt()))

    # meu lado, mesmos ids
    rq, rf, q, k, v, pr, g, u, d = m.blocks[0]
    H, HD, HQ, KV = m.hidden, m.head_dim, m.heads, m.kv_heads
    h = m.embed[:, ids].T.unsqueeze(0) * m.embed_scale
    S = h.shape[-2]
    x = m.rmsnorm(h, rq)
    print("norm_attn:", ["%.4f" % v for v in rel(x, cap["norm_attn"])])
    qv = (x @ q[0].float()) * q[1]
    kv = (x @ k[0].float()) * k[1]
    vv = (x @ v[0].float()) * v[1]
    print("q_proj:  ", ["%.4f" % v for v in rel(qv, cap["q"])])
    print("k_proj:  ", ["%.4f" % v for v in rel(kv, cap["k"])])
    print("v_proj:  ", ["%.4f" % v for v in rel(vv, cap["v"])])
    qv = qv.view(-1, S, HQ, HD)
    kv = kv.view(-1, S, KV, HD)
    vv = vv.view(-1, S, KV, HD)
    c, s = m.rope_cos[:S], m.rope_sin[:S]

    def rot(t):
        a, b = t[..., 0::2], t[..., 1::2]
        return torch.stack([a * c[:, None, :] - b * s[:, None, :],
                            a * s[:, None, :] + b * c[:, None, :]], -1).flatten(-2)

    qv, kv = rot(qv.squeeze(0)).unsqueeze(0), rot(kv.squeeze(0)).unsqueeze(0)
    rep = HQ // KV
    qh = qv.transpose(1, 2)
    kh = kv.repeat_interleave(rep, dim=2).transpose(1, 2)
    vh = vv.repeat_interleave(rep, dim=2).transpose(1, 2)
    att = (qh @ kh.transpose(-1, -2)) / (HD ** 0.5)
    T = S
    qi = torch.arange(S)[:, None]
    kj = torch.arange(T)[None, :]
    att = att.masked_fill((qi < kj).unsqueeze(0).unsqueeze(0), float("-inf"))
    att = torch.softmax(att, -1)
    o = (att @ vh).transpose(1, 2).reshape(-1, S, HQ * HD)
    o = (o @ pr[0].float()) * pr[1]
    print("o_proj:  ", ["%.4f" % v for v in rel(o, cap["o"])])
    h2 = h + o
    y = m.rmsnorm(h2, rf)
    print("norm_ffn:", ["%.4f" % v for v in rel(y, cap["norm_ffn"])])
    import torch.nn.functional as F
    gate = (y @ g[0].float()) * g[1]
    up = (y @ u[0].float()) * u[1]
    print("gate:    ", ["%.4f" % v for v in rel(gate, cap["gate"])])
    print("up:      ", ["%.4f" % v for v in rel(up, cap["up"])])
    down = (F.silu(gate) * up) @ d[0].float() * d[1]
    print("down:    ", ["%.4f" % v for v in rel(down, cap["down"])])

    # ── pesos do ARQUIVO vs referencia (elementwise) + tabela de scales ──
    print("\n== pesos ==")
    from safetensors import safe_open
    from transformers.integrations.bitnet import unpack_weights as ref_unpack
    names = ["q", "k", "v", "o", "gate", "up", "down"]
    hn = {n: f"model.layers.0.self_attn.{n}_proj.weight" if n in "qkvo"
          else f"model.layers.0.mlp.{n}_proj.weight" for n in names}
    with safe_open(str(ROOT / "target" / "falcon3-1b" / "model.safetensors"),
                   framework="pt") as f:
        for ni, nm in enumerate(names):
            w, s = m.blocks[0][2 + ni]
            raw = f.get_tensor(hn[nm])
            rr = ref_unpack(raw, torch.int8).numpy()
            mine = w.numpy()
            cands = [("direto", rr)]
            if rr.shape == mine.shape[::-1]:
                cands.append(("T", rr.T))
            best, ndiff = "?", mine.size + 1
            for cname, c in cands:
                if c.shape == mine.shape:
                    dd = int((mine != c).sum())
                    if dd < ndiff:
                        ndiff, best = dd, cname
            tag = "MATCH" if ndiff == 0 else f"DIFF {ndiff}/{mine.size} via-{best}"
            print(f"W-{nm}: scale_arquivo={s:g} {tag}")


if __name__ == "__main__":
    main()
