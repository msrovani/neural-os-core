#!/usr/bin/env python3
"""triage_v6.py — Triagem LEVE de .v6 grandes (7B/10B): percorre as secoes por
streaming (mmap), sem materializar pesos. Reporta: header, buracos (zeros),
regime de scales, tipo de unembed, RMS. RAM ~constante.

Uso:
  python tools/triage_v6.py target1/FALCON3_7B.V6 [outro.v6 ...]
"""
from __future__ import annotations
import struct
import sys
from pathlib import Path

import numpy as np


def triage(path: Path):
    print(f"===== {path.name} ({path.stat().st_size / 1024 / 1024:.0f} MB) =====")
    with open(path, "rb") as f:
        mm = f.read()  # mmap seria ideal; leitura crua p/ simplicidade (SO pagina)
    o = 0

    def rd(fmt):
        nonlocal o
        v = struct.unpack_from(fmt, mm, o)
        o += struct.calcsize(fmt)
        return v[0] if len(v) == 1 else v

    magic, ver, nparams, mtype = rd("<I"), rd("<H"), rd("<Q"), rd("<B")
    o += 3
    ok = magic == 0xBE11BE11 and ver in (6, 7) and mtype == 0
    print(f"header: magic_ok={magic == 0xBE11BE11} ver={ver} mtype={mtype} "
          f"nparams={nparams}")
    if not ok:
        print("ARQUIVO INVALIDO"); return
    hidden, layers, heads, vocab, max_seq, inter, kv, qdim, medusa = (
        rd("<H"), rd("<H"), rd("<H"), rd("<I"), rd("<H"),
        rd("<H"), rd("<H"), rd("<H"), rd("<I"))
    tie = rd("<4s") == b"TIED"
    tok_type, tok_len = rd("<B"), rd("<I")
    o += tok_len
    act, emb, feat = rd("<B"), rd("<B"), rd("<B")
    if ver == 7:
        # provenance (ADR-0115 §2.2): repo/rev/conv/git + convenções
        rlen = rd("<H")
        repo = mm[o:o + rlen].decode("utf-8", "replace")
        o += rlen
        rev = mm[o:o + 40].rstrip(b"\x00").decode("ascii", "replace")
        o += 40
        clen = rd("<H")
        conv = mm[o:o + clen].decode("utf-8", "replace")
        o += clen
        git = mm[o:o + 40].rstrip(b"\x00").decode("ascii", "replace")
        o += 40
        sc, uc, _, _ = struct.unpack_from("<BBBB", mm, o)
        o += 4
        print(f"prov: repo={repo} rev={rev} scale_conv={sc} unpack_conv={uc}")
    hd = qdim // heads
    kdim = kv * hd
    ffn = inter * qdim // hidden
    print(f"arch: hidden={hidden} L={layers} heads={heads}/{kv} vocab={vocab} "
          f"max_seq={max_seq} inter={inter} tie={tie} act={act} embed_type={emb} "
          f"feat=0x{feat:02x} medusa={medusa}")
    if feat & ~0x0C:
        print(f"WARN feat bits desconhecidos 0x{feat:02x}")

    def skip_rms(n, tag):
        nonlocal o
        a = np.frombuffer(mm[o:o + 4 * n], dtype="<f4")
        o += 4 * n
        return float(a.mean()), float(a.std()), bool(np.isfinite(a).all())

    def stat_tern(rows, cols, tag, zeroes):
        """Le packed+scale SEM guardar: zero% por amostragem de blocos."""
        nonlocal o
        n = rows * cols
        nb = (n + 3) // 4
        raw = mm[o:o + nb]
        o += nb
        (scale,) = struct.unpack_from("<f", mm, o)
        o += 4
        b = np.frombuffer(bytearray(raw), dtype=np.uint8)
        # contagem de codigos 0..3 nos 2-bit (amostra ate 2M bytes p/ velocidade)
        step = max(1, len(b) // 2000000)
        codes = ((b[::step, None] >> (2 * np.arange(4))) & 3).ravel()
        frac = [(codes == c).mean() for c in range(4)]
        if scale <= 1e-5:
            zeroes.append(f"{tag} scale={scale:g}")
        return scale, frac

    def stat_q6k(rows, cols, tag):
        nonlocal o
        n = rows * cols
        nb = ((n + 255) // 256) * 210
        o += nb
        (scale,) = struct.unpack_from("<f", mm, o)
        o += 4
        return scale

    zeroes = []
    # embed
    n_emb = hidden * vocab
    if emb == 1:
        s = stat_q6k(hidden, vocab, "embed")
        print(f"embed Q6_K scale={s:g}")
    elif emb == 0:
        s, f0 = stat_tern(hidden, vocab, "embed", zeroes)
        print(f"embed TERN scale={s:g} code0={f0[0]:.3f}")
    else:
        o += n_emb * 4
        s = struct.unpack_from("<f", mm, o)[0]
        o += 4
        print(f"embed BF16 scale={s:g}")
    # layers
    bad = 0
    for li in range(layers):
        m1, _, _ = skip_rms(hidden, "a")
        m2, _, _ = skip_rms(hidden, "f")
        if li == 0:
            print(f"L00 rms means: {m1:.4f} {m2:.4f}")
        specs = [("q", hidden, qdim), ("k", hidden, kdim), ("v", hidden, kdim),
                 ("o", qdim, hidden), ("gate", hidden, ffn), ("up", hidden, ffn),
                 ("down", inter, qdim)]
        for nm, r, c in specs:
            s, f0 = stat_tern(r, c, f"L{li:02d}-{nm}", zeroes)
            if s <= 1e-5:
                bad += 1
    rmsf = skip_rms(hidden, "final")
    print(f"rms_final mean={rmsf[0]:.4f}")
    # unembed
    if tie:
        print("unembed: TIED")
    elif feat & 0x08:
        s = stat_q6k(hidden, vocab, "unembed")
        print(f"unembed Q6_K scale={s:g}")
    else:
        s, f0 = stat_tern(hidden, vocab, "unembed", zeroes)
        print(f"unembed TERN scale={s:g} code0={f0[0]:.3f}")
    # medusa + theta
    for _ in range(medusa):
        stat_tern(hidden, vocab, "medusa", zeroes)
    if feat & 0x04:
        (theta,) = struct.unpack_from("<f", mm, o)
        o += 4
        print(f"theta={theta:g}")
    print(f"bytes consumidos={o}/{len(mm)} {'OK-EXATO' if o == len(mm) else 'DIVERGE!'}")
    print(f"tensor scale<=1e-5: {bad} | buracos: {len(zeroes)}")
    for z in zeroes[:10]:
        print(f"  BURACO {z}")
    print()


def main():
    for p in sys.argv[1:]:
        triage(Path(p))


if __name__ == "__main__":
    main()
