#!/usr/bin/env python3
import struct, random, argparse, os

VOCAB_SIZE = 99; HIDDEN = 64; NUM_LAYERS = 4; NUM_HEADS = 4
HEAD_DIM = HIDDEN // NUM_HEADS; FFN_DIM = HIDDEN * 2; MEDUSA_HEADS = 3

def make_ternary(rng):
    r = rng.random()
    return -1 if r < 0.34 else (1 if r > 0.67 else 0)

def pack_ternary(weights):
    packed = bytearray()
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
    return pack_ternary([make_ternary(rng) for _ in range(rows * cols)])

def f32_tensor(rng, rows, cols):
    data = bytearray()
    for _ in range(rows * cols):
        data.extend(struct.pack('<f', rng.uniform(-1.0, 1.0)))
    return bytes(data)

def gen_model(seed=42):
    rng = random.Random(seed)
    buf = bytearray()
    buf.extend(struct.pack('<I', 0xBE11BE11))
    buf.extend(struct.pack('<H', 2))                      # version (2 = Medusa heads present)
    buf.extend(struct.pack('<I', 52000))
    buf.extend(struct.pack('<H', HIDDEN))
    buf.extend(struct.pack('<H', NUM_LAYERS))
    buf.extend(struct.pack('<H', NUM_HEADS))
    buf.extend(struct.pack('<H', VOCAB_SIZE))
    buf.extend(struct.pack('<H', 64))
    buf.extend(struct.pack('<B', MEDUSA_HEADS))            # num_medusa_heads
    buf.extend(b'\x00' * 3)                               # padding
    buf.extend(f32_tensor(rng, VOCAB_SIZE, HIDDEN))       # embed
    for _ in range(NUM_LAYERS):                           # layers
        buf.extend(struct.pack('<f', 1.0))
        buf.extend(ternary_tensor(rng, HIDDEN, HIDDEN))   # q
        buf.extend(ternary_tensor(rng, HIDDEN, HIDDEN))   # k
        buf.extend(ternary_tensor(rng, HIDDEN, HIDDEN))   # v
        buf.extend(ternary_tensor(rng, HIDDEN, HIDDEN))   # o
        buf.extend(struct.pack('<f', 1.0))
        buf.extend(ternary_tensor(rng, HIDDEN, FFN_DIM))  # gate
        buf.extend(ternary_tensor(rng, HIDDEN, FFN_DIM))  # up
        buf.extend(ternary_tensor(rng, FFN_DIM, HIDDEN))  # down
    buf.extend(ternary_tensor(rng, HIDDEN, VOCAB_SIZE))   # unembed
    for _ in range(MEDUSA_HEADS):                         # Medusa heads
        buf.extend(ternary_tensor(rng, HIDDEN, VOCAB_SIZE))
    return bytes(buf)

def main():
    parser = argparse.ArgumentParser(description='Generate micro-model for Neural OS Cortex')
    parser.add_argument('--seed', type=int, default=42)
    parser.add_argument('--output', '-o', default='micro.bitnet')
    args = parser.parse_args()
    data = gen_model(seed=args.seed)
    os.makedirs(os.path.dirname(args.output) or '.', exist_ok=True)
    with open(args.output, 'wb') as f: f.write(data)
    print(f"Generated {args.output}: {len(data)} bytes")
    print(f"  hidden={HIDDEN}, layers={NUM_LAYERS}, heads={NUM_HEADS}, medusa={MEDUSA_HEADS}")

if __name__ == '__main__':
    main()
