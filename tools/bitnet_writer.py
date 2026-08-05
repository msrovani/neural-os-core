"""bitnet_writer.py — Writer canônico .bitnet v6 (ADR-0085).

API pura (numpy + struct, sem torch/transformers). Gera bytes alinhados
byte-a-byte com save_model_v6 (cortex.rs). Self-check: --self-check gera
tools/golden_v6.bin a partir de modelo sintético determinístico (mesmo
LCG do test Rust v6_writer_parity).
"""
import struct
import sys
import numpy as np

# ── Constantes ──────────────────────────────────────────────────────────
MAGIC = 0xBE11BE11
VERSION = 6

MODEL_LLM = 0
MODEL_HWEXPERT = 1
MODEL_ROUTER = 2

ACT_SILU = 0
ACT_RELU2 = 1

EMBED_TERNARY = 0
EMBED_Q6K = 1
EMBED_BF16 = 2


# ── Packing ─────────────────────────────────────────────────────────────

def pack_ternary(flat_int8: np.ndarray) -> bytes:
    """Pack flat i8 array {-1,0,1} → 4 trits/byte (2 bits/trit).

    Layout: bits[trit*2 : trit*2+2] → 01=+1, 10=-1, 00=0.
    Padding: zero bits to next multiple of 4.
    Byte: b[0] | (b[1]<<2) | (b[2]<<4) | (b[3]<<6)
    """
    n = len(flat_int8)
    bits = np.zeros(n, dtype=np.uint8)
    pos = flat_int8 > 0
    neg = flat_int8 < 0
    bits[pos] = 0b01
    bits[neg] = 0b10
    pad = (-n) % 4
    if pad:
        bits = np.concatenate([bits, np.zeros(pad, dtype=np.uint8)])
    b = bits.reshape(-1, 4)
    packed = b[:, 0] | (b[:, 1] << 2) | (b[:, 2] << 4) | (b[:, 3] << 6)
    return packed.tobytes()


# ── Helpers ─────────────────────────────────────────────────────────────

def compute_feat(has_inner: bool, has_ffn: bool, has_theta: bool) -> int:
    """Feature bits: bit0=rms_inner_attn, bit1=rms_ffn_norm, bit2=theta."""
    return ((1 if has_inner else 0) << 0) | \
           ((1 if has_ffn else 0) << 1) | \
           ((1 if has_theta else 0) << 2)


def _tern_packed_len(rows: int, cols: int) -> int:
    return (rows * cols + 3) // 4


# ── Header ──────────────────────────────────────────────────────────────

def write_header_v6(f, *, model_type, num_params, hidden, layers, heads, vocab,
                    max_seq, intermediate, kv_heads, q_dim, medusa, tie,
                    tok_type=0, tok_data=b'', act_type=ACT_SILU,
                    embed_type=EMBED_TERNARY, feat=0):
    """Write v6 header + transformer block (model_type 0 or 1)."""
    # Preamble comum (17 bytes)
    f.write(struct.pack('<I', MAGIC))          # 0:  magic
    f.write(struct.pack('<H', VERSION))         # 4:  version
    f.write(struct.pack('<Q', num_params))      # 6:  num_params u64
    f.write(struct.pack('<B', model_type))       # 14: model_type
    f.write(b'\x00\x00\x00')                    # 15: reserved

    if model_type in (MODEL_LLM, MODEL_HWEXPERT):
        # Bloco transformer (a partir do offset 18)
        f.write(struct.pack('<H', hidden))       # 18
        f.write(struct.pack('<H', layers))       # 20
        f.write(struct.pack('<H', heads))        # 22
        f.write(struct.pack('<I', vocab))        # 24
        f.write(struct.pack('<H', max_seq))      # 28
        f.write(struct.pack('<H', intermediate)) # 30
        f.write(struct.pack('<H', kv_heads))     # 32
        f.write(struct.pack('<H', q_dim))        # 34
        f.write(struct.pack('<I', medusa))       # 36
        f.write(b'TIED' if tie else b'\x00' * 4)  # 40: tie_flag
        f.write(struct.pack('<B', tok_type))      # 44
        f.write(struct.pack('<I', len(tok_data))) # 45
        if tok_data:
            f.write(tok_data)
        f.write(struct.pack('<B', act_type))      # after tok_data
        f.write(struct.pack('<B', embed_type))
        f.write(struct.pack('<B', feat))
    elif model_type == MODEL_ROUTER:
        # Bloco router (a partir do offset 17)
        f.write(struct.pack('<I', vocab))        # 17
        f.write(struct.pack('<H', hidden))       # 21
        f.write(struct.pack('<H', layers))       # 23  (n_experts)


