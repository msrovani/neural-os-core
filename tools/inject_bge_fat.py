#!/usr/bin/env python3
"""Substitui/injeta BGE.BIN no FAT32 existente (target/disk_qemu.raw) sem recriar o disco."""
import os, struct, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DISK = os.path.join(ROOT, "target", "disk_qemu.raw")
SRC = os.path.join(ROOT, "target", "bge-small.bitnet")


def name83(name: str) -> bytes:
    if "." in name:
        base, _, ext = name.rpartition(".")
        return (base[:8].ljust(8) + ext[:3].ljust(3)).upper().encode("ascii")
    return name[:11].ljust(11).upper().encode("ascii")


def main():
    if not os.path.exists(DISK):
        print(f"[ERR] {DISK} ausente — rode python tools/build_image.py")
        return 1
    if not os.path.exists(SRC):
        print(f"[ERR] {SRC} ausente — rode python tools/make_bge_stub.py")
        return 1
    data = open(SRC, "rb").read()
    want = name83("BGE.BIN")

    with open(DISK, "r+b") as f:
        mbr = f.read(512)
        part_lba = None
        for i in range(4):
            off = 0x1BE + i * 16
            typ = mbr[off + 4]
            if typ in (0x0B, 0x0C, 0x1C):
                part_lba = struct.unpack_from("<I", mbr, off + 8)[0]
                break
        if part_lba is None:
            print("[ERR] sem particao FAT32")
            return 1
        f.seek(part_lba * 512)
        bpb = f.read(512)
        bps = struct.unpack_from("<H", bpb, 0x0B)[0]
        spc = bpb[0x0D]
        reserved = struct.unpack_from("<H", bpb, 0x0E)[0]
        fat_count = bpb[0x10]
        fat_sec = struct.unpack_from("<I", bpb, 0x24)[0]
        root = struct.unpack_from("<I", bpb, 0x2C)[0]
        fat_lba = part_lba + reserved
        data_lba = part_lba + reserved + fat_count * fat_sec

        def fat_get(cl):
            f.seek(fat_lba * 512 + cl * 4)
            return struct.unpack("<I", f.read(4))[0] & 0x0FFFFFFF

        def fat_set(cl, val):
            f.seek(fat_lba * 512 + cl * 4)
            f.write(struct.pack("<I", val & 0x0FFFFFFF))
            # mirror second FAT if present
            if fat_count > 1:
                f.seek((fat_lba + fat_sec) * 512 + cl * 4)
                f.write(struct.pack("<I", val & 0x0FFFFFFF))

        def free_chain(start):
            cl = start
            while 2 <= cl < 0x0FFFFFF8:
                nxt = fat_get(cl)
                fat_set(cl, 0)
                cl = nxt

        # Find / delete existing BGE.BIN dir entries
        dir_cl = root
        found_slots = []
        while 2 <= dir_cl < 0x0FFFFFF8:
            root_lba = data_lba + (dir_cl - 2) * spc
            for offs in range(0, spc * bps, 32):
                f.seek(root_lba * 512 + offs)
                ent = bytearray(f.read(32))
                if ent[0] == 0:
                    break
                if ent[0] == 0xE5:
                    continue
                if ent[11] & 0x0F == 0x0F:
                    continue
                if ent[0:11] == want:
                    start = struct.unpack_from("<H", ent, 26)[0] | (struct.unpack_from("<H", ent, 20)[0] << 16)
                    free_chain(start)
                    ent[0] = 0xE5
                    f.seek(root_lba * 512 + offs)
                    f.write(ent)
                    found_slots.append((root_lba, offs))
                    print(f"  [del] BGE.BIN antigo cluster={start}")
            dir_cl = fat_get(dir_cl)

        # Allocate clusters
        need = (len(data) + spc * bps - 1) // (spc * bps)
        free = []
        for cl in range(3, 0x0FFFFFF0):
            if fat_get(cl) == 0:
                free.append(cl)
                if len(free) >= need:
                    break
        if len(free) < need:
            print(f"[ERR] sem espaco ({len(free)}/{need})")
            return 1
        for i, cl in enumerate(free):
            cl_lba = data_lba + (cl - 2) * spc
            chunk = data[i * spc * bps : (i + 1) * spc * bps]
            f.seek(cl_lba * 512)
            f.write(chunk + b"\x00" * (spc * bps - len(chunk)))
            fat_set(cl, free[i + 1] if i + 1 < len(free) else 0x0FFFFFFF)

        entry = bytearray(32)
        entry[0:11] = want
        entry[11] = 0x20
        struct.pack_into("<I", entry, 28, len(data))
        struct.pack_into("<H", entry, 26, free[0] & 0xFFFF)
        struct.pack_into("<H", entry, 20, (free[0] >> 16) & 0xFFFF)

        placed = False
        if found_slots:
            rlba, offs = found_slots[0]
            f.seek(rlba * 512 + offs)
            f.write(entry)
            placed = True
        else:
            dir_cl = root
            while 2 <= dir_cl < 0x0FFFFFF8 and not placed:
                root_lba = data_lba + (dir_cl - 2) * spc
                for offs in range(0, spc * bps, 32):
                    f.seek(root_lba * 512 + offs)
                    first = f.read(1)[0]
                    if first in (0x00, 0xE5):
                        f.seek(root_lba * 512 + offs)
                        f.write(entry)
                        placed = True
                        break
                if not placed:
                    dir_cl = fat_get(dir_cl)

        if not placed:
            print("[ERR] sem slot dir")
            return 1
        print(f"[OK] BGE.BIN -> {DISK} ({len(data)} bytes, {need} clusters, start={free[0]})")
        return 0


if __name__ == "__main__":
    sys.exit(main())
