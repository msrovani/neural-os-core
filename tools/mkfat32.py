#!/usr/bin/env python3
"""Cria imagem FAT32 com MBR, formata, copia modelos .bitnet e CONFIG.TXT.
Uso: python tools/mkfat32.py [--size 512] [--label NEURAL-OS] [--output target/disk_qemu.raw]

Inclui BITNET-2B se existir (nao pula). Tamanho generoso p/ QEMU e HW (32GB pendrive ok).
BOOT_MODE via env BOOT_MODE=qemu|hw (default: inferido do nome do arquivo).
"""
import os, struct, sys, argparse

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_SIZE_MB = 1024

def find_file(name):
    for d in [ROOT, os.path.join(ROOT, "target"), os.path.join(ROOT, "firmware"),
              os.path.join(ROOT, "crates/neural-kernel"), os.path.join(ROOT, "tools/target")]:
        p = os.path.join(d, name)
        if os.path.exists(p): return p
    return None

def align_up(v, a): return (v + a - 1) // a * a

def create_fat32(path, size_mb, label):
    size = size_mb * 1024 * 1024
    bps, spc, reserved, fat_count = 512, 1, 32, 2
    total_sectors = size // bps
    # FAT32: data area starts after reserved + FATs
    # Root cluster is always 2
    # Calculate FAT size in sectors
    data_sectors = total_sectors - reserved
    fat_sectors = align_up(((data_sectors * 4) + bps - 1) // bps, 1)
    # Adjust: data area = total - reserved - fat_count * fat_sectors
    data_start = reserved + fat_count * fat_sectors
    data_sectors = total_sectors - data_start
    total_clusters = data_sectors // spc
    # FAT32 needs >= 65525 clusters
    if total_clusters < 65525:
        fat_sectors = align_up(fat_sectors * 2, 1)
        data_start = reserved + fat_count * fat_sectors
        data_sectors = total_sectors - data_start
        total_clusters = data_sectors // spc
    # Create file
    with open(path, "wb") as f:
        f.write(b'\x00' * size)
    # Write MBR
    mbr = bytearray(512)
    mbr[0x1BE] = 0x00; mbr[0x1BF] = 0x01; mbr[0x1C0] = 0x01; mbr[0x1C1] = 0x00
    mbr[0x1C2] = 0x0C  # FAT32 LBA
    mbr[0x1C3] = 0xFE; mbr[0x1C4] = 0xFF; mbr[0x1C5] = 0xFF
    struct.pack_into("<I", mbr, 0x1C6, 2048)  # LBA start
    struct.pack_into("<I", mbr, 0x1CA, (total_sectors - 2048))  # sectors
    mbr[0x1FE], mbr[0x1FF] = 0x55, 0xAA
    with open(path, "r+b") as f:
        f.write(mbr)
    # Write BPB at LBA 2048 (partition start)
    bpb = bytearray(512)
    struct.pack_into("<H", bpb, 0x0B, bps)      # bytes per sector
    bpb[0x0D] = spc                             # sectors per cluster
    struct.pack_into("<H", bpb, 0x0E, reserved) # reserved sectors
    bpb[0x10] = fat_count                       # FAT count
    struct.pack_into("<H", bpb, 0x11, 0)        # root entries (0 for FAT32)
    struct.pack_into("<H", bpb, 0x13, 0)        # total sectors 16-bit (0 for FAT32)
    bpb[0x15] = 0xF8                            # media descriptor (hard disk)
    struct.pack_into("<H", bpb, 0x16, 0)        # FAT size 16 (0 for FAT32)
    struct.pack_into("<H", bpb, 0x18, 0)        # sectors per track
    struct.pack_into("<H", bpb, 0x1A, 0)        # heads
    struct.pack_into("<I", bpb, 0x1C, 0)        # hidden sectors
    struct.pack_into("<I", bpb, 0x20, total_sectors)  # total sectors 32-bit
    struct.pack_into("<I", bpb, 0x24, fat_sectors)    # FAT size 32
    bpb[0x28] = 0                                # extended flags
    bpb[0x29] = 0                                # FS version
    struct.pack_into("<I", bpb, 0x2C, 2)         # root cluster
    struct.pack_into("<H", bpb, 0x30, 1)         # FSInfo sector
    struct.pack_into("<H", bpb, 0x32, 6)         # backup boot sector
    bpb[0x40] = 0x80                             # drive number
    bpb[0x41] = 0                                # reserved
    bpb[0x42] = 0x29                             # boot signature
    import random
    struct.pack_into("<I", bpb, 0x43, random.randint(0, 0xFFFFFFFF))
    label_bytes = label.encode().ljust(11, b' ')[:11]
    bpb[0x47:0x47+11] = label_bytes
    bpb[0x52:0x52+8] = b'FAT32   '
    bpb[0x1FE], bpb[0x1FF] = 0x55, 0xAA
    with open(path, "r+b") as f:
        f.seek(2048 * 512)
        f.write(bpb)
        # FSInfo sector
        fsinfo = bytearray(512)
        struct.pack_into("<I", fsinfo, 0, 0x41615252)
        struct.pack_into("<I", fsinfo, 484, 0x61417272)
        struct.pack_into("<I", fsinfo, 488, 0xFFFFFFFF)
        struct.pack_into("<I", fsinfo, 492, total_clusters - 1)
        struct.pack_into("<I", fsinfo, 508, 0xAA550000)
        f.write(fsinfo)
        # Zero FAT tables
        for i in range(fat_count):
            f.seek((2048 + reserved + i * fat_sectors) * 512)
            fat = bytearray(fat_sectors * 512)
            # FAT[0] = media descriptor + dirty flags
            struct.pack_into("<I", fat, 0, 0x0FFFFF8)
            struct.pack_into("<I", fat, 4, 0x0FFFFFF)
            # FAT[2] = root directory (EOC) — nunca alocar como data
            struct.pack_into("<I", fat, 8, 0x0FFFFFFF)
            f.write(fat)
        # Zero root directory cluster (cluster 2)
        root_lba = 2048 + data_start + (2 - 2) * spc
        f.seek(root_lba * 512)
        f.write(b'\x00' * spc * bps)
        # Zero FSInfo backup
        print(f"[OK] {path}: {size_mb}MB, {total_clusters} clusters, FAT {fat_sectors}s x {fat_count}")

def populate(path):
    files = [
        ("BGE.BIN", find_file("bge-small.bitnet") or find_file("bge.bin")),
        ("RUSTCDR.BITNET", find_file("rust_coder.bitnet") or find_file("RUSTCDR.BITNET")),
        ("HW_EXPERT.BITNET", find_file("hw_expert_tf.bitnet") or find_file("hw_expert_v3.bitnet")),
        ("HWEXPRT.BIN", find_file("hw_expert_v3.bitnet") or find_file("hw_expert_tf.bitnet")),
        # BITNET2B primeiro; BITNET.BIN = alias só se ≠ 2B (evita 2×577MB no disco 1G)
        ("BITNET2B.BIN", find_file("bitnet_2B.bitnet") or find_file("BITNET2B.BIN")
         or find_file("BITNET-2B.BITNET") or find_file("bitnet-BitNet-b1_58-2B-4T.bitnet")),
        ("BITNET.BIN", find_file("BITNET.BIN") if find_file("BITNET.BIN")
         and find_file("BITNET.BIN") != (find_file("bitnet_2B.bitnet") or find_file("BITNET2B.BIN"))
         else None),
        ("PIPER.BIN", find_file("PIPER_PT_BR.BIN") or find_file("PIPER.BIN")),
        ("PIPER_EN.BIN", find_file("PIPER_EN.BIN")),
        ("STT.BIN", find_file("STT.BIN")),
        ("BPE.BIN", find_file("bpe_vocab.bin") or find_file("BPE.BIN")),
        ("MICRO.BITNET", find_file("MICRO.BITNET") or find_file("target/MICRO.BITNET")),
        ("MICRO.BIN", find_file("MICRO.BITNET")),
    ]
    # SKIP_2B removido: com imagem 512MB+ e pendrive 32GB, sempre inclui se existir
    # Firmware blobs (todos os grupos)
    fw_root = os.path.join(ROOT, "firmware")
    if os.path.isdir(fw_root):
        for root, dirs, fnames in os.walk(fw_root):
            for name in fnames:
                ext = os.path.splitext(name)[1].lower()
                if ext not in (".bin", ".fw", ".ucode"): continue
                rel = os.path.relpath(root, fw_root)
                prefix = rel.upper().replace("\\", "_").replace("/", "_")
                fw_path = os.path.join(root, name)
                # NVIDIA GP108 mantém nome curto (FW_FECS_BL_BIN) p/ compatibilidade
                if prefix == "NVIDIA_GP108":
                    fw_name = "FW_" + name.upper().replace(".", "_")
                else:
                    # .ucode → _UCODE no 8.3-ish FAT name
                    fw_name = f"FW_{prefix}_{name.upper().replace('.', '_')}"
                files.append((fw_name, fw_path))
    # CONFIG.TXT — BOOT_MODE via env ou inferido do path (passado por populate caller)
    boot_mode = os.environ.get("BOOT_MODE", "hw").strip().lower()
    if boot_mode not in ("qemu", "hw"):
        boot_mode = "hw"
    platform = "virtio-qemu" if boot_mode == "qemu" else "baremetal"
    config_content = (
        f"BOOT_MODE={boot_mode}\nPLATFORM={platform}\nGPU=auto\nLOG_TO_FAT32=1\n"
    ).encode()
    files.append(("CONFIG.TXT", config_content))

    with open(path, "r+b") as f:
        f.seek(2048 * 512)  # skip MBR + partition start
        bpb = bytearray(f.read(512))
        bps = struct.unpack_from("<H", bpb, 0x0B)[0]
        spc = bpb[0x0D]
        reserved = struct.unpack_from("<H", bpb, 0x0E)[0]
        fat_count = bpb[0x10]
        fat_sectors = struct.unpack_from("<I", bpb, 0x24)[0]
        root_cluster = struct.unpack_from("<I", bpb, 0x2C)[0]
        data_lba = 2048 + reserved + fat_count * fat_sectors
        fat_lba = 2048 + reserved

        for name, src in files:
            data = src if isinstance(src, bytes) else (open(src, "rb").read() if src else None)
            if data is None:
                print(f"  [--] {name} — nao encontrado")
                continue
            clusters_needed = (len(data) + spc * bps - 1) // (spc * bps)
            # Find free clusters in FAT
            free = []
            for cl in range(2, 0x0FFFFFF0):
                fat_sec_lba = fat_lba + (cl * 4) // bps
                f.seek(fat_sec_lba * 512 + (cl * 4) % bps)
                entry = struct.unpack("<I", f.read(4))[0] & 0x0FFFFFFF
                if entry == 0:
                    free.append(cl)
                    if len(free) >= clusters_needed:
                        break
            if len(free) < clusters_needed:
                print(f"  [--] {name}: sem espaco ({len(free)}/{clusters_needed} clusters)")
                continue
            # Write data & FAT chain
            for idx, cl in enumerate(free):
                cl_lba = data_lba + (cl - 2) * spc
                chunk = data[idx * spc * bps : (idx + 1) * spc * bps]
                if chunk:
                    f.seek(cl_lba * 512)
                    f.write(chunk + b'\x00' * (spc * bps - len(chunk)))
                next_cl = free[idx + 1] if idx + 1 < len(free) else 0x0FFFFFFF
                f.seek(fat_lba * 512 + cl * 4)
                f.write(struct.pack("<I", next_cl & 0x0FFFFFFF))
            # Write directory entry into root (extend cluster chain if needed)
            name83 = name.upper().replace(".", "").ljust(11) if False else None
            # 8.3: split name/ext
            if "." in name:
                base, _, ext = name.rpartition(".")
                name_bytes = (base[:8].ljust(8) + ext[:3].ljust(3)).upper().encode("ascii", "replace")
            else:
                name_bytes = name[:11].ljust(11).upper().encode("ascii", "replace")
            entry_data = bytearray(32)
            entry_data[0:11] = name_bytes
            entry_data[11] = 0x20  # archive
            struct.pack_into("<I", entry_data, 28, len(data))
            struct.pack_into("<H", entry_data, 26, free[0] & 0xFFFF)
            struct.pack_into("<H", entry_data, 20, (free[0] >> 16) & 0xFFFF)

            def fat_get(cl):
                f.seek(fat_lba * 512 + cl * 4)
                return struct.unpack("<I", f.read(4))[0] & 0x0FFFFFFF

            def fat_set(cl, val):
                f.seek(fat_lba * 512 + cl * 4)
                f.write(struct.pack("<I", val & 0x0FFFFFFF))

            def find_any_free_cluster():
                # start after last used free chain to avoid O(n^2) from 2
                start = free[-1] + 1 if free else 3
                for cl in range(start, 0x0FFFFFF0):
                    if fat_get(cl) == 0:
                        return cl
                for cl in range(3, start):
                    if fat_get(cl) == 0:
                        return cl
                return None

            placed = False
            dir_cl = root_cluster
            while not placed and dir_cl >= 2 and dir_cl < 0x0FFFFFF8:
                root_lba = data_lba + (dir_cl - 2) * spc
                for offs in range(0, spc * bps, 32):
                    f.seek(root_lba * 512 + offs)
                    first = f.read(1)
                    if first in (b"\x00", b"\xE5"):
                        f.seek(root_lba * 512 + offs)
                        f.write(entry_data)
                        placed = True
                        break
                if placed:
                    break
                nxt = fat_get(dir_cl)
                if nxt >= 0x0FFFFFF8 or nxt < 2:
                    # extend root directory cluster chain
                    new_cl = find_any_free_cluster()
                    if new_cl is None:
                        break
                    fat_set(dir_cl, new_cl)
                    fat_set(new_cl, 0x0FFFFFFF)
                    f.seek((data_lba + (new_cl - 2) * spc) * 512)
                    f.write(b"\x00" * spc * bps)
                    dir_cl = new_cl
                else:
                    dir_cl = nxt

            status = "OK" if placed else "sem slot dir"
            print(f"  [{status}] {name} ({len(data)//1024}K, {clusters_needed} cls)")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--size", default=str(DEFAULT_SIZE_MB),
                        help=f"Size in MB (default: {DEFAULT_SIZE_MB})")
    parser.add_argument("--label", default="NEURAL-OS", help="Volume label")
    parser.add_argument("--output", default=None, help="Output path (default: target/disk_qemu.raw)")
    args = parser.parse_args()
    size_mb = int(args.size.replace("M", "").replace("m", ""))
    target_dir = os.path.join(ROOT, "target")
    os.makedirs(target_dir, exist_ok=True)
    disks = []
    if args.output:
        out = args.output if os.path.isabs(args.output) else os.path.join(ROOT, args.output)
        disks.append(out)
    else:
        disks = [os.path.join(target_dir, "disk_qemu.raw")]
    for d in disks:
        os.makedirs(os.path.dirname(d) or ".", exist_ok=True)
        # Infer BOOT_MODE se ainda nao setado
        if "BOOT_MODE" not in os.environ:
            base = os.path.basename(d).lower()
            os.environ["BOOT_MODE"] = "hw" if "disk_hw" in base else "qemu"
        print(f"\n=== {os.path.basename(d)} ({size_mb}MB) BOOT_MODE={os.environ.get('BOOT_MODE')} ===")
        if os.path.exists(d):
            os.remove(d)
        create_fat32(d, size_mb, args.label)
        populate(d)
