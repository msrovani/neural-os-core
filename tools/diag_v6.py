#!/usr/bin/env python3
"""diag_v6.py — Diagnostico causa-raiz de geracao degenerada em .v6.

Separa 3 hipoteses:
  H1 (conversao): scales variados => absmean de modelo denso (fonte ruim);
                  scales==1.0 => fonte nativa 1.58bit. zeros/NaN/RMS acusam
                  corrupcao.
  H2 (tokenizer): roundtrip encode->decode do prompt + ids impressos.
  H3 (forward):   top-10 logits + entropia no passo 0. Entropia ~= ln(vocab)
                  = forward sem sinal (pesos nao estao falando).

Uso:
  python tools/diag_v6.py --model target1/FALCON3_1B.V6 [--prompt "Qual e o seu nome?"]
"""
from __future__ import annotations
import argparse
import math
import sys
from pathlib import Path

import torch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from chat_v6 import V6, BPE, ensure_tokenizer  # noqa: E402


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True)
    ap.add_argument("--prompt", default="Qual e o seu nome?")
    ap.add_argument("--tokenizer", default=None)
    args = ap.parse_args()

    m = V6(ROOT / args.model)

    # ── H1: stats dos tensores ──
    print("\n== H1 stats ==")
    scales = []
    names = ["q", "k", "v", "o", "gate", "up", "down"]
    print("layer tensor  scale       zero%   (so suspeitos: scale<=1e-5 ou zero%>99)")
    for li, blk in enumerate(m.blocks):
        rq, rf, *tensors = blk
        if li == 0:
            print(f"rms_attn mean={rq.float().mean():.4f} std={rq.float().std():.4f} "
                  f"rms_ffn mean={rf.float().mean():.4f}")
            assert torch.isfinite(rq.float()).all() and torch.isfinite(rf.float()).all()
        for nm, (w, s) in zip(names, tensors):
            scales.append(s)
            zf = float((w == 0).float().mean())
            if s <= 1e-5 or zf > 0.99:
                print(f"L{li:02d} {nm:4s}  {s:.4g}  {zf * 100:6.2f}%  <-- BURACO")
    import collections
    print(f"scales distintas={len(set(scales))}: {sorted(set(scales))[:8]}")
    all1 = all(s == 1.0 for s in scales)
    n_one = sum(1 for s in scales if s == 1.0)
    n_tiny = sum(1 for s in scales if s <= 1e-5)
    print(f"scales: n_1.0={n_one} n_1e-6={n_tiny} "
          f"(1e-6 com 100% zeros = tensor ausente na conversao)")
    print(f"embed_scale={m.embed_scale:g} unembed_scale={m.unembed_scale:g} "
          f"embed finite={bool(torch.isfinite(m.embed).all())} "
          f"embed mean={m.embed.mean():.4g} std={m.embed.std():.4g}")

    # ── H2: tokenizer ──
    print("\n== H2 tokenizer ==")
    tp = Path(args.tokenizer) if args.tokenizer else ensure_tokenizer(ROOT / args.model)
    bpe = BPE(tp)
    framed = f"<|user|>\n{args.prompt}\n<|assistant|>\n"
    ids = [bpe.bos] + bpe.encode(framed)
    back = bpe.decode(ids[1:])
    print(f"bos={bpe.bos} eos={bpe.eos} n_ids={len(ids)}")
    print(f"ids[:12]={ids[:12]}")
    print(f"roundtrip ok={back.replace(' ', '') == framed.replace(' ', '').replace(chr(10), '')} "
          f"(decode inicia com: {back[:60]!r})")

    # ── H3: logits passo 0 ──
    print("\n== H3 logits passo 0 ==")
    ids = [i for i in ids if i < m.vocab] or [0]

    @torch.no_grad()
    def prefill_logits():
        kvc = [[torch.empty(1, 0, m.kv_heads, m.head_dim),
                torch.empty(1, 0, m.kv_heads, m.head_dim)] for _ in range(m.layers)]
        h = m.embed[:, ids].T.unsqueeze(0) * m.embed_scale
        for li, blk in enumerate(m.blocks):
            h = m.forward_layer(h, blk, kvc[li], 0)
        last = h[:, -1:, :]
        U = m.unembed.float() if m.unembed is not None else m.embed
        return (m.rmsnorm(last, m.rms_final) @ U) * m.unembed_scale

    # ── H4: trace de magnitudes (onde o sinal morre) ──
    print("\n== H4 trace ==")
    for li, blk in enumerate(m.blocks):
        rq, rf = blk[0], blk[1]
        print(f"L{li:02d} rms_attn mean={rq.float().mean():+.4f} rms_ffn mean={rf.float().mean():+.4f}")
    uw = m.unembed
    print(f"unembed zero%={float((uw == 0).float().mean()) * 100:.2f} "
          f"unembed_scale={m.unembed_scale:g} "
          f"|row| medio={uw.float().pow(2).sum(0).sqrt().mean():.4f}")
    blk = m.blocks[0]
    rq, rf, (qw, qs), (kw, ks), (vw, vs), (ow, os_), (gw, gs), (uw, us), (dw, ds) = blk
    for nm, w in [("q", qw), ("k", kw), ("v", vw), ("o", ow),
                  ("gate", gw), ("up", uw), ("down", dw)]:
        wf = w.float()
        print(f"W-{nm}: zero%={float((w == 0).float().mean()) * 100:.1f} "
              f"rms={wf.pow(2).mean().sqrt():.4f}")
    h = m.embed[:, ids].T.unsqueeze(0) * m.embed_scale
    print(f"h-embed rms={h.pow(2).mean().sqrt():.4f}")
    x = m.rmsnorm(h, rq)
    print(f"x|rms_attn rms={x.pow(2).mean().sqrt():.6f}")
    S = x.shape[-2]
    H, HD, HQ, KV = m.hidden, m.head_dim, m.heads, m.kv_heads
    qv = (x @ qw.float()) * qs
    kv = (x @ kw.float()) * ks
    vv = (x @ vw.float()) * vs
    print(f"q rms={qv.pow(2).mean().sqrt():.4f} k rms={kv.pow(2).mean().sqrt():.4f} "
          f"v rms={vv.pow(2).mean().sqrt():.4f}")
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
    print(f"scores: mean={att.mean():.4f} std={att.std():.4f} max={att.max():.4f}")
    T = S
    qi = torch.arange(S)[:, None]
    kj = torch.arange(T)[None, :]
    att = att.masked_fill((qi < kj).unsqueeze(0).unsqueeze(0), float("-inf"))
    att = torch.softmax(att, -1)
    print(f"attw row0: max={att[0, 0, 0].max():.4f} ent={float(-(att[0,0,0]* (att[0,0,0]+1e-12).log()).sum(dim=-1).mean()):.3f}")
    o = (att @ vh).transpose(1, 2).reshape(-1, S, HQ * HD)
    print(f"attn-out rms={o.pow(2).mean().sqrt():.4f}")
    h2 = h + (o @ ow.float()) * os_
    print(f"h+attn rms={h2.pow(2).mean().sqrt():.4f}")
    y = m.rmsnorm(h2, rf)
    gate = (y @ gw.float()) * gs
    up = (y @ uw.float()) * us
    import torch.nn.functional as F
    h3 = h2 + (F.silu(gate) * up) @ dw.float() * ds
    print(f"gate rms={gate.pow(2).mean().sqrt():.4f} up rms={up.pow(2).mean().sqrt():.4f} "
          f"h+mlp rms={h3.pow(2).mean().sqrt():.4f}")

    logits = prefill_logits()[0, 0]
    assert torch.isfinite(logits).all(), "logits com NaN/Inf!"
    print(f"logits std={logits.std():.4f} max={logits.max():.4f}")
    probs = torch.softmax(logits, -1)
    ent = float(-(probs * (probs + 1e-12).log()).sum())
    topv, topi = torch.topk(probs, 10)
    print(f"entropia={ent:.2f} nats (uniforme seria {math.log(m.vocab):.2f})")
    print(f"max prob={topv[0]:.6f}")
    for p, i in zip(topv.tolist(), topi.tolist()):
        print(f"  {p:.6f} id={i} piece={bpe.decode([i])!r}")


if __name__ == "__main__":
    main()
