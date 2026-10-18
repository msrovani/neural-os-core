#!/usr/bin/env python3
"""Extrai BOOT.LOG da ESP FAT32 (s493 / NeuralFS.md §14) para leitura na IDE.

Aceita:
  - target/uefi.img / limine-esp.img (disco ESP só)
  - target/usb_hw.img (GPT: part ESP)

Uso:
  python tools/extract_esp_bootlog.py target/uefi.img -o logs/BOOT.LOG
  python tools/extract_esp_bootlog.py target/usb_hw.img
"""
from __future__ import annotations

import argparse
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def read_bpb(img, lba: int):
    img.seek(lba * 512)
    return img.read(512)


def find_boot_log(img, part_lba: int) -> bytes | None:
    bpb = read_bpb(img, part_lba)
    if len(bpb) < 512 or bpb[0:3] == b"\x00\x00\x00":
        return None
    if bpb[3:11] == b"EXFAT   ":
        return None
    bps = struct.unpack_from("<H", bpb, 0x0B)[0]
    spc = bpb[0x0D]
    reserved = struct.unpack_from("<H", bpb, 0x0E)[0]
    fats = bpb[0x10]
    root_ent = struct.unpack_from("<H", bpb, 0x11)[0]
    if root_ent != 0 or bps < 512 or spc == 0:
        return None
    spf = struct.unpack_from("<I", bpb, 0x24)[0]
    root_cl = struct.unpack_from("<I", bpb, 0x2C)[0]
    fat_lba = part_lba + reserved
    data_lba = fat_lba + fats * spf
    want = b"BOOT    LOG"
    cluster = root_cl
    walked = 0
    while 2 <= cluster < 0x0FFFFFF8 and walked < 64:
        walked += 1
        clba = data_lba + (cluster - 2) * spc
        img.seek(clba * 512)
        dirbuf = img.read(spc * bps)
        for off in range(0, len(dirbuf), 32):
            e = dirbuf[off : off + 32]
            if not e or e[0] == 0:
                break
            if e[0] == 0xE5 or (e[11] & 0x0F) == 0x0F:
                continue
            if e[0:11] != want:
                continue
            size = struct.unpack_from("<I", e, 28)[0]
            fc = struct.unpack_from("<H", e, 26)[0] | (struct.unpack_from("<H", e, 20)[0] << 16)
            out = bytearray()
            c = fc
            while c >= 2 and c < 0x0FFFFFF8 and len(out) < size:
                cba = data_lba + (c - 2) * spc
                img.seek(cba * 512)
                chunk = img.read(spc * bps)
                need = size - len(out)
                out.extend(chunk[:need])
                # next FAT
                fat_off = c * 4
                img.seek(fat_lba * 512 + (fat_off // bps) * bps)
                fsec = img.read(bps)
                bo = fat_off % bps
                c = struct.unpack_from("<I", fsec, bo)[0] & 0x0FFFFFFF
            return bytes(out)
        # next root cluster
        fat_off = cluster * 4
        img.seek(fat_lba * 512 + (fat_off // bps) * bps)
        fsec = img.read(bps)
        bo = fat_off % bps
        cluster = struct.unpack_from("<I", fsec, bo)[0] & 0x0FFFFFFF
    return None


def gpt_esp_lba(img) -> int | None:
    img.seek(512)
    hdr = img.read(92)
    if hdr[0:8] != b"EFI PART":
        return None
    # entry LBA at 72, sizeof entry 128, num entries at 80
    ent_lba = struct.unpack_from("<Q", hdr, 72)[0]
    num = struct.unpack_from("<I", hdr, 80)[0]
    esz = struct.unpack_from("<I", hdr, 84)[0]
    img.seek(ent_lba * 512)
    esp_guid = bytes.fromhex("28732ac11ff8d211ba4b00a0c93ec93b")  # EFI System LE
    for _ in range(min(num, 128)):
        e = img.read(esz)
        if len(e) < 56:
            break
        if e[0:16] == esp_guid:
            return struct.unpack_from("<Q", e, 32)[0]
    return None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("image", nargs="?", default=os.path.join(ROOT, "target", "uefi.img"))
    ap.add_argument("-o", "--output", default=None)
    args = ap.parse_args()
    path = args.image if os.path.isabs(args.image) else os.path.join(ROOT, args.image)
    if not os.path.isfile(path):
        print(f"missing {path}", file=sys.stderr)
        return 1
    with open(path, "rb") as img:
        # Try whole image as FAT (uefi.img) @ LBA 0 or 2048
        data = find_boot_log(img, 0) or find_boot_log(img, 2048)
        if data is None:
            esp = gpt_esp_lba(img)
            if esp is not None:
                data = find_boot_log(img, int(esp))
        if data is None:
            print("BOOT.LOG not found in ESP", file=sys.stderr)
            return 2
    out = args.output or os.path.join(ROOT, "logs", "BOOT.LOG")
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with open(out, "wb") as f:
        f.write(data)
    text = data.decode("utf-8", errors="replace")
    lines = [ln for ln in text.splitlines() if ln.strip()]
    print(f"[OK] {out}: {len(data)} bytes, {len(lines)} non-empty lines")
    for ln in lines[:5]:
        print(" ", ln[:120])
    return 0


if __name__ == "__main__":
    sys.exit(main())
