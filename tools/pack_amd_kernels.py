#!/usr/bin/env python3
"""Pack AMD KernelPack (NKP1) — clang amdgcn → HSACO (ADR-0049 P1).

Host-only. gfx1030 (RDNA2) primário; gfx90c / gfx1036 / gfx1103 / gfx1100.
Unsigned: OS promote_with_session at boot (same as NVIDIA/Intel).
"""
from __future__ import annotations

import argparse
import os
import struct
import subprocess
import tempfile
from pathlib import Path

MAGIC = b"NKP1"
ABI = 1
VENDOR_AMD = 2
ISA_TAG = {
    "gfx90c": 4,
    "gfx1036": 5,
    "gfx1103": 6,
    "gfx1100": 6,
    "gfx1030": 7,
}
OP_VECTOR_ADD = 1
GOLDEN = 1
COMPILER_CLANG = 2
COMPILER_HOST = 5
IR_HSACO = 2
IR_CPU = 4
HEADER_LEN = 48

CL_SRC = r"""
__kernel void vector_add(__global const float* a, __global const float* b, __global float* c, int n) {
    int i = get_global_id(0);
    if (i < n) c[i] = a[i] + b[i];
}
"""


def fnv1a64(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for b in data:
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def build_header(isa: int, compiler: int, ir: int, wg: int, smem: int, plen: int) -> bytes:
    buf = bytearray()
    buf += MAGIC
    buf += struct.pack("<I", ABI)
    buf += struct.pack("<I", VENDOR_AMD)
    buf += struct.pack("<I", isa)
    buf += struct.pack("<I", OP_VECTOR_ADD)
    buf += struct.pack("<I", GOLDEN)
    buf += struct.pack("<I", compiler)
    buf += struct.pack("<I", ir)
    buf += struct.pack("<I", wg)
    buf += struct.pack("<I", smem)
    buf += struct.pack("<I", plen)
    while len(buf) < HEADER_LEN:
        buf.append(0)
    return bytes(buf)


def try_clang_amdgcn(gfx: str, out: Path) -> bool:
    try:
        with tempfile.TemporaryDirectory() as td:
            src = Path(td) / "va.cl"
            src.write_text(CL_SRC, encoding="utf-8")
            bc = Path(td) / "va.bc"
            obj = Path(td) / "va.o"
            # OpenCL C → bitcode → amdgcn object (needs libclc / ROCm clang)
            subprocess.check_call(
                [
                    "clang",
                    "-O2",
                    "-target",
                    f"amdgcn-amd-amdhsa",
                    f"-mcpu={gfx}",
                    "-c",
                    "-emit-llvm",
                    str(src),
                    "-o",
                    str(bc),
                ]
            )
            subprocess.check_call(
                [
                    "clang",
                    "-O2",
                    "-target",
                    f"amdgcn-amd-amdhsa",
                    f"-mcpu={gfx}",
                    "-c",
                    str(bc),
                    "-o",
                    str(obj),
                ]
            )
            if obj.is_file() and obj.stat().st_size > 0:
                out.write_bytes(obj.read_bytes())
                return True
    except (FileNotFoundError, subprocess.CalledProcessError):
        pass
    return False


def try_sign(canonical: bytes, seed_hex: str | None) -> bytes:
    if not seed_hex:
        return bytes(64)
    seed_hex = seed_hex.strip().lower().replace("0x", "")
    if len(seed_hex) != 64:
        return bytes(64)
    try:
        from nacl.signing import SigningKey  # type: ignore

        sk = SigningKey(bytes.fromhex(seed_hex))
        return bytes(sk.sign(canonical).signature)
    except Exception as e:
        print(f"[pack_amd] sign skipped ({e})")
        return bytes(64)


def pack(gfx: str, out: Path, seed_hex: str | None, force_stub: bool) -> None:
    isa = ISA_TAG[gfx]
    hs = out.with_suffix(".hsaco")
    if not force_stub and try_clang_amdgcn(gfx, hs):
        payload = hs.read_bytes()
        compiler, ir = COMPILER_CLANG, IR_HSACO
        print(f"[pack_amd] hsaco {gfx} {len(payload)}B")
    else:
        print("[pack_amd] clang/amdgcn unavailable / --stub; CPU stub payload")
        payload = b"CPU_VECTOR_ADD_STUB\0" + gfx.encode()
        compiler, ir = COMPILER_HOST, IR_CPU
    hdr = build_header(isa, compiler, ir, 256, 0, len(payload))
    canonical = hdr + payload
    h = fnv1a64(canonical)
    sig = try_sign(canonical, seed_hex)
    out.write_bytes(canonical + struct.pack("<Q", h) + sig)
    print(f"[pack_amd] wrote {out} ({len(payload)}B, {gfx}, hash={h:#x})")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--gfx", choices=sorted(ISA_TAG), default="gfx1030")
    ap.add_argument("-o", "--output", type=Path, default=Path("target/NKP_GFX1030.BIN"))
    ap.add_argument("--seed-hex", default=os.environ.get("NKP_SIGNING_SEED_HEX"))
    ap.add_argument("--unsigned", action="store_true")
    ap.add_argument("--stub", action="store_true")
    args = ap.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    seed = None if args.unsigned else args.seed_hex
    pack(args.gfx, args.output, seed, args.stub)


if __name__ == "__main__":
    main()
