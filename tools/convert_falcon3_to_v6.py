#!/usr/bin/env python3
"""Converte Falcon3 3B (tiiuae/Falcon3-3B-Base) -> .bitnet v6 (ADR-0085).

Suporta dois modos:
  (a) denso BF16 ternarizado (threshold media abs, pack 4/byte)
  (b) nativo 1.58bit (tiiuae/Falcon3-3B-Instruct-1.58bit, ja ternario)

Reusa bitnet_writer.py: write_header_v6, pack_ternary, _encode_q6k, compute_feat.
"""
from __future__ import annotations
import argparse
import json
import os
import struct
import sys
from pathlib import Path

import numpy as np

try:
    from tools.bitnet_writer import (
        write_header_v6, write_embed, write_rms, write_ternary, compute_feat,
        MODEL_LLM, ACT_SILU, EMBED_Q6K,
    )
except ImportError:
    from bitnet_writer import (
        write_header_v6, write_embed, write_rms, write_ternary, compute_feat,
        MODEL_LLM, ACT_SILU, EMBED_Q6K,
    )

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SRC = ROOT / "target" / "falcon3"
DEFAULT_OUT = ROOT / "target1" / "FALCON3.V6"

# Falcon3-3B-Base specs (Librarian)
FALCON_HIDDEN = 3072
FALCON_LAYERS = 22
FALCON_HEADS = 12
FALCON_KV_HEADS = 4
FALCON_INTERMEDIATE = 9216
FALCON_VOCAB = 131072
FALCON_MAX_SEQ = 2048
FALCON_ROPE_THETA = 1000042.0
FALCON_TIE = False


def absmean_quantize(mat_f: np.ndarray) -> tuple[np.ndarray, float]:
    """BF16/f32 -> ternary {-1,0,1} via absmean (BitNet)."""
    x = mat_f.astype(np.float32)
    scale = float(np.mean(np.abs(x))) + 1e-6
    q = np.round(x / scale)
    q = np.clip(q, -1, 1).astype(np.int8)
    return q, scale


def unpack_hf_oi(u8: np.ndarray) -> np.ndarray:
    """HF (out/4, in) uint8 -> int8 (out,in). Packing ao longo de out."""
    out4, inn = u8.shape
    flat = u8.astype(np.uint8).reshape(-1)
    codes = np.empty((flat.size, 4), dtype=np.uint8)
    for i in range(4):
        codes[:, i] = (flat >> (2 * i)) & 3
    lut = np.array([0, 1, -1, 0], dtype=np.int8)
    trits = lut[codes]
    return trits.reshape(out4, inn, 4).transpose(0, 2, 1).reshape(out4 * 4, inn)


def load_state_dict(src_dir: Path) -> tuple[dict, dict]:
    """Carrega safetensors (merge shards) + config.json. Retorna (state, cfg)."""
    cfg_path = src_dir / "config.json"
    if not cfg_path.exists():
        # fallback: procurar em ROOT/target/falcon3
        raise FileNotFoundError(f"config.json nao encontrado em {src_dir}")
    with open(cfg_path, encoding="utf-8") as f:
        cfg = json.load(f)

    # safetensors: single or sharded model-*.safetensors
    shards = sorted(src_dir.glob("model*.safetensors"))
    if not shards:
        shards = sorted(src_dir.glob("*.safetensors"))
    if not shards:
        raise FileNotFoundError(f"nenhum .safetensors em {src_dir}")

    try:
        from safetensors.torch import load_file
        import torch
    except ImportError as e:
        raise SystemExit(f"safetensors/torch nao instalado: {e}. pip install safetensors torch --upgrade")

    state = {}
    for p in shards:
        print(f"[LOAD] {p.name} ({os.path.getsize(p)/1e9:.2f}GB)")
        part = load_file(str(p))
        for k, v in part.items():
            # converte para numpy float32 (torch tensor -> numpy)
            if hasattr(v, "to"):
                # torch tensor
                if v.dtype == torch.bfloat16:
                    arr = v.to(torch.float32).cpu().numpy()
                elif str(v.dtype) == "torch.float8_e4m3fn":
                    arr = v.to(torch.float32).cpu().numpy()
                else:
                    arr = v.cpu().numpy()
            else:
                arr = np.asarray(v)
            state[k] = arr
    return state, cfg


