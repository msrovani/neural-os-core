#!/usr/bin/env python3
"""sim_load_model_hwexpert.py — Sprint 107 Part B #8 verification.

Simula (host-side, sem cargo/QEMU) a logica de
`crates/neural-kernel/src/cortex.rs::load_model()` para confirmar que o
header corrigido (`tools/fix_bitnet_header.py`) faz o parse deixar de
retornar None ("parse FAILED"). NAO reimplementa 100% do Rust — replica
os campos/offsets/bounds-checks que decidem sucesso/falha do parse.

Uso: python tools/sim_load_model_hwexpert.py target/hw_expert_v3.bitnet
"""
import struct
import sys
from pathlib import Path


def ru16(d, off):
    return struct.unpack_from("<H", d, off)[0], off + 2


def ru32(d, off):
    return struct.unpack_from("<I", d, off)[0], off + 4


def rf32(d, off):
    return struct.unpack_from("<f", d, off)[0], off + 4


def ternary_bytes(rows, cols):
    return (rows * cols + 3) // 4


def sim(path: Path):
    data = path.read_bytes()
    off = 0
    magic, off = ru32(data, off)
    assert magic == 0xBE11BE11, f"magic invalido {hex(magic)}"
    version, off = ru16(data, off)
    num_params, off = ru32(data, off)
    hidden, off = ru16(data, off)
    num_layers, off = ru16(data, off)
    off = 4 + 2 + 4 + 2 + 2  # reset como no kernel
    num_heads, off = ru16(data, off)
    vocab_size, off = ru32(data, off)
    max_seq, off = ru16(data, off)
    print(f"version={version} hidden={hidden} num_layers={num_layers} num_heads={num_heads} "
          f"vocab_size={vocab_size} max_seq={max_seq}")

    if version < 3:
        print("[FAIL-SIM] versao <3 nao simulada aqui")
        return False

    intermediate_size, off = ru16(data, off)
    num_kv_heads, off = ru16(data, off)
    q_dim, off = ru16(data, off)
    num_medusa, off = ru32(data, off)
    tie = data[off:off + 4]; off += 4
    tie_embeddings = (tie == b"TIED")
    tok_type = data[off]; off += 1
    tok_len, off = ru32(data, off)
    tok = data[off:off + tok_len]; off += tok_len
    print(f"intermediate_size={intermediate_size} num_kv_heads={num_kv_heads} q_dim={q_dim} "
          f"num_medusa={num_medusa} tie={tie!r} tok={tok!r}")

    if version >= 4:
        layer_features = data[off]; off += 1
        has_inner_attn_ln = bool(layer_features & 0x01)
        has_ffn_layernorm = bool(layer_features & 0x02)
        has_rope = bool(layer_features & 0x04)
        print(f"layer_features={layer_features:#x} inner={has_inner_attn_ln} "
              f"ffn_ln={has_ffn_layernorm} rope={has_rope}")
    else:
        has_inner_attn_ln = has_ffn_layernorm = has_rope = False

    embed_count = ternary_bytes(hidden, vocab_size)
    if off + embed_count > len(data):
        print(f"[FAIL] embed tensor out of bounds: need {embed_count}B at off={off}, "
              f"file={len(data)}B")
        return False
    off += embed_count
    print(f"[OK] embed tensor read ({embed_count}B), off={off}")

    kv_head_dim = q_dim // max(num_heads, 1)
    k_dim = num_kv_heads * kv_head_dim
    ffn_group = intermediate_size * q_dim // max(hidden, 1)
    down_out = q_dim

    tern_per = (
        ternary_bytes(hidden, q_dim)
        + 2 * ternary_bytes(hidden, k_dim)
        + ternary_bytes(q_dim, hidden)
        + 2 * ternary_bytes(hidden, ffn_group)
        + ternary_bytes(intermediate_size, down_out)
    )
    rem = len(data) - off
    best = None
    for basic in (False, True):
        for inner in (False, True):
            for ffn in (False, True):
                per = tern_per
                if basic:
                    per += hidden * 8
                if inner:
                    per += kv_head_dim * num_heads * 4
                if ffn:
                    per += intermediate_size * 4
                need = per * num_layers
                d = abs(rem - need)
                if best is None or d < best[0]:
                    best = (d, basic, inner, ffn, need)
    d, has_basic_rms, has_inner_attn_ln, has_ffn_layernorm, need = best
    print(f"[LAYOUT] q_dim={q_dim} kv_head_dim={kv_head_dim} k_dim={k_dim} ffn_group={ffn_group} "
          f"tern_per={tern_per} rem={rem}B need={need}B d={d}B "
          f"basic_rms={has_basic_rms} inner={has_inner_attn_ln} ffn_ln={has_ffn_layernorm}")
    if d > 0:
        print(f"[WARN] layout NAO fecha exatamente com o arquivo (mismatch {d}B) — "
              "esperado para o formato custom BitNetLM (train_gpu_full.py); "
              "parse deve continuar mas pesos ficam semanticamente incorretos.")

    for li in range(num_layers):
        for name, n in (
            ("rms_attn", hidden if has_basic_rms else 0),
            ("rms_ffn", hidden if has_basic_rms else 0),
            ("rms_inner_attn", kv_head_dim * num_heads if has_inner_attn_ln else 0),
            ("rms_ffn_norm", intermediate_size if has_ffn_layernorm else 0),
        ):
            if n and off + n * 4 > len(data):
                print(f"[FAIL] layer {li} {name} out of bounds off={off} need={n*4}B")
                return False
            off += n * 4
        for name, rows, cols in (
            ("q", hidden, q_dim), ("k", hidden, k_dim), ("v", hidden, k_dim),
            ("o", q_dim, hidden), ("gate", hidden, ffn_group), ("up", hidden, ffn_group),
            ("down", intermediate_size, down_out),
        ):
            n = ternary_bytes(rows, cols)
            if off + n > len(data):
                print(f"[FAIL] layer {li} tensor {name} out of bounds off={off} need={n}B "
                      f"file={len(data)}B")
                return False
            off += n

    print(f"[OK] all {num_layers} layers read, off={off}")

    if off + hidden * 4 <= len(data):
        off += hidden * 4  # rms_final
        print(f"[OK] rms_final read, off={off}")
    else:
        print("[OK] rms_final absent (tied/EOF fallback), off unchanged")

    expected = ternary_bytes(hidden, vocab_size)
    if not tie_embeddings and off + expected <= len(data):
        is_zeroed = all(b == 0 for b in data[off:min(off + 16, len(data))])
        if is_zeroed:
            tie_embeddings = True
            print("[OK] unembed region all-zero -> tie_embeddings inferred")
        else:
            off += expected
            print(f"[OK] unembed tensor read ({expected}B), off={off}")
    else:
        tie_embeddings = True
        print("[OK] unembed absent/short -> tie_embeddings fallback")

    if num_medusa > 0:
        for m in range(num_medusa):
            n = ternary_bytes(hidden, vocab_size)
            if off + n > len(data):
                print(f"[FAIL] medusa head {m} out of bounds off={off} need={n}B")
                return False
            off += n
        print(f"[OK] {num_medusa} medusa heads read, off={off}")
    else:
        print("[OK] num_medusa=0, no medusa heads")

    print(f"[PASS] load_model() simulation returns Some(model) — NAO retorna None. "
          f"final_off={off}B file={len(data)}B remaining={len(data)-off}B")
    return True


if __name__ == "__main__":
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "target/hw_expert_v3.bitnet")
    ok = sim(path)
    sys.exit(0 if ok else 1)
