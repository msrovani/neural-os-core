"""S467: conta dirents BOOT.LOG no root da FAT32 (regressao do dup s402/s139).

Uso: python tools/count_bootlog_dirents.py <img> [part_lba]
part_lba default = slot0 do MBR (unified: data 0x0C @262144; imagem so-FAT: 2048).
Conta por nome 8.3 EXATO ("BOOT    LOG") — substring pegava o dir 'boot' da ESP.
"""
import struct
import sys

path = sys.argv[1] if len(sys.argv) > 1 else "target/bootlog_dedup_test.img"
if len(sys.argv) > 2:
    part_lba = int(sys.argv[2], 0)
else:
    with open(path, "rb") as mbr_f:
        mbr_f.seek(0x1BE)  # slot0 (0x1CE = slot1 = ESP no unified!)
        slots = [mbr_f.read(16) for _ in range(4)]
    # dados FAT32 primeiro (0x0B/0x0C/0x1C); ESP 0xEF so como fallback
    part_lba = 0
    for e in slots:
        if e[4] in (0x0B, 0x0C, 0x1C):
            part_lba = struct.unpack("<I", e[8:12])[0]
            break
    if part_lba == 0:
        part_lba = 2048  # imagem mkfat32 sem particao slot0 (fallback legado)
with open(path, "rb") as f:
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

exact = [n for n in names if n == "BOOT    LOG"]
print(f"part_lba={part_lba} root dirents={len(names)} BOOT.LOG(exato)={len(exact)}")
assert len(exact) == 1, f"esperado 1 BOOT.LOG, achou {len(exact)}: {exact}"
print("PASS: exatamente 1 BOOT.LOG")
