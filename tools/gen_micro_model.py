#!/usr/bin/env python3
"""Generate micro-model weights for Neural OS Hermes Cortex.
Produces a .bitnet file with random ternary weights for development.

Usage:
    python gen_micro_model.py
    python gen_micro_model.py --seed 42 --output micro.bitnet
"""

import struct
import random
import argparse
import os

VOCAB_SIZE = 99
HIDDEN = 64
NUM_LAYERS = 4
NUM_HEADS = 4
HEAD_DIM = HIDDEN // NUM_HEADS
FFN_DIM = HIDDEN * 2

def make_ternary(rng):
    """Return -1, 0, or +1 with roughly equal probability."""
    r = rng.random()
    if r < 0.34:
        return -1
    elif r < 0.67:
        return 0
    else:
        return 1

def pack_ternary(weights):
    """Pack 4 ternary weights (-1,0,+1) into one byte (2-bit encoding)."""
    packed = []
    for i in range(0, len(weights), 4):
        byte = 0
        for j in range(4):
            if i + j < len(weights):
                v = weights[i + j]
                bits = 0b00 if v == 0 else (0b01 if v == 1 else 0b10)
                byte |= bits << (j * 2)
        packed.append(byte)
    return bytes(packed)

def ternary_tensor(rng, rows, cols):
    """Generate random ternary weight matrix and pack it."""
    weights = [make_ternary(rng) for _ in range(rows * cols)]
    return pack_ternary(weights)

def f32_tensor(rng, rows, cols):
    """Generate random f32 weight matrix."""
    data = bytearray()
    for _ in range(rows * cols):
        val = rng.uniform(-1.0, 1.0)
        data.extend(struct.pack('<f', val))
    return bytes(data)

def gen_model(seed=42):
    """Generate complete micro-model weights."""
    rng = random.Random(seed)
    buf = bytearray()

    # Header
    buf.extend(struct.pack('<I', 0xBE11BE11))  # magic
    buf.extend(struct.pack('<H', 1))            # version
    buf.extend(struct.pack('<I', 52000))        # num_params (approx)
    buf.extend(struct.pack('<H', HIDDEN))       # hidden_dim
    buf.extend(struct.pack('<H', NUM_LAYERS))   # num_layers
    buf.extend(struct.pack('<H', NUM_HEADS))    # num_heads
    buf.extend(struct.pack('<H', VOCAB_SIZE))   # vocab_size
    buf.extend(struct.pack('<H', 64))           # max_seq_len
    buf.extend(b'\x00' * 4)                    # reserved

    # Embedding table: f32 [VOCAB_SIZE, HIDDEN]
    buf.extend(f32_tensor(rng, VOCAB_SIZE, HIDDEN))

    # Layers
    for _ in range(NUM_LAYERS):
        buf.extend(struct.pack('<f', 1.0))      # rms_attn_weight
        buf.extend(ternary_tensor(rng, HIDDEN, HIDDEN))  # q_proj
        buf.extend(ternary_tensor(rng, HIDDEN, HIDDEN))  # k_proj
        buf.extend(ternary_tensor(rng, HIDDEN, HIDDEN))  # v_proj
        buf.extend(ternary_tensor(rng, HIDDEN, HIDDEN))  # o_proj
        buf.extend(struct.pack('<f', 1.0))      # rms_ffn_weight
        buf.extend(ternary_tensor(rng, HIDDEN, FFN_DIM))  # gate_proj
        buf.extend(ternary_tensor(rng, HIDDEN, FFN_DIM))  # up_proj
        buf.extend(ternary_tensor(rng, FFN_DIM, HIDDEN))  # down_proj

    # Unembedding table: ternary [HIDDEN, VOCAB_SIZE]
    buf.extend(ternary_tensor(rng, HIDDEN, VOCAB_SIZE))

    return bytes(buf)

def main():
    parser = argparse.ArgumentParser(description='Generate micro-model for Neural OS Cortex')
    parser.add_argument('--seed', type=int, default=42, help='Random seed')
    parser.add_argument('--output', '-o', default='micro.bitnet', help='Output file')
    args = parser.parse_args()

    data = gen_model(seed=args.seed)
    os.makedirs(os.path.dirname(args.output) or '.', exist_ok=True)
    with open(args.output, 'wb') as f:
        f.write(data)

    size = len(data)
    param_count = size * 4  # approx: each byte packs 4 ternary weights
    print(f"Generated {args.output}: {size} bytes (~{param_count} ternary params)")
    print(f"  hidden_dim={HIDDEN}, layers={NUM_LAYERS}, heads={NUM_HEADS}")
    print(f"  vocab={VOCAB_SIZE}, ffn_dim={FFN_DIM}")
    print(f"  Embed + 4 layers + Unembed = ~{param_count // 1000}K ternary params")

if __name__ == '__main__':
    main()
