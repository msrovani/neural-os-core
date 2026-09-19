#!/usr/bin/env python3
"""Pack NVIDIA KernelPack (NKP1) — nvcc → CUBIN sm_52..sm_89.

Host-only. Does not run inside Neural OS.
Lab: CTK 13.4 → sm_75+ (sm_86 = RTX 3050); CTK 12.9 still needed for ≤sm_70.

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
ISA = {
    "sm_52": 10,
    "sm_61": 1,
    "sm_70": 11,
    "sm_75": 2,
    "sm_80": 12,
    "sm_86": 13,
    "sm_89": 3,
}
OP_VECTOR_ADD = 1
OP_W2A8 = 2
GOLDEN_VECTOR_ADD = 1
GOLDEN_W2A8 = 2
COMPILER_CUDA129 = 1
COMPILER_RUST_CUDA = 6
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


def build_header(
    isa: int, op: int, golden: int, compiler: int, ir: int, wg: int, smem: int, plen: int
) -> bytes:
    buf = bytearray()
    buf += MAGIC
    buf += struct.pack("<I", ABI)
    buf += struct.pack("<I", VENDOR_NVIDIA)
    buf += struct.pack("<I", isa)
    buf += struct.pack("<I", op)
    buf += struct.pack("<I", golden)
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

# BitLinear W2A8 decode GEMV (M=1): signed i8 act × ternary w — DP4A path (sm_61+).
# Layout: x_i8[K], w_i8 col-major [N*K], out[N] = si * sum(x*w). Host golden = gpu_kernels.
W2A8_CU = r"""
extern "C" __global__ void bitlinear_w2a8(
    const char* __restrict__ x_i8,
    const char* __restrict__ w_i8,
    float* __restrict__ out,
    float si,
    int K,
    int N
) {
    int j = blockIdx.x * blockDim.x + threadIdx.x;
    if (j >= N) return;
    int acc = 0;
    const char* wj = w_i8 + (size_t)j * (size_t)K;
    for (int t = 0; t < K; ++t) {
        acc += (int)x_i8[t] * (int)wj[t];
    }
    out[j] = si * (float)acc;
}
"""


def compile_cubin(sm: str, out_cubin: Path, source_text: str, src_name: str) -> bool:
    try:
        with tempfile.TemporaryDirectory() as td:
            src = Path(td) / src_name
            src.write_text(source_text, encoding="utf-8")
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


def pack(
    sm: str,
    out: Path,
    seed_hex: str | None,
    force_stub: bool,
    op: str = "vector_add",
    source: str = "cu",
) -> None:
    isa = ISA[sm]
    if op == "w2a8":
        op_id, golden = OP_W2A8, GOLDEN_W2A8
        stub_tag = b"CPU_W2A8_STUB\0"
        cu_src, cu_name = W2A8_CU, "bitlinear_w2a8.cu"
    else:
        op_id, golden = OP_VECTOR_ADD, GOLDEN_VECTOR_ADD
        stub_tag = b"CPU_VECTOR_ADD_STUB\0"
        cu_src, cu_name = VECTOR_ADD_CU, "vector_add.cu"
    cubin_path = out.with_suffix(".cubin")
    # Track Rust (--source rust): host tool only; without Rust-CUDA/nvptx → stub honest.
    if source == "rust" and not force_stub:
        print(
            f"[pack_nvidia] --source rust op={op}: Rust-CUDA/nvptx host track — "
            f"sem toolchain → stub (CTK 12.9 p/ ISA≤sm_70 se --source cu)"
        )
        payload = stub_tag + f"rust_{sm}".encode()
        compiler, ir = COMPILER_RUST_CUDA, IR_CPU
        print(f"[pack_nvidia] stub payload op={op} source=rust for {sm} ({len(payload)}B)")
    elif not force_stub and source == "cu" and compile_cubin(sm, cubin_path, cu_src, cu_name):
        payload = cubin_path.read_bytes()
        compiler, ir = COMPILER_CUDA129, IR_CUBIN
        print(f"[pack_nvidia] CUBIN {sm} op={op} {len(payload)}B")
    else:
        payload = stub_tag + sm.encode()
        compiler, ir = COMPILER_HOST, IR_CPU
        print(f"[pack_nvidia] stub payload op={op} for {sm} ({len(payload)}B)")
    hdr = build_header(isa, op_id, golden, compiler, ir, 256, 0, len(payload))
    canonical = hdr + payload
    h = fnv1a64(canonical)
    sig = try_sign(canonical, seed_hex)
    signed = any(b != 0 for b in sig)
    out.write_bytes(canonical + struct.pack("<Q", h) + sig)
    print(
        f"[pack_nvidia] wrote {out} op={op} source={source} payload={len(payload)}B isa={sm} "
        f"hash={h:#x} signed={signed}"
    )


def main() -> None:
    ap = argparse.ArgumentParser(description="Build NVIDIA NKP1 KernelPack")
    ap.add_argument("--sm", choices=sorted(ISA), default="sm_61")
    ap.add_argument("--op", choices=["vector_add", "w2a8"], default="vector_add")
    ap.add_argument(
        "--source",
        choices=["cu", "rust"],
        default="cu",
        help="cu=nvcc CUBIN (CTK 12.9 for ≤sm_70); rust=Rust-CUDA/nvptx host track",
    )
    ap.add_argument(
        "-o",
        "--output",
        type=Path,
        default=None,
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
    if args.output is None:
        if args.op == "w2a8":
            args.output = Path(f"target/NKP_W2A8_{args.sm.upper()}.BIN")
        else:
            args.output = Path(f"target/NKP_{args.sm.upper().replace('SM_', 'SM')}.BIN")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    seed = None if args.unsigned else args.seed_hex
    pack(args.sm, args.output, seed, args.stub, args.op, args.source)


if __name__ == "__main__":
    main()
