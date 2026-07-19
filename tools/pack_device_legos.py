#!/usr/bin/env python3
"""Empacota DeviceRecipe goldens → target/lego/*.MD (8.3) p/ mkfat32/mkexfat.

Fonte: ecosystem/devices/<package_id>/RECIPE.md
Calcula content_hash FNV-1a64 (mesmo algoritmo PackageHub).
signature fica vazia (disk seed) — Cap gate H1 usa GOLDEN_RECIPES in-tree.

Uso:
  python tools/pack_device_legos.py
  python tools/pack_device_legos.py --check
"""
from __future__ import annotations

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "ecosystem", "devices")
OUT = os.path.join(ROOT, "target", "lego")

# package_id → FAT 8.3 short name
GOLDENS = [
    ("net.virtio", "LEGOVNET.MD"),
    ("wifi.qca6174.ath10k", "LEGOATHK.MD"),
    ("gpu.nvidia.gp108", "LEGOGP08.MD"),
    ("usb.xhci.host", "LEGOXHCI.MD"),
]


def fnv1a64(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for b in data:
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def body_for_sign(content: str) -> str:
    out = []
    for line in content.replace("\r\n", "\n").splitlines():
        t = line.strip()
        if t.startswith("content_hash:") or t.startswith("signature:"):
            continue
        out.append(line)
    return "\n".join(out) + ("\n" if out else "")


def seal_hash(content: str) -> str:
    canonical = body_for_sign(content)
    hx = f"{fnv1a64(canonical.encode('utf-8')):016x}"
    lines = content.replace("\r\n", "\n").splitlines()
    out = []
    for line in lines:
        t = line.strip()
        if t.startswith("content_hash:"):
            out.append(f'content_hash: "{hx}"')
        elif t.startswith("signature:"):
            out.append('signature: ""')
        else:
            out.append(line)
    text = "\n".join(out)
    if not text.endswith("\n"):
        text += "\n"
    return text


def pack() -> list[tuple[str, str]]:
    os.makedirs(OUT, exist_ok=True)
    written: list[tuple[str, str]] = []
    for pkg, short in GOLDENS:
        src = os.path.join(SRC, pkg, "RECIPE.md")
        if not os.path.isfile(src):
            print(f"[ERR] missing {src}", file=sys.stderr)
            sys.exit(1)
        with open(src, "r", encoding="utf-8") as f:
            raw = f.read()
        sealed = seal_hash(raw)
        dst = os.path.join(OUT, short)
        with open(dst, "w", encoding="utf-8", newline="\n") as f:
            f.write(sealed)
        written.append((short, dst))
        print(f"[OK] {pkg} -> {short} ({len(sealed)} B)")
    # Índice legível no root FAT
    idx_path = os.path.join(OUT, "LEGOIDX.TXT")
    with open(idx_path, "w", encoding="utf-8", newline="\n") as f:
        f.write("# Neural Device LEGO index (ADR-0056)\n")
        f.write("# short=package_id\n")
        for pkg, short in GOLDENS:
            f.write(f"{short}={pkg}\n")
    written.append(("LEGOIDX.TXT", idx_path))
    print(f"[OK] LEGOIDX.TXT ({len(GOLDENS)} goldens)")
    return written


def fat_entries(*, force_pack: bool = False) -> list[tuple[str, str]]:
    """Lista (fat_name, path) para mkfat/mkexfat — gera se faltar."""
    need = force_pack
    for _pkg, short in GOLDENS:
        if not os.path.isfile(os.path.join(OUT, short)):
            need = True
            break
    if need or not os.path.isfile(os.path.join(OUT, "LEGOIDX.TXT")):
        pack()
    out = [(short, os.path.join(OUT, short)) for _pkg, short in GOLDENS]
    out.append(("LEGOIDX.TXT", os.path.join(OUT, "LEGOIDX.TXT")))
    return out


def append_lego_files(files: list) -> int:
    """Injeta LEGOs na lista (name, src) de mkexfat/mkfat32. Retorna quantos arquivos.

    Uso tipico dentro de collect_files()/populate():
        n = append_lego_files(files)
    """
    n = 0
    try:
        entries = fat_entries(force_pack=True)
    except SystemExit:
        raise
    except Exception as e:
        print(f"[LEGO] ERROR pack failed: {e}")
        return 0
    existing = {name for name, _ in files if isinstance(name, str)}
    for fat_name, src in entries:
        if not src or not os.path.isfile(src):
            print(f"[LEGO] MISS {fat_name}")
            continue
        if fat_name in existing:
            # substitui path se ja existir placeholder
            for i, (n0, _s0) in enumerate(files):
                if n0 == fat_name:
                    files[i] = (fat_name, src)
                    break
        else:
            files.append((fat_name, src))
        n += 1
        print(f"[LEGO] inject {fat_name} ({os.path.getsize(src)} B)")
    print(f"[LEGO] injected={n}/{len(GOLDENS) + 1} (goldens+index)")
    return n


def ensure_packed() -> None:
    """Garante target/lego/*.MD antes de mkexfat/mkfat32 (build_image)."""
    fat_entries(force_pack=True)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="só valida fontes")
    args = ap.parse_args()
    if args.check:
        for pkg, short in GOLDENS:
            src = os.path.join(SRC, pkg, "RECIPE.md")
            ok = os.path.isfile(src)
            print(f"{'OK' if ok else 'MISS'} {pkg} -> {short}")
            if not ok:
                return 1
        return 0
    pack()
    return 0


if __name__ == "__main__":
    sys.exit(main())
