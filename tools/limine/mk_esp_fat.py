#!/usr/bin/env python3
"""ADR-0065 — ESP **somente FAT32** + LFN (limine.conf / neural-kernel).

Sem FAT12/FAT16: <65525 clusters → SystemExit (aumente --size-mb).

Uso:
  python tools/limine/mk_esp_fat.py --esp-dir tools/limine/esp --output target/limine-esp.img
"""
from __future__ import annotations

import argparse
import os
import struct
import sys

SECTOR = 512
# Microsoft: volume com < 65525 clusters NÃO é FAT32 (vira FAT16 na prática).
FAT32_MIN_CLUSTERS = 65525


def align_up(v: int, a: int) -> int:
    return (v + a - 1) // a * a


def short83(path: str, used: set[str]) -> bytes:
    """Gera 8.3 único a partir do nome longo."""
    base = os.path.basename(path).upper().replace(" ", "")
    if "." in base:
        name, ext = base.rsplit(".", 1)
    else:
        name, ext = base, ""
    name = "".join(c for c in name if c.isalnum() or c in "_-")[:8] or "FILE"
    ext = "".join(c for c in ext if c.isalnum())[:3]
    candidate = (name.ljust(8) + ext.ljust(3)).encode("ascii")
    n = 1
    while candidate in used:
        suffix = f"~{n}"
        stem = name[: max(1, 8 - len(suffix))] + suffix
        candidate = (stem.ljust(8) + ext.ljust(3)).encode("ascii")
        n += 1
    used.add(candidate)
    return candidate


def lfn_checksum(name83: bytes) -> int:
    s = 0
    for b in name83:
        s = ((s & 1) << 7) + (s >> 1) + b
        s &= 0xFF
    return s


def lfn_entries(long_name: str, name83: bytes) -> list[bytes]:
    """Entradas LFN (última primeiro) antes do 8.3."""
    ucs = long_name.encode("utf-16le") + b"\x00\x00"
    while len(ucs) % 26:
        ucs += b"\xff"
    nents = (len(ucs) + 25) // 26
    chk = lfn_checksum(name83)
    out: list[bytes] = []
    for seq in range(nents, 0, -1):
        chunk = ucs[(seq - 1) * 26 : seq * 26]
        if len(chunk) < 26:
            chunk = chunk + b"\xff" * (26 - len(chunk))
        e = bytearray(32)
        ord_id = seq | (0x40 if seq == nents else 0)
        e[0] = ord_id
        e[1:11] = chunk[0:10]
        e[11] = 0x0F
        e[12] = 0
        e[13] = chk
        e[14:26] = chunk[10:22]
        e[26:28] = b"\x00\x00"
        e[28:32] = chunk[22:26]
        out.append(bytes(e))
    return out


def dir_entry_83(name83: bytes, attr: int, cluster: int, size: int) -> bytes:
    e = bytearray(32)
    e[0:11] = name83
    e[11] = attr
    struct.pack_into("<H", e, 26, cluster & 0xFFFF)
    struct.pack_into("<H", e, 20, (cluster >> 16) & 0xFFFF)
    struct.pack_into("<I", e, 28, size)
    return bytes(e)


