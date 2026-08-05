#!/usr/bin/env python3
"""Converte microsoft/bitnet-b1.58-2B-4T safetensors → .bitnet v6 (ADR-0085).

Usa bitnet_writer.py como writer canônico.

Uso: python tools/convert_bitnet.py
"""
from __future__ import annotations

import json
import os
from pathlib import Path

import numpy as np

from tools.bitnet_writer import (
    write_header_v6, write_embed, write_rms, write_ternary, compute_feat,
    MODEL_LLM, ACT_RELU2, EMBED_Q6K,
)

ROOT = Path(__file__).resolve().parents[1]
# Modelos novos vão para target1/ (decisão do dono 2026-08-05)
TARGET = ROOT / "target1"


def absmean_quantize(mat_f: np.ndarray) -> np.ndarray:
    """bf16/f32 → ternary {-1,0,1} via absmean (BitNet)."""
    x = mat_f.astype(np.float32)
    scale = np.mean(np.abs(x)) + 1e-6
    q = np.round(x / scale)
    return np.clip(q, -1, 1).astype(np.int8)


def unpack_hf_oi(u8: np.ndarray) -> np.ndarray:
    """HF (out/4, in) uint8 → int8 matrix (out, in). Packing ao longo de `out`."""
    out4, inn = u8.shape
    flat = u8.astype(np.uint8).reshape(-1)
    codes = np.empty((flat.size, 4), dtype=np.uint8)
    for i in range(4):
        codes[:, i] = (flat >> (2 * i)) & 3
    lut = np.array([0, 1, -1, 0], dtype=np.int8)
    trits = lut[codes]
    return trits.reshape(out4, inn, 4).transpose(0, 2, 1).reshape(out4 * 4, inn)


def hf_proj_packed(state: dict, name: str) -> np.ndarray:
    """Unpack HF 2-bit packed projection → int8 (out, in) row-major."""
    u8 = state[name].cpu().numpy().astype(np.uint8)
    return unpack_hf_oi(u8)


