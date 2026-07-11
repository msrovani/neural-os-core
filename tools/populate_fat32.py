#!/usr/bin/env python3
"""Popula imagem FAT32 com modelos .bitnet e CONFIG.TXT"""
import os, struct, sys

WORKSPACE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(WORKSPACE)

def read_sectors(f, lba, count):
    f.seek(lba * 512)
    return f.read(count * 512)

def write_sectors(f, lba, data):
    f.seek(lba * 512)
    f.write(data)

def find_file(name):
    for d in [ROOT, os.path.join(ROOT, "target"), os.path.join(ROOT, "crates/neural-kernel")]:
        p = os.path.join(d, name)
        if os.path.exists(p): return p
    return None

def fat32_write(disk_path):
    with open(disk_path, "r+b") as f:
        mbr = read_sectors(f, 0, 1)
        if mbr[0x1FE] != 0x55 or mbr[0x1FF] != 0xAA:
            print(f"[ERR] MBR signature not found in {disk_path}")
            return
        # Find FAT32 partition
        for i in range(4):
            off = 0x1BE + i * 16
            typ = mbr[off + 4]
            if typ in (0x0B, 0x0C, 0x1C):
                lba_start = struct.unpack_from("<I", mbr, off + 8)[0]
                break
        else:
            print("[ERR] No FAT32 partition found")
            return
        # Read BPB
        bpb = read_sectors(f, lba_start, 1)
        bps = struct.unpack_from("<H", bpb, 0x0B)[0]
        spc = bpb[0x0D]
        reserved = struct.unpack_from("<H", bpb, 0x0E)[0]
        fat_count = bpb[0x10]
        spf = struct.unpack_from("<I", bpb, 0x24)[0]
        root_cluster = struct.unpack_from("<I", bpb, 0x2C)[0]
        data_lba = lba_start + reserved + fat_count * spf
        # Files to write
        files = [
            ("MICRO.BITNET", find_file("micro.bitnet")),
            ("RUSTCDR.BITNET", find_file("rust_coder.bitnet") or find_file("RUSTCDR.BITNET")),
            ("HW_EXPERT.BITNET", find_file("hw_expert.bitnet") or find_file("HW_EXPERT.BITNET")),
            ("BGE.BIN", find_file("bge-small.bitnet") or find_file("BGE.BIN") or find_file("bge.bin")),
            ("CONFIG.TXT", None),
        ]
        # Create CONFIG.TXT content
        config_content = f"BOOT_MODE=hw\nPLATFORM=baremetal\nGPU=auto\nLOG_TO_FAT32=1\n".encode()
        files[4] = ("CONFIG.TXT", config_content)

        for name, src in files:
            if src is None:
                print(f"  [--] {name} — nao encontrado, pulando")
                continue
            data = src if isinstance(src, bytes) else open(src, "rb").read()
            size = len(data)
            clusters_needed = (size + spc * bps - 1) // (spc * bps) + 1
            # Find free clusters in FAT
            fat_lba = lba_start + reserved
            free_clusters = []
            for cl in range(2, 0x0FFFFFF0):
                fat_sec = fat_lba + (cl * 4) // bps
                fat_off = (cl * 4) % bps
                if fat_off == 0:
                    sec_data = read_sectors(f, fat_sec, 1)
                entry = struct.unpack_from("<I", sec_data, fat_off)[0] & 0x0FFFFFFF
                if entry == 0:
                    free_clusters.append(cl)
                    if len(free_clusters) >= clusters_needed:
                        break
            if len(free_clusters) < clusters_needed:
                print(f"  [--] {name}: sem espaco ({clusters_needed} clusters)")
                continue
            # Write file data to clusters
            written = 0
            for idx, cl in enumerate(free_clusters):
                cl_lba = data_lba + (cl - 2) * spc
                chunk = data[written:written + spc * bps]
                if chunk:
                    padded = chunk + b'\x00' * (spc * bps - len(chunk))
                    write_sectors(f, cl_lba, padded)
                    written += len(chunk)
                # Update FAT entry
                next_cl = free_clusters[idx + 1] if idx + 1 < len(free_clusters) else 0x0FFFFFFF
                fat_sec = fat_lba + (cl * 4) // bps
                fat_off = (cl * 4) % bps
                if fat_off == 0:
                    sec_data = bytearray(read_sectors(f, fat_sec, 1))
                struct.pack_into("<I", sec_data, fat_off, next_cl & 0x0FFFFFFF)
                if fat_off == 0:
                    write_sectors(f, fat_sec, bytes(sec_data))
            # Write directory entry
            name_bytes = name.ljust(11, ' ')[:11].encode()
            entry_data = name_bytes + b'\x20' + b'\x00' * 8 + struct.pack("<HHI", 0, 0, size) + struct.pack("<H", free_clusters[0] & 0xFFFF) + struct.pack("<H", (free_clusters[0] >> 16) & 0xFFFF)
            # Find slot in root dir
            root_first_lba = data_lba + (root_cluster - 2) * spc
            for offs in range(0, spc * bps, 32):
                sec = root_first_lba + offs // bps
                secoff = offs % bps
                if secoff == 0:
                    dir_sec = bytearray(read_sectors(f, sec, 1))
                if dir_sec[secoff] == 0 or dir_sec[secoff] == 0xE5:
                    dir_sec[secoff:secoff + 32] = entry_data
                    write_sectors(f, sec, bytes(dir_sec))
                    break
            print(f"  [OK] {name} ({size // 1024}K, {clusters_needed} clusters)")

if __name__ == "__main__":
    for disk in [os.path.join(WORKSPACE, "disk_qemu.raw"), os.path.join(WORKSPACE, "disk_hw.raw")]:
        if os.path.exists(disk):
            print(f"\n=== {os.path.basename(disk)} ===")
            fat32_write(disk)
