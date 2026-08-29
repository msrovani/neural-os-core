#!/usr/bin/env python3
"""Valida MBR/GPT/BPB de target/usb_hw.img (Windows mount readiness)."""
import argparse
import os
import struct
import sys
import uuid

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ESP_TYPE = uuid.UUID("C12A7328-F81F-11D2-BA4B-00A0C93EC93B").bytes_le


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Valida layout USB HW (removable ou fixed-disk).")
    p.add_argument("path", nargs="?", default=os.path.join(ROOT, "target", "usb_hw.img"))
    p.add_argument(
        "--fixed-disk",
        action="store_true",
        help="Espera MBR protetor 0xEE + ESP so na GPT (HD externo USB).",
    )
    return p.parse_args()


args = parse_args()
path = args.path
fixed_disk = args.fixed_disk

with open(path, "rb") as f:
    mbr = f.read(512)
    print("=== MBR ===")
    typ0 = mbr[0x1BE + 4]
    typ1 = mbr[0x1CE + 4]
    for i, off in enumerate([0x1BE, 0x1CE, 0x1DE, 0x1EE]):
        e = mbr[off : off + 16]
        if e[4] == 0:
            continue
        typ = e[4]
        start = struct.unpack_from("<I", e, 8)[0]
        n = struct.unpack_from("<I", e, 12)[0]
        print(f"  part{i}: type=0x{typ:02X} start={start} sectors={n} (~{n * 512 / 1024 / 1024:.1f}MB)")

    auto_fixed = typ0 == 0xEE and typ1 == 0x00
    if fixed_disk or auto_fixed:
        if typ0 != 0xEE:
            print(f"FAIL: fixed-disk espera MBR 0xEE; got typ0=0x{typ0:02X}")
            sys.exit(1)
        print("  MBR fixed-disk: protetor GPT 0xEE OK")
    else:
        if typ0 not in (0x0B, 0x0C, 0x07, 0x1C) or typ1 not in (0xEF, 0x00):
            print(
                "FAIL: esperado MBR removable (slot0=dados FAT/exFAT, slot1=ESP 0xEF); "
                f"got typ0=0x{typ0:02X} typ1=0x{typ1:02X}"
            )
            sys.exit(1)
        data_mbr_start = struct.unpack_from("<I", mbr, 0x1BE + 8)[0]
        print(f"  MBR dados LBA={data_mbr_start} (Windows removable monta isto)")

    f.seek(512)
    gpt_sig = f.read(8)
    if gpt_sig != b"EFI PART":
        print(f"FAIL: assinatura GPT ausente ({gpt_sig!r})")
        sys.exit(1)

    f.seek(2 * 512)
    print("=== GPT + BPB ===")
    data_lba = None
    esp_lba = None
    for i in range(2):
        e = f.read(128)
        s, en = struct.unpack_from("<QQ", e, 32)
        ptype = e[0:16]
        name = e[56 : 56 + 32].decode("utf-16-le", "replace").split("\x00")[0]
        pos = f.tell()
        f.seek(s * 512)
        b = f.read(512)
        f.seek(pos)
        print(f"  GPT{i} {name}: LBA {s}..{en}")
        jmp = b[0:3].hex()
        oem = b[3:11]
        fat = b[0x52:0x5A]
        hid = struct.unpack_from("<I", b, 0x1C)[0]
        tot = struct.unpack_from("<I", b, 0x20)[0]
        print(f"    jmp={jmp} oem={oem!r} fat={fat!r} hid={hid} tot={tot}")
        if ptype == ESP_TYPE or name.upper().startswith("EFI"):
            esp_lba = s
        elif i == 1 or name.upper().startswith("NEURAL"):
            data_lba = s
            if b[0] not in (0xEB, 0xE9) or fat != b"FAT32   ":
                print("FAIL: BPB FAT32 invalido para Windows")
                sys.exit(1)
            if hid != s or tot != (en - s + 1):
                print(f"FAIL: hidden/tot mismatch (hid={hid} tot={tot} expect hid={s} tot={en - s + 1})")
                sys.exit(1)

    if esp_lba is None:
        print("FAIL: particao ESP (EFI System) ausente na GPT")
        sys.exit(1)
    print(f"  ESP GPT LBA={esp_lba}")

    f.seek(data_lba * 512)
    b = f.read(512)
    reserved = struct.unpack_from("<H", b, 0x0E)[0]
    fatsz = struct.unpack_from("<I", b, 0x24)[0]
    spc = b[0x0D]
    root_cl = struct.unpack_from("<I", b, 0x2C)[0]
    fat_lba = data_lba + reserved
    data_area = data_lba + reserved + 2 * fatsz

    def fat_get(cl: int) -> int:
        f.seek(fat_lba * 512 + cl * 4)
        return struct.unpack("<I", f.read(4))[0] & 0x0FFFFFFF

    found = False
    cl = root_cl
    walked = 0
    while cl >= 2 and cl < 0x0FFFFFF8 and walked < 256 and not found:
        walked += 1
        f.seek((data_area + (cl - 2) * spc) * 512)
        dirents = f.read(spc * 512)
        for i in range(0, len(dirents), 32):
            if dirents[i] == 0:
                break
            if dirents[i : i + 11] == b"BOOT    LOG":
                print("  BOOT.LOG size", struct.unpack_from("<I", dirents, i + 28)[0])
                found = True
                break
        cl = fat_get(cl)
    if not found:
        print("FAIL: BOOT.LOG ausente na raiz FAT32")
        sys.exit(1)

if fixed_disk or auto_fixed:
    print("OK: MBR fixed-disk (0xEE) + GPT ESP/dados FAT32 prontos para HD externo UEFI")
else:
    print("OK: MBR removable (dados 0x0C primeiro) + GPT/FAT32 prontos para Windows + UEFI")
