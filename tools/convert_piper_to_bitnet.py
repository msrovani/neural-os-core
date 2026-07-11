#!/usr/bin/env python3
"""Converte Piper TTS ONNX → .bitnet para kernel neural-os-core.
Uso: python tools/convert_piper_to_bitnet.py --voice pt_BR-cadu-medium
"""
import argparse, json, os, struct, sys
from pathlib import Path

import onnx
import numpy as np

MAGIC = 0xBE11BE11
ROOT = Path(__file__).parent.parent
TARGET = ROOT / "target" / "piper"

def convert_voice(voice_name):
    """Converte uma voz Piper para .bitnet"""
    parts = voice_name.split("-", 1)
    lang, name_qual = parts[0], parts[1]
    qual = "medium" if "medium" in name_qual else "low"
    name = name_qual.replace("-medium", "").replace("-low", "")

    onnx_path = TARGET / f"{voice_name}.onnx"
    if not onnx_path.exists():
        print(f"[ERR] {onnx_path} not found")
        return

    print(f"[LOAD] {onnx_path}")
    model = onnx.load(str(onnx_path))
    graph = model.graph
    weights = {init.name: init for init in graph.initializer}

    # Escreve .bitnet no estilo tensorpart (igual Pocket TTS)
    output_path = ROOT / "target" / f"PIPER_{voice_name.upper()}.bitnet"
    with open(output_path, "wb") as f:
        # Header
        f.write(struct.pack("<I", MAGIC))
        f.write(struct.pack("<I", 3))   # version
        f.write(struct.pack("<I", len(weights)))  # nparts
        f.write(struct.pack("<I", 0))   # reserved

        # Coleta tensores f32
        tensors = []
        for name, tensor in weights.items():
            raw = tensor.raw_data
            if tensor.data_type == 1:  # FLOAT
                arr = np.frombuffer(raw, dtype=np.float32).copy()
            elif tensor.data_type == 7:  # INT64
                arr = np.frombuffer(raw, dtype=np.int64).astype(np.float32)
            else:
                arr = np.frombuffer(raw, dtype=np.float32)
            tensors.append((name, arr))

        # Escreve indice (nome, offset, count)
        data_start = 16 + len(tensors) * 40
        data_off = 0
        for name, arr in tensors:
            bname = name.encode().ljust(32, b'\x00')[:32]
            cnt = len(arr)
            f.seek(16 + len(tensors) * 40 + data_off)
            data_start = f.tell()
            f.write(arr.tobytes())
            f.seek(16 + tensors.index((name, arr)) * 40)
            f.write(bname)
            f.write(struct.pack("<I", data_start // 4))
            f.write(struct.pack("<I", cnt))
            data_off += cnt * 4

        # Atualiza nparts
        f.seek(8)
        f.write(struct.pack("<I", len(tensors)))

        sz = os.path.getsize(output_path)
        print(f"[OK] {output_path}: {sz:,} bytes ({sz/1024/1024:.1f} MB, {len(tensors)} tensors)")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--voice", default="pt_BR-cadu-medium",
                       help="Voice name (e.g. pt_BR-cadu-medium, en_US-amy-medium)")
    args = parser.parse_args()
    convert_voice(args.voice)
