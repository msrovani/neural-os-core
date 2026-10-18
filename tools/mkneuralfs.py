#!/usr/bin/env python3
"""Seed lab NeuralFS (s493 / NeuralFS.md §14) — NÃO é soberano; produto = SysInstaller.

Gera disk_*.neural.raw com MBR 0x7F + NeuralFS @ LBA 2048 via teste host k-nano:
  export_lab_seed (cargo test --ignored).

Uso:
  python tools/mkneuralfs.py --size 2048 --output target/disk_qemu.neural.raw
  PACK_LLM=falcon3 python tools/mkneuralfs.py   # empacota Active/Learner se existirem

BOOT.LOG NÃO vai neste volume (canal DEV = ESP).
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def find_blob(*names: str) -> str | None:
    for d in (
        os.path.join(ROOT, "target1"),
        os.path.join(ROOT, "models"),
        ROOT,
    ):
        for n in names:
            p = os.path.join(d, n)
            if os.path.isfile(p) and os.path.getsize(p) > 64:
                return p
    return None


def build_manifest(pack: set[str], path: str) -> None:
    lines: list[str] = ["# path_nrfs|host_file"]
    mapping = []
    if "falcon3" in pack or "all" in pack or not pack:
        # default: tenta Active se existir
        pass
    if "falcon3" in pack or "all" in pack:
        p = find_blob("FALCON3.V7", "FALCON3_BASE.V7", "FALCON3_BASE.V6")
        if p:
            mapping.append(("ecosystem/models/active.v7", p))
    if "falcon3-1b" in pack or "all" in pack or "1b" in pack:
        p = find_blob("LEARNER.V7", "FALCON1B.V7", "FALCON3_1B.V7")
        if p:
            mapping.append(("ecosystem/models/learner.v7", p))
    if "falcon3-7b" in pack or "all" in pack or "7b" in pack or "pro" in pack:
        p = find_blob("PRO.V7", "FALCON7B.V7", "FALCON3_7B.V7")
        if p:
            mapping.append(("ecosystem/models/pro.v7", p))
    if "falcon3-10b" in pack or "all" in pack or "10b" in pack:
        p = find_blob("FALCON10.V7", "FALCON3_10B.V7")
        if p:
            mapping.append(("ecosystem/models/falcon10.v7", p))
    # Nichos: só se pediu pack (não no smoke PACK_LLM=none)
    if pack:
        for nrfs, hosts in (
            ("ecosystem/models/vision.bin", ("VISION.BIN",)),
            ("ecosystem/models/reranker.bin", ("RERANKER.BIN",)),
            ("ecosystem/models/rustcdr.bin", ("RUSTCDR3.BIN",)),
            ("ecosystem/models/hwexpert.bitnet", ("hw_expert_v6.bitnet", "HW_EXPERT.BITNET")),
            (
                "ecosystem/models/piper.bin",
                ("PIPER.BIN", "PIPER_PT_BR.BIN", "piper.onnx"),
            ),
            ("ecosystem/models/bge.bin", ("BGE.BIN", "BGE_M3.BIN")),
            ("ecosystem/models/stt.bin", ("STT.BIN",)),
        ):
            p = find_blob(*hosts)
            if p:
                mapping.append((nrfs, p))
        if not any(m[0].endswith("active.v7") for m in mapping):
            p = find_blob("FALCON3.V7", "FALCON3_BASE.V7")
            if p:
                mapping.append(("ecosystem/models/active.v7", p))

    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
        for nrfs, host in mapping:
            f.write(f"{nrfs}|{host}\n")
            print(f"[mkneuralfs] pack {nrfs} <- {host}")


def pack_tokens() -> set[str]:
    raw = os.environ.get("PACK_LLM", "falcon3").strip().lower()
    if raw in ("none", "0", "off"):
        return set()
    if raw in ("all", "*"):
        return {"falcon3", "falcon3-1b", "falcon3-7b", "all"}
    out: set[str] = set()
    for t in raw.replace(";", ",").split(","):
        t = t.strip()
        if t in ("falcon3", "3b", "lab", "daily"):
            out.add("falcon3")
        elif t in ("1b", "falcon3-1b", "learner"):
            out.add("falcon3-1b")
        elif t in ("7b", "pro", "falcon3-7b"):
            out.add("falcon3-7b")
        elif t == "all":
            out.add("all")
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="Seed NeuralFS lab image (MBR 0x7F)")
    ap.add_argument("--size", type=int, default=2048, help="MB da imagem")
    ap.add_argument(
        "--output",
        default=os.path.join(ROOT, "target", "disk_qemu.neural.raw"),
    )
    args = ap.parse_args()
    out = args.output if os.path.isabs(args.output) else os.path.join(ROOT, args.output)
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)

    # Windows: ficheiro sparse (evita alocar size_mb densos na RAM/disco).
    size_bytes = args.size * 1024 * 1024
    if os.name == "nt":
        try:
            if os.path.isfile(out):
                os.remove(out)
            # createnew 0 + sparse + seteof
            open(out, "wb").close()
            subprocess.run(
                ["fsutil", "sparse", "setflag", out],
                check=False,
                capture_output=True,
            )
            subprocess.run(
                ["fsutil", "file", "seteof", out, str(size_bytes)],
                check=True,
                capture_output=True,
            )
            print(f"[mkneuralfs] sparse prealloc {args.size}MB -> {out}")
        except Exception as e:
            print(f"[WARN] sparse prealloc falhou ({e}); export tentara set_len")
    else:
        # POSIX: truncate sparse
        with open(out, "wb") as f:
            f.truncate(size_bytes)

    with tempfile.NamedTemporaryFile("w", suffix=".manifest", delete=False, encoding="utf-8") as tf:
        man = tf.name
    try:
        build_manifest(pack_tokens(), man)
        env = os.environ.copy()
        env["MKNEURALFS_OUT"] = out
        env["MKNEURALFS_SIZE_MB"] = str(args.size)
        env["MKNEURALFS_MANIFEST"] = man
        # Isolar target dir de teste
        env.setdefault("CARGO_TARGET_DIR", os.path.join(ROOT, "target", "mkneuralfs"))
        print(f"[mkneuralfs] cargo test export_lab_seed size={args.size}MB -> {out}")
        r = subprocess.run(
            [
                "cargo",
                "test",
                "-p",
                "k-nano",
                "--lib",
                "export_lab_seed",
                "--",
                "--ignored",
                "--exact",
                "neural_fs::tests::export_lab_seed",
                "--nocapture",
            ],
            cwd=ROOT,
            env=env,
        )
        if r.returncode != 0:
            print("[ERRO] export_lab_seed falhou", file=sys.stderr)
            return r.returncode
        if not os.path.isfile(out):
            print(f"[ERRO] output ausente: {out}", file=sys.stderr)
            return 1
        mb = os.path.getsize(out) // (1024 * 1024)
        # Preflight magic no superbloco da partição (LBA 2048 + bloco 1 = +8 setores)
        with open(out, "rb") as f:
            f.seek((2048 + 8) * 512)  # bloco 1 do FS @ part start: journal layout uses block 1 for SB
            # Superblock is at block 1 relative to start_lba → LBA start+8
            magic = f.read(8)
        # Actually format writes SB at start_lba + 8 sectors (block 1 * 8)
        # Verify NEURALFS somewhere near partition start
        with open(out, "rb") as f:
            f.seek(2048 * 512)
            chunk = f.read(64 * 1024)
        if b"NEURALFS" not in chunk:
            print("[WARN] magic NEURALFS nao visto nos primeiros 64KB da particao", file=sys.stderr)
        else:
            print("[OK] magic NEURALFS presente")
        print(f"[OK] {out}: {mb}MB NeuralFS (BOOT.LOG=ESP, nao neste volume)")
        return 0
    finally:
        try:
            os.unlink(man)
        except OSError:
            pass


if __name__ == "__main__":
    sys.exit(main())