# ── Body writers ────────────────────────────────────────────────────────

def write_embed(f, vals: np.ndarray, embed_type: int, scale: float):
    """Write embedding + f32 scale.

    Args:
        vals: (hidden, vocab) row-major f32 or int8.
        embed_type: EMBED_TERNARY | EMBED_Q6K | EMBED_BF16.
        scale: always written as f32.
    """
    if embed_type == EMBED_TERNARY:
        flat = vals.astype(np.int8).ravel()
        f.write(pack_ternary(flat))
    elif embed_type == EMBED_BF16:
        f.write(vals.astype(np.float32).tobytes())
    elif embed_type == EMBED_Q6K:
        f.write(_encode_q6k(vals))
    f.write(struct.pack('<f', scale))


def write_rms(f, vec: np.ndarray):
    """Write f32 RMS norm vector (exact, no length prefix)."""
    f.write(np.asarray(vec, dtype=np.float32).tobytes())


def write_ternary(f, q_int8: np.ndarray, scale: float):
    """Write packed ternary tensor + f32 scale."""
    f.write(pack_ternary(q_int8.astype(np.int8).ravel()))
    f.write(struct.pack('<f', scale))


def write_q6k(f, vals: np.ndarray, rows: int, cols: int):
    """Write Q6_K-encoded tensor (210B per 256-weight super-block).

    Args:
        vals: (rows, cols) f32 row-major.
    """
    f.write(_encode_q6k(vals))


# ── Q6_K encoder ────────────────────────────────────────────────────────

def _encode_q6k(vals: np.ndarray) -> bytes:
    """Encode f32 ndarray → Q6_K super-blocks (GGUF layout, 210B/256 pesos).

    Espelha dequantize_q6_k_block (gguf.rs): 256 pesos por super-bloco.
    Dequant de um elemento e (0..255):
      half = e//128, rem = e%128, lane = rem//32, l = rem%32, is = l//16
      q6 (6-bit) vem de ql/qh por lane; scale = scales[half*8 + is + lane*2]
      value = d * scale * (q6 - 32)
    """
    total = vals.size
    flat = vals.astype(np.float32).ravel()
    num_blocks = (total + 255) // 256
    out = bytearray()

    for b in range(num_blocks):
        start = b * 256
        end = min(start + 256, total)
        block_vals = np.zeros(256, dtype=np.float32)
        n_valid = end - start
        block_vals[:n_valid] = flat[start:end]

        # d global do bloco: escolhido para scales int8 usar a faixa 1..127.
        # eff = d*scale_i ≈ sub_max/31 → q6=63 reconstroi ≈ sub_max (ADR-0084 M4)
        block_max = float(np.max(np.abs(block_vals))) if n_valid > 0 else 0.0
        if block_max > 0:
            d = block_max / (31.0 * 127.0)
        else:
            d = 1e-9

        ql = bytearray(128)
        qh = bytearray(64)
        scales = bytearray(16)

        for sb in range(8):  # 8 sub-blocos de 16 por metade
            for half in range(2):
                sb_vals = block_vals[half * 128 + sb * 16: half * 128 + sb * 16 + 16]
                sb_max = float(np.max(np.abs(sb_vals)))
                if block_max > 0:
                    scale_i = int(np.clip(np.round(127.0 * sb_max / block_max), 1, 127))
                else:
                    scale_i = 1
                scales[half * 8 + sb] = scale_i
                eff = d * scale_i
                for j in range(16):
                    e = half * 128 + sb * 16 + j
                    w = block_vals[e]
                    q6 = int(np.clip(np.round(w / eff) + 32, 0, 63))
                    _store_q6(ql, qh, e, q6)

        out += bytes(ql)
        out += bytes(qh)
        out += bytes(scales)
        out += struct.pack('<H', _f32_to_f16(d))
    return bytes(out)


def _store_q6(ql: bytearray, qh: bytearray, e: int, q6: int) -> None:
    """Guarda q6 (0..63) no layout Q6_K: lane decide posição em ql/qh."""
    half = e // 128
    rem = e % 128
    lane = rem // 32
    l = rem % 32
    ql_off = half * 64
    qh_off = half * 32
    low = q6 & 0xF
    high = (q6 >> 4) & 3
    if lane == 0:
        ql[ql_off + l] |= low
        qh[qh_off + l] |= high << 0
    elif lane == 1:
        ql[ql_off + l + 32] |= low
        qh[qh_off + l] |= high << 2
    elif lane == 2:
        ql[ql_off + l] |= low << 4
        qh[qh_off + l] |= high << 4
    else:
        ql[ql_off + l + 32] |= low << 4
        qh[qh_off + l] |= high << 6


