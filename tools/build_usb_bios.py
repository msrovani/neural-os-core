#!/usr/bin/env python3
"""USB HW com boot BIOS legado (sem UEFI/OVMF).

Layout MBR (bootloader 0.11 BiosBoot + particao de dados):
  Part 0 — stage BIOS  type=0x20  (de target/bios.img)  ACTIVE
  Part 1 — FAT boot    type=0x0C  (kernel do bios.img, ~10MB)
  Part 2 — FAT32 dados type=0x0C  (modelos, BOOT.LOG, LEGOs)  ← Windows ve NEURAL-OS

Uso:
  cargo build --release -p boot
  python tools/build_usb_bios.py --build-boot
  python tools/build_usb_bios.py --size 1024 -o target/usb_hw_bios.img

Rufus: modo DD → grave no pendrive. No firmware: Legacy/CSM ON, Secure Boot OFF.
Nota: BiosBoot no QEMU costuma triple-fault; este path e para HW real com CSM.
"""
from __future__ import annotations

import argparse
import os
import shutil
import struct
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SECTOR = 512
DEFAULT_SIZE_MB = 3072

# Reusa gerador de volume de dados do unified
sys.path.insert(0, os.path.join(ROOT, "tools"))
from build_usb_unified import (  # noqa: E402
    align_up,
    build_data_volume,
    patch_fat_bpb,
)


def ensure_bios_img(bios_path: str, build_boot: bool) -> None:
    if build_boot:
        print("=== cargo build --release -p boot (gera bios.img) ===")
        env = os.environ.copy()
        env.setdefault("CARGO_TARGET_DIR", os.path.join(ROOT, "target"))
        r = subprocess.run(
            ["cargo", "build", "--release", "-p", "boot"],
            cwd=ROOT,
            env=env,
            timeout=3600,
        )
        if r.returncode != 0 or not os.path.exists(bios_path):
            raise SystemExit("[ERRO] falha ao gerar bios.img via cargo build -p boot")
        return
    if os.path.exists(bios_path) and os.path.getsize(bios_path) > 1024 * 1024:
        return
    raise SystemExit(
        f"[ERRO] {bios_path} ausente. Rode: cargo build --release -p boot\n"
        "       ou passe --build-boot"
    )


def parse_bios_parts(bios_path: str) -> list[tuple[int, int, int, int]]:
    """Retorna lista (bootable, type, lba_start, sectors) das partes nao-vazias."""
    with open(bios_path, "rb") as f:
        mbr = f.read(SECTOR)
    if mbr[0x1FE] != 0x55 or mbr[0x1FF] != 0xAA:
        raise SystemExit("[ERRO] bios.img sem assinatura MBR 55AA")
    parts = []
    for off in (0x1BE, 0x1CE, 0x1DE, 0x1EE):
        e = mbr[off : off + 16]
        typ = e[4]
        if typ == 0:
            continue
        bootable = e[0]
        lba = struct.unpack_from("<I", e, 8)[0]
        sz = struct.unpack_from("<I", e, 12)[0]
        parts.append((bootable, typ, lba, sz))
    if len(parts) < 2:
        raise SystemExit(f"[ERRO] bios.img esperava >=2 partes, achou {len(parts)}")
    return parts


