#!/usr/bin/env python3
"""Verifica que os modelos v6 (hwexpert + BITNET2B) estão no FAT32 de usb_hw.img."""
import struct

img = r"target\usb_hw.img"
SECTOR = 512
part_lba = 262144  # partição de dados no unificado

with open(img, "rb") as f:
    f.seek(part_lba * SECTOR)
    bpb = f.read(512)
    bps = struct.unpack_from("<H", bpb, 0x0B)[0]
    spc = bpb[0x0D]
    reserved = struct.unpack_from("<H", bpb, 0x0E)[0]
    fat_count = bpb[0x10]
    fat_sectors = struct.unpack_from("<I", bpb, 0x24)[0]
    root_cluster = struct.unpack_from("<I", bpb, 0x2C)[0]
    data_lba = part_lba + reserved + fat_count * fat_sectors
    fat_lba = part_lba + reserved
    print(f"bps={bps} spc={spc} reserved={reserved} fats={fat_count} fat_sec={fat_sectors} root_cl={root_cluster}")

    def fat_get(cl):
        f.seek(fat_lba * SECTOR + cl * 4)
        return struct.unpack("<I", f.read(4))[0] & 0x0FFFFFFF

    def read_cluster(cl):
        f.seek((data_lba + (cl - 2) * spc) * SECTOR)
        return f.read(spc * bps)

    cl = root_cluster
    seen = set()
    entries = []
    while cl not in seen and cl >= 2 and cl < 0x0FFFFFF8:
        seen.add(cl)
        data = read_cluster(cl)
        for off in range(0, len(data), 32):
            name = data[off:off + 11]
            first = name[0]
            if first in (0x00, 0xE5):
                continue
            if data[off + 11] == 0x0F:
                continue
            size = struct.unpack_from("<I", data, off + 28)[0]
            nm = name.rstrip(b" ").decode("ascii", "replace")
            entries.append((nm, size))
        nxt = fat_get(cl)
        if nxt == cl:
            break
        cl = nxt

    print(f"Total entradas: {len(entries)}")
    for nm, size in sorted(entries, key=lambda e: -e[1])[:15]:
        print(f"  {nm:20s} {size / 1048576:9.1f} MB")
    print("--- alvo ---")
    for want in ("HWEXPRT4.BIN", "HWEXPRT.BIN", "HW_EXPERT.BITNET", "BITNET2B.BIN", "BITNET850.BIN"):
        hit = [(nm, size) for nm, size in entries if nm == want]
        print(f"  {want}: {hit if hit else 'AUSENTE'}")
