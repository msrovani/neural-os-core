#!/usr/bin/env python3
"""Pack Intel KernelPack (NKP1) — ocloc/IGC → zebin Gen9 | Arc (dg2).

Host-only. Gen9 and Arc are separate ISA tags / backends.
Unsigned packs: OS promote_with_session at boot (same as NVIDIA).
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
VENDOR_INTEL = 3
ISA = {"gen9": 8, "dg2": 9}
OP_VECTOR_ADD = 1
GOLDEN = 1
COMPILER_OCLOC = 4
COMPILER_HOST = 5
IR_ZEBIN = 3
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
    buf += struct.pack("<I", VENDOR_INTEL)
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


def try_ocloc(device: str, out: Path) -> bool:
    try:
        with tempfile.TemporaryDirectory() as td:
            src = Path(td) / "va.cl"
            src.write_text(CL_SRC, encoding="utf-8")
            cmd = ["ocloc", "bin", "-device", device, "-file", str(src), "-out_dir", td]
            subprocess.check_call(cmd)
            bins = list(Path(td).glob("*.bin")) + list(Path(td).glob("*.spv"))
            if not bins:
                return False
            out.write_bytes(bins[0].read_bytes())
            return True
    except (FileNotFoundError, subprocess.CalledProcessError):
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
        print(f"[pack_intel] sign skipped ({e})")
        return bytes(64)


def pack(isa_name: str, out: Path, seed_hex: str | None, force_stub: bool) -> None:
    isa = ISA[isa_name]
    device = "skl" if isa_name == "gen9" else "dg2"
    zb = out.with_suffix(".zebin")
    if not force_stub and try_ocloc(device, zb):
        payload = zb.read_bytes()
        compiler, ir = COMPILER_OCLOC, IR_ZEBIN
        print(f"[pack_intel] zebin {isa_name} {len(payload)}B")
    else:
        print("[pack_intel] ocloc unavailable / --stub; CPU stub payload")
        payload = b"CPU_VECTOR_ADD_STUB\0" + isa_name.encode()
        compiler, ir = COMPILER_HOST, IR_CPU
    hdr = build_header(isa, compiler, ir, 16, 0, len(payload))
    canonical = hdr + payload
    h = fnv1a64(canonical)
    sig = try_sign(canonical, seed_hex)
    out.write_bytes(canonical + struct.pack("<Q", h) + sig)
    print(f"[pack_intel] wrote {out} ({len(payload)}B, {isa_name}, hash={h:#x})")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--isa", choices=sorted(ISA), default="gen9")
    ap.add_argument("-o", "--output", type=Path, default=Path("target/NKP_GEN9.BIN"))
    ap.add_argument("--seed-hex", default=os.environ.get("NKP_SIGNING_SEED_HEX"))
    ap.add_argument("--unsigned", action="store_true")
    ap.add_argument("--stub", action="store_true")
    args = ap.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    seed = None if args.unsigned else args.seed_hex
    pack(args.isa, args.output, seed, args.stub)


if __name__ == "__main__":
    main()
