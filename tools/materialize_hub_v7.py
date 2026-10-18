#!/usr/bin/env python3
"""Materializa nomes canônicos .V7 em target1/ a partir dos SKUs TII validados.

ADR-0078 §2.2 / SESSION_491–492:
  Active      ← FALCON3_BASE.V7  → FALCON3.V7
  GeneratorPro← FALCON3_7B.V7   → PRO.V7 (+ FALCON7B.V7)
  GeneratorPro← FALCON3_10B.V7  → FALCON10.V7
  Learner     ← FALCON3_1B.V7   → LEARNER.V7 (+ FALCON1B.V7)

Hardlink quando possível (economiza ~7GB); copy2 se FS não permitir.
NÃO inventa .V7 a partir de v5 (VISION/RERANKER/RUSTCDR) — loaders de
encoder/legacy exigem magic+ver correta; upgrade real = conversor dedicado.

Uso:
  python tools/materialize_hub_v7.py
  python tools/materialize_hub_v7.py --force   # recria mesmo se destino existe
"""
from __future__ import annotations

import argparse
import os
import shutil
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET1 = ROOT / "target1"
MAGIC = 0xBE11BE11

# (destino canônico, fonte preferida, [alternativas], want_(layers,hidden)|None)
MAP = [
    ("FALCON3.V7", "FALCON3_BASE.V7", ["FALCON3_BASE.V6"], (22, 3072)),
    ("PRO.V7", "FALCON3_7B.V7", ["FALCON3_7B.V6", "PRO.V6"], (28, 3072)),
    ("FALCON7B.V7", "FALCON3_7B.V7", ["FALCON3_7B.V6"], (28, 3072)),
    ("FALCON10.V7", "FALCON3_10B.V7", ["FALCON3_10B.V6"], (40, 3072)),
    ("LEARNER.V7", "FALCON3_1B.V7", ["FALCON3_1B.V6", "FALCON3_1B_PERROW.V7"], (18, 2048)),
    ("FALCON1B.V7", "FALCON3_1B.V7", ["FALCON3_1B.V6"], (18, 2048)),
]


def read_ver_shape(path: Path) -> tuple[int, int, int] | None:
    """(version, layers, hidden) ou None."""
    try:
        with path.open("rb") as f:
            hdr = f.read(32)
        if len(hdr) < 32:
            return None
        magic, ver = struct.unpack_from("<IB", hdr, 0)
        if magic != MAGIC or ver not in (6, 7):
            return None
        if ver == 6:
            # v6: hidden@18 u16, layers@20 u16 (ADR-0085; convert uses writer)
            # probe via same layout as audit / bitnet_writer
            hidden = struct.unpack_from("<H", hdr, 18)[0]
            layers = struct.unpack_from("<H", hdr, 20)[0]
        else:
            # v7 same body as v6 for shape fields (version byte only differs early)
            hidden = struct.unpack_from("<H", hdr, 18)[0]
            layers = struct.unpack_from("<H", hdr, 20)[0]
        return ver, layers, hidden
    except OSError:
        return None


def resolve_src(primary: str, alts: list[str], want: tuple[int, int] | None) -> Path | None:
    candidates = [TARGET1 / primary] + [TARGET1 / a for a in alts]
    for p in candidates:
        if not p.is_file():
            continue
        shape = read_ver_shape(p)
        if want is None:
            return p
        if shape is None:
            print(f"  [!] skip {p.name}: header ilegível")
            continue
        _ver, layers, hidden = shape
        if (layers, hidden) == want:
            return p
        print(f"  [!] skip {p.name}: L={layers} h={hidden} want={want}")
    return None


def link_or_copy(src: Path, dst: Path, force: bool) -> str:
    if dst.exists():
        if not force:
            if dst.resolve() == src.resolve() or (
                dst.stat().st_size == src.stat().st_size
                and dst.stat().st_mtime >= src.stat().st_mtime
            ):
                return "ok-exists"
            return "skip-exists"
        dst.unlink()
    try:
        os.link(src, dst)
        return "hardlink"
    except OSError:
        shutil.copy2(src, dst)
        return "copy"


def main() -> int:
    ap = argparse.ArgumentParser(description="Materializa aliases .V7 canônicos em target1/")
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    if not TARGET1.is_dir():
        print(f"FAIL: {TARGET1} ausente", file=sys.stderr)
        return 1

    print(f"[hub-v7] target1={TARGET1}")
    ok = 0
    fail = 0
    for dst_name, src_name, alts, want in MAP:
        src = resolve_src(src_name, alts, want)
        dst = TARGET1 / dst_name
        if src is None:
            print(f"  FAIL {dst_name}: fonte ausente ({src_name})")
            fail += 1
            continue
        shape = read_ver_shape(src)
        shape_s = f"v{shape[0]} L={shape[1]} h={shape[2]}" if shape else "?"
        mb = src.stat().st_size / (1024 * 1024)
        if args.dry_run:
            print(f"  DRY {dst_name} <- {src.name} ({mb:.1f}MB {shape_s})")
            ok += 1
            continue
        how = link_or_copy(src, dst, args.force)
        print(f"  {how:12} {dst_name} <- {src.name} ({mb:.1f}MB {shape_s})")
        if how.startswith("skip") and how != "ok-exists":
            fail += 1
        else:
            ok += 1

    print("\n[hub-v7] nichos NÃO materializados como V7 (sem conversor; legado válido):")
    for name in ("VISION.BIN", "RERANKER.BIN", "RUSTCDR3.BIN", "hw_expert_v6.bitnet"):
        p = TARGET1 / name
        st = f"{p.stat().st_size/1e6:.1f}MB" if p.is_file() else "AUSENTE"
        print(f"  keep {name} ({st})")
    print(f"\n[hub-v7] done ok={ok} fail={fail}")
    return 0 if fail == 0 else 2


if __name__ == "__main__":
    sys.exit(main())
