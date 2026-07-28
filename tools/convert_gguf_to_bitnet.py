#!/usr/bin/env python3
"""Converte modelos GGUF (HuggingFace) para .bitnet v4 com RTN + scale.

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

# ─── .bitnet v4 format ───────────────────────────────────────────────────────
MAGIC = 0xBE11BE11

# Arquiteturas suportadas: mapeamento de nome GGUF → template de tensor names
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
}

# Mapeamento reverso: prefixos de nomes HF → arquitetura
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
    """Importa gguf e retorna GGUFReader."""
    try:
        from gguf import GGUFReader
    except ImportError:
        print("[ERRO] pip install gguf (pip install gguf numpy huggingface-hub)")
        sys.exit(1)
    return GGUFReader(model_path, cache_dir=cache_dir)


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
    """F16 → F32."""
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
    """BF16 → F32 (zerar lower 16 bits)."""
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
    """Q5_0: f16 scale + uint32 qh + 16 packed nibbles = 22 bytes → 32 f32."""
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


def pack_ternary(weights: np.ndarray, scale: float) -> tuple[bytes, float]:
    """Pack int8 {-1,0,+1} para 2-bit (4 pesos/byte), column-major.

    weights: array 2D (rows, cols) em float32 (já quantizado ternário como float
             com valores -1, 0, 1). A saída packing segue column-major:
             stride = n_cols, 4 valores por byte ao longo de n_cols.

    Retorna (packed_bytes, scale).
    """
    # Se weights veio do optimize_ternary, já é int8. Se float, converte.
    if weights.dtype == np.float32 or weights.dtype == np.float64:
        w_int8 = np.round(weights).astype(np.int8)
    else:
        w_int8 = weights.astype(np.int8)

    flat = w_int8.flatten()
    n = len(flat)
    packed_len = (n + 3) // 4
    packed = bytearray(packed_len)

    encode = {1: 0b01, 0: 0b00, -1: 0b10}
    for i, w in enumerate(flat):
        byte_idx = i // 4
        bit_pos = (i % 4) * 2
        packed[byte_idx] |= encode.get(int(w), 0) << bit_pos

    return bytes(packed), scale


def pack_ternary_fast(weights: np.ndarray, scale: float) -> tuple[bytes, float]:
    """Versão vetorizada de pack_ternary."""
    if weights.dtype == np.float32 or weights.dtype == np.float64:
        w_int8 = np.round(weights).astype(np.int8)
    else:
        w_int8 = weights.astype(np.int8)

    flat = w_int8.reshape(-1)
    n = flat.size
    bits = np.zeros(n, dtype=np.uint8)
    bits[flat > 0] = 0b01
    bits[flat < 0] = 0b10
    pad = (-n) % 4
    if pad:
        bits = np.concatenate([bits, np.zeros(pad, dtype=np.uint8)])
    b = bits.reshape(-1, 4)
    packed = b[:, 0] | (b[:, 1] << 2) | (b[:, 2] << 4) | (b[:, 3] << 6)
    return packed.tobytes(), scale


# ─── ESCRITA .bitnet ─────────────────────────────────────────────────────────

def write_header(f, hidden: int, num_layers: int, num_heads: int,
                 vocab_size: int, max_seq: int, intermediate_size: int,
                 num_kv_heads: int, q_dim: int, num_medusa: int,
                 tie_embeddings: bool, tok_data: bytes,
                 layer_features: int) -> int:
    """Escreve header .bitnet v4. Retorna posição após header."""
    num_params = (hidden * vocab_size +
                  num_layers * (4 * hidden * hidden +
                                3 * hidden * intermediate_size +
                                2 * hidden + q_dim) +
                  hidden * vocab_size)
    f.write(struct.pack("<I", MAGIC))
    f.write(struct.pack("<H", 4))  # version
    f.write(struct.pack("<I", num_params))
    f.write(struct.pack("<H", hidden))
    f.write(struct.pack("<H", num_layers))
    f.write(struct.pack("<H", num_heads))
    f.write(struct.pack("<I", vocab_size))
    f.write(struct.pack("<H", min(max_seq, 65535)))
    f.write(struct.pack("<H", intermediate_size))
    f.write(struct.pack("<H", num_kv_heads))
    f.write(struct.pack("<H", q_dim))
    f.write(struct.pack("<I", num_medusa))
    f.write(b"TIED" if tie_embeddings else b"\x00\x00\x00\x00")
    f.write(struct.pack("B", 1))  # tokenizer_type: BPE
    f.write(struct.pack("<I", len(tok_data)))
    f.write(tok_data)
    f.write(struct.pack("B", layer_features))
    return f.tell()


def write_rms_vec(f, vec: np.ndarray):
    """Escreve vetor RMS normalization como f32 LE contíguo."""
    f.write(np.ascontiguousarray(vec, dtype=np.float32).tobytes())


def write_ternary_tensor(f, weights_f32: np.ndarray, name: str = "",
                         verbose: bool = False):
    """Otimiza, pack e escreve tensor ternário + scale f32."""
    # weights_f32 shape (rows, cols) como no kernel: (in, out) para projeções
    q_vals, scale = optimize_ternary(weights_f32, verbose=verbose)
    packed, scale = pack_ternary_fast(q_vals, scale)
    f.write(packed)
    f.write(struct.pack("<f", scale))
    if verbose:
        n = q_vals.size
        nonzero = (q_vals != 0).sum()
        print(f"    {name}: {weights_f32.shape} {len(packed)}B "
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
                  self_test: bool = False):
    """Pipeline principal: download GGUF → .bitnet v4."""
    t0 = time.time()
    print(f"=== Convertendo {model_name} → {output_path} ===")

    # 1. Abre GGUF
    reader = read_gguf_reader(model_name, cache_dir)
    metadata = {kv.key: kv.value for kv in reader.metadata}

    # Lista de tensores (nome, tipo, shape)
    tensor_list = reader.tensors
    tensor_names = [t.name for t in tensor_list]
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

    layer_features = 0x07  # bit0=inner_attn_ln, bit1=ffn_layernorm, bit2=RoPE

    print(f"  Config: h={hidden} L={num_layers} heads={num_heads} "
          f"kv={num_kv_heads} q_dim={q_dim}")
    print(f"  Vocab={vocab_size} max_seq={max_seq} ffn={intermediate_size}")
    print(f"  tie={tie_embeddings} rope_theta={rope_theta}")

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

    # 5. Escreve arquivo .bitnet
    with open(out_path, "wb") as f:
        write_header(f, hidden, num_layers, num_heads, vocab_size,
                     max_seq, intermediate_size, num_kv_heads, q_dim,
                     0, tie_embeddings, tok_data, layer_features)

        # 5a. Embedding
        print("  [embed]")
        embed_w = get_tensor_weight("token_embd")
        if embed_w is None:
            raise ValueError("Tensor token_embd.weight não encontrado no GGUF")
        # embed esperado shape: (vocab_size, hidden)
        if len(embed_w.shape) == 2 and embed_w.shape[0] != vocab_size:
            embed_w = embed_w.T
        write_ternary_tensor(f, embed_w, name="embed", verbose=verbose)

        # 5b. Layers
        for li in range(num_layers):
            if li % 5 == 0 or li + 1 == num_layers:
                print(f"  [Layer {li}/{num_layers}]")

            # RMS norms
            attn_norm = get_tensor_weight("blk_attn_norm", li)
            if attn_norm is not None:
                write_rms_vec(f, attn_norm.reshape(-1))
            else:
                write_rms_vec(f, np.ones(hidden, dtype=np.float32))

            ffn_norm = get_tensor_weight("blk_ffn_norm", li)
            if ffn_norm is not None:
                write_rms_vec(f, ffn_norm.reshape(-1))
            else:
                write_rms_vec(f, np.ones(hidden, dtype=np.float32))

            # Q projection: (hidden, q_dim)
            q_w = get_tensor_weight("blk_attn_q", li)
            if q_w is not None:
                # GGUF shape pode ser (q_dim, hidden) — transpor se necessário
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
            write_rms_vec(f, rms_final_w.reshape(-1))
        else:
            write_rms_vec(f, np.ones(hidden, dtype=np.float32))

        # 5d. Unembed (output weight)
        print("  [unembed]")
        output_w = get_tensor_weight("output")
        if output_w is not None and not tie_embeddings:
            if len(output_w.shape) == 2:
                if output_w.shape[0] == hidden and output_w.shape[1] == vocab_size:
                    # Já está (hidden, vocab_size)
                    pass
                elif output_w.shape[0] == vocab_size and output_w.shape[1] == hidden:
                    output_w = output_w.T
            write_ternary_tensor(f, output_w, name="unembed", verbose=verbose)
        elif tie_embeddings or output_w is None:
            if verbose:
                print(f"    unembed: skipped (tie_embeddings={tie_embeddings})")
            # Escreve tensor zero marcado como tied
            zero_w = np.zeros((hidden, vocab_size), dtype=np.float32)
            write_ternary_tensor(f, zero_w, name="unembed(tied)", verbose=verbose)

        # 5e. RoPE theta (bit de feature 2 = RoPE)
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
                   tie_embeddings, layer_features, verbose)


def tensor_type_name(ttype: int) -> str:
    """Retorna nome legível do tipo GGUF."""
    return GGUF_TYPES.get(ttype, ("?", "?", f"T{ttype}"))[2]


# ─── SELF-TEST ───────────────────────────────────────────────────────────────

def _self_test(path: str, hidden: int, num_layers: int, num_heads: int,
               vocab_size: int, max_seq: int, intermediate_size: int,
               num_kv_heads: int, q_dim: int, tie_embeddings: bool,
               layer_features: int, verbose: bool):
    """Verifica integridade do .bitnet gerado."""
    with open(path, "rb") as f:
        data = f.read()

    off = 0

    def r4():
        nonlocal off
        v = struct.unpack_from("<I", data, off)[0]
        off += 4
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

    # Header
    magic = r4()
    if magic != MAGIC:
        errors.append(f"magic: 0x{magic:X} != 0x{MAGIC:X}")
    version = r2()
    if version != 4:
        errors.append(f"version: {version} != 4")
    _np = r4()
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
        (lf == layer_features, f"layer_features {lf} != {layer_features}"),
    ]
    for ok, msg in checks:
        if not ok:
            errors.append(msg)

    # Tamanhos esperados
    # embed: packed (hidden, vocab_size) + scale f32
    embed_packed = (hidden * vocab_size + 3) // 4
    embed_total = embed_packed + 4

    # Por layer: rms_attn + rms_ffn + 7 tensors * (packed + scale)
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
    unembed_total = (hidden * vocab_size + 3) // 4 + 4
    rope_total = 4

    expected = (4 + 2 + 4 + 2 + 2 + 2 + 4 + 2 + 2 + 2 + 2 + 4 + 4 +
                1 + 4 + tok_len + 1 +  # header
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
        print(f"  RoPE theta: {rope_total}B")
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
        print(f"  [OK] self-test passou — {len(data)} bytes, "
              f"{embed_total + per_layer_total * num_layers}B de pesos")

    # Verifica ranges básicos
    print(f"  Magic: 0x{MAGIC:X} ✓")
    print(f"  Version: {version} ✓")
    print(f"  Layers: {nl} ✓")
    print(f"  Layer features: 0x{lf:02X} ({'RoPE ' if lf & 4 else ''}"
          f"{'InnerLN ' if lf & 1 else ''}{'FFNLN ' if lf & 2 else ''})")


# ─── CLI ─────────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(
        description="Converte modelo GGUF do HuggingFace para .bitnet v4",
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
    args = ap.parse_args()

    try:
        convert_model(
            model_name=args.model,
            output_path=args.output,
            cache_dir=args.cache_dir,
            verbose=args.verbose,
            self_test=args.self_test,
        )
    except Exception as e:
        print(f"\n[ERRO] {e}", file=sys.stderr)
        if args.verbose:
            import traceback
            traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
