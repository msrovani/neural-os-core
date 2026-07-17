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

Sprint Sound: valida nomes/dims/tipos e imprime manifesto; NÃO implementa
forward VITS (neural-lite no kernel).
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


def validate_tensors(tensors: list[tuple[str, np.ndarray]]) -> bool:
    ok = True
    if not tensors:
        print("[ERR] nenhum tensor")
        return False
    names = {n for n, _ in tensors}
    has_emb = "emb.weight" in names or "sid" in names
    if not has_emb:
        print("[WARN] sem emb.weight/sid — neural-lite no kernel cairá em formant")
        ok = False
    for name, arr in tensors:
        if arr.dtype != np.float32:
            print(f"[ERR] {name}: dtype={arr.dtype} (esperado float32)")
            ok = False
        if arr.size == 0:
            print(f"[ERR] {name}: vazio")
            ok = False
        if not np.isfinite(arr).all():
            print(f"[ERR] {name}: NaN/Inf")
            ok = False
        if name in ("emb.weight", "sid") and arr.size % 192 != 0:
            print(f"[WARN] {name}: size={arr.size} não múltiplo de 192")
    print(f"[MANIFEST] {len(tensors)} tensors, has_emb={has_emb}")
    # Top-10 por tamanho
    top = sorted(tensors, key=lambda x: -x[1].size)[:10]
    for n, a in top:
        print(f"  {n}: {a.size} f32 ({a.size*4/1024:.1f} KB)")
    return ok


def convert_voice(voice_name: str) -> None:
    parts = voice_name.split("-", 1)
    onnx_path = TARGET / f"{voice_name}.onnx"
    if not onnx_path.exists():
        lang = parts[0]
        rest = parts[1] if len(parts) > 1 else ""
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
            print(f"[WARN] skip {name} data_type={tensor.data_type}")
            continue
        tensors.append((name, arr))

    sid = next((a for n, a in tensors if n == "sid"), None)
    if sid is not None and not any(n == "emb.weight" for n, _ in tensors):
        tensors.insert(0, ("emb.weight", sid.copy()))
        print(f"[ALIAS] emb.weight <- sid  cnt={len(sid)}")

    if not validate_tensors(tensors):
        print("[WARN] validação com avisos — exportando mesmo assim")

    out_bin = ROOT / "target" / "PIPER_PT_BR.BIN"
    out_bitnet = ROOT / "target" / f"PIPER_{voice_name.upper().replace('-', '_')}.bitnet"

    def write_file(output_path: Path) -> None:
        n = len(tensors)
        index_bytes = n * 40
        with open(output_path, "wb") as f:
            f.write(struct.pack("<IIII", MAGIC, 3, n, 0))
            f.write(b"\x00" * index_bytes)
            data_off_f32 = (16 + index_bytes) // 4
            for i, (name, arr) in enumerate(tensors):
                bname = name.encode("utf-8", errors="replace").ljust(32, b"\x00")[:32]
                cnt = int(arr.size)
                f.seek(data_off_f32 * 4)
                f.write(arr.astype(np.float32, copy=False).tobytes())
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
    print("[NOTE] Kernel usa neural-lite (emb+oscilador). VITS/HiFi-GAN = soft-float blocker.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--voice",
        default="pt_BR-cadu-medium",
        help="Voice name (e.g. pt_BR-cadu-medium, en_US-amy-medium)",
    )
    args = parser.parse_args()
    convert_voice(args.voice)
