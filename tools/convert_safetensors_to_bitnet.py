#!/usr/bin/env python3
"""Converte modelo BitNet safetensors (HuggingFace, LLaMA-style) → .bitnet v6 (ADR-0085).

Escreve um body LLM v6 REAL via tools/bitnet_writer (não mais container
prefixado): header do config, embed ternário (hidden, vocab), 2+2 norms e
7 tensores por layer, rms_final, unembed (se não tied) e theta no EOF.

Uso: python tools/convert_safetensors_to_bitnet.py [--hf microsoft/bitnet-b1.58-2B-4T]
"""
import argparse, json, os, struct, tempfile
from pathlib import Path

import numpy as np

from tools.bitnet_writer import (
    write_header_v6, write_embed, write_rms, write_ternary, compute_feat,
    MODEL_LLM, ACT_SILU, ACT_RELU2, EMBED_TERNARY,
)

TARGET = Path(__file__).parent / "target"


def download_hf(repo_id, filename, dest):
    from huggingface_hub import hf_hub_download
    print(f"  [DL] {filename}")
    downloaded = hf_hub_download(repo_id=repo_id, filename=filename, local_dir=dest.parent)
    return downloaded


def absmean_quantize(mat_f: np.ndarray) -> tuple[np.ndarray, float]:
    """f32 → ternary {-1,0,+1} via absmean (BitNet). Retorna (int8, scale)."""
    x = mat_f.astype(np.float32)
    abs_mean = float(np.mean(np.abs(x))) + 1e-10
    scale = 1.0 / abs_mean
    q = np.round(x / abs_mean)
    return np.clip(q, -1, 1).astype(np.int8), scale


def unpack_hf_oi(u8: np.ndarray) -> np.ndarray:
    """HF (out/4, in) uint8 → (out, in) int8 {-1,0,+1}. Packing ao longo de `out`."""
    out4, inn = u8.shape
    flat = u8.astype(np.uint8).reshape(-1)
    codes = np.empty((flat.size, 4), dtype=np.uint8)
    for i in range(4):
        codes[:, i] = (flat >> (2 * i)) & 3
    lut = np.array([0, 1, -1, 0], dtype=np.int8)
    trits = lut[codes]
    return trits.reshape(out4, inn, 4).transpose(0, 2, 1).reshape(out4 * 4, inn)


def to_ternary_i8(t) -> tuple[np.ndarray, float]:
    """Tensor → (int8 row-major, scale). uint8 = HF 2-bit packed → unpack; senão absmean."""
    arr = t.cpu().numpy()
    if arr.dtype == np.uint8:
        return unpack_hf_oi(arr), 1.0
    return absmean_quantize(arr)


def _first(state: dict, keys: list) -> str | None:
    """Primeira chave de `keys` presente no state (ou None)."""
    for k in keys:
        if k in state:
            return k
    return None