def convert() -> None:
    safetensors_path = TARGET / "model.safetensors"
    config_path = TARGET / "config.json"
    output_path = TARGET / "bitnet_2B.bitnet"

    if not safetensors_path.exists():
        print(f"[ERR] {safetensors_path} not found.")
        return

    import torch
    from safetensors.torch import load_file

    print(f"[LOAD] {safetensors_path} ({os.path.getsize(safetensors_path) / 1e9:.2f}GB)")
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

    q0 = state["model.layers.0.self_attn.q_proj.weight"]
    q_dim = int(q0.shape[0]) * 4  # 640*4 = 2560
    head_dim = q_dim // num_heads
    k_dim = num_kv_heads * head_dim
    ffn_group = intermediate_size  # gate/up out = 6912

    print(f"  hidden={hidden} L={num_layers} heads={num_heads} kv={num_kv_heads}")
    print(f"  q_dim={q_dim} head_dim={head_dim} k_dim={k_dim} ffn={ffn_group} tie={tie}")

    # num_params informativo (ADR-0085)
    num_params = hidden * vocab_size  # embed
    num_params += intermediate_size * num_layers  # rms_ffn_norm
    per_layer = (hidden * q_dim + hidden * k_dim * 2 + q_dim * hidden
                 + hidden * ffn_group * 2 + intermediate_size * q_dim)
    num_params += per_layer * num_layers
    if not tie:
        num_params += hidden * vocab_size  # unembed

    # feat: bit0=rms_inner_attn, bit1=rms_ffn_norm, bit2=theta (500000)
    has_inner = True   # 2B4T has attn_sub_norm
    has_ffn = True     # 2B4T has ffn_sub_norm
    has_theta = True   # theta=500000
    feat = compute_feat(has_inner, has_ffn, has_theta)
    # ponytail: embed_type=TERNARY for now; upgrade to Q6_K in Phase 5 (ADR-0085 F3)
    # when encoder is implemented. 2B4T embed decay Q6_K=17.149 vs BF16=17.109 PPL.

    with open(output_path, "wb") as f:
        # Header v6 (ADR-0085 §2)
        write_header_v6(
            f,
            model_type=MODEL_LLM,
            num_params=num_params,
            hidden=hidden,
            layers=num_layers,
            heads=num_heads,
            vocab=vocab_size,
            max_seq=min(max_seq, 65535),
            intermediate=intermediate_size,
            kv_heads=num_kv_heads,
            q_dim=q_dim,
            medusa=0,
            tie=tie,
            act_type=ACT_RELU2,       # 2B4T uses ReLU² (ADR-0084 M1)
            embed_type=EMBED_Q6K,     # embed BF16 → Q6_K (ADR-0085 §10.1, M4)
            feat=feat,
        )

        # embed (vocab, hidden) BF16 → Q6_K (hidden, vocab) row-major.
        # write_embed ravel row-major → passar (hidden, vocab) transposto.
        emb = state["model.embed_tokens.weight"].to(torch.float32).numpy()  # (vocab, hidden)
        write_embed(f, emb.T, EMBED_Q6K, scale=1.0)
        print(f"  [T] embed {emb.shape} → Q6_K {(emb.size + 255) // 256 * 210} bytes")

        for li in range(num_layers):
            p = f"model.layers.{li}"
            # RMS norms — ordem: rms_attn, rms_ffn, rms_inner_attn, rms_ffn_norm
            write_rms(f, state[f"{p}.input_layernorm.weight"].to(torch.float32).numpy())
            write_rms(f, state[f"{p}.post_attention_layernorm.weight"].to(torch.float32).numpy())
            write_rms(f, state[f"{p}.self_attn.attn_sub_norm.weight"].to(torch.float32).numpy())
            write_rms(f, state[f"{p}.mlp.ffn_sub_norm.weight"].to(torch.float32).numpy())

            # 7 weight tensors: q, k, v, o, gate, up, down
            # Sempre com f32 scale (ADR-0085 D1); scale=1.0 p/ HF-proj (já ternário)
            tensors = [
                (hf_proj_packed(state, f"{p}.self_attn.q_proj.weight"), hidden, q_dim),
                (hf_proj_packed(state, f"{p}.self_attn.k_proj.weight"), hidden, k_dim),
                (hf_proj_packed(state, f"{p}.self_attn.v_proj.weight"), hidden, k_dim),
                (hf_proj_packed(state, f"{p}.self_attn.o_proj.weight"), q_dim, hidden),
                (hf_proj_packed(state, f"{p}.mlp.gate_proj.weight"), hidden, ffn_group),
                (hf_proj_packed(state, f"{p}.mlp.up_proj.weight"), hidden, ffn_group),
                (hf_proj_packed(state, f"{p}.mlp.down_proj.weight"), intermediate_size, q_dim),
            ]
            for mat, rows, cols in tensors:
                write_ternary(f, mat.ravel(), scale=1.0)

            if li % 5 == 0 or li + 1 == num_layers:
                print(f"  [L] {li}/{num_layers} off={f.tell() // 1024}KB")

        # rms_final
        write_rms(f, state["model.norm.weight"].to(torch.float32).numpy())

        # theta (feat bit2 = has_theta)
        theta = float(cfg.get("rope_theta", 10000.0))
        import struct
        f.write(struct.pack("<f", theta))

        # tied → no unembed (ADR-0085 D3)
        if not tie:
            # ponytail: tied é True p/ 2B4T, este caminho não executa
            pass

    sz = os.path.getsize(output_path)
    print(f"\n[OK] {output_path}: {sz:,} bytes ({sz / 1024 / 1024:.1f} MB)")
    print(f"  v6: act=RELU2 embed=TERNARY feat=0x{feat:02x} tie={tie} theta={theta}")


if __name__ == "__main__":
    convert()