def get_proj_tensor(state: dict, key: str, native: bool, rows: int, cols: int):
    """Recupera tensor ternario (rows,cols) int8 + scale. Suporta BF16 denso e packed u8."""
    arr = state.get(key)
    if arr is None:
        # tenta sem .weight suffix
        arr = state.get(key.replace(".weight", ""))
    if arr is None:
        print(f"  [WARN] {key} ausente -> zeros")
        q, scale = absmean_quantize(np.zeros((rows, cols), dtype=np.float32))
        return q, scale

    # caso packed HF uint8 (out/4, in)
    if native and arr.dtype == np.uint8:
        # HF packed along out: shape (out/4, in) -> unpack to (out, in)
        if arr.ndim == 2 and arr.shape[0] * 4 == rows and arr.shape[1] == cols:
            q = unpack_hf_oi(arr)
            return q.astype(np.int8), 1.0
        # fallback 1D packed
        if arr.ndim == 1:
            u8 = arr.reshape(rows // 4, cols)
            q = unpack_hf_oi(u8)
            return q.astype(np.int8), 1.0

    # se ja ternario int8 {-1,0,1}
    if arr.dtype == np.int8 or (arr.dtype.kind in "iu" and set(np.unique(arr).tolist()) <= {-1, 0, 1}):
        q = arr.astype(np.int8).reshape(rows, cols)
        return q, 1.0
    # se valores ja ternarios mas em float
    if arr.dtype.kind == "f" and arr.size == rows * cols:
        uniq = set(np.unique(np.round(arr).astype(int).ravel().tolist()))
        if uniq <= {-1, 0, 1} and native:
            return np.round(arr).astype(np.int8).reshape(rows, cols), 1.0

    # denso BF16/f32 -> ternariza
    mat = arr.astype(np.float32)
    # corrige transposicao se shape invertida (ex: safetensors (in, out) vs (out, in))
    if mat.shape == (cols, rows):
        mat = mat.T
    elif mat.shape != (rows, cols):
        # tenta reshape se flat
        if mat.size == rows * cols:
            mat = mat.reshape(rows, cols)
        else:
            print(f"  [WARN] {key} shape {mat.shape} != ({rows},{cols}) -> reshape flat")
            mat = mat.reshape(rows, cols) if mat.size == rows * cols else np.zeros((rows, cols), dtype=np.float32)
    q, scale = absmean_quantize(mat)
    return q, scale


def convert(source: Path, output: Path, native: bool) -> None:
    state, cfg = load_state_dict(source)

    hidden = int(cfg.get("hidden_size", FALCON_HIDDEN))
    num_layers = int(cfg.get("num_hidden_layers", FALCON_LAYERS))
    num_heads = int(cfg.get("num_attention_heads", FALCON_HEADS))
    vocab_size = int(cfg.get("vocab_size", FALCON_VOCAB))
    max_seq = int(cfg.get("max_position_embeddings", FALCON_MAX_SEQ))
    intermediate_size = int(cfg.get("intermediate_size", FALCON_INTERMEDIATE))
    num_kv_heads = int(cfg.get("num_key_value_heads", FALCON_KV_HEADS))
    tie = bool(cfg.get("tie_word_embeddings", FALCON_TIE))
    rope_theta = float(cfg.get("rope_theta", FALCON_ROPE_THETA))

    # Falcon3 q_dim == hidden (3072), head_dim = 256
    head_dim = hidden // num_heads
    q_dim = hidden  # spec: q_dim == hidden (3072)
    k_dim = num_kv_heads * head_dim  # 1024
    ffn_group = intermediate_size * q_dim // hidden  # 9216

    print(f"  hidden={hidden} L={num_layers} heads={num_heads} kv={num_kv_heads}")
    print(f"  q_dim={q_dim} k_dim={k_dim} ffn_group={ffn_group} tie={tie} native={native}")
    print(f"  vocab={vocab_size} intermediate={intermediate_size} theta={rope_theta}")

    # Detecta feat: Falcon nao tem inner/ffn_norm (so rms_attn + rms_ffn)
    has_inner = any("attn_sub_norm" in k or "inner" in k for k in state.keys())
    has_ffn = any("ffn_sub_norm" in k for k in state.keys())
    # conforme task: feat bit0/bit1 conforme BitNet -> se quiser forcar, use has_inner/has_ffn da deteccao
    # default Falcon denso = False/False
    has_theta = True
    feat = compute_feat(has_inner, has_ffn, has_theta)

    num_params = hidden * vocab_size  # embed
    if has_ffn:
        num_params += intermediate_size * num_layers  # rms_ffn_norm
    per_layer = (hidden * q_dim + hidden * k_dim * 2 + q_dim * hidden
                 + hidden * ffn_group * 2 + intermediate_size * q_dim)
    num_params += per_layer * num_layers
    if not tie:
        num_params += hidden * vocab_size  # unembed
    # medusa 0

    output.parent.mkdir(parents=True, exist_ok=True)
    with open(output, "wb") as f:
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
            act_type=ACT_SILU,
            embed_type=EMBED_Q6K,
            feat=feat,
        )
        # embed (vocab, hidden) BF16 -> Q6_K (hidden, vocab) row-major
        emb_key = "model.embed_tokens.weight"
        if emb_key not in state:
            # fallback procura
            for k in state:
                if "embed_tokens" in k:
                    emb_key = k
                    break
        emb = state[emb_key].astype(np.float32)  # (vocab, hidden)
        if emb.shape != (vocab_size, hidden):
            # pode estar transposto
            if emb.shape == (hidden, vocab_size):
                emb = emb.T
        # write_embed espera (hidden, vocab) row-major
        write_embed(f, emb.T, EMBED_Q6K, scale=1.0)
        print(f"  [T] embed {emb.shape} -> Q6_K {((hidden*vocab_size+255)//256)*210} bytes")

        for li in range(num_layers):
            p = f"model.layers.{li}"
            # RMS norms: ordem rms_attn, rms_ffn, rms_inner (se feat&1), rms_ffn_norm (se feat&2)
            for rk in [f"{p}.input_layernorm.weight", f"{p}.post_attention_layernorm.weight"]:
                arr = state.get(rk)
                if arr is None:
                    arr = np.ones(hidden, dtype=np.float32)
                else:
                    arr = arr.astype(np.float32).reshape(-1)
                # garante tamanho hidden
                if arr.size != hidden:
                    arr = np.ones(hidden, dtype=np.float32)
                write_rms(f, arr)
            if has_inner:
                rk = f"{p}.self_attn.attn_sub_norm.weight"
                arr = state.get(rk, np.ones(hidden, dtype=np.float32)).astype(np.float32).reshape(-1)
                write_rms(f, arr)
            if has_ffn:
                rk = f"{p}.mlp.ffn_sub_norm.weight"
                arr = state.get(rk, np.ones(intermediate_size, dtype=np.float32)).astype(np.float32).reshape(-1)
                write_rms(f, arr)

            tensors = [
                (f"{p}.self_attn.q_proj.weight", hidden, q_dim),
                (f"{p}.self_attn.k_proj.weight", hidden, k_dim),
                (f"{p}.self_attn.v_proj.weight", hidden, k_dim),
                (f"{p}.self_attn.o_proj.weight", q_dim, hidden),
                (f"{p}.mlp.gate_proj.weight", hidden, ffn_group),
                (f"{p}.mlp.up_proj.weight", hidden, ffn_group),
                (f"{p}.mlp.down_proj.weight", intermediate_size, q_dim),
            ]
            for key, rows, cols in tensors:
                q, scale = get_proj_tensor(state, key, native, rows, cols)
                write_ternary(f, q.ravel(), scale)

            if li % 5 == 0 or li + 1 == num_layers:
                print(f"  [L] {li}/{num_layers} off={f.tell()//1024}KB")

        # rms_final
        rk = "model.norm.weight"
        arr = state.get(rk, np.ones(hidden, dtype=np.float32)).astype(np.float32).reshape(-1)
        write_rms(f, arr)

        # unembed tied? Falcon3 tie=false -> escreve
        if not tie:
            lk = "lm_head.weight"
            # procura lm_head em state (pode ser model.lm_head.weight)
            lm = state.get("lm_head.weight", state.get("model.lm_head.weight", state.get(lk)))
            if lm is None:
                for k in state:
                    if "lm_head" in k:
                        lm = state[k]
                        break
            if lm is None:
                print("  [WARN] lm_head ausente -> zeros")
                q, scale = absmean_quantize(np.zeros((hidden, vocab_size), dtype=np.float32))
                write_ternary(f, q.ravel(), scale)
            else:
                lm = lm.astype(np.float32)
                if lm.shape == (vocab_size, hidden):
                    # precisa (hidden, vocab) para write_ternary? loader espera (hidden, vocab)
                    q, scale = get_proj_tensor({lk: lm.T}, lk, native, hidden, vocab_size)
                elif lm.shape == (hidden, vocab_size):
                    q, scale = get_proj_tensor({lk: lm}, lk, native, hidden, vocab_size)
                else:
                    q, scale = absmean_quantize(lm.reshape(hidden, vocab_size))
                write_ternary(f, q.ravel(), scale)
        # theta (feat bit2)
        if has_theta:
            f.write(struct.pack("<f", rope_theta))

    sz = os.path.getsize(output)
    print(f"\n[OK] {output}: {sz:,} bytes ({sz/1024/1024:.1f} MB)")
    print(f"  v6: act=SILU embed=Q6_K feat=0x{feat:02x} tie={tie} theta={rope_theta}")


def main():
    ap = argparse.ArgumentParser(description="Falcon3 3B -> .bitnet v6 (dense ou nativo 1.58bit)")
    ap.add_argument("--source", type=Path, default=DEFAULT_SRC, help="dir com config.json + *.safetensors")
    ap.add_argument("--output", type=Path, default=DEFAULT_OUT, help="arquivo .bitnet v6 saida (FALCON3.V6)")
    ap.add_argument("--native", action="store_true", help="entrada ja ternaria (1.58bit): so transpor/unpack, sem re-ternarizar")
    ap.add_argument("--hf-repo", type=str, default=None, help="atalho: baixa via download_falcon3.py e converte")
    args = ap.parse_args()
    src = args.source
    if args.hf_repo:
        # baixa primeiro
        import subprocess
        dl = ROOT / "tools" / "download_falcon3.py"
        variant = "1.58bit" if args.native else "base"
        subprocess.run([sys.executable, str(dl), "--variant", variant], check=True)
        src = DEFAULT_SRC
    convert(src, args.output, args.native)


if __name__ == "__main__":
    main()
