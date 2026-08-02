#!/usr/bin/env python3
"""Gera imagem FAT32 (default) ou exFAT para QEMU e HW real.
BOOT.LOG em disco funciona em FAT32 (pré-alocado 256KB). exFAT opt-in com --exfat.
Inclui: .bitnet (incl. BITNET-2B se existir), firmware blobs, CONFIG.TXT,
Device LEGO goldens (LEGO*.MD / LEGOIDX.TXT — ADR-0056).

Uso:
  python tools/build_image.py                  # QEMU -> target/disk_qemu.raw (FAT32)
  python tools/build_image.py --exfat          # experimental exFAT (sem BOOT.LOG)
  python tools/build_image.py --hw             # HW   -> target/disk_hw.raw
  python tools/build_image.py --hw --unified   # USB  -> target/usb_hw.img (ESP+dados)
  python tools/build_image.py --size 512 --output target/disk_qemu.raw

Pendrive 32 GB: tamanho generoso ok - nao pular BITNET-2B.
Fluxo QEMU: cargo build --release -> python tools/build_image.py -> .\\run-qemu-uefi.ps1 -Window
HW 1 stick: cargo build -p boot + python tools/build_image.py --hw --unified
FIT_GATE=1: filtra PACK_LLM via llmfit_pack_filter (propagado ao mkfat/mkexfat).
"""
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_SIZE_MB = 3072  # 3GB dados — 850→1.3→2B→3B progressivo; arquivo FAT32 ≤4GB-1


def parse_args():
    p = argparse.ArgumentParser(
        description="Gera disk_qemu.raw / disk_hw.raw / usb_hw.img com modelos .bitnet + firmware"
    )
    p.add_argument(
        "--size",
        type=int,
        default=DEFAULT_SIZE_MB,
        help=f"Tamanho da imagem (ou particao de dados com --unified) em MB (default: {DEFAULT_SIZE_MB})",
    )
    p.add_argument(
        "--output",
        default=None,
        help="Caminho de saida (default: target/disk_qemu.raw | disk_hw.raw | usb_hw.img)",
    )
    p.add_argument(
        "--hw",
        action="store_true",
        help="Imagem para HW/pendrive -> target/disk_hw.raw (BOOT_MODE=hw)",
    )
    p.add_argument(
        "--unified",
        action="store_true",
        help="USB unificado ESP+dados (requer --hw) -> target/usb_hw.img; ver tools/build_usb_unified.py",
    )
    p.add_argument(
        "--build-boot",
        action="store_true",
        help="Com --unified: se faltar uefi.img, roda cargo build -p boot",
    )
    p.add_argument(
        "--exfat",
        action="store_true",
        help="Usar mkexfat.py em vez de mkfat32.py (experimental, sem BOOT.LOG)",
    )
    return p.parse_args()


def main():
    args = parse_args()
    target_dir = os.path.join(ROOT, "target")
    os.makedirs(target_dir, exist_ok=True)

    if args.unified:
        if not args.hw:
            print("[ERRO] --unified requer --hw (USB HW real, nao QEMU)")
            sys.exit(2)
        out = args.output
        if out is None:
            out = os.path.join(target_dir, "usb_hw.img")
        elif not os.path.isabs(out):
            out = os.path.join(ROOT, out)
        cmd = [
            sys.executable,
            os.path.join(ROOT, "tools", "build_usb_unified.py"),
            "--size",
            str(args.size),
            "--output",
            out,
        ]
        if args.build_boot:
            cmd.append("--build-boot")
        print(f"=== USB unificado (BOOT_MODE=hw) -> {out} ===")
        r = subprocess.run(cmd, cwd=ROOT)
        sys.exit(r.returncode)

    if args.output:
        out = args.output if os.path.isabs(args.output) else os.path.join(ROOT, args.output)
    elif args.hw:
        out = os.path.join(target_dir, "disk_hw.raw")
    else:
        out = os.path.join(target_dir, "disk_qemu.raw")

    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)

    src_v3 = os.path.join(target_dir, "hw_expert_v3.bitnet")
    dst_v3 = os.path.join(target_dir, "hw_expert_tf.bitnet")
    if os.path.exists(src_v3) and not os.path.exists(dst_v3):
        shutil.copy2(src_v3, dst_v3)
        print(f"[OK] hw_expert_v3.bitnet ({os.path.getsize(src_v3)//1024}KB) -> hw_expert_tf.bitnet")

    boot_mode = "hw" if (args.hw or "disk_hw" in os.path.basename(out).lower()) else "qemu"
    env = os.environ.copy()
    env.pop("SKIP_2B", None)
    env["BOOT_MODE"] = boot_mode

    # ADR-0056: pack LEGOs antes do maker (mkexfat/mkfat32 tambem reinjeta)
    print("=== Device LEGO pack (ecosystem/devices -> target/lego) ===")
    r_lego = subprocess.run(
        [sys.executable, os.path.join(ROOT, "tools", "pack_device_legos.py")],
        cwd=ROOT,
        env={**env, "PYTHONIOENCODING": "utf-8"},
        capture_output=True,
        text=True,
        timeout=120,
    )
    if r_lego.stdout:
        sys.stdout.write(r_lego.stdout)
        if not r_lego.stdout.endswith("\n"):
            sys.stdout.write("\n")
    if r_lego.returncode != 0:
        print(f"[ERRO] pack_device_legos.py exit={r_lego.returncode}: {(r_lego.stderr or '')[:400]}")
        sys.exit(r_lego.returncode)

    maker = "mkexfat.py" if args.exfat else "mkfat32.py"
    fs_name = "exFAT" if args.exfat else "FAT32"
    cmd = [
        sys.executable,
        os.path.join(ROOT, "tools", maker),
        "--size",
        str(args.size),
        "--output",
        out,
    ]
    print(f"=== Criando imagem {args.size}MB {fs_name} (BOOT_MODE={boot_mode}) -> {out} ===")
    print("    BITNET-2B incluso se arquivo existir em repo/target/")
    print("    Device LEGO: LEGOVNET/ATHK/GP08/XHCI.MD + LEGOIDX.TXT")
    # ponytail: 1800s — mkfat32 copia ~4.2GB de modelos (LLAMA8B 2GB, AGENT 788MB…);
    # 600s estourava em disco lento, matando o subprocess no meio da cópia.
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=1800, env=env)
    if r.stdout:
        sys.stdout.buffer.write(r.stdout.encode("utf-8", errors="replace"))
        sys.stdout.buffer.write(b"\n")
    if r.returncode != 0:
        err = (r.stderr or "")[:500]
        print(f"[ERRO] {maker} exit={r.returncode}: {err}")
        sys.exit(r.returncode)

    if not os.path.exists(out):
        print(f"[ERRO] arquivo nao criado: {out}")
        sys.exit(1)

    final_size = os.path.getsize(out)
    print(f"\n[OK] {out}: {final_size // 1024 // 1024}MB ({fs_name})")
    print(f"QEMU:  .\\run-qemu-uefi.ps1 -Window")
    print(f"HW dados-only: dd if={out} of=/dev/sdX bs=4M status=progress")
    print(f"HW 1 stick:    python tools/build_image.py --hw --unified")


if __name__ == "__main__":
    main()
