#!/usr/bin/env python3
"""Converte HF1BitLLM/Llama3-8B-1.58-100B-tokens (safetensors) → .bitnet v5 para slot Pro.

Uso:
  python tools/convert_llama3_8b_bitnet.py
      --output target/models/BITNETLLAMA8B.BIN

Requer: pip install safetensors numpy huggingface-hub torch
"""

import argparse, json, os, struct, sys, tempfile, time
from pathlib import Path

import numpy as np

# ─── .bitnet v5 format constants ────────────────────────────────────────
MAGIC = 0xBE11BE11
VERSION = 5
# layer_features bitmask
HAS_INNER_ATTN_LN = 0x01
HAS_FFN_LAYERNORM = 0x02
HAS_ROPE = 0x04

# ─── Mapping: safetensors name → .bitnet role ───────────────────────────
# Tensores de peso (ternários) que entram no .bitnet
# Formato: (nome_safetensors, shape_in, shape_out, role_name)
LAYER_WEIGHT_KEYS = [
    ("self_attn.q_proj",   "hidden", "q_dim",    "q"),
    ("self_attn.k_proj",   "hidden", "k_dim",    "k"),
    ("self_attn.v_proj",   "hidden", "k_dim",    "v"),
    ("self_attn.o_proj",   "q_dim",  "hidden",   "o"),
    ("mlp.gate_proj",      "hidden", "ffn_g",    "gate"),
    ("mlp.up_proj",        "hidden", "ffn_g",    "up"),
    ("mlp.down_proj",      "ffn",    "hidden",   "down"),
]

LAYER_NORM_KEYS = [
    ("input_layernorm",          "rms_attn"),
    ("post_attention_layernorm", "rms_ffn"),
]

# ─── helpers ────────────────────────────────────────────────────────────

