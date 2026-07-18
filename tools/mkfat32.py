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
    part_lba = 2048
    part_sectors = total_sectors - part_lba
    # FAT32: geometria relativa a particao (nao ao disco inteiro)
    data_sectors = part_sectors - reserved
    fat_sectors = align_up(((data_sectors * 4) + bps - 1) // bps, 1)
    data_start = reserved + fat_count * fat_sectors
    data_sectors = part_sectors - data_start
    total_clusters = data_sectors // spc
    # FAT32 needs >= 65525 clusters
    if total_clusters < 65525:
        fat_sectors = align_up(fat_sectors * 2, 1)
        data_start = reserved + fat_count * fat_sectors
        data_sectors = part_sectors - data_start
        total_clusters = data_sectors // spc
    # Create file
    with open(path, "wb") as f:
        f.write(b'\x00' * size)
    # Write MBR
    mbr = bytearray(512)
    mbr[0x1BE] = 0x00; mbr[0x1BF] = 0x01; mbr[0x1C0] = 0x01; mbr[0x1C1] = 0x00
    mbr[0x1C2] = 0x0C  # FAT32 LBA
    mbr[0x1C3] = 0xFE; mbr[0x1C4] = 0xFF; mbr[0x1C5] = 0xFF
    struct.pack_into("<I", mbr, 0x1C6, part_lba)  # LBA start
    struct.pack_into("<I", mbr, 0x1CA, part_sectors)  # sectors
    mbr[0x1FE], mbr[0x1FF] = 0x55, 0xAA
    with open(path, "r+b") as f:
        f.write(mbr)
    # Write BPB at LBA 2048 (partition start)
    bpb = bytearray(512)
    # jmp + OEM — sem isso o Windows recusa montar FAT32
    bpb[0], bpb[1], bpb[2] = 0xEB, 0x58, 0x90
    bpb[3:11] = b"MSWIN4.1"
    struct.pack_into("<H", bpb, 0x0B, bps)      # bytes per sector
    bpb[0x0D] = spc                             # sectors per cluster
    struct.pack_into("<H", bpb, 0x0E, reserved) # reserved sectors
    bpb[0x10] = fat_count                       # FAT count
    struct.pack_into("<H", bpb, 0x11, 0)        # root entries (0 for FAT32)
    struct.pack_into("<H", bpb, 0x13, 0)        # total sectors 16-bit (0 for FAT32)
    bpb[0x15] = 0xF8                            # media descriptor (hard disk)
    struct.pack_into("<H", bpb, 0x16, 0)        # FAT size 16 (0 for FAT32)
    struct.pack_into("<H", bpb, 0x18, 63)       # sectors per track (CHS dummy)
    struct.pack_into("<H", bpb, 0x1A, 255)      # heads
    struct.pack_into("<I", bpb, 0x1C, part_lba) # hidden sectors (= LBA start)
    struct.pack_into("<I", bpb, 0x20, part_sectors)  # total sectors na particao
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
        f.seek(part_lba * 512)
        f.write(bpb)
        # FSInfo sector
        fsinfo = bytearray(512)
        struct.pack_into("<I", fsinfo, 0, 0x41615252)
        struct.pack_into("<I", fsinfo, 484, 0x61417272)
        struct.pack_into("<I", fsinfo, 488, 0xFFFFFFFF)
        struct.pack_into("<I", fsinfo, 492, total_clusters - 1)
        struct.pack_into("<I", fsinfo, 508, 0xAA550000)
        f.write(fsinfo)
        # Backup boot + FSInfo (@ reserved sectors 6 and 7)
        f.seek((part_lba + 6) * 512)
        f.write(bpb)
        f.write(fsinfo)
        # Zero FAT tables
        for i in range(fat_count):
            f.seek((part_lba + reserved + i * fat_sectors) * 512)
            fat = bytearray(fat_sectors * 512)
            # FAT[0] = media descriptor + dirty flags
            struct.pack_into("<I", fat, 0, 0x0FFFFF8)
            struct.pack_into("<I", fat, 4, 0x0FFFFFF)
            # FAT[2] = root directory (EOC) — nunca alocar como data
            struct.pack_into("<I", fat, 8, 0x0FFFFFFF)
            f.write(fat)
        # Zero root directory cluster (cluster 2)
        root_lba = part_lba + data_start + (2 - 2) * spc
        f.seek(root_lba * 512)
        f.write(b'\x00' * spc * bps)
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
    # Firmware blobs — política FAT (GSP dezenas de MB; não embutir catálogo inteiro).
    # FW_FAT_CHIPS=gp108,skl,kbl,green_sardine  (default)
    # Opt-in GSP: FW_FAT_CHIPS=...,tu102,ad102,ga102
    # FW_FAT_CHIPS=all → legado (tudo em firmware/)
    fw_root = os.path.join(ROOT, "firmware")
    fat_chips = os.environ.get("FW_FAT_CHIPS", "gp108,skl,kbl,green_sardine").strip().lower()
    chip_allow = {c.strip() for c in fat_chips.split(",") if c.strip()} if fat_chips != "all" else None
    # Mapa chip → substrings no path relativo (posix)
    chip_paths = {
        "gp108": ("nvidia/gp108/",),
        "tu102": ("nvidia/tu102/",),
        "tu106": ("nvidia/tu106/",),
        "ad102": ("nvidia/ad102/",),
        "ga102": ("nvidia/ga102/",),
        "green_sardine": ("amdgpu/green_sardine_",),
        "raphael": ("amdgpu/gc_10_3_6_", "amdgpu/psp_13_0_5_", "amdgpu/sdma_5_2_6"),
        "gc_11_5_0": ("amdgpu/gc_11_5_0_",),
        "skl": ("i915/skl_",),
        "kbl": ("i915/kbl_",),
        "dg2": ("i915/dg2_",),
        "xe": ("xe/",),
        # WiFi/NIC legado (se chip listado ou all)
        "rtl_nic": ("rtl_nic/",),
        "rtlwifi": ("rtlwifi/",),
        "iwlwifi": ("intel/iwlwifi/",),
    }

    def fw_allowed(rel_posix: str) -> bool:
        if chip_allow is None:
            return True
        # NIC/WiFi legado: sempre no FAT (não é GSP)
        if rel_posix.startswith(("rtl_nic/", "rtlwifi/", "intel/iwlwifi/", "realtek/")):
            return True
        # GSP: opt-in explícito (dezenas de MB)
        if "/gsp/" in rel_posix:
            return bool(chip_allow & {"tu102", "ad102", "ga102", "tu106"})
        for chip in chip_allow:
            for prefix in chip_paths.get(chip, ()):
                if rel_posix.startswith(prefix) or prefix.rstrip("_") in rel_posix:
                    return True
        return False

    def collect_fw_from(fw_base: str, gsp_only: bool = False) -> None:
        if not os.path.isdir(fw_base):
            return
        for root, dirs, fnames in os.walk(fw_base):
            # Skip clone tree se existir sob target/firmware
            dirs[:] = [d for d in dirs if d != "linux-firmware"]
            for name in fnames:
                ext = os.path.splitext(name)[1].lower()
                if ext not in (".bin", ".fw", ".ucode"):
                    continue
                rel = os.path.relpath(root, fw_base)
                if rel == ".":
                    rel_posix = name
                else:
                    rel_posix = rel.replace("\\", "/") + "/" + name
                if gsp_only and "/gsp/" not in rel_posix:
                    continue
                if not fw_allowed(rel_posix):
                    continue
                prefix = "" if rel == "." else rel.upper().replace("\\", "_").replace("/", "_")
                fw_path = os.path.join(root, name)
                # NVIDIA GP108: nomes 8.3 reais (kernel Fat32Reader usa encode_83).
                # Sem isso FW_FECS_BL_BIN vira "FW_FECS_BL_" e o ACR nunca acha o blob.
                gp108_short = {
                    "fecs_bl.bin": "FECS_BL.BIN",
                    "fecs_data.bin": "FECS_DAT.BIN",
                    "fecs_inst.bin": "FECS_INS.BIN",
                    "fecs_sig.bin": "FECS_SIG.BIN",
                    "gpccs_bl.bin": "GPCCS_BL.BIN",
                    "gpccs_data.bin": "GPCCS_DA.BIN",
                    "gpccs_inst.bin": "GPCCS_IN.BIN",
                    "gpccs_sig.bin": "GPCCS_SI.BIN",
                    "sw_ctx.bin": "SW_CTX.BIN",
                    "sw_bundle_init.bin": "SW_BNDL.BIN",
                    "sw_method_init.bin": "SW_MTHD.BIN",
                    "sw_nonctx.bin": "SW_NONC.BIN",
                    "bl.bin": "ACR_BL.BIN",
                    "ucode_load.bin": "ACRLOAD.BIN",
                    "ucode_unload.bin": "ACRUNLD.BIN",
                    "unload_bl.bin": "ACR_UBL.BIN",
                }
                lname = name.lower()
                if ("nvidia" in prefix.lower() and "gp108" in prefix.lower()) or prefix in (
                    "NVIDIA_GP108",
                    "NVIDIA_GP108_GR",
                    "NVIDIA_GP108_ACR",
                ):
                    if lname in gp108_short:
                        fw_name = gp108_short[lname]
                    elif prefix in ("NVIDIA_GP108",):
                        fw_name = "FW_" + name.upper().replace(".", "_")
                    else:
                        fw_name = "FW_GP108_" + name.upper().replace(".", "_")
                elif prefix in ("NVIDIA_GP108",):
                    fw_name = "FW_" + name.upper().replace(".", "_")
                elif prefix.startswith("NVIDIA_GP108"):
                    fw_name = "FW_GP108_" + name.upper().replace(".", "_")
                else:
                    fw_name = (
                        f"FW_{prefix}_{name.upper().replace('.', '_')}"
                        if prefix
                        else f"FW_{name.upper().replace('.', '_')}"
                    )
                # Evita duplicar se já coletado de firmware/
                if any(existing[0] == fw_name for existing in files):
                    continue
                files.append((fw_name, fw_path))

    # Catálogo repo (sem GSP — gitignore)
    collect_fw_from(fw_root, gsp_only=False)
    # GSP só em target/firmware; embute no FAT apenas com FW_FAT_CHIPS opt-in
    if chip_allow is None or (chip_allow & {"tu102", "ad102", "ga102", "tu106"}):
        collect_fw_from(os.path.join(ROOT, "target", "firmware"), gsp_only=True)

    # KernelPack NVIDIA/Intel (host packers → target/)
    for nkp_name, fat_name in (
        ("NKP_SM61.BIN", "NKP_SM61.BIN"),
        ("NKP_VECTOR_ADD.BIN", "NKP_VADD.BIN"),
        ("NKP_GEN9.BIN", "NKP_GEN9.BIN"),
        ("NKP_DG2.BIN", "NKP_DG2.BIN"),
        ("NKP_GFX1030.BIN", "NKP_GFX1030.BIN"),
        ("NKP_GFX1103.BIN", "NKP_GFX1103.BIN"),
        ("NKP_GFX90C.BIN", "NKP_GFX90C.BIN"),
    ):
        nkp_path = os.path.join(ROOT, "target", nkp_name)
        if os.path.isfile(nkp_path) and not any(e[0] == fat_name for e in files):
            files.append((fat_name, nkp_path))

    # CONFIG.TXT — BOOT_MODE via env ou inferido do path (passado por populate caller)
    boot_mode = os.environ.get("BOOT_MODE", "hw").strip().lower()
    if boot_mode not in ("qemu", "hw"):
        boot_mode = "hw"
    platform = "virtio-qemu" if boot_mode == "qemu" else "baremetal"
    config_content = (
        f"BOOT_MODE={boot_mode}\nPLATFORM={platform}\nGPU=auto\nLOG_TO_FAT32=1\n"
    ).encode()
    files.append(("CONFIG.TXT", config_content))
    # BOOT.LOG pre-alocado (256 KiB): USB-MSC ou soft-reboot UEFI ramlog.
    boot_log = (
        b"[S] neural-os-core BOOT.LOG\n"
        b"# Placeholder - apos soft-reboot HW deve ter linhas [T+] / Knn:\n"
        b"# Se ainda so isto: UEFI nao achou SFS do volume de dados.\n"
        + b"\x00" * (256 * 1024 - 160)
    )
    files.append(("BOOT.LOG", boot_log[: 256 * 1024]))

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
