#!/usr/bin/env python3
"""Converte modelo BitNet safetensors (HuggingFace) para .bitnet v4.
Uso: python tools/convert_safetensors_to_bitnet.py [--hf microsoft/bitnet-b1.58-2B-4T]
"""
import argparse, json, os, struct, sys, tempfile
from pathlib import Path

TARGET = Path(__file__).parent / "target"

def download_hf(repo_id, filename, dest):
    from huggingface_hub import hf_hub_download
    print(f"  [DL] {filename}")
    downloaded = hf_hub_download(repo_id=repo_id, filename=filename, local_dir=dest.parent)
    return downloaded

def convert_safetensors_to_bitnet(safetensors_path, config_path, output_path):
    import torch
    from safetensors.torch import load_file

    print(f"  [LOAD] {safetensors_path}")
    state = load_file(safetensors_path)

    with open(config_path) as f:
        cfg = json.load(f)

    hidden = cfg["hidden_size"]
    num_layers = cfg["num_hidden_layers"]
    num_heads = cfg["num_attention_heads"]
    vocab_size = cfg["vocab_size"]
    max_seq = cfg.get("max_position_embeddings", 2048)
    intermediate_size = cfg["intermediate_size"]
    num_kv_heads = cfg.get("num_key_value_heads", num_heads)
    # BitNet-b1.58-2B-4T: HF head_dim=128 é enganoso; packing real = head_dim=32.
    if hidden == 2560 and num_heads == 20 and num_kv_heads == 5:
        head_dim = 32
    else:
        head_dim = cfg.get("head_dim", hidden // num_heads)
    q_dim = head_dim * num_heads
    tie_embeddings = cfg.get("tie_word_embeddings", False)

    print(f"  Model: hidden={hidden} layers={num_layers} heads={num_heads}")
    print(f"  Vocab: {vocab_size} max_seq={max_seq} ffn={intermediate_size}")
    print(f"  q_dim={q_dim} (head_dim={head_dim}) kv_heads={num_kv_heads}")

    # Write .bitnet v4 header
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
        f.write(struct.pack("<I", 0))  # num_medusa
        f.write(b"TIED" if tie_embeddings else b"\x00\x00\x00\x00")
        f.write(struct.pack("B", 1))
        # Tokenizer data (simplified char-level for now)
        tok_data = b"CHAR:32-126"
        f.write(struct.pack("<I", len(tok_data)))
        f.write(tok_data)
        f.write(struct.pack("B", 0x07))

        # Write tensors
        def quantize_ternary(arr_1d, threshold=0.5):
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
            flat = tensor.cpu().numpy().reshape(-1)
            f.write(struct.pack("<I", len(flat)))
            f.write(struct.pack("<I", 0))
            data = quantize_ternary(flat)
            f.write(data)
            print(f"  [TENSOR] {name}: {tensor.shape} -> {len(data)} bytes")

        sz = os.path.getsize(output_path)
        print(f"\n  [OK] {output_path}: {sz:,} bytes ({sz/1024/1024:.1f} MB)")

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--hf", default="microsoft/bitnet-b1.58-2B-4T")
    parser.add_argument("--output", default=None)
    args = parser.parse_args()

    TARGET.mkdir(exist_ok=True)
    repo = args.hf
    out = args.output or (TARGET / f"{repo.replace('/', '_')}.bitnet")

    print(f"=== Convertendo {repo} ===")

    # Download config first
    config_path = Path(tempfile.mktemp(suffix=".json"))
    print(f"  [CFG] Baixando config.json...")
    download_hf(repo, "config.json", config_path)

    # Download model weights
    model_path = Path(tempfile.mktemp(suffix=".safetensors"))
    download_hf(repo, "model.safetensors", model_path)

    convert_safetensors_to_bitnet(str(model_path), str(config_path), str(out))

    os.unlink(config_path)
    os.unlink(model_path)

if __name__ == "__main__":
    main()
