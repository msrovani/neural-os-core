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
import uuid
import zlib

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


def crc32(data: bytes) -> int:
    """Standard CRC-32 (zlib)."""
    return zlib.crc32(data) & 0xFFFFFFFF


def write_gpt(
    img: bytearray, total_sectors: int, part_lba: int, part_sectors: int
) -> None:
    """Escreve GPT header (LBA 1), entries (LBA 2-33), backup entries (últimos LBAs - 32),
    backup header (último LBA). FAT32 em part_lba permanece inalterado.
    """
    esp_type_guid = bytes.fromhex("C12A7328F81F11D2BA4B00A0C93EC93B")
    last_lba = total_sectors - 1
    end_lba = part_lba + part_sectors - 1
    last_usable_lba = last_lba - 33  # = total_sectors - 34

    # --- Partition entry (128 bytes) ---
    entry = bytearray(128)
    entry[0:16] = esp_type_guid
    entry[16:32] = uuid.uuid4().bytes_le  # unique partition GUID
    struct.pack_into("<Q", entry, 32, part_lba)   # Start LBA
    struct.pack_into("<Q", entry, 40, end_lba)    # End LBA
    struct.pack_into("<Q", entry, 48, 0)           # Attributes
    name_bytes = "EFI System Partition".encode("utf-16-le")[:72]
    entry[56 : 56 + len(name_bytes)] = name_bytes

    # --- Partition entries area (32 sectors, 128 entries × 128 bytes) ---
    entries_bytes = bytearray(32 * SECTOR)
    entries_bytes[0:128] = entry

    # Write primary partition entries at LBA 2
    img[2 * SECTOR : 2 * SECTOR + 32 * SECTOR] = entries_bytes

    # --- Primary GPT header at LBA 1 ---
    hdr = bytearray(SECTOR)
    hdr[0:8] = b"EFI PART"
    struct.pack_into("<I", hdr, 8, 0x00010000)     # Revision 1.0
    struct.pack_into("<I", hdr, 12, 92)             # Header size
    struct.pack_into("<I", hdr, 16, 0)              # CRC32 (placeholder)
    struct.pack_into("<I", hdr, 20, 0)              # Reserved
    struct.pack_into("<Q", hdr, 24, 1)              # This LBA (primary)
    struct.pack_into("<Q", hdr, 32, last_lba)       # Backup LBA
    struct.pack_into("<Q", hdr, 40, 34)             # First usable LBA
    struct.pack_into("<Q", hdr, 48, last_usable_lba)  # Last usable LBA
    hdr[56:72] = uuid.uuid4().bytes_le              # Disk GUID
    struct.pack_into("<Q", hdr, 72, 2)              # Partition entries LBA
    struct.pack_into("<I", hdr, 80, 128)            # Number of partition entries
    struct.pack_into("<I", hdr, 84, 128)            # Size of partition entry
    entries_crc = crc32(bytes(entries_bytes))
    struct.pack_into("<I", hdr, 88, entries_crc)    # CRC32 of entries
    # Compute header CRC (with CRC32 field zeroed)
    hdr_crc = crc32(bytes(hdr[:92]))
    struct.pack_into("<I", hdr, 16, hdr_crc)

    # Write primary header at LBA 1
    img[SECTOR : 2 * SECTOR] = hdr

    # --- Backup GPT ---
    # Backup partition entries at last_lba - 32 (32 sectors)
    backup_entries_lba = last_lba - 32
    bak_off = backup_entries_lba * SECTOR
    img[bak_off : bak_off + 32 * SECTOR] = entries_bytes

    # Backup GPT header at last_lba
    hdr_bak = bytearray(hdr)  # copy primary (entries CRC is same)
    struct.pack_into("<Q", hdr_bak, 24, last_lba)           # This LBA = backup
    struct.pack_into("<Q", hdr_bak, 32, 1)                  # Backup LBA = primary
    struct.pack_into("<Q", hdr_bak, 72, backup_entries_lba) # Partition entries LBA
    # Recompute header CRC
    struct.pack_into("<I", hdr_bak, 16, 0)
    hdr_crc_bak = crc32(bytes(hdr_bak[:92]))
    struct.pack_into("<I", hdr_bak, 16, hdr_crc_bak)

    img[last_lba * SECTOR : (last_lba + 1) * SECTOR] = hdr_bak


def build_esp(esp_dir: str, out_path: str, size_mb: int = 128) -> None:
    """MBR + partição ESP FAT32 real. Nunca FAT16."""
    if size_mb < 64:
        raise SystemExit(
            f"ERRO: size_mb={size_mb} insuficiente para FAT32; use >= 64 (recomendado 128)."
        )

    size = size_mb * 1024 * 1024
    total_sectors = size // SECTOR
    part_lba = 2048
    # Account for backup GPT (32 entry sectors + 1 header sector at end)
    part_sectors = total_sectors - 34 - part_lba + 1
    last_usable_lba = part_lba + part_sectors - 1

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
    part[0] = 0x00
    part[4] = 0xEE  # Protective GPT
    struct.pack_into("<I", part, 8, 1)  # start at LBA 1
    struct.pack_into("<I", part, 12, total_sectors - 1)  # covers whole disk except LBA 0
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

    # Write GPT structures (primary at LBA 0-33, backup at final 33 sectors)
    write_gpt(img, total_sectors, part_lba, part_sectors)

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
