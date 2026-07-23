#!/usr/bin/env python3
"""Cria disco QEMU enxuto (64MB) com MICRO/BGE/experts — sem BitNet 2B duplicado."""
import os, struct, random, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "target", "disk_qemu.raw")
SIZE_MB = 64


def find_file(name):
    for d in [ROOT, os.path.join(ROOT, "target"), os.path.join(ROOT, "firmware")]:
        p = os.path.join(d, name)
        if os.path.exists(p):
            return p
    return None


def main():
    size = SIZE_MB * 1024 * 1024
    bps, spc, reserved, fat_count = 512, 1, 32, 2
    total_sectors = size // bps
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    print(f"writing {OUT} {SIZE_MB}MB ...")
    with open(OUT, "wb") as f:
        f.truncate(size)
        f.seek(0)
        f.write(b"\x00" * 4096)  # touch start; sparse OK on NTFS if truncate works
        # ensure full size for QEMU IDE
        f.seek(size - 1)
        f.write(b"\x00")

    data_sectors = total_sectors - reserved
    fat_sectors = ((data_sectors * 4) + bps - 1) // bps
    with open(OUT, "r+b") as f:
        mbr = bytearray(512)
        mbr[0x1BE] = 0
        mbr[0x1BF] = 1
        mbr[0x1C0] = 1
        mbr[0x1C1] = 0
        mbr[0x1C2] = 0x0C
        mbr[0x1C3] = 0xFE
        mbr[0x1C4] = 0xFF
        mbr[0x1C5] = 0xFF
        struct.pack_into("<I", mbr, 0x1C6, 2048)
        struct.pack_into("<I", mbr, 0x1CA, total_sectors - 2048)
        mbr[0x1FE], mbr[0x1FF] = 0x55, 0xAA
        f.write(mbr)

        bpb = bytearray(512)
        struct.pack_into("<H", bpb, 0x0B, bps)
        bpb[0x0D] = spc
        struct.pack_into("<H", bpb, 0x0E, reserved)
        bpb[0x10] = fat_count
        bpb[0x15] = 0xF8
        struct.pack_into("<I", bpb, 0x20, total_sectors)
        struct.pack_into("<I", bpb, 0x24, fat_sectors)
        struct.pack_into("<I", bpb, 0x2C, 2)
        struct.pack_into("<H", bpb, 0x30, 1)
        struct.pack_into("<H", bpb, 0x32, 6)
        bpb[0x40] = 0x80
        bpb[0x42] = 0x29
        struct.pack_into("<I", bpb, 0x43, random.randint(0, 0xFFFFFFFF))
        bpb[0x47:0x52] = b"NEURAL-OS  "
        bpb[0x52:0x5A] = b"FAT32   "
        bpb[0x1FE], bpb[0x1FF] = 0x55, 0xAA
        f.seek(2048 * 512)
        f.write(bpb)
        for i in range(fat_count):
            f.seek((2048 + reserved + i * fat_sectors) * 512)
            fat = bytearray(fat_sectors * 512)
            struct.pack_into("<I", fat, 0, 0x0FFFFF8)
            struct.pack_into("<I", fat, 4, 0x0FFFFFF)
            struct.pack_into("<I", fat, 8, 0x0FFFFFFF)
            f.write(fat)
    print(f"FAT ready fat_sectors={fat_sectors}")

    files = [
        ("MICRO.BITNET", find_file("MICRO.BITNET")),
        ("BGE.BIN", find_file("bge-small.bitnet") or find_file("bge.bin")),
        ("RUSTCDR.BITNET", find_file("rust_coder.bitnet")),
        ("HWEXPRT.BIN", find_file("hw_expert_v3.bitnet") or find_file("hw_expert_tf.bitnet")),
        (
            "CONFIG.TXT",
            b"BOOT_MODE=qemu\nPLATFORM=virtio-qemu\nGPU=auto\nLOG_TO_FAT32=1\n",
        ),
    ]

    with open(OUT, "r+b") as f:
        f.seek(2048 * 512)
        bpb = bytearray(f.read(512))
        bps = struct.unpack_from("<H", bpb, 0x0B)[0]
        spc = bpb[0x0D]
        reserved = struct.unpack_from("<H", bpb, 0x0E)[0]
        fat_count = bpb[0x10]
        fat_sectors = struct.unpack_from("<I", bpb, 0x24)[0]
        root_cluster = struct.unpack_from("<I", bpb, 0x2C)[0]
        data_lba = 2048 + reserved + fat_count * fat_sectors
        fat_lba = 2048 + reserved

        def fat_get(c):
            f.seek(fat_lba * 512 + c * 4)
            return struct.unpack("<I", f.read(4))[0] & 0x0FFFFFFF

        def fat_set(c, v):
            f.seek(fat_lba * 512 + c * 4)
            f.write(struct.pack("<I", v & 0x0FFFFFFF))

        def encode(name):
            if "." in name:
                b, e = name.upper().rsplit(".", 1)
                return (b[:8].ljust(8) + e[:3].ljust(3)).encode()
            return name[:11].ljust(11).upper().encode()

        next_free = 3
        for name, src in files:
            data = src if isinstance(src, (bytes, bytearray)) else (open(src, "rb").read() if src else None)
            if data is None:
                print(f"  [--] {name}")
                continue
            need = (len(data) + spc * bps - 1) // (spc * bps)
            free = []
            cl = next_free
            while len(free) < need and cl < 0x0FFFFFF0:
                if fat_get(cl) == 0:
                    free.append(cl)
                cl += 1
            next_free = free[-1] + 1 if free else next_free
            if len(free) < need:
                print(f"  [--] {name} no space")
                continue
            for i, c in enumerate(free):
                chunk = data[i * spc * bps : (i + 1) * spc * bps]
                f.seek((data_lba + (c - 2) * spc) * 512)
                f.write(chunk + b"\x00" * (spc * bps - len(chunk)))
                fat_set(c, free[i + 1] if i + 1 < len(free) else 0x0FFFFFFF)
            entry = bytearray(32)
            entry[0:11] = encode(name)
            entry[11] = 0x20
            struct.pack_into("<I", entry, 28, len(data))
            struct.pack_into("<H", entry, 26, free[0] & 0xFFFF)
            struct.pack_into("<H", entry, 20, (free[0] >> 16) & 0xFFFF)
            root_lba = data_lba + (root_cluster - 2) * spc
            f.seek(root_lba * 512)
            sec = bytearray(f.read(spc * bps))
            for off in range(0, len(sec), 32):
                if sec[off] in (0, 0xE5):
                    sec[off : off + 32] = entry
                    f.seek(root_lba * 512)
                    f.write(sec)
                    break
            print(f"  [OK] {name} ({len(data)//1024}K)")

    print(f"[OK] slim disk {OUT}")


if __name__ == "__main__":
    main()