def main() -> None:
    p = argparse.ArgumentParser(
        description="USB HW BIOS legado: bios.img + FAT32 dados (sem UEFI)"
    )
    p.add_argument("--size", type=int, default=DEFAULT_SIZE_MB, help="Particao de dados MB")
    p.add_argument("--output", default=None, help="Saida (default: target/usb_hw_bios.img)")
    p.add_argument("--bios", default=None, help="Caminho bios.img")
    p.add_argument("--data-raw", default=None, help="Reusar disk FAT32 ja gerado")
    p.add_argument("--build-boot", action="store_true", help="Recompila bios.img via cargo")
    args = p.parse_args()

    target_dir = os.path.join(ROOT, "target")
    os.makedirs(target_dir, exist_ok=True)
    bios_path = args.bios or os.path.join(target_dir, "bios.img")
    out = args.output or os.path.join(target_dir, "usb_hw_bios.img")
    if not os.path.isabs(out):
        out = os.path.join(ROOT, out)

    ensure_bios_img(bios_path, args.build_boot)
    bios_parts = parse_bios_parts(bios_path)
    bios_bytes = os.path.getsize(bios_path)
    bios_sectors = (bios_bytes + SECTOR - 1) // SECTOR
    print(f"[OK] bios.img: {bios_bytes // 1024} KB, {len(bios_parts)} partes MBR")
    for i, (b, t, lba, sz) in enumerate(bios_parts):
        print(f"  bios part{i}: boot={b:02x} type={t:02x} LBA {lba}..{lba + sz - 1}")

    with tempfile.TemporaryDirectory(prefix="nk_bios_") as tmp:
        if args.data_raw:
            data_raw = args.data_raw
            if not os.path.isabs(data_raw):
                data_raw = os.path.join(ROOT, data_raw)
            if not os.path.exists(data_raw):
                raise SystemExit(f"[ERRO] --data-raw nao existe: {data_raw}")
            part_lba_in_raw = 2048
        else:
            data_raw = os.path.join(tmp, "data_fat32.raw")
            part_lba_in_raw = build_data_volume(args.size, data_raw, use_fat32=True)

        data_file_sectors = os.path.getsize(data_raw) // SECTOR
        data_payload_sectors = data_file_sectors - part_lba_in_raw
        if data_payload_sectors < 65525:
            raise SystemExit("[ERRO] particao de dados muito pequena para FAT32")

        data_start = align_up(bios_sectors + 1, 2048)
        data_sectors = data_payload_sectors
        data_end = data_start + data_sectors - 1
        total_sectors = data_end + 1
        total_bytes = total_sectors * SECTOR

        print(f"=== Layout BIOS legado -> {out} ===")
        print(f"  BIOS : setores 0..{bios_sectors - 1} ({bios_bytes // 1024} KB) MBR+kernel")
        print(
            f"  DATA : LBA {data_start}..{data_end} "
            f"({data_sectors * SECTOR // (1024 * 1024)} MB) type=0x0C FAT32"
        )
        print(f"  Total: {total_bytes // (1024 * 1024)} MB")

        os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
        with open(out, "wb") as f:
            f.truncate(total_bytes)

        with open(out, "r+b") as f:
            # 1) Copia bios.img inteiro (MBR bootstrap + partes 0/1)
            with open(bios_path, "rb") as src:
                remaining = bios_bytes
                f.seek(0)
                while remaining > 0:
                    chunk = src.read(min(8 * 1024 * 1024, remaining))
                    f.write(chunk)
                    remaining -= len(chunk)

            # 2) Reescreve MBR com part 2 = dados (preserva bootstrap do bios)
            f.seek(0)
            bootstrap = bytearray(f.read(0x1BE))
            mbr = bytearray(SECTOR)
            mbr[0:0x1BE] = bootstrap
            slots = [0x1BE, 0x1CE, 0x1DE, 0x1EE]
            for i, (bootable, typ, lba, sz) in enumerate(bios_parts[:2]):
                off = slots[i]
                mbr[off] = bootable
                mbr[off + 4] = typ & 0xFF
                struct.pack_into("<I", mbr, off + 8, lba)
                struct.pack_into("<I", mbr, off + 12, min(sz, 0xFFFFFFFF))
            # Dados
            off = slots[2]
            mbr[off] = 0x00
            mbr[off + 4] = 0x0C
            struct.pack_into("<I", mbr, off + 8, data_start)
            struct.pack_into("<I", mbr, off + 12, min(data_sectors, 0xFFFFFFFF))
            mbr[0x1FE], mbr[0x1FF] = 0x55, 0xAA
            f.seek(0)
            f.write(mbr)

            # 3) Copia payload FAT32 dados
            with open(data_raw, "rb") as src:
                src.seek(part_lba_in_raw * SECTOR)
                remaining = data_sectors * SECTOR
                f.seek(data_start * SECTOR)
                while remaining > 0:
                    chunk = src.read(min(8 * 1024 * 1024, remaining))
                    if not chunk:
                        break
                    f.write(chunk)
                    remaining -= len(chunk)
                if remaining != 0:
                    raise SystemExit("[ERRO] copia incompleta da particao de dados")

            patch_fat_bpb(f, data_start, data_sectors)

    alias = os.path.join(target_dir, "disk_hw_bios.raw")
    if os.path.abspath(out) != os.path.abspath(alias):
        shutil.copy2(out, alias)
        print(f"[OK] alias {alias}")

    print(f"\n[OK] {out}: {os.path.getsize(out) // (1024 * 1024)} MB (BIOS MBR + FAT32 dados)")
    print("Rufus: modo Imagem DD -> pendrive.")
    print("Firmware: Legacy Boot / CSM ON, UEFI OFF, Secure Boot OFF.")
    print("Windows: volume NEURAL-OS (3a particao) com BOOT.LOG / LEGO*.MD.")
    print("AVISO: BiosBoot no QEMU costuma triple-fault — teste em HW real.")


if __name__ == "__main__":
    main()
