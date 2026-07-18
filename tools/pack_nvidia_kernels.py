#!/usr/bin/env python3
"""Pack NVIDIA KernelPack (NKP1) — CUDA 12.9 → CUBIN sm_61/sm_75/sm_89.

Host-only. Does not run inside Neural OS.

Signing (optional, no secrets in-repo):
  NKP_SIGNING_SEED_HEX=<64 hex chars>  — Ed25519 seed (32 B); requires PyNaCl
  or --seed-hex / --unsigned

Without seed: emits valid FNV hash + zero signature. The OS can re-sign with
boot session key (`kernel_pack::promote_with_session`) so canary Ready works
without embedding a lab private key in git.

Without nvcc: CPU stub payload (IR=CpuStub) for envelope tests only.
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
VENDOR_NVIDIA = 1
ISA = {"sm_61": 1, "sm_75": 2, "sm_89": 3}
OP_VECTOR_ADD = 1
GOLDEN_VECTOR_ADD = 1
COMPILER_CUDA129 = 1
COMPILER_HOST = 5
IR_CUBIN = 1
IR_CPU = 4
HEADER_LEN = 48


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
    buf += struct.pack("<I", VENDOR_NVIDIA)
    buf += struct.pack("<I", isa)
    buf += struct.pack("<I", OP_VECTOR_ADD)
    buf += struct.pack("<I", GOLDEN_VECTOR_ADD)
    buf += struct.pack("<I", compiler)
    buf += struct.pack("<I", ir)
    buf += struct.pack("<I", wg)
    buf += struct.pack("<I", smem)
    buf += struct.pack("<I", plen)
    while len(buf) < HEADER_LEN:
        buf.append(0)
    return bytes(buf)


VECTOR_ADD_CU = r"""
extern "C" __global__ void vector_add(const float* a, const float* b, float* c, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) c[i] = a[i] + b[i];
}
"""


def compile_cubin(sm: str, out_cubin: Path) -> bool:
    try:
        with tempfile.TemporaryDirectory() as td:
            src = Path(td) / "vector_add.cu"
            src.write_text(VECTOR_ADD_CU, encoding="utf-8")
            cmd = [
                "nvcc",
                "-cubin",
                f"-arch={sm}",
                "-o",
                str(out_cubin),
                str(src),
            ]
            subprocess.check_call(cmd)
            return out_cubin.is_file() and out_cubin.stat().st_size > 0
    except (FileNotFoundError, subprocess.CalledProcessError) as e:
        print(f"[pack_nvidia] nvcc failed ({e}); using CPU stub payload")
        return False


def try_sign(canonical: bytes, seed_hex: str | None) -> bytes:
    """Return 64-byte Ed25519 signature, or zeros if unsigned / unavailable."""
    if not seed_hex:
        return bytes(64)
    seed_hex = seed_hex.strip().lower().replace("0x", "")
    if len(seed_hex) != 64:
        print(f"[pack_nvidia] seed must be 64 hex chars (32 B), got {len(seed_hex)}")
        return bytes(64)
    try:
        seed = bytes.fromhex(seed_hex)
    except ValueError:
        print("[pack_nvidia] invalid seed hex")
        return bytes(64)
    try:
        from nacl.signing import SigningKey  # type: ignore
    except ImportError:
        print("[pack_nvidia] PyNaCl not installed — unsigned pack (pip install pynacl)")
        return bytes(64)
    sk = SigningKey(seed)
    sig = sk.sign(canonical).signature
    pk = sk.verify_key.encode().hex()
    print(f"[pack_nvidia] signed with lab seed (pk={pk[:16]}…); add PK to identity TRUSTED if needed")
    return bytes(sig)


def pack(sm: str, out: Path, seed_hex: str | None, force_stub: bool) -> None:
    isa = ISA[sm]
    cubin_path = out.with_suffix(".cubin")
    if not force_stub and compile_cubin(sm, cubin_path):
        payload = cubin_path.read_bytes()
        compiler, ir = COMPILER_CUDA129, IR_CUBIN
        print(f"[pack_nvidia] CUBIN {sm} {len(payload)}B")
    else:
        payload = b"CPU_VECTOR_ADD_STUB\0" + sm.encode()
        compiler, ir = COMPILER_HOST, IR_CPU
        print(f"[pack_nvidia] stub payload for {sm} ({len(payload)}B)")
    hdr = build_header(isa, compiler, ir, 256, 0, len(payload))
    canonical = hdr + payload
    h = fnv1a64(canonical)
    sig = try_sign(canonical, seed_hex)
    signed = any(b != 0 for b in sig)
    out.write_bytes(canonical + struct.pack("<Q", h) + sig)
    # Mirror short name for FAT 8.3 / VFS loaders.
    print(
        f"[pack_nvidia] wrote {out} payload={len(payload)}B isa={sm} "
        f"hash={h:#x} signed={signed}"
    )


def main() -> None:
    ap = argparse.ArgumentParser(description="Build NVIDIA NKP1 KernelPack")
    ap.add_argument("--sm", choices=sorted(ISA), default="sm_61")
    ap.add_argument(
        "-o",
        "--output",
        type=Path,
        default=Path("target/NKP_SM61.BIN"),
    )
    ap.add_argument(
        "--seed-hex",
        default=os.environ.get("NKP_SIGNING_SEED_HEX"),
        help="Ed25519 seed (64 hex). Prefer env NKP_SIGNING_SEED_HEX — never commit.",
    )
    ap.add_argument(
        "--unsigned",
        action="store_true",
        help="Force zero signature (OS may promote via session key)",
    )
    ap.add_argument(
        "--stub",
        action="store_true",
        help="Force CPU stub even if nvcc is available",
    )
    args = ap.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    seed = None if args.unsigned else args.seed_hex
    pack(args.sm, args.output, seed, args.stub)


if __name__ == "__main__":
    main()
