#!/usr/bin/env python3
"""Testa se q_dim=128 (header v6) vs q_dim=32 (v5) muda predições hwexpert."""
import struct
import sys

sys.path.insert(0, "tools")
import validate_hw_expert_v4 as V

d = open(r"tools\target\hw_expert_v6.bitnet", "rb").read()
h, nl, vocab, ff = 128, 6, 64, 256
off = 52


def rf32():
    global off
    v = struct.unpack("<f", d[off:off + 4])[0]
    off += 4
    return v


def rtern(rows, cols):
    global off
    n = (rows * cols + 3) // 4
    raw = d[off:off + n]
    off += n
    rf32()
    w = []
    for i in range(rows * cols):
        b = raw[i // 4]
        bits = (b >> ((i % 4) * 2)) & 0b11
        w.append(1.0 if bits == 0b01 else (-1.0 if bits == 0b10 else 0.0))
    return w


def rfvec(n):
    global off
    v = list(struct.unpack(f"<{n}f", d[off:off + n * 4]))
    off += n * 4
    return v


emb = rtern(h, vocab)
layers = []
for _ in range(nl):
    ra = rfvec(h)
    rf = rfvec(h)
    ri = rfvec(h)
    rfn = rfvec(ff)
    t = {k: rtern(h, h) for k in ("q", "k", "v", "o")}
    t.update({k: rtern(h, ff) for k in ("gate", "up")})
    t["down"] = rtern(ff, h)
    layers.append(dict(rms_attn=ra, rms_ffn=rf, rms_inner=ri, rms_ffn_norm=rfn,
                       q_dim=128, intermediate=ff, **t))
rms_final = rfvec(h)
heads = [rtern(h, 17), rtern(h, 8), rtern(h, 9), rtern(h, 10), rtern(h, 9)]


def mk(qd):
    ls = []
    for l in layers:
        ll = dict(l)
        ll["q_dim"] = qd
        ls.append(ll)
    return dict(hidden=h, num_layers=nl, embed=emb, layers=ls, rms_final=rms_final,
                family_head=heads[0], fw_head=heads[1], agent_head=heads[2],
                caps_head=heads[3], next_head=heads[4], vocab=vocab,
                q_dim=qd, intermediate=ff)


m32 = mk(32)
m128 = mk(128)
for vid, did, name in V.TEST_DEVICES:
    p5 = V.predict(m32, vid, did)
    p128 = V.predict(m128, vid, did)
    tag = "SAME" if p5 == p128 else "***DIFF***"
    print(f"{name}: qd=32 {p5} | qd=128 {p128} {tag}")