def convert_safetensors_to_bitnet(safetensors_path, config_path, output_path):
    import torch
    from safetensors.torch import load_file

    print(f"  [LOAD] {safetensors_path}")
    state = load_file(safetensors_path)

    with open(config_path) as f:
        cfg = json.load(f)

    hidden = cfg["hidden_size"]
    num_layers = cfg["num_hidden_layers"]
    num_heads = cfg["num_attention_heads"]
    vocab_size = cfg["vocab_size"]
    max_seq = cfg.get("max_position_embeddings", 2048)
    intermediate_size = cfg["intermediate_size"]
    num_kv_heads = cfg.get("num_key_value_heads", num_heads)
    # BitNet-b1.58-2B-4T: HF head_dim=128 é enganoso; packing real = head_dim=32.
    if hidden == 2560 and num_heads == 20 and num_kv_heads == 5:
        head_dim = 32
    else:
        head_dim = cfg.get("head_dim", hidden // num_heads)
    q_dim = head_dim * num_heads
    k_dim = num_kv_heads * (q_dim // num_heads)
    ffn_group = intermediate_size * q_dim // hidden
    down_out = q_dim
    tie = cfg.get("tie_word_embeddings", False)
    act_type = ACT_RELU2 if str(cfg.get("hidden_act", "")).lower() == "relu2" else ACT_SILU
    theta = float(cfg.get("rope_theta", 10000.0))

    print(f"  Model: hidden={hidden} layers={num_layers} heads={num_heads}")
    print(f"  Vocab: {vocab_size} max_seq={max_seq} ffn={intermediate_size}")
    print(f"  q_dim={q_dim} (head_dim={head_dim}) kv_heads={num_kv_heads} tie={tie}")

    # ── Validação de schema: LLaMA-style (fail loudly, nada de lixo) ──
    missing_top = [k for k in ("model.embed_tokens.weight", "model.norm.weight")
                   if k not in state]
    if missing_top:
        raise ValueError(
            f"Schema não-LLaMA: faltam tensores top-level {missing_top}. "
            f"Presentes: {sorted(state.keys())[:12]}...")
    if not tie and "lm_head.weight" not in state:
        raise ValueError("tie_word_embeddings=false mas 'lm_head.weight' ausente")
    p0 = "model.layers.0"
    req_layer = [
        "input_layernorm.weight", "post_attention_layernorm.weight",
        "self_attn.q_proj.weight", "self_attn.k_proj.weight",
        "self_attn.v_proj.weight", "self_attn.o_proj.weight",
        "mlp.gate_proj.weight", "mlp.up_proj.weight", "mlp.down_proj.weight",
    ]
    missing_layer = [f"{p0}.{k}" for k in req_layer if f"{p0}.{k}" not in state]
    if missing_layer:
        raise ValueError(
            f"Schema não-LLaMA: layer 0 sem {missing_layer}. "
            f"Tensores em layers.0: {sorted(k for k in state if k.startswith(p0 + '.'))}")

    # Feature bits: detecta norms extras na layer 0 (bit0/bit1) + theta (bit2)
    l0_keys = [k for k in state if k.startswith(p0 + ".")]
    has_inner = any(k.endswith(s) for k in l0_keys
                    for s in ("rms_inner_attn.weight", "inner_attn_norm.weight", "attn_sub_norm.weight"))
    has_ffn = any(k.endswith(s) for k in l0_keys
                  for s in ("rms_ffn_norm.weight", "ffn_layernorm.weight", "ffn_sub_norm.weight"))
    feat = compute_feat(has_inner, has_ffn, True)
    num_params = sum(t.numel() for t in state.values())

    with open(output_path, "wb") as f:
        # ── Header v6 ──
        write_header_v6(
            f, model_type=MODEL_LLM, num_params=num_params,
            hidden=hidden, layers=num_layers, heads=num_heads, vocab=vocab_size,
            max_seq=min(max_seq, 65535), intermediate=intermediate_size,
            kv_heads=num_kv_heads, q_dim=q_dim, medusa=0, tie=tie,
            act_type=act_type, embed_type=EMBED_TERNARY, feat=feat,
        )

        # ── Embed (vocab, hidden) → ternary, (hidden, vocab) transposto ──
        emb_q, emb_scale = to_ternary_i8(state["model.embed_tokens.weight"])
        write_embed(f, emb_q.T, EMBED_TERNARY, emb_scale)
        print(f"  [T] embed {tuple(state['model.embed_tokens.weight'].shape)} "
              f"scale={emb_scale:.4f}")

        for li in range(num_layers):
            p = f"model.layers.{li}"
            # RMS norms — ordem v6: rms_attn, rms_ffn, rms_inner_attn, rms_ffn_norm
            write_rms(f, state[f"{p}.input_layernorm.weight"].to(torch.float32).numpy())
            write_rms(f, state[f"{p}.post_attention_layernorm.weight"].to(torch.float32).numpy())
            if has_inner:
                ik = _first(state, [f"{p}.rms_inner_attn.weight", f"{p}.inner_attn_norm.weight",
                                    f"{p}.attn_sub_norm.weight", f"{p}.self_attn.attn_sub_norm.weight"])
                write_rms(f, state[ik].to(torch.float32).numpy())
            if has_ffn:
                fk = _first(state, [f"{p}.rms_ffn_norm.weight", f"{p}.ffn_layernorm.weight",
                                    f"{p}.ffn_sub_norm.weight", f"{p}.mlp.ffn_sub_norm.weight"])
                write_rms(f, state[fk].to(torch.float32).numpy())

            # 7 tensors: q, k, v, o, gate, up, down (packed + f32 scale — ADR-0085 D1)
            tensors = [
                (f"{p}.self_attn.q_proj.weight", hidden, q_dim),
                (f"{p}.self_attn.k_proj.weight", hidden, k_dim),
                (f"{p}.self_attn.v_proj.weight", hidden, k_dim),
                (f"{p}.self_attn.o_proj.weight", q_dim, hidden),
                (f"{p}.mlp.gate_proj.weight", hidden, ffn_group),
                (f"{p}.mlp.up_proj.weight", hidden, ffn_group),
                (f"{p}.mlp.down_proj.weight", intermediate_size, down_out),
            ]
            for name, _rows, _cols in tensors:
                q, scale = to_ternary_i8(state[name])
                write_ternary(f, q, scale)
            if li % 5 == 0 or li + 1 == num_layers:
                print(f"  [L] {li}/{num_layers} off={f.tell() // 1024}KB")

        # ── rms_final ──
        write_rms(f, state["model.norm.weight"].to(torch.float32).numpy())

        # ── Unembed se não tied (D3: tied → nenhum byte) ──
        if not tie:
            q, scale = to_ternary_i8(state["lm_head.weight"])
            write_ternary(f, q, scale)
            print(f"  [T] lm_head {tuple(state['lm_head.weight'].shape)} scale={scale:.4f}")

        # ── Theta (RoPE) — feat bit2 — no EOF ──
        f.write(struct.pack("<f", theta))

        sz = os.path.getsize(output_path)
        print(f"\n  [OK] {output_path}: {sz:,} bytes ({sz/1024/1024:.1f} MB)")
        print(f"  v6: act={'RELU2' if act_type == ACT_RELU2 else 'SILU'} "
              f"embed=TERNARY feat=0x{feat:02x} tie={tie} theta={theta}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--hf", default="microsoft/bitnet-b1.58-2B-4T")
    parser.add_argument("--output", default=None)
    args = parser.parse_args()

    TARGET.mkdir(exist_ok=True)
    repo = args.hf
    out = args.output or (TARGET / f"{repo.replace('/', '_')}.bitnet")

    print(f"=== Convertendo {repo} ===")

    # Download config first
    config_path = Path(tempfile.mktemp(suffix=".json"))
    print(f"  [CFG] Baixando config.json...")
    download_hf(repo, "config.json", config_path)

    # Download model weights
    model_path = Path(tempfile.mktemp(suffix=".safetensors"))
    download_hf(repo, "model.safetensors", model_path)

    convert_safetensors_to_bitnet(str(model_path), str(config_path), str(out))

    os.unlink(config_path)
    os.unlink(model_path)


if __name__ == "__main__":
    main()
