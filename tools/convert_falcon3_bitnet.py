#!/usr/bin/env python3
"""Converte tiiuae/Falcon3-*-1.58bit (safetensors) → .bitnet v5.

Lida com:
  - Pesos BitLinear já em packing U8 (HF format) → unpack + repack no layout nosso
  - Embed/lm_head em BF16 → quantiza ternário com absmean
  - RMS norms em BF16 → f32

Uso:
  python tools/convert_falcon3_bitnet.py --hf-repo tiiuae/Falcon3-7B-Instruct-1.58bit --output target/models/PRO.BIN
  python tools/convert_falcon3_bitnet.py --hf-repo tiiuae/Falcon3-3B-Instruct-1.58bit --output target/models/AGENT.BIN
  
  # Com cache local (evita re-download):
  python tools/convert_falcon3_bitnet.py --hf-repo ... --output ... --cache-dir D:\llm-models
"""

import argparse, json, os, struct, sys, time
from pathlib import Path

import numpy as np
import torch

# ─── .bitnet v5 constants ────────────────────────────────────────────────
MAGIC = 0xBE11BE11
VERSION = 5

# layer_features bitmask
HAS_INNER_ATTN_LN = 0x01
HAS_FFN_LAYERNORM = 0x02
HAS_ROPE = 0x04

# ─── Ternary encoding helpers ────────────────────────────────────────────

def encode_trit(v: int) -> int:
    """0=0, +1=0b01, -1=0b10"""
    if v > 0: return 0b01
    if v < 0: return 0b10
    return 0b00

def decode_trit(bits: int) -> int:
    b = bits & 0b11
    if b == 0b01: return 1
    if b == 0b10: return -1
    return 0

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

def pack_flat_colmajor(weights: np.ndarray) -> tuple[bytes, float]:
    """(out, in) int8 {-1,0,+1} → packed bytes (col-major flat) + scale.
    
    Nosso formato: packing ao longo de toda a matriz em ordem col-major.
    Byte p codifica 4 pesos: (p*4)//in, (p*4+1)//in, ..., com 2 bits cada.
    Scale = 1.0 porque os valores já são ternários.
    """
    out, inn = weights.shape
    # Column-major: transpor para (in, out) e achatar
    cm = np.ascontiguousarray(weights.T)  # (in, out)
    flat = cm.reshape(-1)
    n = flat.size
    # Codificar 4 pesos/byte
    bits = np.zeros(n, dtype=np.uint8)
    bits[flat > 0] = 0b01
    bits[flat < 0] = 0b10
    pad = (-n) % 4
    if pad:
        bits = np.concatenate([bits, np.zeros(pad, dtype=np.uint8)])
    b = bits.reshape(-1, 4)
    packed = b[:, 0] | (b[:, 1] << 2) | (b[:, 2] << 4) | (b[:, 3] << 6)
    return packed.tobytes(), 1.0

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

