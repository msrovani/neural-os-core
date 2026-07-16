#!/usr/bin/env python3
"""Converte Piper TTS ONNX → .bitnet para kernel neural-os-core.
Uso: python tools/convert_piper_to_bitnet.py --voice pt_BR-cadu-medium

Formato (v3):
  magic u32=0xBE11BE11 | version u32=3 | nparts u32 | reserved u32
  index: nparts × (name[32] + offset_f32 u32 + count u32)
  payload: f32 blobs contíguos

Nota: no ONNX Piper, o embedding de fonemas é o tensor `sid` [256,192]
(Gather(sid, input)). Gravamos também alias `emb.weight` apontando aos
mesmos floats para o loader do kernel.
"""
import argparse
import os
import struct
import sys
from pathlib import Path

import numpy as np
import onnx

MAGIC = 0xBE11BE11
ROOT = Path(__file__).parent.parent
TARGET = ROOT / "target" / "piper"


def convert_voice(voice_name: str) -> None:
    parts = voice_name.split("-", 1)
    onnx_path = TARGET / f"{voice_name}.onnx"
    # Layout HF-style: target/piper/pt/pt_BR/cadu/medium/pt_BR-cadu-medium.onnx
    if not onnx_path.exists():
        lang = parts[0]
        rest = parts[1] if len(parts) > 1 else ""
        # pt_BR-cadu-medium → pt/pt_BR/cadu/medium/
        bits = rest.split("-")
        if len(bits) >= 2:
            speaker, quality = bits[0], bits[-1]
            cand = (
                TARGET
                / lang.split("_")[0]
                / lang
                / speaker
                / quality
                / f"{voice_name}.onnx"
            )
            if cand.exists():
                onnx_path = cand
    if not onnx_path.exists():
        print(f"[ERR] {onnx_path} not found")
        return

    print(f"[LOAD] {onnx_path}")
    model = onnx.load(str(onnx_path))
    weights = {init.name: init for init in model.graph.initializer}

    tensors: list[tuple[str, np.ndarray]] = []
    for name, tensor in weights.items():
        raw = tensor.raw_data
        if tensor.data_type == 1:  # FLOAT
            arr = np.frombuffer(raw, dtype=np.float32).copy()
        elif tensor.data_type == 7:  # INT64
            arr = np.frombuffer(raw, dtype=np.int64).astype(np.float32)
        else:
            arr = np.frombuffer(raw, dtype=np.float32).copy()
        tensors.append((name, arr))

    # Alias: sid [V,192] é o emb de fonemas no export Piper ONNX
    sid = next((a for n, a in tensors if n == "sid"), None)
    if sid is not None and not any(n == "emb.weight" for n, _ in tensors):
        tensors.insert(0, ("emb.weight", sid.copy()))
        print(f"[ALIAS] emb.weight <- sid  shape-ish cnt={len(sid)}")

    out_bin = ROOT / "target" / "PIPER_PT_BR.BIN"
    out_bitnet = ROOT / "target" / f"PIPER_{voice_name.upper().replace('-', '_')}.bitnet"

    def write_file(output_path: Path) -> None:
        n = len(tensors)
        index_bytes = n * 40
        with open(output_path, "wb") as f:
            f.write(struct.pack("<IIII", MAGIC, 3, n, 0))
            # placeholder index
            f.write(b"\x00" * index_bytes)
            data_off_f32 = (16 + index_bytes) // 4
            for i, (name, arr) in enumerate(tensors):
                bname = name.encode("utf-8", errors="replace").ljust(32, b"\x00")[:32]
                cnt = int(arr.size)
                # write payload
                f.seek(data_off_f32 * 4)
                f.write(arr.astype(np.float32, copy=False).tobytes())
                # write index entry
                f.seek(16 + i * 40)
                f.write(bname)
                f.write(struct.pack("<II", data_off_f32, cnt))
                data_off_f32 += cnt
            f.seek(8)
            f.write(struct.pack("<I", n))
        sz = os.path.getsize(output_path)
        print(f"[OK] {output_path}: {sz:,} bytes ({sz/1024/1024:.1f} MB, {n} tensors)")

    write_file(out_bin)
    write_file(out_bitnet)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--voice",
        default="pt_BR-cadu-medium",
        help="Voice name (e.g. pt_BR-cadu-medium, en_US-amy-medium)",
    )
    args = parser.parse_args()
    convert_voice(args.voice)