def _f32_to_f16(x: float) -> int:
    """Convert f32 to f16 bits (truncation, not round-to-nearest-even)."""
    import struct
    f32 = struct.pack('>f', x)
    bits = struct.unpack('>I', f32)[0]
    sign = (bits >> 16) & 0x8000
    exp = ((bits >> 23) & 0xFF) - 127
    mant = (bits >> 13) & 0x3FF  # upper 10 bits of mantissa
    if exp >= 16:  # overflow → Inf
        return sign | 0x7C00
    if exp <= -25:  # underflow → 0
        return sign
    if exp <= -15:  # subnormal
        mant = (mant | 0x400) >> (-14 - exp)
        return sign | mant
    return sign | ((exp + 15) << 10) | mant


def _f16_to_f32(bits: int) -> float:
    """f16 bits → f32 float (espelha gguf.rs f16_to_f32)."""
    import struct
    sign = (bits >> 15) & 1
    exp = (bits >> 10) & 0x1F
    mant = bits & 0x3FF
    if exp == 0:
        v = mant / 1024.0 * 2.0 ** -14
    elif exp == 31:
        v = float('inf')
    else:
        v = (1.0 + mant / 1024.0) * 2.0 ** (exp - 15)
    return -v if sign else v


def decode_q6k(data: bytes, rows: int, cols: int) -> np.ndarray:
    """Port Python do dequantize_q6_k (gguf.rs) — para self-check do encoder.

    Dado bytes Q6_K (210B/bloco), retorna (rows, cols) f32 row-major.
    """
    total = rows * cols
    out = np.zeros(total, dtype=np.float32)
    num_blocks = (total + 255) // 256
    for b in range(num_blocks):
        block = data[b * 210: b * 210 + 210]
        if len(block) < 210:
            break
        ql = block[0:128]
        qh = block[128:192]
        scales = block[192:208]
        d = _f16_to_f32(struct.unpack('<H', block[208:210])[0])
        y = 0
        ql_off = 0
        qh_off = 0
        sc_off = 0
        for _half in range(2):
            for l in range(32):
                is_ = l // 16
                q1 = ((ql[ql_off + l] & 0xF) | ((qh[qh_off + l] >> 0) & 3) << 4) - 32
                q2 = ((ql[ql_off + l + 32] & 0xF) | ((qh[qh_off + l] >> 2) & 3) << 4) - 32
                q3 = ((ql[ql_off + l] >> 4) | ((qh[qh_off + l] >> 4) & 3) << 4) - 32
                q4 = ((ql[ql_off + l + 32] >> 4) | ((qh[qh_off + l] >> 6) & 3) << 4) - 32
                s0 = scales[sc_off + is_] if isinstance(scales[sc_off + is_], int) else scales[sc_off + is_]
                s0 = struct.unpack('b', bytes([scales[sc_off + is_]]))[0]
                s2 = struct.unpack('b', bytes([scales[sc_off + is_ + 2]]))[0]
                s4 = struct.unpack('b', bytes([scales[sc_off + is_ + 4]]))[0]
                s6 = struct.unpack('b', bytes([scales[sc_off + is_ + 6]]))[0]
                base = b * 256 + y
                if base + l < total:
                    out[base + l] = d * s0 * q1
                if base + l + 32 < total:
                    out[base + l + 32] = d * s2 * q2
                if base + l + 64 < total:
                    out[base + l + 64] = d * s4 * q3
                if base + l + 96 < total:
                    out[base + l + 96] = d * s6 * q4
            y += 128
            ql_off += 64
            qh_off += 32
            sc_off += 8
    return out.reshape(rows, cols)


# ── Self-check: golden_v6.bin ────────────────────────────────────────────

