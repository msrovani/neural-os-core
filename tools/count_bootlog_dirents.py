"""S467: conta dirents BOOT.LOG no root da FAT32 (regressao do dup s402/s139)."""
import struct
import sys

path = sys.argv[1] if len(sys.argv) > 1 else "target/bootlog_dedup_test.img"
with open(path, "rb") as f:
    # FAT32 dentro de particao iniciando em LBA 2048 (mkfat32 create_fat32)
    part_lba = 2048
    base = part_lba * 512
    f.seek(base + 11)
    bps = struct.unpack("<H", f.read(2))[0]
    spc = f.read(1)[0]
    f.seek(base + 36)
    fat_sz = struct.unpack("<I", f.read(4))[0]
    f.seek(base + 44)
    root_cl = struct.unpack("<I", f.read(4))[0]
    fat_off = base + 32 * bps

    def next_cl(c):
        f.seek(fat_off + c * 4)
        return struct.unpack("<I", f.read(4))[0] & 0x0FFFFFFF

    data_off = fat_off + 2 * fat_sz * bps
    names = []
    c = root_cl
    for _ in range(65536):  # anti-loop: cadeia corrompida nao gira p/ sempre
        if c >= 0x0FFFFFF8:
            break
        f.seek(data_off + (c - 2) * spc * bps)
        sec = f.read(spc * bps)
        for i in range(0, len(sec), 32):
            e = sec[i : i + 32]
            if e[0] in (0x00, 0xE5):
                continue
            if e[11] == 0x0F:
                continue
            names.append(e[0:11].decode("ascii", "replace"))
        c = next_cl(c)
    else:
        raise SystemExit("FAIL: cadeia do root dir com loop (>65536 clusters)")

boot = [n for n in names if "BOOT" in n]
print(f"root dirents={len(names)} BOOT*={boot}")
assert boot == ["BOOT    LOG"], boot
print("PASS: exatamente 1 BOOT.LOG")
