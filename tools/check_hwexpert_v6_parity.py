#!/usr/bin/env python3
"""Verifica parity byte-exata entre o v5 legado e o v6 convertido do HW Expert.

Decode do v6 (body sem prefixos, shapes colapsados q/k/v/o=(h,h), g/u=(h,ff),
d=(ff,h)) e compara tensor a tensor com o decode v5 (port Rust-exact). Também
compara predições nos 10 devices canônicos.
"""
import struct
import sys

sys.path.insert(0, "tools")
import validate_hw_expert_v4 as V

V5 = r"models\hw_expert\hw_expert_v4.bitnet"
V6 = r"tools\target\hw_expert_v6.bitnet"


def load_v6(data):
    h, nl, vocab, ff = 128, 6, 64, 256
    off = 52  # header v6 (mt=1, tok_len=0): magic4+ver2+np8+mt1+res3+bloco24+tok... act/emb/feat
    # sanity: ver header
    assert struct.unpack("<I", data[0:4])[0] == 0xBE11BE11
    assert struct.unpack("<H", data[4:6])[0] == 6
    assert data[14] == 1  # model_type hwexpert

    def rf32():
        nonlocal off
        v = struct.unpack("<f", data[off:off + 4])[0]
        off += 4
        return v

    def rtern(rows, cols):
        nonlocal off
        n = (rows * cols + 3) // 4
        raw = data[off:off + n]
        off += n
        rf32()  # scale (1.0 no v6)
        w = []
        for i in range(rows * cols):
            b = raw[i // 4]
            bits = (b >> ((i % 4) * 2)) & 0b11
            w.append(1.0 if bits == 0b01 else (-1.0 if bits == 0b10 else 0.0))
        return w

    def rfvec(n):
        nonlocal off
        v = list(struct.unpack(f"<{n}f", data[off:off + n * 4]))
        off += n * 4
        return v

    emb = rtern(h, vocab)
    layers = []
    for _ in range(nl):
        ra = rfvec(h)
        rf = rfvec(h)
        ri = rfvec(h)   # feat bit0
        rfn = rfvec(ff)  # feat bit1
        t = {k: rtern(h, h) for k in ("q", "k", "v", "o")}
        t.update({k: rtern(h, ff) for k in ("gate", "up")})
        t["down"] = rtern(ff, h)
        layers.append(dict(rms_attn=ra, rms_ffn=rf, rms_inner=ri, rms_ffn_norm=rfn,
                           q_dim=32, intermediate=ff, **t))
    rms_final = rfvec(h)
    heads = [rtern(h, 17), rtern(h, 8), rtern(h, 9), rtern(h, 10), rtern(h, 9)]
    return off, dict(hidden=h, num_layers=nl, embed=emb, layers=layers,
                     rms_final=rms_final, family_head=heads[0], fw_head=heads[1],
                     agent_head=heads[2], caps_head=heads[3], next_head=heads[4],
                     vocab=vocab, q_dim=32, intermediate=ff)


def main():
    v5 = open(V5, "rb").read()
    m5, end5 = V.load_v5(v5)
    assert m5 is not None and end5 == len(v5), "v5 parse falhou"
    v6 = open(V6, "rb").read()
    end6, m6 = load_v6(v6)
    print(f"v6 parse_end={end6} size={len(v6)} OK={end6 == len(v6)}")

    assert m6["embed"] == m5["embed"], "EMBED DIFF"
    for i, (l6, l5) in enumerate(zip(m6["layers"], m5["layers"])):
        for k in ("rms_attn", "rms_ffn", "rms_inner", "rms_ffn_norm",
                  "q", "k", "v", "o", "gate", "up", "down"):
            assert l6[k] == l5[k], f"layer {i} {k} DIFF"
    assert m6["rms_final"] == m5["rms_final"], "RMS_FINAL DIFF"
    for name in ("family_head", "fw_head", "agent_head", "caps_head", "next_head"):
        assert m6[name] == m5[name], f"head {name} DIFF"
    print("PARITY tensores v5==v6: PASS (embed, 6 layers x 11, rms_final, 5 heads)")

    ok = True
    for vid, did, name in V.TEST_DEVICES:
        p5 = V.predict(m5, vid, did)
        p6 = V.predict(m6, vid, did)
        same = p5 == p6
        ok &= same
        print(f"  {name}: v5={p5} v6={p6} {'OK' if same else 'DIFF'}")
    print("PREDICTION PARITY:", "PASS" if ok else "FAIL")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
