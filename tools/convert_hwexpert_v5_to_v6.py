#!/usr/bin/env python3
"""convert_hwexpert_v5_to_v6.py — HW Expert v5 → v6 (ADR-0085 §3.2).

Lê o artefato v5 multi-head (export do retrain_hw_expert_v4.py, formato com
prefixos u32 len + u32 scale por tensor) e reescreve como .bitnet v6
(model_type=1) sem prefixos, com act_type/embed_type/feat no header.

Diferenças v5 → v6 hwexpert:
  - header: num_params u32 → u64; model_type=1; reserved 3B; q_dim==hidden
    (colapsa q/k/v/o=(h,h), g/u=(h,ff), d=(ff,h)); act/embed/feat no fim.
  - body: sem prefixos; rms = f32 puro; tensor = packed + f32 scale.
  - rope (16 f32/layer, lido e descartado no v5) NÃO é escrito no v6 —
    o forward hwexpert não usa rope (heads são matmul puro).
  - feat=0x03 (rms_inner + rms_ffn_norm presentes, como no v5 layer_features).

Uso:
  python tools/convert_hwexpert_v5_to_v6.py \
      models/hw_expert/hw_expert_v4.bitnet tools/target/hw_expert_v6.bitnet
"""
from __future__ import annotations

import struct
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
import validate_hw_expert_v4 as V  # noqa: E402  (Rust-exact v5 port, numpy only)
import bitnet_writer as BW  # noqa: E402  (canonical v6 writer, ADR-0085)

MAGIC = 0xBE11BE11


def convert(v5_path: Path, v6_path: Path) -> None:
    data = v5_path.read_bytes()
    m, end = V.load_v5(data)
    if m is None or end != len(data):
        raise SystemExit(f"[ERRO] {v5_path}: v5 parse falhou (end={end}/{len(data)})")

    h = m["hidden"]
    nl = m["num_layers"]
    nh = max(1, h // max(1, m["q_dim"]))  # export v5: qd = h // heads
    vocab = m["vocab"]
    ff = m["intermediate"]
    # v6 hwexpert: shapes fixos q/k/v/o=(h,h), g/u=(h,ff), d=(ff,h) — o
    # forward (predict_hw_v4) usa model.q_dim para TRUNCAR a atenção, e o
    # modelo foi treinado com qd = h // nh = 32. Preservar q_dim no header
    # (não colapsar para hidden) mantém predições idênticas ao v5 treinado.
    qd = m["q_dim"]
    nkv = nh

    # num_params informativo (soma de todos os elementos)
    num_params = h * vocab  # embed
    per_layer = (h * h * 4 + h * ff * 3)  # q,k,v,o,g,u,d
    num_params += ff * nl + per_layer * nl
    num_params += h * 17 + h * 8 + h * 9 + h * 10 + h * 9  # 5 heads

    out = []
    f = out.append

    def w(bs: bytes) -> None:
        f(bs)

    # ── Header v6 (ADR-0085 §2 + §3.2) ─────────────────────────────
    # Escrita direta do header (write_header_v6 exige file-like com write;
    # aqui acumulamos bytes — replicamos o layout exato).
    w(struct.pack("<I", MAGIC))
    w(struct.pack("<H", 6))                    # version
    w(struct.pack("<Q", num_params))           # num_params u64
    w(struct.pack("<B", BW.MODEL_HWEXPERT))    # model_type=1
    w(b"\x00\x00\x00")                         # reserved
    w(struct.pack("<H", h))                    # hidden @18
    w(struct.pack("<H", nl))                   # layers
    w(struct.pack("<H", nh))                   # heads
    w(struct.pack("<I", vocab))                # vocab
    w(struct.pack("<H", 16))                   # max_seq
    w(struct.pack("<H", ff))                   # intermediate
    w(struct.pack("<H", nkv))                  # kv_heads
    w(struct.pack("<H", qd))                   # q_dim == hidden
    w(struct.pack("<I", 0))                    # medusa
    w(b"\x00" * 4)                             # tie_flag (não TIED)
    w(struct.pack("<B", 0))                    # tok_type = none
    w(struct.pack("<I", 0))                    # tok_len
    w(struct.pack("<B", BW.ACT_SILU))          # act_type (não usado p/ hwexpert)
    w(struct.pack("<B", BW.EMBED_TERNARY))     # embed_type = ternary
    w(struct.pack("<B", 0x03))                 # feat: bit0 inner + bit1 ffn_norm

    # ── Body v6 (sem prefixos) ──────────────────────────────────────
    # embed: packed + f32 scale (mesmos bytes packed do v5, scale 1.0)
    w(_packed_of(m["embed"]))
    w(struct.pack("<f", 1.0))

    for layer in m["layers"]:
        w(_f32s(layer["rms_attn"]))
        w(_f32s(layer["rms_ffn"]))
        w(_f32s(layer["rms_inner"]))       # feat bit0
        w(_f32s(layer["rms_ffn_norm"]))    # feat bit1 (ff)
        for key in ("q", "k", "v", "o", "gate", "up", "down"):
            w(_packed_of(layer[key]))
            w(struct.pack("<f", 1.0))

    w(_f32s(m["rms_final"]))
    for head in ("family_head", "fw_head", "agent_head", "caps_head", "next_head"):
        w(_packed_of(m[head]))
        w(struct.pack("<f", 1.0))

    blob = b"".join(out)
    v6_path.parent.mkdir(parents=True, exist_ok=True)
    v6_path.write_bytes(blob)
    print(f"[OK] {v5_path.name} ({len(data)//1024}KB, v5) -> {v6_path} ({len(blob)//1024}KB, v6 mt=1)")
    print(f"  h={h} L={nl} heads={nh} vocab={vocab} ff={ff} qd={qd} feat=0x03")
    print(f"  bytes: {len(blob)} (v5 {len(data)} — rope/prefixos removidos)")


def _f32s(v) -> bytes:
    return np.asarray(v, dtype=np.float32).tobytes()


def _packed_of(w_flat) -> bytes:
    """Re-packs a decoded ternary list {1.0,-1.0,0.0} → 2-bit packed (writer layout)."""
    arr = np.asarray(w_flat, dtype=np.float32)
    i8 = np.zeros(arr.size, dtype=np.int8)
    i8[arr > 0] = 1
    i8[arr < 0] = -1
    return BW.pack_ternary(i8)


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(1)
    convert(Path(sys.argv[1]), Path(sys.argv[2]))
