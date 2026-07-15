#!/usr/bin/env python3
"""Converte microsoft/bitnet-b1.58-2B-4T safetensors → .bitnet v4 estruturado.

HF guarda projeções uint8 já em packing 2-bit (4 trits/byte) com shape (out/4, in).
O kernel espera PackedTernaryTensor(k=in, n=out) sem prefixos de comprimento.

Uso: python tools/convert_bitnet.py
"""
from __future__ import annotations

import json
import os
import struct
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"


def decode_trit(bits: int) -> int:
    b = bits & 0b11
    if b == 0b01:
        return 1
    if b == 0b10:
        return -1
    return 0


def encode_trit(v: int) -> int:
    if v > 0:
        return 0b01
    if v < 0:
        return 0b10
    return 0b00


def unpack_hf_oi(u8: np.ndarray) -> np.ndarray:
    """HF (out/4, in) uint8 → int8 matrix (out, in). Packing ao longo de `out`."""
    out4, inn = u8.shape
    flat = u8.astype(np.uint8).reshape(-1)
    # 4 trits/byte → códigos 0,1,2 → mapear para 0,+1,-1
    codes = np.empty((flat.size, 4), dtype=np.uint8)
    for i in range(4):
        codes[:, i] = (flat >> (2 * i)) & 3
    lut = np.array([0, 1, -1, 0], dtype=np.int8)
    trits = lut[codes]  # (nbytes, 4)
    return trits.reshape(out4, inn, 4).transpose(0, 2, 1).reshape(out4 * 4, inn)


