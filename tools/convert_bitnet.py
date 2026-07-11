#!/usr/bin/env python3
"""Converte modelo BitNet safetensors para .bitnet v4.
Uso: python tools/convert_bitnet.py
"""
import json, os, struct, sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
TARGET = ROOT / "target"
MODEL_DIR = TARGET

def convert():
    safetensors_path = MODEL_DIR / "model.safetensors"
    config_path = MODEL_DIR / "config.json"
    output_path = TARGET / "bitnet_2B.bitnet"

    if not safetensors_path.exists():
        print(f"[ERR] {safetensors_path} not found. Run download first.")
        return

    import torch
    from safetensors.torch import load_file

    print(f"[LOAD] {safetensors_path} ({os.path.getsize(safetensors_path)/1e9:.1f}GB)")
    state = load_file(str(safetensors_path))

    with open(config_path) as f:
        cfg = json.load(f)

    hidden = cfg["hidden_size"]
    num_layers = cfg["num_hidden_layers"]
    num_heads = cfg["num_attention_heads"]
    vocab_size = cfg["vocab_size"]
    max_seq = cfg.get("max_position_embeddings", 2048)
    intermediate_size = cfg["intermediate_size"]
    num_kv_heads = cfg.get("num_key_value_heads", num_heads)
    q_dim = cfg.get("head_dim", hidden // num_heads) * num_heads

    print(f"  hidden={hidden} layers={num_layers} heads={num_heads} vocab={vocab_size}")

    MAGIC = 0xBE11BE11
    with open(output_path, "wb") as f:
        num_params = sum(t.numel() for t in state.values())
        f.write(struct.pack("<I", MAGIC))
        f.write(struct.pack("<H", 4))
        f.write(struct.pack("<I", num_params))
        f.write(struct.pack("<H", hidden))
        f.write(struct.pack("<H", num_layers))
        f.write(struct.pack("<H", num_heads))
        f.write(struct.pack("<I", vocab_size))
        f.write(struct.pack("<H", min(max_seq, 65535)))
        f.write(struct.pack("<H", intermediate_size))
        f.write(struct.pack("<H", num_kv_heads))
        f.write(struct.pack("<H", q_dim))
        f.write(struct.pack("<I", 0))
        f.write(b"\x00\x00\x00\x00")
        f.write(struct.pack("B", 1))
        tok_data = b"CHAR:32-126"
        f.write(struct.pack("<I", len(tok_data)))
        f.write(tok_data)
        f.write(struct.pack("B", 0x07))

        def quantize(arr_1d, threshold=0.5):
            packed = bytearray()
            for i in range(0, len(arr_1d), 4):
                byte = 0
                for j in range(4):
                    if i + j < len(arr_1d):
                        v = float(arr_1d[i + j])
                        bits = 0b01 if v > threshold else (0b10 if v < -threshold else 0b00)
                        byte |= bits << (j * 2)
                packed.append(byte)
            return bytes(packed)

        for name, tensor in state.items():
            flat = tensor.cpu().to(torch.float32).numpy().reshape(-1)
            f.write(struct.pack("<I", len(flat)))
            f.write(struct.pack("<I", 0))
            data = quantize(flat)
            f.write(data)
            print(f"  [T] {name}: {tensor.shape} -> {len(data)//1024}KB")

        sz = os.path.getsize(output_path)
        print(f"\n[OK] {output_path}: {sz:,} bytes ({sz/1024/1024:.1f} MB)")

if __name__ == "__main__":
    convert()
