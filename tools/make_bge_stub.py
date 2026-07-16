#!/usr/bin/env python3
"""Gera BGE.BIN mínimo válido para load_bge (STATUS=LOADED em QEMU).

Formato (memory_systems::load_bge):
  magic(4) + ver(4) + vocab(4) + hidden(4) + layers(4) + ffn(4) + heads(4) + max_seq(4)
  + model_type(16) + ntensors(4)
  + tensor: name[64] + n_orig(u32) + n_quant(u32) + f32[n_orig] + quant[n_quant]
"""
import os, struct, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "target", "bge-small.bitnet")
MAGIC = 0xBE11BE11
VOCAB = 256
HIDDEN = 384


def main():
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    n_orig = VOCAB * HIDDEN
    with open(OUT, "wb") as f:
        f.write(struct.pack("<8I", MAGIC, 1, VOCAB, HIDDEN, 1, 0, 12, 512))
        f.write(b"bge_stub_v1\x00\x00\x00\x00\x00")  # 16 bytes model_type
        f.write(struct.pack("<I", 1))  # ntensors
        name = b"word_embeddings_weight"
        f.write(name.ljust(64, b"\x00"))
        f.write(struct.pack("<II", n_orig, 0))
        # Embeddings pequenos determinísticos (não-zero) — só para parse/STATUS
        for i in range(n_orig):
            f.write(struct.pack("<f", ((i % 97) - 48) / 97.0))
    alias = os.path.join(ROOT, "target", "BGE.BIN")
    with open(OUT, "rb") as src, open(alias, "wb") as dst:
        dst.write(src.read())
    sz = os.path.getsize(OUT)
    print(f"[OK] {OUT} + target/BGE.BIN ({sz} bytes, vocab={VOCAB} hidden={HIDDEN})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