def quantize_ternary(arr: np.ndarray) -> tuple[bytes, float]:
    """Quantiza array float32 {-1,0,+1} com scale, packing 2-bit (4 weights/byte).
    
    Usa absmean quantization: threshold = 0.7 * mean(|w|).
    """
    flat = arr.reshape(-1)
    abs_mean = float(np.mean(np.abs(flat)))
    if abs_mean < 1e-10:
        scale = 1.0
        packed = b'\x00' * ((len(flat) + 3) // 4)
        return packed, scale

    threshold = abs_mean * 0.7
    scale = 1.0 / threshold

    packed = bytearray()
    for i in range(0, len(flat), 4):
        byte = 0
        for j in range(4):
            if i + j < len(flat):
                v = float(flat[i + j])
                if v > threshold:
                    bits = 0b01  # +1
                elif v < -threshold:
                    bits = 0b10  # -1
                else:
                    bits = 0b00  # 0
                byte |= bits << (j * 2)
        packed.append(byte)
    return bytes(packed), scale


def pack_as_is(arr: np.ndarray) -> bytes:
    """Para tensores que já são packing 2-bit (int8, shape com /4).
    Se o tensor já veio quantizado, extrai direto sem re-quantizar.
    """
    flat = arr.reshape(-1)
    if max(flat) <= 127.0 and min(flat) >= -128.0:
        # Já é int8 → packing direto
        i8 = flat.astype(np.int8)
        # Se o tensor é grande e já tem packing 2-bit, o conteúdo
        # deve ser lido byte a byte
        return i8.tobytes()
    # Float → quantiza
    packed, _ = quantize_ternary(arr)
    return packed


def read_safetensors(path: str) -> dict:
    from safetensors import safe_open
    tensors = {}
    with safe_open(path, framework="np") as f:
        for key in f.keys():
            tensors[key] = f.get_tensor(key)
    return tensors


def download_hf(repo_id: str, filename: str, dest: Path):
    from huggingface_hub import hf_hub_download
    print(f"  [DL] {filename}")
    return hf_hub_download(repo_id=repo_id, filename=filename, local_dir=str(dest.parent))


# ─── main converter ─────────────────────────────────────────────────────

def convert(hf_repo: str, output_path: str, skip_download: bool = False):
    target_dir = Path(output_path).parent
    target_dir.mkdir(parents=True, exist_ok=True)

    # 1. Download config
    print(f"=== Baixando {hf_repo} ===")
    tmp_config = Path(tempfile.mktemp(suffix=".json"))
    if skip_download:
        print("  [SKIP] usando arquivos locais (assume model.safetensors + config.json no CWD)")
        cfg_path = Path("config.json")
    else:
        download_hf(hf_repo, "config.json", tmp_config)
        cfg_path = tmp_config

    with open(cfg_path) as f:
        cfg = json.load(f)

    hidden = cfg["hidden_size"]          # 4096
    num_layers = cfg["num_hidden_layers"] # 32
    num_heads = cfg["num_attention_heads"] # 32
    num_kv_heads = cfg.get("num_key_value_heads", num_heads)  # 8
    vocab_size = cfg["vocab_size"]        # 128256
    max_seq = cfg.get("max_position_embeddings", 8192)
    intermediate_size = cfg["intermediate_size"]  # 14336
    head_dim = cfg.get("head_dim", hidden // num_heads)  # 128
    q_dim = head_dim * num_heads           # 4096
    k_dim = head_dim * num_kv_heads        # 1024
    tie_embeddings = cfg.get("tie_word_embeddings", False)
    ffn_g = intermediate_size * q_dim // hidden  # 14336
    rope_theta = cfg.get("rope_theta", 500000.0)

    print(f"\n  Arquitetura: Llama + BitNet 1.58")
    print(f"  hidden={hidden} layers={num_layers} heads={num_heads} kv={num_kv_heads}")
    print(f"  vocab={vocab_size} max_seq={max_seq} ffn={intermediate_size}")
    print(f"  q_dim={q_dim} k_dim={k_dim} ffn_g={ffn_g}")
    print(f"  tie_embeddings={tie_embeddings} rope_theta={rope_theta}")

    # 2. Download model
    tmp_model = Path(tempfile.mktemp(suffix=".safetensors"))
    if skip_download:
        model_path = Path("model.safetensors")
    else:
        download_hf(hf_repo, "model.safetensors", tmp_model)
        model_path = tmp_model

    print(f"  [LOAD] {model_path} ({os.path.getsize(model_path)/1e9:.2f} GB)")
    t0 = time.time()
    state = read_safetensors(str(model_path))
    elapsed = time.time() - t0
    print(f"  [OK] {len(state)} tensors loaded in {elapsed:.1f}s")

    # 3. Build tensor index
    print("  [IDX] Indexando tensores...")
    t_idx = {name: arr for name, arr in state.items()}
    # Normalizar nomes
    norm_names = {}
    for name in t_idx:
        # model.layers.0.self_attn.q_proj.weight → layers[0].self_attn.q_proj
        if name.startswith("model.layers."):
            parts = name.split(".")
            li = int(parts[2])
            remainder = ".".join(parts[3:-1])  # remove ".weight"
            norm_names.setdefault(li, {})[remainder] = t_idx[name]
        elif "embed_tokens" in name and "weight" in name:
            norm_names["embed"] = t_idx[name]
        elif "lm_head" in name and "weight" in name:
            norm_names["lm_head"] = t_idx[name]
        elif "norm.weight" in name:
            norm_names["rms_final"] = t_idx[name]

    if "embed" not in norm_names:
        print("  [ERR] embed_tokens.weight not found!")
        tensors = [k for k in state.keys() if "embed" in k.lower() or "tok" in k.lower()]
        print(f"  Candidates: {tensors[:5]}")
        sys.exit(1)

    # 4. Write .bitnet v5
    print(f"  [WRITE] {output_path}")
    with open(output_path, "wb") as f:
        # ── Header ──
        num_params = sum(int(np.prod(arr.shape)) for arr in t_idx.values())
        f.write(struct.pack("<I", MAGIC))
        f.write(struct.pack("<H", VERSION))
        f.write(struct.pack("<Q", num_params))  # v5: u64
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
        f.write(struct.pack("B", 1))  # tok_type = 1 (BPE)
        # Tokenizer data (placeholder — Llama 3 BPE)
        tok_data = b"LLAMA3_BPE"
        f.write(struct.pack("<I", len(tok_data)))
        f.write(tok_data)
        layer_features = HAS_ROPE  # bit 2 = RoPE
        f.write(struct.pack("B", layer_features))

        # ── embed ──
        print("  [EMBED] token_embd")
        embed_arr = norm_names["embed"]  # (vocab_size, hidden)
        embed_data, embed_scale = quantize_ternary(embed_arr)
        f.write(embed_data)
        f.write(struct.pack("<f", embed_scale))

        # ── layers ──
        for li in range(num_layers):
            if li % 4 == 0 or li + 1 == num_layers:
                print(f"  [LAYER] {li+1}/{num_layers}")
            l = norm_names.get(li, {})
            off_start = f.tell()

            # RMS norms
            for safe_key, role in LAYER_NORM_KEYS:
                arr = l.get(safe_key)
                if arr is not None:
                    norm = arr.reshape(-1).astype(np.float32)
                else:
                    norm = np.ones(hidden, dtype=np.float32)
                f.write(norm.tobytes())

            # inner_attn_norm (não existe no Llama = RMS per-head pré-Q/K não usado)
            # Escreve ones(hidden) para compatibilidade com loader
            f.write(np.ones(hidden, dtype=np.float32).tobytes())
            # ffn_layernorm (não existe no Llama)
            f.write(np.ones(intermediate_size, dtype=np.float32).tobytes())

            # Weight tensors
            for safe_key, sin, sout, role in LAYER_WEIGHT_KEYS:
                arr = l.get(safe_key)
                if arr is None:
                    print(f"  [WARN] layer {li} missing {safe_key} — using zeros")
                    if sin == "hidden" and sout == "q_dim":
                        shape = (hidden, q_dim)
                    elif sin == "hidden" and sout == "k_dim":
                        shape = (hidden, k_dim)
                    elif sin == "q_dim" and sout == "hidden":
                        shape = (q_dim, hidden)
                    elif sin == "hidden" and sout == "ffn_g":
                        shape = (hidden, ffn_g)
                    elif sin == "ffn" and sout == "hidden":
                        shape = (intermediate_size, hidden)
                    else:
                        shape = (hidden, hidden)
                    fake = np.zeros(shape, dtype=np.float32)
                    wdata, wscale = quantize_ternary(fake)
                else:
                    wdata, wscale = quantize_ternary(arr)
                f.write(wdata)
                f.write(struct.pack("<f", wscale))

            off_layer = f.tell() - off_start
            if li < 3 or li + 1 == num_layers:
                print(f"         {off_layer//1024} KB")

        # ── rms_final ──
        if "rms_final" in norm_names:
            rmsf = norm_names["rms_final"].reshape(-1).astype(np.float32)
        else:
            rmsf = np.ones(hidden, dtype=np.float32)
        f.write(rmsf.tobytes())

        # ── unembed (lm_head) ──
        if not tie_embeddings and "lm_head" in norm_names:
            print("  [UNEMBED] lm_head")
            unembed_arr = norm_names["lm_head"]
            u_data, u_scale = quantize_ternary(unembed_arr)
            f.write(u_data)
            f.write(struct.pack("<f", u_scale))
        else:
            print("  [UNEMBED] tied — skipped")
            # Escreve zero para indicar tied (loader detecta tudo zero)
            expected = (hidden * vocab_size + 3) // 4
            f.write(b'\x00' * expected)
            f.write(struct.pack("<f", 1.0))

    # Cleanup
    if not skip_download:
        tmp_config.unlink(missing_ok=True)
        tmp_model.unlink(missing_ok=True)

    sz = os.path.getsize(output_path)
    print(f"\n  [OK] {output_path}: {sz:,} bytes ({sz/1024/1024:.1f} MB)")
    print(f"       Arquivo para slot GeneratorPro (Pro)")
    print(f"       Use: Dump no pendrive FAT32 como BITNETLLAMA8B.BIN")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description="Converte Llama3-8B-1.58 → .bitnet v5")
    p.add_argument("--output", default="target/models/BITNETLLAMA8B.BIN")
    p.add_argument("--hf-repo", default="HF1BitLLM/Llama3-8B-1.58-100B-tokens")
    p.add_argument("--skip-download", action="store_true",
                   help="Usa model.safetensors + config.json do diretório atual")
    args = p.parse_args()

    convert(args.hf_repo, args.output, args.skip_download)
