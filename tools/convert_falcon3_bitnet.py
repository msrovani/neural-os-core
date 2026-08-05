#!/usr/bin/env python3
"""Converte tiiuae/Falcon3-*-1.58bit (safetensors) → .bitnet v6 (ADR-0085).

Lida com:
  - Pesos BitLinear já em packing U8 (HF format) → unpack + repack no layout nosso
  - Embed/lm_head em BF16 → quantiza ternário com absmean
  - RMS norms em BF16 → f32

Uso:
  python tools/convert_falcon3_bitnet.py --hf-repo tiiuae/Falcon3-7B-Instruct-1.58bit --output target/models/PRO.BIN
  python tools/convert_falcon3_bitnet.py --hf-repo tiiuae/Falcon3-3B-Instruct-1.58bit --output target/models/AGENT.BIN
  
  # Com cache local (evita re-download):
  python tools/convert_falcon3_bitnet.py --hf-repo ... --output ... --cache-dir D:\\llm-models
"""

import argparse, json, os, struct, sys, time
from pathlib import Path

import numpy as np
import torch

# ─── .bitnet v6 writer (ADR-0085) ────────────────────────────────────────
from tools.bitnet_writer import (
    write_header_v6, write_embed, write_rms, write_ternary, compute_feat,
    MODEL_LLM, ACT_SILU, EMBED_TERNARY,
)

# ─── Ternary encoding helpers ────────────────────────────────────────────

def unpack_hf_oi(u8: np.ndarray) -> np.ndarray:
    """HF (out/4, in) uint8 → (out, in) int8 com valores {-1,0,+1}.
    
    HF empacota 4 pesos ternários por byte ao longo da dimensão output.
    Cada byte em (r, c) codifica pesos para output rows (r*4..r*4+3) 
    na input column c. Encoding: 00=0, 01=+1, 10=-1.
    """
    out4, inn = u8.shape
    flat = u8.astype(np.uint8).reshape(-1)
    codes = np.empty((flat.size, 4), dtype=np.uint8)
    for i in range(4):
        codes[:, i] = (flat >> (2 * i)) & 3
    lut = np.array([0, 1, -1, 0], dtype=np.int8)
    trits = lut[codes]  # (nbytes, 4)
    return trits.reshape(out4, 4, inn).transpose(0, 2, 1).reshape(out4 * 4, inn)

def absmean_quantize(mat_f: np.ndarray) -> tuple[np.ndarray, float]:
    """BF16/f32 → ternary {-1,0,+1} via absmean quantization.
    
    Retorna (int8 array, scale_factor).
    """
    x = mat_f.astype(np.float32)
    abs_mean = float(np.mean(np.abs(x))) + 1e-10
    scale = 1.0 / abs_mean
    q = np.round(x / abs_mean)
    q = np.clip(q, -1, 1).astype(np.int8)
    return q, scale

# ─── SafeTensors loading ─────────────────────────────────────────────────

def read_safetensors(path: str) -> dict:
    from safetensors.torch import load_file
    state = load_file(str(path))
    tensors = {}
    for key, t in state.items():
        if t.dtype == torch.bfloat16:
            tensors[key] = t.float().cpu().numpy()
        elif t.dtype == torch.uint8:
            tensors[key] = t.cpu().numpy().astype(np.uint8)
        else:
            tensors[key] = t.cpu().numpy()
    return tensors

def download_hf(repo_id: str, filename: str, cache_dir: str | None = None) -> str:
    from huggingface_hub import hf_hub_download
    kw = {"repo_id": repo_id, "filename": filename}
    if cache_dir:
        kw["local_dir"] = str(Path(cache_dir) / repo_id.split("/")[-1])
        kw["local_dir_use_symlinks"] = False
    return hf_hub_download(**kw)

# ─── Main converter ──────────────────────────────────────────────────────

