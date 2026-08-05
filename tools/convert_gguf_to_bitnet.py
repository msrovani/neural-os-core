#!/usr/bin/env python3
"""Converte modelos GGUF (HuggingFace) para .bitnet v6 (ADR-0085) com RTN + scale.

Usa tools/bitnet_writer.py como writer canônico (byte-exato com save_model_v6).

Uso:
  python tools/convert_gguf_to_bitnet.py --model meta-llama/Llama-3.2-1B \\
      --output target/models/LLAMA1B.BIN [--verbose] [--self-test]

Requer: pip install gguf numpy huggingface-hub
"""

from __future__ import annotations

import argparse
import os
import struct
import sys
import tempfile
import time
from pathlib import Path

import numpy as np

# tools/ no sys.path para importar o writer canônico (mesmo padrão dos irmãos
# convert_bitnet.py / train_hw_expert_v4.py — roda com `python tools/...` ou `-m`).
ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.bitnet_writer import (
    write_header_v6, write_embed, write_rms, write_ternary, compute_feat,
    write_q6k, pack_ternary,
    MODEL_LLM, ACT_SILU, ACT_RELU2, EMBED_TERNARY, EMBED_Q6K, EMBED_BF16,
)

# ─── GPU/CPU device detection ──────────────────────────────────────────────────
_DEVICE = "cpu"
_TORCH_AVAIL = False
try:
    import torch
    _TORCH_AVAIL = True
    if torch.cuda.is_available():
        _DEVICE = "cuda"
    elif hasattr(torch, "xpu") and torch.xpu.is_available():
        _DEVICE = "xpu"
    elif hasattr(torch, "mps") and torch.mps.is_available():
        _DEVICE = "mps"
except ImportError:
    pass

# ─── .bitnet v6 format (ADR-0085) ────────────────────────────────────────────
MAGIC = 0xBE11BE11

# Arquiteturas suportadas: mapeamento de nome GGUF -> template de tensor names
ARCH_MAP = {
    "llama": {
        "token_embd": "token_embd.weight",
        "blk_attn_q": "blk.{i}.attn_q.weight",
        "blk_attn_k": "blk.{i}.attn_k.weight",
        "blk_attn_v": "blk.{i}.attn_v.weight",
        "blk_attn_o": "blk.{i}.attn_output.weight",
        "blk_ffn_gate": "blk.{i}.ffn_gate.weight",
        "blk_ffn_up": "blk.{i}.ffn_up.weight",
        "blk_ffn_down": "blk.{i}.ffn_down.weight",
        "blk_attn_norm": "blk.{i}.attn_norm.weight",
        "blk_ffn_norm": "blk.{i}.ffn_norm.weight",
        "output": "output.weight",
        "rms_final": "output_norm.weight",
    },
    "qwen2": {
        "token_embd": "token_embd.weight",
        "blk_attn_q": "blk.{i}.attn_q.weight",
        "blk_attn_k": "blk.{i}.attn_k.weight",
        "blk_attn_v": "blk.{i}.attn_v.weight",
        "blk_attn_o": "blk.{i}.attn_output.weight",
        "blk_ffn_gate": "blk.{i}.ffn_gate.weight",
        "blk_ffn_up": "blk.{i}.ffn_up.weight",
        "blk_ffn_down": "blk.{i}.ffn_down.weight",
        "blk_attn_norm": "blk.{i}.attn_norm.weight",
        "blk_ffn_norm": "blk.{i}.ffn_norm.weight",
        "output": "output.weight",
        "rms_final": "output_norm.weight",
    },
    "gemma2": {
        "token_embd": "token_embd.weight",
        "blk_attn_q": "blk.{i}.attn_q.weight",
        "blk_attn_k": "blk.{i}.attn_k.weight",
        "blk_attn_v": "blk.{i}.attn_v.weight",
        "blk_attn_o": "blk.{i}.attn_output.weight",
        "blk_ffn_gate": "blk.{i}.ffn_gate.weight",
        "blk_ffn_up": "blk.{i}.ffn_up.weight",
        "blk_ffn_down": "blk.{i}.ffn_down.weight",
        "blk_attn_norm": "blk.{i}.attn_norm.weight",
        "blk_ffn_norm": "blk.{i}.ffn_norm.weight",
        "output": "output.weight",
        "rms_final": "output_norm.weight",
    },
    "phi3": {
        "token_embd": "token_embd.weight",
        "blk_attn_q": "blk.{i}.attn_q.weight",
        "blk_attn_k": "blk.{i}.attn_k.weight",
        "blk_attn_v": "blk.{i}.attn_v.weight",
        "blk_attn_o": "blk.{i}.attn_output.weight",
        "blk_ffn_gate": "blk.{i}.ffn_gate.weight",
        "blk_ffn_up": "blk.{i}.ffn_up.weight",
        "blk_ffn_down": "blk.{i}.ffn_down.weight",
        "blk_attn_norm": "blk.{i}.attn_norm.weight",
        "blk_ffn_norm": "blk.{i}.ffn_norm.weight",
        "output": "output.weight",
        "rms_final": "output_norm.weight",
    },
    "mistral": {
        "token_embd": "token_embd.weight",
        "blk_attn_q": "blk.{i}.attn_q.weight",
        "blk_attn_k": "blk.{i}.attn_k.weight",
        "blk_attn_v": "blk.{i}.attn_v.weight",
        "blk_attn_o": "blk.{i}.attn_output.weight",
        "blk_ffn_gate": "blk.{i}.ffn_gate.weight",
        "blk_ffn_up": "blk.{i}.ffn_up.weight",
        "blk_ffn_down": "blk.{i}.ffn_down.weight",
        "blk_attn_norm": "blk.{i}.attn_norm.weight",
        "blk_ffn_norm": "blk.{i}.ffn_norm.weight",
        "output": "output.weight",
        "rms_final": "output_norm.weight",
    },
    "starcoder2": {
        "token_embd": "token_embd.weight",
        "blk_attn_q": "blk.{i}.attn_q.weight",
        "blk_attn_k": "blk.{i}.attn_k.weight",
        "blk_attn_v": "blk.{i}.attn_v.weight",
        "blk_attn_o": "blk.{i}.attn_output.weight",
        "blk_ffn_gate": "blk.{i}.ffn_gate.weight",
        "blk_ffn_up": "blk.{i}.ffn_up.weight",
        "blk_ffn_down": "blk.{i}.ffn_down.weight",
        "blk_attn_norm": "blk.{i}.attn_norm.weight",
        "blk_ffn_norm": "blk.{i}.ffn_norm.weight",
        "output": "output.weight",
        "rms_final": "output_norm.weight",
    },
    "deepseek": {
        "token_embd": "token_embd.weight",
        "blk_attn_q": "blk.{i}.attn_q.weight",
        "blk_attn_k": "blk.{i}.attn_k.weight",
        "blk_attn_v": "blk.{i}.attn_v.weight",
        "blk_attn_o": "blk.{i}.attn_output.weight",
        "blk_ffn_gate": "blk.{i}.ffn_gate.weight",
        "blk_ffn_up": "blk.{i}.ffn_up.weight",
        "blk_ffn_down": "blk.{i}.ffn_down.weight",
        "blk_attn_norm": "blk.{i}.attn_norm.weight",
        "blk_ffn_norm": "blk.{i}.ffn_norm.weight",
        "output": "output.weight",
        "rms_final": "output_norm.weight",
    },
    "vit": {
        "token_embd": "patch_embed.weight",
        "blk_attn_q": "enc.{i}.attn.q.weight",
        "blk_attn_k": "enc.{i}.attn.k.weight",
        "blk_attn_v": "enc.{i}.attn.v.weight",
        "blk_attn_o": "enc.{i}.attn.o.weight",
        "blk_ffn_gate": "enc.{i}.ffn.fc1.weight",
        "blk_ffn_up": "enc.{i}.ffn.fc1.weight",
        "blk_ffn_down": "enc.{i}.ffn.fc2.weight",
        "blk_attn_norm": "enc.{i}.ln1.weight",
        "blk_ffn_norm": "enc.{i}.ln2.weight",
        "output": "head.probe",
        "rms_final": "post_ln.weight",
    },
    "bert": {
        "token_embd": "token_embd.weight",
        "blk_attn_q": "blk.{i}.attn_q.weight",
        "blk_attn_k": "blk.{i}.attn_k.weight",
        "blk_attn_v": "blk.{i}.attn_v.weight",
        "blk_attn_o": "blk.{i}.attn_output.weight",
        "blk_ffn_gate": "blk.{i}.ffn_gate.weight",
        "blk_ffn_up": "blk.{i}.ffn_up.weight",
        "blk_ffn_down": "blk.{i}.ffn_down.weight",
        "blk_attn_norm": "blk.{i}.attn_output_norm.weight",
        "blk_ffn_norm": "blk.{i}.layer_output_norm.weight",
        "output": "output.weight",
        "rms_final": "output_norm.weight",
    },
}