def build_esp(esp_dir: str, out_path: str, size_mb: int = 128) -> None:
    """MBR + partição ESP FAT32 real. Nunca FAT16."""
    if size_mb < 64:
        raise SystemExit(
            f"ERRO: size_mb={size_mb} insuficiente para FAT32; use >= 64 (recomendado 128)."
        )

    size = size_mb * 1024 * 1024
    total_sectors = size // SECTOR
    part_lba = 2048
    part_sectors = total_sectors - part_lba

    reserved = 32
    fats = 2
    # Escolhe SPC para garantir FAT32 real (>=65525 clusters) no size pedido.
    # spc=8 em 128MB falha (~32k clusters); spc=1..4 normalmente ok em >=64MB.
    spc = None
    fat_sectors = 1
    clusters = 0
    for try_spc in (8, 4, 2, 1):
        fat_sectors = 1
        for _ in range(16):
            data_start = reserved + fats * fat_sectors
            data_sectors = part_sectors - data_start
            clusters = max(data_sectors // try_spc, 1)
            need = align_up((clusters + 2) * 4, SECTOR) // SECTOR
            if need == fat_sectors:
                break
            fat_sectors = need
        data_start = reserved + fats * fat_sectors
        data_sectors = part_sectors - data_start
        clusters = data_sectors // try_spc
        if clusters >= FAT32_MIN_CLUSTERS:
            spc = try_spc
            break
    if spc is None:
        raise SystemExit(
            f"ERRO: impossivel FAT32 real com size_mb={size_mb} "
            f"(max clusters com spc=1 < {FAT32_MIN_CLUSTERS}). Aumente --size-mb."
        )
    data_start = reserved + fats * fat_sectors
    data_sectors = part_sectors - data_start
    clusters = data_sectors // spc
    if clusters < FAT32_MIN_CLUSTERS:
        raise SystemExit(
            f"ERRO: clusters={clusters} < {FAT32_MIN_CLUSTERS} — recusando FAT16/falso-FAT32. "
            f"Aumente --size-mb (atual {size_mb})."
        )

    img = bytearray(size)

    img[510:512] = b"\x55\xaa"
    part = bytearray(16)
    part[0] = 0x80
    part[4] = 0xEF  # ESP
    struct.pack_into("<I", part, 8, part_lba)
    struct.pack_into("<I", part, 12, part_sectors)
    img[446:462] = part

    off = part_lba * SECTOR
    bpb = bytearray(SECTOR)
    bpb[0:3] = b"\xeb\x58\x90"
    bpb[3:11] = b"MSWIN4.1"
    struct.pack_into("<H", bpb, 11, SECTOR)
    bpb[13] = spc
    struct.pack_into("<H", bpb, 14, reserved)
    bpb[16] = fats
    struct.pack_into("<H", bpb, 17, 0)  # RootEntCnt=0
    struct.pack_into("<H", bpb, 19, 0)  # TotSec16=0
    bpb[21] = 0xF8
    struct.pack_into("<H", bpb, 22, 0)  # FATSz16=0 (FAT32)
    struct.pack_into("<H", bpb, 24, 63)
    struct.pack_into("<H", bpb, 26, 255)
    struct.pack_into("<I", bpb, 28, part_lba)
    struct.pack_into("<I", bpb, 32, part_sectors)
    struct.pack_into("<I", bpb, 36, fat_sectors)
    struct.pack_into("<H", bpb, 40, 0)
    struct.pack_into("<H", bpb, 42, 0)
    struct.pack_into("<I", bpb, 44, 2)
    struct.pack_into("<H", bpb, 48, 1)
    struct.pack_into("<H", bpb, 50, 6)
    bpb[66] = 0x80
    bpb[67] = 0x00
    bpb[68] = 0x29
    struct.pack_into("<I", bpb, 69, 0x4E45524F)
    bpb[71:82] = b"NEURAL-ESP "
    bpb[82:90] = b"FAT32   "
    bpb[510:512] = b"\x55\xaa"
    img[off : off + SECTOR] = bpb

    fsi = bytearray(SECTOR)
    struct.pack_into("<I", fsi, 0, 0x41615252)
    struct.pack_into("<I", fsi, 484, 0x61417272)
    struct.pack_into("<I", fsi, 488, 0xFFFFFFFF)
    struct.pack_into("<I", fsi, 492, 0xFFFFFFFF)
    fsi[510:512] = b"\x55\xaa"
    img[off + SECTOR : off + 2 * SECTOR] = fsi

    fat0 = off + reserved * SECTOR
    fat = bytearray(fat_sectors * SECTOR)
    struct.pack_into("<I", fat, 0, 0x0FFFFFF8)
    struct.pack_into("<I", fat, 4, 0x0FFFFFFF)
    struct.pack_into("<I", fat, 8, 0x0FFFFFFF)

    data_off = fat0 + fats * fat_sectors * SECTOR
    next_cluster = 3

    def cluster_to_off(c: int) -> int:
        return data_off + (c - 2) * spc * SECTOR

    def alloc_chain(data: bytes) -> int:
        nonlocal next_cluster, fat
        if not data:
            return 0
        first = next_cluster
        cluster = first
        remaining = data
        while True:
            chunk = remaining[: spc * SECTOR]
            remaining = remaining[spc * SECTOR :]
            c_off = cluster_to_off(cluster)
            img[c_off : c_off + len(chunk)] = chunk
            if not remaining:
                struct.pack_into("<I", fat, cluster * 4, 0x0FFFFFFF)
                next_cluster = cluster + 1
                break
            nxt = cluster + 1
            struct.pack_into("<I", fat, cluster * 4, nxt & 0x0FFFFFFF)
            cluster = nxt
            next_cluster = cluster + 1
        return first

    files: list[tuple[str, bytes]] = []
    for dirpath, _, filenames in os.walk(esp_dir):
        for fn in filenames:
            full = os.path.join(dirpath, fn)
            rel = os.path.relpath(full, esp_dir).replace("\\", "/")
            with open(full, "rb") as f:
                files.append((rel, f.read()))

    buckets: dict[str, list[tuple[str, bytes]]] = {
        "": [],
        "EFI": [],
        "EFI/BOOT": [],
        "boot": [],
    }
    for rel, data in files:
        parts = [p for p in rel.split("/") if p]
        if len(parts) == 1:
            buckets[""].append((parts[0], data))
        elif parts[0].upper() == "EFI" and len(parts) >= 3 and parts[1].upper() == "BOOT":
            buckets["EFI/BOOT"].append((parts[-1], data))
        elif parts[0].lower() == "boot" and len(parts) == 2:
            buckets["boot"].append((parts[1], data))
        elif parts[0].upper() == "EFI" and len(parts) == 2:
            buckets["EFI"].append((parts[1], data))
        else:
            buckets["boot"].append((parts[-1], data))

    used83: set[str] = set()

    def build_dir_bytes(
        entries: list[tuple[str, bytes]], is_subdir: bool, parent_cl: int, self_cl: int
    ) -> bytes:
        buf = bytearray()
        if is_subdir:
            buf += dir_entry_83(b".          ", 0x10, self_cl, 0)
            buf += dir_entry_83(b"..         ", 0x10, parent_cl if parent_cl != 2 else 0, 0)
        for long_name, data in entries:
            cl = alloc_chain(data)
            n83 = short83(long_name, used83)
            for le in lfn_entries(long_name, n83):
                buf += le
            buf += dir_entry_83(n83, 0x20, cl, len(data))
        while len(buf) % (spc * SECTOR):
            buf += b"\x00"
        if len(buf) == 0:
            buf = bytearray(spc * SECTOR)
        return bytes(buf)

    efi_boot_cl = next_cluster
    next_cluster += 1
    struct.pack_into("<I", fat, efi_boot_cl * 4, 0x0FFFFFFF)

    boot_cl = next_cluster
    next_cluster += 1
    struct.pack_into("<I", fat, boot_cl * 4, 0x0FFFFFFF)

    efi_cl = next_cluster
    next_cluster += 1
    struct.pack_into("<I", fat, efi_cl * 4, 0x0FFFFFFF)

    def write_dir_at(start_cl: int, content: bytes) -> None:
        nonlocal next_cluster, fat
        remaining = content
        cluster = start_cl
        while True:
            chunk = remaining[: spc * SECTOR]
            remaining = remaining[spc * SECTOR :]
            img[cluster_to_off(cluster) : cluster_to_off(cluster) + len(chunk)] = chunk
            if len(chunk) < spc * SECTOR:
                z0 = cluster_to_off(cluster) + len(chunk)
                img[z0 : cluster_to_off(cluster) + spc * SECTOR] = b"\x00" * (
                    spc * SECTOR - len(chunk)
                )
            if not remaining:
                struct.pack_into("<I", fat, cluster * 4, 0x0FFFFFFF)
                break
            nxt = next_cluster
            next_cluster += 1
            struct.pack_into("<I", fat, cluster * 4, nxt & 0x0FFFFFFF)
            cluster = nxt

    write_dir_at(efi_boot_cl, build_dir_bytes(buckets["EFI/BOOT"], True, efi_cl, efi_boot_cl))
    write_dir_at(boot_cl, build_dir_bytes(buckets["boot"], True, 2, boot_cl))

    efi_dir = bytearray()
    efi_dir += dir_entry_83(b".          ", 0x10, efi_cl, 0)
    efi_dir += dir_entry_83(b"..         ", 0x10, 0, 0)
    n83 = short83("BOOT", used83)
    for le in lfn_entries("BOOT", n83):
        efi_dir += le
    efi_dir += dir_entry_83(n83, 0x10, efi_boot_cl, 0)
    for long_name, data in buckets["EFI"]:
        cl = alloc_chain(data)
        n83 = short83(long_name, used83)
        for le in lfn_entries(long_name, n83):
            efi_dir += le
        efi_dir += dir_entry_83(n83, 0x20, cl, len(data))
    while len(efi_dir) % (spc * SECTOR):
        efi_dir += b"\x00"
    write_dir_at(efi_cl, bytes(efi_dir))

    root = bytearray()
    n83 = short83("EFI", used83)
    for le in lfn_entries("EFI", n83):
        root += le
    root += dir_entry_83(n83, 0x10, efi_cl, 0)
    n83 = short83("boot", used83)
    for le in lfn_entries("boot", n83):
        root += le
    root += dir_entry_83(n83, 0x10, boot_cl, 0)
    for long_name, data in buckets[""]:
        cl = alloc_chain(data)
        n83 = short83(long_name, used83)
        for le in lfn_entries(long_name, n83):
            root += le
        root += dir_entry_83(n83, 0x20, cl, len(data))
    while len(root) % (spc * SECTOR):
        root += b"\x00"
    if len(root) == 0:
        root = bytearray(spc * SECTOR)
    write_dir_at(2, bytes(root))

    for i in range(fats):
        s = fat0 + i * fat_sectors * SECTOR
        img[s : s + len(fat)] = fat

    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    with open(out_path, "wb") as f:
        f.write(img)
    print(
        f"[OK] ESP FAT32+LFN {size_mb}MB -> {out_path} "
        f"files={len(files)} clusters={clusters} spc={spc} (FAT16 removido)"
    )


def main() -> None:
    ap = argparse.ArgumentParser(description="ESP Limine FAT32+LFN only (no FAT16)")
    ap.add_argument("--esp-dir", required=True)
    ap.add_argument("--output", required=True)
    ap.add_argument(
        "--size-mb",
        type=int,
        default=128,
        help="MB da imagem (min 64 FAT32 real; default 128)",
    )
    args = ap.parse_args()
    if not os.path.isdir(args.esp_dir):
        print(f"ERRO: esp-dir ausente: {args.esp_dir}", file=sys.stderr)
        sys.exit(1)
    build_esp(args.esp_dir, args.output, args.size_mb)


if __name__ == "__main__":
    main()