def convert(hf_repo: str, output_path: str, cache_dir: str | None = None):
    target_dir = Path(output_path).parent
    target_dir.mkdir(parents=True, exist_ok=True)

    # 1. Download config
    print(f"=== Baixando {hf_repo} ===")
    cfg_path = download_hf(hf_repo, "config.json", cache_dir)
    with open(cfg_path) as f:
        cfg = json.load(f)

    hidden = cfg["hidden_size"]
    num_layers = cfg["num_hidden_layers"]
    num_heads = cfg["num_attention_heads"]
    num_kv_heads = cfg.get("num_key_value_heads", num_heads)
    vocab_size = cfg["vocab_size"]
    max_seq = cfg.get("max_position_embeddings", 32768)
    intermediate_size = cfg["intermediate_size"]
    head_dim = cfg.get("head_dim", hidden // num_heads)
    tie_embeddings = cfg.get("tie_word_embeddings", False)
    rope_theta = cfg.get("rope_theta", 10000.0)  # Falcon3 default

    q_dim = head_dim * num_heads       # 12 * 256 = 3072
    k_dim = head_dim * num_kv_heads    # 4 * 256 = 1024
    ffn_group = intermediate_size * q_dim // hidden  # = intermediate_size qdo q_dim==hidden
    down_out = q_dim

    print(f"\n  Arquitetura: Llama + BitNet 1.58 (Falcon3)")
    print(f"  hidden={hidden} layers={num_layers} heads={num_heads} kv={num_kv_heads}")
    print(f"  head_dim={head_dim} q_dim={q_dim} k_dim={k_dim}")
    print(f"  vocab={vocab_size} max_seq={max_seq}")
    print(f"  intermediate={intermediate_size} ffn_g={ffn_group}")
    print(f"  tie_embeddings={tie_embeddings} rope_theta={rope_theta}")

    # 2. Download model
    print(f"  [DL] model.safetensors...")
    t0 = time.time()
    model_path = download_hf(hf_repo, "model.safetensors", cache_dir)
    dl_time = time.time() - t0
    gb = os.path.getsize(model_path) / 1e9
    print(f"  [DL] OK {model_path} ({gb:.2f} GB, {dl_time:.0f}s)")

    t0 = time.time()
    print(f"  [LOAD] Carregando tensors...")
    state = read_safetensors(str(model_path))
    print(f"  [OK] {len(state)} tensors loaded in {time.time() - t0:.1f}s")

    # Libera o arquivo safetensors do disco AGORA — dados já estão em RAM
    if cache_dir and os.path.exists(model_path):
        safetensors_size = os.path.getsize(model_path)
        os.remove(model_path)
        # Tenta limpar subpastas de cache vazias
        cache_root = Path(cache_dir) / hf_repo.split("/")[-1]
        for p in [cache_root / ".cache" / "huggingface" / "download",
                  cache_root / ".cache" / "huggingface",
                  cache_root / ".cache",
                  cache_root]:
            try:
                if p.exists() and not any(p.iterdir()):
                    p.rmdir()
            except: pass
        print(f"  [CLEAN] removido {model_path} ({safetensors_size/1e9:.2f} GB) — dados em RAM")

    # 3. Index tensors by role
    print(f"  [IDX] Indexando tensores...")
    embed_arr = state.get("model.embed_tokens.weight")
    lm_head_arr = state.get("model.lm_head.weight")
    rms_final_arr = state.get("model.norm.weight")

    # 4. Write .bitnet v6 (ADR-0085)
    print(f"  [WRITE] {output_path}")
    # Calcular num_params total
    num_params = sum(int(np.prod(arr.shape)) for arr in state.values())

    # v6: só rms_attn + rms_ffn por layer (Falcon3 não tem inner/ffn norms);
    # theta (RoPE) sempre no EOF → feat bit2.
    feat = compute_feat(False, False, True)
    tok_label = b"FALCON3_BPE"

    with open(output_path, "wb") as f:
        # ── Header v6 ──
        write_header_v6(
            f, model_type=MODEL_LLM, num_params=num_params,
            hidden=hidden, layers=num_layers, heads=num_heads, vocab=vocab_size,
            max_seq=min(max_seq, 65535), intermediate=intermediate_size,
            kv_heads=num_kv_heads, q_dim=q_dim, medusa=0, tie=tie_embeddings,
            tok_type=1, tok_data=tok_label, act_type=ACT_SILU,
            embed_type=EMBED_TERNARY, feat=feat,
        )

        # ── Embed ──
        print(f"  [EMBED] token_embd {embed_arr.shape}")
        if embed_arr.dtype in (np.float16, np.float32, np.float64):
            embed_i8, embed_scale = absmean_quantize(embed_arr.astype(np.float32))
        else:
            # Já em packing HF → unpack p/ (vocab, hidden) int8
            embed_i8 = unpack_hf_oi(embed_arr)
            embed_scale = 1.0
        # write_embed espera (hidden, vocab) row-major → transpor
        write_embed(f, embed_i8.T, EMBED_TERNARY, embed_scale)
        print(f"         {embed_i8.size // 4 // 1024} KB  scale={embed_scale:.4f}")

        # ── Layers ──
        for li in range(num_layers):
            if li % 4 == 0 or li + 1 == num_layers:
                print(f"  [LAYER] {li+1}/{num_layers}")
            prefix = f"model.layers.{li}"
            off_start = f.tell()

            # --- RMS norms (BF16 → f32): apenas rms_attn + rms_ffn (v6) ---
            for norm_key in ["input_layernorm", "post_attention_layernorm"]:
                key = f"{prefix}.{norm_key}.weight"
                arr = state.get(key)
                if arr is not None:
                    norm = arr.astype(np.float32).reshape(-1)
                else:
                    norm = np.ones(hidden, dtype=np.float32)
                write_rms(f, norm)

            # --- Weight tensors ---
            # Mapeamento: nome HF → (shape_out, shape_in, role)
            proj_map = [
                ("self_attn.q_proj",   hidden, q_dim, "q"),
                ("self_attn.k_proj",   hidden, k_dim, "k"),
                ("self_attn.v_proj",   hidden, k_dim, "v"),
                ("self_attn.o_proj",   q_dim,  hidden, "o"),
                ("mlp.gate_proj",      hidden, ffn_group, "gate"),
                ("mlp.up_proj",        hidden, ffn_group, "up"),
                ("mlp.down_proj",      intermediate_size, down_out, "down"),
            ]

            for hf_name, rows, cols, role in proj_map:
                key = f"{prefix}.{hf_name}.weight"
                arr = state.get(key)
                if arr is None:
                    print(f"    [WARN] layer {li} missing {key} — zeros")
                    q, wscale = absmean_quantize(np.zeros((rows, cols), dtype=np.float32))
                elif arr.dtype in (np.float16, np.float32, np.float64):
                    # BF16 → quantiza ternário
                    q, wscale = absmean_quantize(arr.astype(np.float32))
                elif arr.dtype == np.uint8:
                    # Já em packing HF → unpack (out/4, in) → (out, in) int8
                    if arr.ndim == 1:
                        u8 = arr.reshape(rows // 4, cols)
                    else:
                        u8 = arr
                    q = unpack_hf_oi(u8)
                    wscale = 1.0
                else:
                    # Fallback
                    q, wscale = absmean_quantize(arr.astype(np.float32))
                write_ternary(f, q, wscale)

            off_layer = f.tell() - off_start
            if li < 3 or li + 1 == num_layers:
                print(f"         {off_layer//1024} KB")

        # ── rms_final ──
        if rms_final_arr is not None:
            rmsf = rms_final_arr.astype(np.float32).reshape(-1)
        else:
            rmsf = np.ones(hidden, dtype=np.float32)
        write_rms(f, rmsf)

        # ── Unembed (D3: tied → nenhum byte) ──
        if not tie_embeddings:
            if lm_head_arr is not None:
                print(f"  [UNEMBED] lm_head {lm_head_arr.shape}")
                if lm_head_arr.dtype in (np.float16, np.float32, np.float64):
                    q, u_scale = absmean_quantize(lm_head_arr.astype(np.float32))
                else:
                    q = unpack_hf_oi(lm_head_arr)
                    u_scale = 1.0
                write_ternary(f, q, u_scale)
                print(f"           {q.size // 4 // 1024} KB  scale={u_scale:.4f}")
            else:
                print(f"  [UNEMBED] lm_head ausente — zeros")
                q, u_scale = absmean_quantize(np.zeros((hidden, vocab_size), dtype=np.float32))
                write_ternary(f, q, u_scale)
        else:
            print(f"  [UNEMBED] tied — sem bytes de unembed (D3)")

        # ── Theta (RoPE) — feat bit2 — sempre no EOF ──
        f.write(struct.pack("<f", rope_theta))

    # Stats
    sz = os.path.getsize(output_path)
    print(f"\n  [OK] {output_path}: {sz:,} bytes ({sz/1024/1024:.1f} MB)")
    print(f"       Formato .bitnet v6 (ADR-0085) — pronto para FAT32/carga")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description="Converte Falcon3-1.58bit → .bitnet v6 (ADR-0085)")
    p.add_argument("--hf-repo", default="tiiuae/Falcon3-3B-Instruct-1.58bit",
                   help="HF repo ID (ex: tiiuae/Falcon3-7B-Instruct-1.58bit)")
    p.add_argument("--output", default="target/models/FALCON3.BIN",
                   help="Caminho do .bitnet de saída")
    p.add_argument("--cache-dir", default=None,
                   help="Diretório de cache HF (opcional, evita re-download)")
    args = p.parse_args()

    convert(args.hf_repo, args.output, args.cache_dir)