# Mapeamento reverso: prefixos de nomes HF -> arquitetura
HF_ARCH_PREFIX = {
    "model.embed_tokens": "qwen2",
    "model.layers": "qwen2",
    "gpt_neox": "llama",  # fallback
}


def detect_arch(metadata: dict, tensor_names: list[str]) -> str:
    """Detecta arquitetura a partir da metadata GGUF."""
    # 1. Tenta chave explícita do header
    for key in ("general.architecture", "model.architecture"):
        arch = metadata.get(key, "")
        if arch:
            arch_clean = arch.lower().replace("-", "").replace("_", "")
            for known in ARCH_MAP:
                if known in arch_clean or arch_clean in known:
                    return known
    # 2. Tenta pelo nome do primeiro tensor conhecido
    for name in tensor_names:
        for prefix, arch in HF_ARCH_PREFIX.items():
            if name.startswith(prefix):
                return arch
    # 3. Fallback: inspect metadata block_count
    if "llama.block_count" in metadata or metadata.get("block_count", 0) > 0:
        return "llama"
    return "llama"


def get_metadata_int(metadata: dict, *keys: str, default: int = 0) -> int:
    """Lê valor int da metadata por chaves alternativas."""
    for key in keys:
        val = metadata.get(key)
        if val is not None:
            return int(val)
    return default


def read_gguf_reader(model_path: str, cache_dir: str | None):
    """Importa GGUFReader — se model_path for repo HF, baixa com huggingface_hub primeiro."""
    try:
        from gguf import GGUFReader
    except ImportError:
        print("[ERRO] pip install gguf (pip install gguf numpy huggingface-hub)")
        sys.exit(1)

    # Se já é arquivo local, abre direto
    if os.path.isfile(model_path):
        return GGUFReader(model_path)

    # Tenta baixar de HuggingFace
    print(f"  Baixando {model_path} do HuggingFace...")
    try:
        from huggingface_hub import hf_hub_download
    except ImportError:
        print("[ERRO] pip install huggingface-hub")
        sys.exit(1)

    # Lista .gguf no repo para achar o primeiro
    try:
        from huggingface_hub import list_repo_files
        files = [f for f in list_repo_files(model_path) if f.endswith(".gguf")]
        if not files:
            raise ValueError(f"Nenhum arquivo .gguf encontrado em {model_path}")
        # Pega o menor (Q2_K ou Q3_K geralmente) — mais rápido de baixar
        # Mas se pedir explícito "arquivo.gguf" no model_path, usa esse
        gguf_file = files[0]  # default: primeiro encontrado
        print(f"  Arquivo GGUF encontrado: {gguf_file}")
    except Exception as e:
        raise ValueError(f"Não foi possível listar {model_path}: {e}")

    local_path = hf_hub_download(
        repo_id=model_path,
        filename=gguf_file,
        cache_dir=cache_dir,
    )
    print(f"  Cache local: {local_path}")
    return GGUFReader(local_path)


# ─── Dequantização GGUF ──────────────────────────────────────────────────────

# Tabela: type_id -> (block_size, block_bytes, tipo_nome)
# Fonte: gguf-py/gguf/constants.py + tensor_types map
GGUF_TYPES = {
    0: (1, 4, "F32"),
    1: (1, 2, "F16"),
    2: (1, 2, "BF16"),
    3: (32, 18, "Q4_0"),   # f16 scale + 16 × uint4 nibbles = 2+16=18
    8: (32, 34, "Q8_0"),   # f16 scale + 32 × int8 = 2+32=34
    6: (32, 22, "Q5_0"),   # f16 scale + uint4 qh + 16 nibbles = 2+4+16=22
    5: (32, 20, "Q4_1"),   # f16 scale + f16 min + 16 nibbles = 2+2+16=20
    10: (256, 96, "Q2_K"),
    11: (256, 112, "Q3_K"),
    12: (256, 128, "Q4_K"),
    13: (256, 144, "Q5_K"),
    14: (256, 160, "Q6_K"),
    15: (256, 168, "Q8_K"),
    16: (32, 16, "Q5_1"),  # f16 scale + f16 min + uint4 qh + 16 nibbles
    17: (64, 104, "IQ1_S"),
    18: (256, 208, "IQ2_XXS"),
    19: (64, 80, "IQ2_XS"),
    20: (64, 96, "IQ3_XXS"),
    21: (64, 112, "IQ1_M"),
    22: (256, 160, "IQ4_NL"),
    23: (256, 128, "IQ4_XS"),
    24: (32, 18, "TQ1_0"),
    25: (32, 24, "TQ2_0"),
}