def self_check():
    """Generate golden_v6.bin from deterministic synthetic model.

    Model spec: hidden=16, L=2, heads=2, vocab=32, max_seq=64,
    intermediate=32, kv_heads=1, q_dim=16, medusa=1, not tied,
    act_type=SILU, embed_type=TERNARY.
    LCG seed = 42 (matches Rust v6_writer_parity).
    """
    hidden = 16
    num_layers = 2
    num_heads = 2
    vocab_size = 32
    max_seq = 64
    intermediate_size = 32
    num_kv_heads = 1
    q_dim = 16
    num_medusa = 1
    tie = False

    kv_head_dim = q_dim // num_heads  # 8
    k_dim = num_kv_heads * kv_head_dim  # 8
    ffn_group = intermediate_size * q_dim // hidden  # 32
    down_out = q_dim  # 16

    # LCG: x_{n+1} = (x_n * 1103515245 + 12345) & 0x7FFFFFFF
    seed = [42]

    def lcg():
        seed[0] = (seed[0] * 1103515245 + 12345) & 0x7FFFFFFF
        return seed[0]

    def tern_i8(rows, cols):
        vals = np.zeros(rows * cols, dtype=np.int8)
        for i in range(rows * cols):
            r = lcg() % 3
            vals[i] = 1 if r == 0 else (-1 if r == 1 else 0)
        return vals

    def rms_vec(n):
        v = np.zeros(n, dtype=np.float32)
        for i in range(n):
            v[i] = 0.5 + (lcg() % 100) / 100.0
        return v

    # Num params: sum of all tensor elements (informational per ADR-0085)
    num_params = hidden * vocab_size  # embed
    num_params += intermediate_size * num_layers  # rms_ffn_norm
    per_layer = (hidden * q_dim       # q
                 + hidden * k_dim * 2  # k + v
                 + q_dim * hidden      # o
                 + hidden * ffn_group * 2  # gate + up
                 + intermediate_size * down_out)  # down
    num_params += per_layer * num_layers
    num_params += hidden * vocab_size  # unembed (not tied)
    num_params += hidden * vocab_size * num_medusa  # medusa

    # Feature bits
    has_inner = True
    has_ffn = True
    has_theta = True
    feat = compute_feat(has_inner, has_ffn, has_theta)

    data = bytearray()

    def w(fmt, *args):
        data.extend(struct.pack(fmt, *args))

    # Header v6
    w('<I', MAGIC)
    w('<H', VERSION)
    w('<Q', num_params)
    w('<B', MODEL_LLM)
    w('<3B', 0, 0, 0)  # reserved
    w('<H', hidden)
    w('<H', num_layers)
    w('<H', num_heads)
    w('<I', vocab_size)
    w('<H', max_seq)
    w('<H', intermediate_size)
    w('<H', num_kv_heads)
    w('<H', q_dim)
    w('<I', num_medusa)
    if tie:
        data.extend(b'TIED')
    else:
        data.extend(b'\x00' * 4)
    w('<B', 0)         # tok_type
    w('<I', 0)         # tok_len
    w('<B', ACT_SILU)  # act_type
    w('<B', EMBED_TERNARY)  # embed_type
    w('<B', feat)

    # Embed (hidden, vocab)
    embed_i8 = tern_i8(hidden, vocab_size)
    embed_scale = 1.5
    data.extend(pack_ternary(embed_i8))
    w('<f', embed_scale)

    # Layers
    for li in range(num_layers):
        # rms_attn
        data.extend(rms_vec(hidden).tobytes())
        # rms_ffn
        data.extend(rms_vec(hidden).tobytes())
        # rms_inner_attn (if feat&1)
        if has_inner:
            data.extend(rms_vec(hidden).tobytes())
        # rms_ffn_norm (if feat&2) — CANÔNICO: intermediate_size
        if has_ffn:
            data.extend(rms_vec(intermediate_size).tobytes())

        # 7 tensors: q,k,v,o,gate,up,down (each packed + f32 scale)
        for (rows, cols, scale) in [
            (hidden, q_dim, 0.5 + li * 0.25),
            (hidden, k_dim, 0.75),
            (hidden, k_dim, 1.25),
            (q_dim, hidden, 0.9),
            (hidden, ffn_group, 1.1),
            (hidden, ffn_group, 0.8),
            (intermediate_size, down_out, 1.05),
        ]:
            data.extend(pack_ternary(tern_i8(rows, cols)))
            w('<f', scale)

    # rms_final
    data.extend(rms_vec(hidden).tobytes())

    # unembed (not tied)
    data.extend(pack_ternary(tern_i8(hidden, vocab_size)))
    w('<f', 0.6)

    # medusa heads
    for _ in range(num_medusa):
        data.extend(pack_ternary(tern_i8(hidden, vocab_size)))
        w('<f', 1.3)

    # theta (feat bit2)
    if has_theta:
        w('<f', 10000.0)

    return bytes(data)


# ── CLI ──────────────────────────────────────────────────────────────────

if __name__ == '__main__':
    if '--self-check' in sys.argv:
        golden = self_check()
        out_path = 'tools/golden_v6.bin'
        with open(out_path, 'wb') as f:
            f.write(golden)
        print(f"golden_v6.bin: {len(golden)} bytes written to {out_path}")
    else:
        print("Usage: python tools/bitnet_writer.py --self-check")
        print("       (library mode: import bitnet_writer)")