def pack_kn(weights_oi: np.ndarray) -> bytes:
    """(out, in) → packed row-major (in, out) no formato do kernel."""
    out, inn = weights_oi.shape
    # kn[t, j] = oi[j, t]  → flatten row-major (in, out)
    kn = np.ascontiguousarray(weights_oi.T)  # (in, out)
    flat = kn.reshape(-1)
    n = flat.size
    packed = bytearray((n + 3) // 4)
    for i, v in enumerate(flat):
        packed[i // 4] |= encode_trit(int(v)) << ((i % 4) * 2)
    return bytes(packed)


def pack_kn_fast(weights_oi: np.ndarray) -> bytes:
    """Versão vetorizada de pack_kn."""
    kn = np.ascontiguousarray(weights_oi.T).reshape(-1)
    n = kn.size
    # map -1,0,1 → 2,0,1
    bits = np.zeros(n, dtype=np.uint8)
    bits[kn > 0] = 0b01
    bits[kn < 0] = 0b10
    pad = (-n) % 4
    if pad:
        bits = np.concatenate([bits, np.zeros(pad, dtype=np.uint8)])
    b = bits.reshape(-1, 4)
    packed = b[:, 0] | (b[:, 1] << 2) | (b[:, 2] << 4) | (b[:, 3] << 6)
    return packed.tobytes()


def absmean_quantize(mat_f: np.ndarray) -> np.ndarray:
    """bf16/f32 → ternary {-1,0,1} via absmean (BitNet)."""
    x = mat_f.astype(np.float32)
    scale = np.mean(np.abs(x)) + 1e-6
    q = np.round(x / scale)
    return np.clip(q, -1, 1).astype(np.int8)


def write_f32_vec(f, arr: np.ndarray) -> None:
    f.write(np.ascontiguousarray(arr, dtype=np.float32).tobytes())


def convert() -> None:
    safetensors_path = TARGET / "model.safetensors"
    config_path = TARGET / "config.json"
    output_path = TARGET / "bitnet_2B.bitnet"

    if not safetensors_path.exists():
        print(f"[ERR] {safetensors_path} not found.")
        return

    import torch
    from safetensors.torch import load_file

    print(f"[LOAD] {safetensors_path} ({os.path.getsize(safetensors_path)/1e9:.2f}GB)")
    state = load_file(str(safetensors_path))
    with open(config_path, encoding="utf-8") as cf:
        cfg = json.load(cf)

    hidden = int(cfg["hidden_size"])
    num_layers = int(cfg["num_hidden_layers"])
    num_heads = int(cfg["num_attention_heads"])
    vocab_size = int(cfg["vocab_size"])
    max_seq = int(cfg.get("max_position_embeddings", 2048))
    intermediate_size = int(cfg["intermediate_size"])
    num_kv_heads = int(cfg.get("num_key_value_heads", num_heads))
    tie = bool(cfg.get("tie_word_embeddings", True))

    # HF packed (out/4,in): q out = num_heads * head_dim_hf = 2560
    q0 = state["model.layers.0.self_attn.q_proj.weight"]
    q_dim = int(q0.shape[0]) * 4  # 640*4 = 2560
    head_dim = q_dim // num_heads
    k_dim = num_kv_heads * head_dim
    ffn_group = intermediate_size  # gate/up out = 6912

    print(f"  hidden={hidden} L={num_layers} heads={num_heads} kv={num_kv_heads}")
    print(f"  q_dim={q_dim} head_dim={head_dim} k_dim={k_dim} ffn={ffn_group} tie={tie}")

    MAGIC = 0xBE11BE11
    # feat: bit0=inner_attn_ln, bit1=ffn_layernorm, bit2=RoPE
    # Sem rope_theta no stream → nao setar bit2 (reader falharia no EOF).
    feat = 0x03

    with open(output_path, "wb") as f:
        f.write(struct.pack("<I", MAGIC))
        f.write(struct.pack("<H", 4))
        f.write(struct.pack("<I", 849787090))
        f.write(struct.pack("<H", hidden))
        f.write(struct.pack("<H", num_layers))
        f.write(struct.pack("<H", num_heads))
        f.write(struct.pack("<I", vocab_size))
        f.write(struct.pack("<H", min(max_seq, 65535)))
        f.write(struct.pack("<H", intermediate_size))
        f.write(struct.pack("<H", num_kv_heads))
        f.write(struct.pack("<H", q_dim))
        f.write(struct.pack("<I", 0))  # medusa
        f.write(b"TIED" if tie else b"\x00\x00\x00\x00")
        f.write(struct.pack("B", 1))
        tok = b"CHAR:32-126"
        f.write(struct.pack("<I", len(tok)))
        f.write(tok)
        f.write(struct.pack("B", feat))

        # embed (vocab, hidden) bf16 → quantize → store as (hidden, vocab)
        emb = state["model.embed_tokens.weight"].to(torch.float32).numpy()
        emb_q = absmean_quantize(emb)  # (vocab, hidden)
        # our layout (hidden, vocab): transpose then pack
        emb_pack = pack_kn_fast(emb_q.T)  # pack_kn expects (out,in)=(vocab,hidden)? 
        # Wait: pack_kn(weights_oi) with oi=(out,in), stores (in,out).
        # embed_lookup wants (hidden, vocab): get_weight(row*vocab+tok)
        # matmul tie: (k=hidden, n=vocab). So shape (hidden, vocab) = (in_for_logits?, vocab)
        # store as packed (hidden, vocab): treat as oi with out=vocab, in=hidden → pack gives (hidden,vocab) ✓
        # emb_q is (vocab,hidden)=(out,in) for that convention:
        emb_pack = pack_kn_fast(emb_q)  # (vocab, hidden) → packed (hidden, vocab)
        assert len(emb_pack) == (hidden * vocab_size + 3) // 4
        f.write(emb_pack)
        print(f"  [T] embed {emb.shape} -> {len(emb_pack)//1024}KB")

        def hf_proj(name: str) -> bytes:
            u8 = state[name].cpu().numpy().astype(np.uint8)
            oi = unpack_hf_oi(u8)
            return pack_kn_fast(oi)

        for li in range(num_layers):
            p = f"model.layers.{li}"
            # RMS f32
            write_f32_vec(f, state[f"{p}.input_layernorm.weight"].to(torch.float32).numpy())
            write_f32_vec(f, state[f"{p}.post_attention_layernorm.weight"].to(torch.float32).numpy())
            write_f32_vec(f, state[f"{p}.self_attn.attn_sub_norm.weight"].to(torch.float32).numpy())
            write_f32_vec(f, state[f"{p}.mlp.ffn_sub_norm.weight"].to(torch.float32).numpy())

            q = hf_proj(f"{p}.self_attn.q_proj.weight")
            k = hf_proj(f"{p}.self_attn.k_proj.weight")
            v = hf_proj(f"{p}.self_attn.v_proj.weight")
            o = hf_proj(f"{p}.self_attn.o_proj.weight")
            gate = hf_proj(f"{p}.mlp.gate_proj.weight")
            up = hf_proj(f"{p}.mlp.up_proj.weight")
            down = hf_proj(f"{p}.mlp.down_proj.weight")

            assert len(q) == (hidden * q_dim + 3) // 4
            assert len(k) == (hidden * k_dim + 3) // 4
            assert len(v) == (hidden * k_dim + 3) // 4
            assert len(o) == (q_dim * hidden + 3) // 4
            assert len(gate) == (hidden * ffn_group + 3) // 4
            assert len(up) == (hidden * ffn_group + 3) // 4
            assert len(down) == (intermediate_size * q_dim + 3) // 4

            f.write(q)
            f.write(k)
            f.write(v)
            f.write(o)
            f.write(gate)
            f.write(up)
            f.write(down)
            if li % 5 == 0 or li + 1 == num_layers:
                print(f"  [L] {li}/{num_layers} off={f.tell()//1024}KB")

        # rms_final
        write_f32_vec(f, state["model.norm.weight"].to(torch.float32).numpy())
        # tied → no unembed

    sz = os.path.getsize(output_path)
    print(f"\n[OK] {output_path}: {sz:,} bytes ({sz/1024/1024:.1f} MB)")


if __name__ == "__main__":
    convert()