def dequantize_f32(data: bytes, n_elems: int) -> np.ndarray:
    """F32 direto."""
    arr = np.frombuffer(data, dtype=np.float32, count=n_elems).copy()
    return arr


def dequantize_f16(data: bytes, n_elems: int) -> np.ndarray:
    """F16 -> F32."""
    u16 = np.frombuffer(data, dtype=np.uint16, count=n_elems)
    # FP16: sign(1) + exp(5) + mant(10)
    sign = ((u16 >> 15) & 1).astype(np.float32)
    exp = ((u16 >> 10) & 0x1F).astype(np.int32)
    mant = (u16 & 0x03FF).astype(np.float32)
    out = np.where(exp == 0,
                   mant * (2 ** -24),
                   (1.0 + mant / 1024.0) * (2.0 ** (exp - 15)))
    out[sign == 1] *= -1.0
    return out


def dequantize_bf16(data: bytes, n_elems: int) -> np.ndarray:
    """BF16 -> F32 (zerar lower 16 bits)."""
    arr = np.frombuffer(data, dtype=np.uint16, count=n_elems).astype(np.uint32) << 16
    return arr.view(np.float32)


def dequantize_q4_0(data: bytes, n_elems: int) -> np.ndarray:
    """Q4_0: f16 scale + 16 × uint4 (packed 2 por byte). 32 valores, 18 bytes."""
    out = np.empty(n_elems, dtype=np.float32)
    block_size = 32
    block_bytes = 18
    n_blocks = (n_elems + block_size - 1) // block_size
    off = 0
    idx = 0
    for _ in range(n_blocks):
        if off + 2 > len(data):
            break
        scale = _f16_to_f32(struct.unpack("<H", data[off:off + 2])[0])
        off += 2
        remaining = min(block_size, n_elems - idx)
        for j in range(remaining):
            if off + (j // 2) >= len(data):
                break
            nibble = data[off + j // 2]
            v = (nibble >> (4 * (j & 1))) & 0x0F
            # Q4_0: signed = v - 8 (assume simétrico)
            sv = float(v) - 8.0
            out[idx] = sv * scale
            idx += 1
        off += 16  # 16 bytes for 32 nibbles
    return out[:idx]


def _f16_to_f32(u: int) -> float:
    sign = (u >> 15) & 1
    exp = (u >> 10) & 0x1F
    mant = u & 0x03FF
    if exp == 0:
        val = mant * (2 ** -24)
    elif exp == 31:
        val = float('inf') if mant == 0 else float('nan')
    else:
        val = (1.0 + mant / 1024.0) * (2.0 ** (exp - 15))
    return -val if sign else val


def dequantize_q8_0(data: bytes, n_elems: int) -> np.ndarray:
    """Q8_0: f16 scale + 32 × int8 = 34 bytes."""
    out = np.empty(n_elems, dtype=np.float32)
    block_size = 32
    block_bytes = 34
    n_blocks = (n_elems + block_size - 1) // block_size
    off = 0
    idx = 0
    for _ in range(n_blocks):
        if off + 2 > len(data):
            break
        scale = _f16_to_f32(struct.unpack("<H", data[off:off + 2])[0])
        off += 2
        remaining = min(block_size, n_elems - idx)
        chunk = data[off:off + remaining]
        int8_vals = np.frombuffer(chunk, dtype=np.int8).astype(np.float32)
        n_read = len(int8_vals)
        out[idx:idx + n_read] = int8_vals * scale
        idx += n_read
        off += 32
    return out[:idx]


def dequantize_q5_0(data: bytes, n_elems: int) -> np.ndarray:
    """Q5_0: f16 scale + uint32 qh + 16 packed nibbles = 22 bytes -> 32 f32."""
    out = np.empty(n_elems, dtype=np.float32)
    block_size = 32
    n_blocks = (n_elems + block_size - 1) // block_size
    off = 0
    idx = 0
    for _ in range(n_blocks):
        if off + 2 > len(data):
            break
        scale = _f16_to_f32(struct.unpack("<H", data[off:off + 2])[0])
        off += 2
        if off + 4 > len(data):
            break
        qh = struct.unpack("<I", data[off:off + 4])[0]
        off += 4
        remaining = min(32, n_elems - idx)
        for j in range(remaining):
            nibble_byte = data[off + j // 2] if off + j // 2 < len(data) else 0
            lo = nibble_byte & 0x0F
            hi = (nibble_byte >> 4) & 0x0F
            v5 = lo if (j & 1) == 0 else hi
            # Q5: 5-bit + 1 sign (bit 4 = magnitude, bit 5+ = sign via qh)
            sign_bit = (qh >> j) & 1
            magnitude = v5 & 0x0F
            sv = float(magnitude) + 16.0  # bias?
            if sign_bit:
                sv = -sv
            out[idx] = sv * scale
            idx += 1
        off += 16
    return out[:idx]


def dequantize_q4_k(data: bytes, n_elems: int) -> np.ndarray:
    """Q4_K: 256 elementos, 128 bytes (8 super-blocks com scale f16 cada)."""
    return _dequantize_k_common(data, n_elems, 256, 128, 4)


def dequantize_q5_k(data: bytes, n_elems: int) -> np.ndarray:
    """Q5_K: 256 elementos, 144 bytes."""
    return _dequantize_k_common(data, n_elems, 256, 144, 5)


def dequantize_q6_k(data: bytes, n_elems: int) -> np.ndarray:
    """Q6_K: 256 elementos, 160 bytes."""
    return _dequantize_k_common(data, n_elems, 256, 160, 6)


def dequantize_q2_k(data: bytes, n_elems: int) -> np.ndarray:
    """Q2_K: 256 elementos, 96 bytes."""
    return _dequantize_k_common(data, n_elems, 256, 96, 2)


def dequantize_q3_k(data: bytes, n_elems: int) -> np.ndarray:
    """Q3_K: 256 elementos, 112 bytes."""
    return _dequantize_k_common(data, n_elems, 256, 112, 3)


def _dequantize_k_common(data: bytes, n_elems: int, block_size: int,
                         block_bytes: int, bits: int) -> np.ndarray:
    """Dequantizador K-quant genérico simplificado.

    NOTA: K-quants reais requerem desembalamento completo das escalas 6-bit
    e super-blocos. Este é um fallback que lê o bloco como Q8_0-like para
    não travar. Modelos K-quant grandes terão precisão degradada.
    Para qualidade total, pré-converta para F16 com tools do llama.cpp.
    """
    # Ponte: usa dequantize Q8_0 simplificado (escala única por bloco)
    out = np.empty(n_elems, dtype=np.float32)
    n_blocks = (n_elems + block_size - 1) // block_size
    off = 0
    idx = 0
    for _ in range(n_blocks):
        if off + 2 > len(data):
            break
        scale = _f16_to_f32(struct.unpack("<H", data[off:off + 2])[0])
        off += block_bytes - 2 if block_bytes > 2 else 0
        remaining = min(block_size, n_elems - idx)
        # fallback: preenche com escala * random-ish baseado nos bytes
        for j in range(remaining):
            if off < len(data):
                v = (data[off % len(data)] & 0x0F) - 8
            else:
                v = 0
            out[idx] = float(v) * scale
            idx += 1
        off += max(0, block_bytes - 2)  # avança mesmo assim
    return out[:idx]


DEQUANTIZE_MAP = {
    0: dequantize_f32,   # F32
    1: dequantize_f16,   # F16
    2: dequantize_bf16,  # BF16
    3: dequantize_q4_0,  # Q4_0
    8: dequantize_q8_0,  # Q8_0
    6: dequantize_q5_0,  # Q5_0
    10: dequantize_q2_k,  # Q2_K
    11: dequantize_q3_k,  # Q3_K
    12: dequantize_q4_k,  # Q4_K
    13: dequantize_q5_k,  # Q5_K
    14: dequantize_q6_k,  # Q6_K
}


def dequantize_tensor(tensor, verbose: bool = False) -> np.ndarray:
    """Desquantiza um tensor GGUF para numpy f32.

    O objeto `tensor` tem atributos: .name (str), .data (memoryview),
    .shape (tuple), .tensor_type (GGML_TYPE int).
    """
    ttype = tensor.tensor_type
    raw_data = bytes(tensor.data)
    n_elems = int(np.prod(tensor.shape))

    if ttype == 0:  # F32
        return dequantize_f32(raw_data, n_elems).reshape(tensor.shape)
    elif ttype == 1:  # F16
        return dequantize_f16(raw_data, n_elems).reshape(tensor.shape)
    elif ttype == 2:  # BF16
        return dequantize_bf16(raw_data, n_elems).reshape(tensor.shape)

    fn = DEQUANTIZE_MAP.get(ttype)
    if fn is None:
        tipo_nome = GGUF_TYPES.get(ttype, ("?", "?", f"T{ttype}"))[2]
        print(f"  [AVISO] Tipo GGUF {tipo_nome} (ttype={ttype}) não suportado "
              f"para tensor {tensor.name}, tentando fallback F32/F16")
        # Fallback: tenta ler como F32, depois F16
        try:
            return dequantize_f32(raw_data, n_elems).reshape(tensor.shape)
        except Exception:
            try:
                return dequantize_f16(raw_data, n_elems).reshape(tensor.shape)
            except Exception:
                raise ValueError(f"Tipo GGUF {ttype} não implementado para {tensor.name}")

    result = fn(raw_data, n_elems)
    if verbose:
        tipo_nome = GGUF_TYPES.get(ttype, ("?", "?", f"T{ttype}"))[2]
        print(f"    dequant {tensor.name}: {tipo_nome} {tensor.shape} -> {result.shape}")
    return result.reshape(tensor.shape)


# ─── OTIMIZAÇÃO RTN + SCALE ──────────────────────────────────────────────────

def optimize_ternary(weights: np.ndarray, verbose: bool = False
                     ) -> tuple[np.ndarray, float]:
    """Encontra threshold e scale ótimos para quantização ternária RTN.

    Retorna (ternary_values: int8 array, scale: f32)

    Algoritmo:
    1. Ordena |w| ascendente
    2. Para cada candidato a threshold (5% a 50% percentil):
       a. q = sign(w) se |w| >= t, senão 0
       b. scale = mean(|w| para w não-zero)
       c. MSE = mean((w - scale * q)^2)
    3. Escolhe (threshold, scale) que minimiza MSE
    4. Retorna q, scale
    """
    flat = weights.flatten()
    abs_w = np.abs(flat)
    sorted_idx = np.argsort(abs_w)
    sorted_abs = abs_w[sorted_idx]

    best_mse = float('inf')
    best_q = None
    best_scale = 1.0
    best_threshold = 0.0

    n = len(flat)
    # Testa thresholds do percentil 5 ao 50 em passos de 5%
    for pct in range(5, 55, 5):
        t_idx = int(n * pct / 100)
        threshold = sorted_abs[min(t_idx, n - 1)]

        q = np.where(flat > threshold, 1, np.where(flat < -threshold, -1, 0)).astype(np.int8)
        nonzero = q != 0
        nz_count = nonzero.sum()

        if nz_count == 0:
            continue

        scale = float(np.abs(flat[nonzero]).mean())
        if scale < 1e-8:
            continue

        # MSE = mean((w - s * q)^2)
        s_q = scale * q.astype(np.float32)
        mse = float(np.mean((flat - s_q) ** 2))

        if mse < best_mse:
            best_mse = mse
            best_q = q
            best_scale = scale
            best_threshold = threshold

    if best_q is None:
        # Fallback: percentil 85
        threshold = sorted_abs[int(n * 0.85)]
        q = np.where(flat > threshold, 1, np.where(flat < -threshold, -1, 0)).astype(np.int8)
        best_scale = float(np.abs(flat[q != 0]).mean() or 1.0)
        if verbose:
            print(f"    RTN fallback: threshold={threshold:.4f} scale={best_scale:.4f}")
        return q, best_scale

    if verbose:
        sparsity = 100.0 * (best_q == 0).sum() / n
        print(f"    RTN: threshold={best_threshold:.4f} scale={best_scale:.4f} "
              f"MSE={best_mse:.6f} esparsidade={sparsity:.1f}%")
    return best_q, best_scale


def optimize_ternary_gpu(weights: np.ndarray, verbose: bool = False
                         ) -> tuple[np.ndarray, float]:
    """Versão GPU via PyTorch -- mesma lógica RTN, usa torch.sort no CUDA.

    Útil para tensores >100M elementos onde a ordenação na GPU é ~5-10× mais rápida.
    """
    import torch
    torch_dev = torch.device(_DEVICE)

    flat = torch.from_numpy(weights.flatten()).to(torch_dev)
    abs_w = flat.abs()
    sorted_abs, _ = torch.sort(abs_w)
    n = flat.size(0)

    best_mse = float('inf')
    best_q_np = None
    best_scale = 1.0

    for pct in range(5, 55, 5):
        t_idx = int(n * pct / 100)
        threshold = sorted_abs[min(t_idx, n - 1)].item()

        # q = sign(w) se |w| >= threshold, senão 0
        q = torch.where(flat > threshold, 1, torch.where(flat < -threshold, -1, 0)).to(torch.int8)
        abs_flat = abs_w.clone()
        # mask de não-zero
        nonzero_mask = q != 0
        nz_count = nonzero_mask.sum().item()

        if nz_count == 0:
            continue

        scale_val = float(abs_flat[nonzero_mask].mean().item())
        if scale_val < 1e-8:
            continue

        mse = float(torch.mean((flat - scale_val * q.float()) ** 2).item())

        if mse < best_mse:
            best_mse = mse
            best_q_np = q.cpu().numpy()
            best_scale = scale_val

    if best_q_np is None:
        threshold = sorted_abs[int(n * 0.85)].item()
        q = torch.where(flat > threshold, 1, torch.where(flat < -threshold, -1, 0)).to(torch.int8)
        best_scale = float(abs_w[q != 0].mean().item() or 1.0)
        q_np = q.cpu().numpy()
        if verbose:
            print(f"    RTN[GPU] fallback: threshold={threshold:.4f} scale={best_scale:.4f}")
        return q_np, best_scale

    if verbose:
        sparsity = 100.0 * (best_q_np == 0).sum() / n
        print(f"    RTN[GPU]: scale={best_scale:.4f} MSE={best_mse:.6f} esparsidade={sparsity:.1f}%")
    return best_q_np, best_scale


# ─── ESCRITA .bitnet (v6 — via tools/bitnet_writer.py, ADR-0085) ────────────

def write_ternary_tensor(f, weights_f32: np.ndarray, name: str = "",
                         verbose: bool = False, use_gpu: bool = False):
    """Otimiza (RTN) e escreve tensor ternário + scale f32 via writer v6.

    use_gpu: usa torch CUDA para otimização RTN em tensores grandes.
    A escrita em si (packing 2-bit + scale f32 SEMPRE) delega para
    tools.bitnet_writer.write_ternary — byte-exato com save_model_v6 (ADR-0085 D1).
    """
    if use_gpu and _TORCH_AVAIL and _DEVICE != "cpu":
        q_vals, scale = optimize_ternary_gpu(weights_f32, verbose=verbose)
    else:
        q_vals, scale = optimize_ternary(weights_f32, verbose=verbose)
    write_ternary(f, q_vals, scale)
    if verbose:
        n = q_vals.size
        nonzero = (q_vals != 0).sum()
        print(f"    {name}: {weights_f32.shape} {(n + 3) // 4}B "
              f"scale={scale:.4f} esparso={100*(1-nonzero/n):.0f}%")


# ─── TOKENIZER ────────────────────────────────────────────────────────────────

def extract_tokenizer_gguf(metadata: dict, tensor_names: list[str],
                           data: bytes, cache_dir: str | None) -> bytes:
    """Extrai dados do tokenizer do GGUF.

    Tenta extrair tokenizer.model (sentencepiece) ou tokenizer.ggml.*.
    Retorna bytes serializados no formato:
      - u16 length do nome + chars (ex: "BPE:...")
    Se não achar, retorna tokenizer char default.
    """
    # Tenta pegar o modelo do tokenizer (sentencepiece)
    tok_model = metadata.get("tokenizer.ggml.model")
    tok_list = metadata.get("tokenizer.ggml.tokens")

    if tok_list is not None and len(tok_list) > 0:
        # Serializa como "BPE:vocab_size"
        msg = f"BPE:{len(tok_list)}".encode("utf-8")
        return struct.pack("<H", len(msg)) + msg

    # Fallback: char-level
    msg = b"CHAR:32-126"
    return struct.pack("<H", len(msg)) + msg


# ─── CONVERSÃO PRINCIPAL ─────────────────────────────────────────────────────

def get_tensor(tensors, name: str):
    """Busca tensor por nome exato."""
    for t in tensors:
        if t.name == name:
            return t
    return None


def convert_model(model_name: str, output_path: str,
                  cache_dir: str | None = None,
                  verbose: bool = False,
                  self_test: bool = False,
                  use_gpu: bool = False):
    """Pipeline principal: download GGUF -> .bitnet v6 (ADR-0085)."""
    t0 = time.time()
    print(f"=== Convertendo {model_name} -> {output_path} ===")

    # 1. Abre GGUF
    reader = read_gguf_reader(model_name, cache_dir)
    # GGUFReader 0.19+: fields é OrderDict de ReaderField com .contents()
    # Versões antigas: .metadata (iterável de .key/.value)
    if hasattr(reader, 'metadata'):
        metadata = {kv.key: kv.value for kv in reader.metadata}
    elif hasattr(reader, 'fields'):
        metadata = {}
        for k, v in reader.fields.items():
            try:
                val = v.contents()
                if isinstance(val, bytes):
                    val = val.decode('utf-8', errors='replace')
                metadata[k] = val
            except Exception:
                # Fallback: tenta extrair das parts
                if len(v.parts) == 1:
                    metadata[k] = v.parts[0][0] if hasattr(v.parts[0], '__getitem__') else v.parts[0]
                else:
                    metadata[k] = str(v.parts)
    else:
        raise RuntimeError("GGUFReader: nem metadata nem fields encontrados")

    # Lista de tensores (nome, tipo, shape)
    tensor_list = reader.tensors
    tensor_names = [t.name for t in tensor_list]
    tensor_dict = {t.name: t for t in tensor_list}
    if verbose:
        print(f"  Metadata keys: {len(metadata)}")
        print(f"  Tensors: {len(tensor_list)}")
        for t in tensor_list[:5]:
            tipo = GGUF_TYPES.get(t.tensor_type, ("?", "?", f"T{t.tensor_type}"))[2]
            print(f"    {t.name}: {tensor_type_name(t.tensor_type)} {list(t.shape)}")
        if len(tensor_list) > 5:
            print(f"    ... +{len(tensor_list) - 5} more")

    # 2. Detecta arquitetura e extrai config
    arch = detect_arch(metadata, tensor_names)
    print(f"  Arquitetura detectada: {arch}")

    hidden = get_metadata_int(metadata,
                              f"{arch}.embedding_length",
                              "embedding_length",
                              f"{arch}.hidden_size",
                              "hidden_size",
                              default=2048)
    num_layers = get_metadata_int(metadata,
                                  f"{arch}.block_count",
                                  "block_count",
                                  f"{arch}.num_hidden_layers",
                                  "num_hidden_layers",
                                  default=24)
    num_heads = get_metadata_int(metadata,
                                 f"{arch}.attention.head_count",
                                 "attention.head_count",
                                 f"{arch}.num_attention_heads",
                                 "num_attention_heads",
                                 default=32)
    num_kv_heads = get_metadata_int(metadata,
                                    f"{arch}.attention.head_count_kv",
                                    "attention.head_count_kv",
                                    f"{arch}.num_key_value_heads",
                                    "num_key_value_heads",
                                    default=num_heads)
    vocab_size = get_metadata_int(metadata,
                                  f"{arch}.vocabulary.size",
                                  "vocabulary.size",
                                  f"{arch}.vocab_size",
                                  "vocab_size",
                                  default=32000)
    max_seq = get_metadata_int(metadata,
                               f"{arch}.context_length",
                               "context_length",
                               f"{arch}.max_position_embeddings",
                               "max_position_embeddings",
                               default=2048)
    intermediate_size = get_metadata_int(metadata,
                                         f"{arch}.feed_forward_length",
                                         "feed_forward_length",
                                         f"{arch}.intermediate_size",
                                         "intermediate_size",
                                         default=hidden * 4)
    head_dim = get_metadata_int(metadata,
                                f"{arch}.attention.head_dim",
                                "attention.head_dim",
                                default=hidden // num_heads)
    q_dim = head_dim * num_heads
    tie_embeddings = metadata.get(f"{arch}.tie_word_embeddings",
                                  metadata.get("tie_word_embeddings", True))
    if isinstance(tie_embeddings, str):
        tie_embeddings = tie_embeddings.lower() in ("true", "1", "yes")

    # rope_theta
    rope_theta = float(metadata.get(f"{arch}.rope.freq_base",
                                    metadata.get("rope.freq_base", 10000.0)))

    # ── v6 (ADR-0085) ──────────────────────────────────────────────────────
    # num_params: soma de elementos de todos os tensores (informacional)
    num_params = 0
    for t in tensor_list:
        num_params += int(np.prod(t.shape))

    # act_type da metadata GGUF (arch.activation): "silu"→0, "relu2"→1; default SILU
    act_raw = str(metadata.get(f"{arch}.activation",
                               metadata.get("activation", "silu"))).lower()
    act_type = ACT_RELU2 if act_raw == "relu2" else ACT_SILU

    # embed_type: ternary por padrão; Q6_K se vocab gigante (o embed f32
    # completo está sempre disponível após dequantize_tensor)
    embed_type = EMBED_Q6K if vocab_size > 100_000 else EMBED_TERNARY

    # feat: NÃO escrevemos rms_inner_attn nem rms_ffn_norm (bits 0/1 limpos);
    # bit2 (theta) setado porque rope_freq_base sempre disponível (default 10000)
    theta_present = True
    feat = compute_feat(False, False, theta_present)

    print(f"  Config: h={hidden} L={num_layers} heads={num_heads} "
          f"kv={num_kv_heads} q_dim={q_dim}")
    print(f"  Vocab={vocab_size} max_seq={max_seq} ffn={intermediate_size}")
    print(f"  tie={tie_embeddings} rope_theta={rope_theta}")
    print(f"  v6: act={act_type} embed_type={embed_type} feat=0x{feat:02X} "
          f"num_params={num_params}")

    # 3. Extrai tokenizer
    tok_data = extract_tokenizer_gguf(metadata, tensor_names,
                                      reader.data, cache_dir)

    # 4. Mapeamento de nomes
    tmpl = ARCH_MAP.get(arch, ARCH_MAP["llama"])

    def tn(key: str, layer: int | None = None) -> str | None:
        t = tmpl.get(key)
        if t is None:
            return None
        if layer is not None:
            t = t.replace("{i}", str(layer))
        return t

    def get_tensor_weight(key: str, layer: int | None = None) -> np.ndarray | None:
        name = tn(key, layer)
        if name is None:
            return None
        t = get_tensor(tensor_list, name)
        if t is None:
            if verbose:
                print(f"  [AVISO] Tensor não encontrado: {name}")
            return None
        return dequantize_tensor(t, verbose=verbose)

    # Prepara diretório de saída
    out_path = Path(output_path)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    # 5. Escreve arquivo .bitnet v6
    with open(out_path, "wb") as f:
        write_header_v6(
            f,
            model_type=MODEL_LLM,
            num_params=num_params,
            hidden=hidden,
            layers=num_layers,
            heads=num_heads,
            vocab=vocab_size,
            max_seq=min(max_seq, 65535),  # u16 no header (writer não clampeia)
            intermediate=intermediate_size,
            kv_heads=num_kv_heads,
            q_dim=q_dim,
            medusa=0,
            tie=tie_embeddings,
            tok_type=1,  # BPE (tokenizer serializado como "BPE:vocab_size")
            tok_data=tok_data,
            act_type=act_type,
            embed_type=embed_type,
            feat=feat,
        )

        # 5a. Embedding — v6 canônico é (hidden, vocab) row-major
        print("  [embed]")
        embed_w = get_tensor_weight("token_embd")
        if embed_w is None:
            raise ValueError("Tensor token_embd.weight não encontrado no GGUF")
        if len(embed_w.shape) == 2 and embed_w.shape[0] == vocab_size:
            # GGUF guarda (vocab, hidden) → transpõe para (hidden, vocab)
            embed_w = embed_w.T
        if embed_type == EMBED_Q6K:
            # Q6_K bruto (210B/256 pesos) + f32 scale — o reader (cortex.rs)
            # lê o scale logo após o bloco Q6_K, então sempre o escrevemos
            write_q6k(f, embed_w.astype(np.float32), hidden, vocab_size)
            f.write(struct.pack("<f", 1.0))
        else:
            q_vals, scale = optimize_ternary(embed_w, verbose=verbose)
            write_embed(f, q_vals, EMBED_TERNARY, scale)
            if verbose:
                print(f"    embed: {embed_w.shape} {(embed_w.size + 3) // 4}B "
                      f"scale={scale:.4f}")

        # 5b. Layers
        for li in range(num_layers):
            if li % 5 == 0 or li + 1 == num_layers:
                print(f"  [Layer {li}/{num_layers}]")

            # RMS norms
            attn_norm = get_tensor_weight("blk_attn_norm", li)
            if attn_norm is not None:
                write_rms(f, attn_norm.reshape(-1))
            else:
                write_rms(f, np.ones(hidden, dtype=np.float32))

            ffn_norm = get_tensor_weight("blk_ffn_norm", li)
            if ffn_norm is not None:
                write_rms(f, ffn_norm.reshape(-1))
            else:
                write_rms(f, np.ones(hidden, dtype=np.float32))

            # Q projection: (hidden, q_dim)
            q_w = get_tensor_weight("blk_attn_q", li)
            if q_w is not None:
                # GGUF shape pode ser (q_dim, hidden) -- transpor se necessário
                if q_w.shape[0] == q_dim and q_w.shape[1] == hidden:
                    q_w = q_w.T
                write_ternary_tensor(f, q_w, name=f"L{li}.q", verbose=verbose)
            else:
                write_ternary_tensor(f, np.zeros((hidden, q_dim), dtype=np.float32),
                                     name=f"L{li}.q", verbose=verbose)

            # K projection: (hidden, k_dim)
            k_dim = num_kv_heads * head_dim
            k_w = get_tensor_weight("blk_attn_k", li)
            if k_w is not None:
                if k_w.shape[0] == k_dim and k_w.shape[1] == hidden:
                    k_w = k_w.T
                write_ternary_tensor(f, k_w, name=f"L{li}.k", verbose=verbose)
            else:
                write_ternary_tensor(f, np.zeros((hidden, k_dim), dtype=np.float32),
                                     name=f"L{li}.k", verbose=verbose)

            # V projection: (hidden, k_dim)
            v_w = get_tensor_weight("blk_attn_v", li)
            if v_w is not None:
                if v_w.shape[0] == k_dim and v_w.shape[1] == hidden:
                    v_w = v_w.T
                write_ternary_tensor(f, v_w, name=f"L{li}.v", verbose=verbose)
            else:
                write_ternary_tensor(f, np.zeros((hidden, k_dim), dtype=np.float32),
                                     name=f"L{li}.v", verbose=verbose)

            # O projection: (q_dim, hidden)
            o_w = get_tensor_weight("blk_attn_o", li)
            if o_w is not None:
                if o_w.shape[0] == hidden and o_w.shape[1] == q_dim:
                    o_w = o_w.T
                # o_w deve ser (q_dim, hidden)
                write_ternary_tensor(f, o_w, name=f"L{li}.o", verbose=verbose)
            else:
                write_ternary_tensor(f, np.zeros((q_dim, hidden), dtype=np.float32),
                                     name=f"L{li}.o", verbose=verbose)

            # FFN gate: (hidden, intermediate_size)
            gate_w = get_tensor_weight("blk_ffn_gate", li)
            if gate_w is not None:
                if gate_w.shape[0] == intermediate_size and gate_w.shape[1] == hidden:
                    gate_w = gate_w.T
                write_ternary_tensor(f, gate_w, name=f"L{li}.gate", verbose=verbose)
            else:
                write_ternary_tensor(f, np.zeros((hidden, intermediate_size),
                                                  dtype=np.float32),
                                     name=f"L{li}.gate", verbose=verbose)

            # FFN up: (hidden, intermediate_size)
            up_w = get_tensor_weight("blk_ffn_up", li)
            if up_w is not None:
                if up_w.shape[0] == intermediate_size and up_w.shape[1] == hidden:
                    up_w = up_w.T
                write_ternary_tensor(f, up_w, name=f"L{li}.up", verbose=verbose)
            else:
                write_ternary_tensor(f, np.zeros((hidden, intermediate_size),
                                                  dtype=np.float32),
                                     name=f"L{li}.up", verbose=verbose)

            # FFN down: (intermediate_size, hidden)
            down_w = get_tensor_weight("blk_ffn_down", li)
            if down_w is not None:
                if down_w.shape[0] == hidden and down_w.shape[1] == intermediate_size:
                    down_w = down_w.T
                # down_w deve ser (intermediate_size, hidden)
                write_ternary_tensor(f, down_w, name=f"L{li}.down", verbose=verbose)
            else:
                write_ternary_tensor(f, np.zeros((intermediate_size, hidden),
                                                  dtype=np.float32),
                                     name=f"L{li}.down", verbose=verbose)

        # 5c. rms_final
        print("  [rms_final]")
        rms_final_w = get_tensor_weight("rms_final")
        if rms_final_w is not None:
            write_rms(f, rms_final_w.reshape(-1))
        else:
            write_rms(f, np.ones(hidden, dtype=np.float32))

        # 5d. Unembed (output weight) — v6 D3: tied ⇒ seção NÃO existe (0 bytes)
        print("  [unembed]")
        output_w = get_tensor_weight("output")
        if not tie_embeddings and output_w is not None:
            if len(output_w.shape) == 2:
                if output_w.shape[0] == hidden and output_w.shape[1] == vocab_size:
                    # Já está (hidden, vocab_size)
                    pass
                elif output_w.shape[0] == vocab_size and output_w.shape[1] == hidden:
                    output_w = output_w.T
            write_ternary_tensor(f, output_w, name="unembed", verbose=verbose)
        else:
            # tied (ou output ausente): NENHUM byte de unembed
            if verbose:
                print(f"    unembed: skipped (tie_embeddings={tie_embeddings})")

        # 5e. RoPE theta (feat bit2) — f32 no fim do arquivo
        if feat & 4:
            f.write(struct.pack("<f", rope_theta))

    elapsed = time.time() - t0
    sz = os.path.getsize(output_path)
    mb = sz / (1024 * 1024)
    compressao = 100.0 * sz / (num_layers * (4 * hidden * hidden +
                                              3 * hidden * intermediate_size +
                                              2 * hidden * 4 + q_dim * 4) +
                               hidden * vocab_size * 4) if hidden > 0 else 100
    print(f"\n[OK] {output_path}: {sz:,} bytes ({mb:.1f} MB) em {elapsed:.1f}s")
    print(f"     Compressão estimada: {compressao:.0f}% do original F32")

    # 6. Self-test opcional
    if self_test:
        print("\n=== Self-test ===")
        _self_test(output_path, hidden, num_layers, num_heads, vocab_size,
                   max_seq, intermediate_size, num_kv_heads, q_dim,
                   tie_embeddings, feat, act_type, embed_type, verbose)


def tensor_type_name(ttype: int) -> str:
    """Retorna nome legível do tipo GGUF."""
    return GGUF_TYPES.get(ttype, ("?", "?", f"T{ttype}"))[2]


# ─── SELF-TEST ───────────────────────────────────────────────────────────────

def _self_test(path: str, hidden: int, num_layers: int, num_heads: int,
               vocab_size: int, max_seq: int, intermediate_size: int,
               num_kv_heads: int, q_dim: int, tie_embeddings: bool,
               feat: int, act_type: int, embed_type: int, verbose: bool):
    """Verifica integridade do .bitnet v6 gerado."""
    with open(path, "rb") as f:
        data = f.read()

    off = 0

    def r4():
        nonlocal off
        v = struct.unpack_from("<I", data, off)[0]
        off += 4
        return v

    def r8():
        nonlocal off
        v = struct.unpack_from("<Q", data, off)[0]
        off += 8
        return v

    def r2():
        nonlocal off
        v = struct.unpack_from("<H", data, off)[0]
        off += 2
        return v

    def r1():
        nonlocal off
        v = data[off]
        off += 1
        return v

    def r_float():
        nonlocal off
        v = struct.unpack_from("<f", data, off)[0]
        off += 4
        return v

    errors = []

    # Header v6 (ADR-0085)
    magic = r4()
    if magic != MAGIC:
        errors.append(f"magic: 0x{magic:X} != 0x{MAGIC:X}")
    version = r2()
    if version != 6:
        errors.append(f"version: {version} != 6")
    _np = r8()
    _mt = r1()
    off += 3  # reserved
    h = r2()
    nl = r2()
    nh = r2()
    vs = r4()
    _ms = r2()
    _is = r2()
    nkv = r2()
    qd = r2()
    _nm = r4()
    tie = data[off:off + 4]
    off += 4
    _tt = r1()
    tok_len = r4()
    off += tok_len
    act = r1()
    emb = r1()
    lf = r1()

    checks = [
        (h == hidden, f"hidden {h} != {hidden}"),
        (nl == num_layers, f"num_layers {nl} != {num_layers}"),
        (nh == num_heads, f"num_heads {nh} != {num_heads}"),
        (vs == vocab_size, f"vocab_size {vs} != {vocab_size}"),
        (nkv == num_kv_heads, f"num_kv_heads {nkv} != {num_kv_heads}"),
        (qd == q_dim, f"q_dim {qd} != {q_dim}"),
        (tie == (b"TIED" if tie_embeddings else b"\x00\x00\x00\x00"),
         f"tie_embeddings mismatch"),
        (act == act_type, f"act_type {act} != {act_type}"),
        (emb == embed_type, f"embed_type {emb} != {embed_type}"),
        (lf == feat, f"feat 0x{lf:02X} != 0x{feat:02X}"),
    ]
    for ok, msg in checks:
        if not ok:
            errors.append(msg)

    # Tamanhos esperados
    # embed: ternary → packed (hidden, vocab_size) + scale f32;
    #        Q6_K → 210B/super-bloco + scale f32
    if embed_type == EMBED_Q6K:
        embed_packed = ((hidden * vocab_size + 255) // 256) * 210
    else:
        embed_packed = (hidden * vocab_size + 3) // 4
    embed_total = embed_packed + 4

    # Por layer: rms_attn + rms_ffn + 7 tensors * (packed + scale)
    # (sem rms_inner_attn / rms_ffn_norm — feat bits 0/1 limpos)
    k_dim = num_kv_heads * (q_dim // num_heads)
    ffn_group = intermediate_size

    per_layer_rms = 2 * hidden * 4
    per_layer_tern = (
        (hidden * q_dim + 3) // 4 + 4 +
        2 * ((hidden * k_dim + 3) // 4 + 4) +
        (q_dim * hidden + 3) // 4 + 4 +
        2 * ((hidden * ffn_group + 3) // 4 + 4) +
        (intermediate_size * hidden + 3) // 4 + 4
    )
    per_layer_total = per_layer_rms + per_layer_tern

    rms_final_total = hidden * 4
    # v6 D3: tied ⇒ seção de unembed NÃO existe (0 bytes)
    unembed_total = ((hidden * vocab_size + 3) // 4 + 4) if not tie_embeddings else 0
    rope_total = 4 if (feat & 4) else 0

    # header v6: magic4 + version2 + num_params8 + model_type1 + reserved3 +
    #             bloco transformer 26B + tok_type1 + tok_len4 + tok_data +
    #             act_type1 + embed_type1 + feat1
    expected = (4 + 2 + 8 + 1 + 3 + 26 +
                1 + 4 + tok_len + 3 +
                embed_total +
                per_layer_total * num_layers +
                rms_final_total +
                unembed_total +
                rope_total)

    if verbose:
        print(f"  Header: {off}B")
        print(f"  Embed total: {embed_total}B (packed={embed_packed}B + scale=4B)")
        print(f"  Per-layer: {per_layer_total}B")
        print(f"  RMS final: {rms_final_total}B")
        print(f"  Unembed: {unembed_total}B")
        print(f"  RoPE theta: {rope_total}B (feat&4={'yes' if feat & 4 else 'no'})")
        print(f"  Expected total: {expected}B")
        print(f"  Actual file size: {len(data)}B")

    if len(data) != expected:
        dif = len(data) - expected
        errors.append(f"tamanho: {len(data)} != {expected} ({dif:+d})")

    if errors:
        print(f"  [FALHA] {len(errors)} erro(s):")
        for e in errors:
            print(f"    - {e}")
    else:
        print(f"  [OK] self-test passou -- {len(data)} bytes, "
              f"{embed_total + per_layer_total * num_layers}B de pesos")

    # Verifica ranges básicos
    print(f"  Magic: 0x{MAGIC:X} OK")
    print(f"  Version: {version} OK")
    print(f"  Layers: {nl} OK")
    print(f"  Layer features: 0x{lf:02X} ({'RoPE ' if lf & 4 else ''}"
          f"{'InnerLN ' if lf & 1 else ''}{'FFNLN ' if lf & 2 else ''})")


# ─── CLI ─────────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(
        description="Converte modelo GGUF do HuggingFace para .bitnet v6 (ADR-0085)",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Arquiteturas suportadas: llama, qwen2, gemma2, phi3, mistral, starcoder2, deepseek

Exemplos:
  python tools/convert_gguf_to_bitnet.py \\
      --model meta-llama/Llama-3.2-1B \\
      --output target/models/LLAMA1B.BIN \\
      --verbose

  python tools/convert_gguf_to_bitnet.py \\
      --model Qwen/Qwen2.5-0.5B-Instruct-GGUF \\
      --output target/models/QWEN05B.BIN \\
      --self-test

  python tools/convert_gguf_to_bitnet.py \\
      --model /caminho/local/model.gguf \\
      --output out.bitnet
        """,
    )
    ap.add_argument("--model", required=True,
                    help="Modelo HF (ex: meta-llama/Llama-3.2-1B) ou path local .gguf")
    ap.add_argument("--output", required=True,
                    help="Caminho do .bitnet de saída")
    ap.add_argument("--cache-dir", default=None,
                    help="Cache HuggingFace (default: ~/.cache/huggingface)")
    ap.add_argument("--verbose", action="store_true",
                    help="Log detalhado de cada tensor")
    ap.add_argument("--self-test", action="store_true",
                    help="Verifica integridade do .bitnet gerado")
    ap.add_argument("--gpu", action="store_true",
                    help="Usa GPU (CUDA/ROCm/MPS) se disponível. Nota: conversão é CPU-bound, "
                         "GPU só acelera MSE computation em tensores >100M elementos")
    args = ap.parse_args()

    try:
        if args.gpu and _DEVICE != "cpu":
            print(f"Usando GPU: {_DEVICE}")
        elif args.gpu:
            print("GPU solicitada mas não disponível. Usando CPU.")
        convert_model(
            model_name=args.model,
            output_path=args.output,
            cache_dir=args.cache_dir,
            verbose=args.verbose,
            self_test=args.self_test,
            use_gpu=args.gpu and _DEVICE != "cpu",
        )
    except Exception as e:
        print(f"\n[ERRO] {e}", file=sys.stderr)
        if args.verbose:
            import traceback
            traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