def pack_and_scale(weights: np.ndarray) -> tuple[bytes, float]:
    """(out, in) → (packed_bytes, scale).
    
    Se os valores já são int8 {-1,0,+1}, não re-quantiza.
    Se são float, aplica absmean quantization.
    """
    if weights.dtype in (np.float32, np.float16, np.float64):
        q, scale = absmean_quantize(weights)
    elif weights.dtype == np.int8 and weights.max() <= 1 and weights.min() >= -1:
        q = weights
        scale = 1.0
    else:
        # Fallback: assume float, tenta converter
        q, scale = absmean_quantize(weights.astype(np.float32))
    packed, s2 = pack_flat_colmajor(q)
    return packed, scale * s2  # combinar escalas

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
    rope_theta = cfg.get("rope_theta", 1000042.0)

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

    # 4. Write .bitnet v5
    print(f"  [WRITE] {output_path}")
    # Calcular num_params total
    num_params = sum(int(np.prod(arr.shape)) for arr in state.values())

    with open(output_path, "wb") as f:
        # ── Header v5 ──
        f.write(struct.pack("<I", MAGIC))
        f.write(struct.pack("<H", VERSION))
        f.write(struct.pack("<Q", num_params))
        f.write(struct.pack("<H", hidden))
        f.write(struct.pack("<H", num_layers))
        f.write(struct.pack("<H", num_heads))
        f.write(struct.pack("<I", vocab_size))
        f.write(struct.pack("<H", min(max_seq, 65535)))
        f.write(struct.pack("<H", intermediate_size))
        f.write(struct.pack("<H", num_kv_heads))
        f.write(struct.pack("<H", q_dim))
        f.write(struct.pack("<I", 0))  # num_medusa = 0
        f.write(b"TIED" if tie_embeddings else b"\x00\x00\x00\x00")
        # Tokenizer: Falcon3 usa BPE HuggingFace
        f.write(struct.pack("B", 1))  # tok_type = BPE
        tok_label = b"FALCON3_BPE"
        f.write(struct.pack("<I", len(tok_label)))
        f.write(tok_label)
        # layer_features: RoPE apenas (sem inner_attn_ln, sem ffn_layernorm)
        layer_features = HAS_ROPE
        f.write(struct.pack("B", layer_features))

        # ── Embed ──
        print(f"  [EMBED] token_embd {embed_arr.shape}")
        if embed_arr.dtype in (np.float16, np.float32, np.float64):
            embed_data, embed_scale = pack_and_scale(embed_arr.astype(np.float32))
        else:
            # Já é int8 → unpack HF primeiro
            oi = unpack_hf_oi(embed_arr)
            embed_data, embed_scale = pack_flat_colmajor(oi)
        f.write(embed_data)
        f.write(struct.pack("<f", embed_scale))
        print(f"         {len(embed_data)//1024} KB  scale={embed_scale:.4f}")

        # ── Layers ──
        for li in range(num_layers):
            if li % 4 == 0 or li + 1 == num_layers:
                print(f"  [LAYER] {li+1}/{num_layers}")
            prefix = f"model.layers.{li}"
            off_start = f.tell()

            # --- RMS norms (BF16 → f32) ---
            for norm_key in ["input_layernorm", "post_attention_layernorm"]:
                key = f"{prefix}.{norm_key}.weight"
                arr = state.get(key)
                if arr is not None:
                    norm = arr.astype(np.float32).reshape(-1)
                else:
                    norm = np.ones(hidden, dtype=np.float32)
                f.write(norm.tobytes())

            # inner_attn_norm (não existe no Falcon3 = Llama padrão)
            f.write(np.ones(hidden, dtype=np.float32).tobytes())
            # ffn_layernorm (não existe)
            f.write(np.ones(intermediate_size, dtype=np.float32).tobytes())

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
                    fake = np.zeros((rows, cols), dtype=np.float32)
                    wdata, wscale = pack_and_scale(fake)
                else:
                    dtype_str = str(arr.dtype)
                    if arr.dtype in (np.float16, np.float32, np.float64):
                        # BF16 → quantiza
                        wdata, wscale = pack_and_scale(arr.astype(np.float32))
                    elif arr.dtype == np.uint8:
                        # Já em packing HF → unpack + repack
                        if arr.ndim == 1:
                            # Flat packed → reshape
                            # Infer shape: col*4 = rows → arr_len * 4 = rows*cols
                            # packed_len = (rows*cols + 3)//4
                            expected_flat = (rows * cols + 3) // 4
                            u8 = arr.reshape(-1, expected_flat // (rows // 4))
                            # u8 shape deve ser (rows//4, cols)
                            # Ou tentar direct unpack
                            print(f"    [WARN] tensor 1D, shape {arr.shape}, tentando unpack")
                            # Assumir que é (out/4, in) com packing ao longo de rows
                            u8 = arr.reshape(rows // 4, cols)
                            oi = unpack_hf_oi(u8)
                            wdata, wscale = pack_flat_colmajor(oi)
                        else:
                            # (out/4, in) uint8
                            oi = unpack_hf_oi(arr)
                            wdata, wscale = pack_flat_colmajor(oi)
                    else:
                        # Fallback
                        wdata, wscale = pack_and_scale(arr.astype(np.float32))
                f.write(wdata)
                f.write(struct.pack("<f", wscale))

            off_layer = f.tell() - off_start
            if li < 3 or li + 1 == num_layers:
                print(f"         {off_layer//1024} KB")

        # ── rms_final ──
        if rms_final_arr is not None:
            rmsf = rms_final_arr.astype(np.float32).reshape(-1)
        else:
            rmsf = np.ones(hidden, dtype=np.float32)
        f.write(rmsf.tobytes())

        # ── Unembed (lm_head) ──
        if not tie_embeddings and lm_head_arr is not None:
            print(f"  [UNEMBED] lm_head {lm_head_arr.shape}")
            if lm_head_arr.dtype in (np.float16, np.float32, np.float64):
                u_data, u_scale = pack_and_scale(lm_head_arr.astype(np.float32))
            else:
                oi = unpack_hf_oi(lm_head_arr)
                u_data, u_scale = pack_flat_colmajor(oi)
            f.write(u_data)
            f.write(struct.pack("<f", u_scale))
            print(f"           {len(u_data)//1024} KB  scale={u_scale:.4f}")
        else:
            print(f"  [UNEMBED] tied ou ausente — zeros")
            expected = (hidden * vocab_size + 3) // 4
            f.write(b'\x00' * expected)
            f.write(struct.pack("<f", 1.0))

    # Stats
    sz = os.path.getsize(output_path)
    print(f"\n  [OK] {output_path}: {sz:,} bytes ({sz/1024/1024:.1f} MB)")
    print(f"       Formato .bitnet v5 — pronto para FAT32/carga")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description="Converte Falcon3-1.58bit → .bitnet v5")
    p.add_argument("--hf-repo", default="tiiuae/Falcon3-3B-Instruct-1.58bit",
                   help="HF repo ID (ex: tiiuae/Falcon3-7B-Instruct-1.58bit)")
    p.add_argument("--output", default="target/models/FALCON3.BIN",
                   help="Caminho do .bitnet de saída")
    p.add_argument("--cache-dir", default=None,
                   help="Diretório de cache HF (opcional, evita re-download)")
    args = p.parse_args()

    convert(args.hf_repo, args.output, args.cache_dir)
