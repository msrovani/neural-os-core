#!/usr/bin/env python3
"""Cria imagem exFAT (MBR type 0x07) com modelos .bitnet e CONFIG.TXT.
Uso: python tools/mkexfat.py [--size 1024] [--label NEURAL-OS] [--output target/disk_qemu.raw]

Layout compativel com crates/neural-kernel/src/exfat.rs (ExfatReader).
ESP UEFI continua FAT; este e o volume de DADOS do boot.
"""
from __future__ import annotations

import argparse
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_SIZE_MB = 3072  # 3GB dados (arquivo FAT32 máx ~4GB-1; partição pode ser maior)
PART_LBA = 2048
BPS = 512
# 8 setores/cluster = 4 KiB (shift=3) — alinhado ao parser do kernel
SPC_SHIFT = 3
SPC = 1 << SPC_SHIFT
CLUSTER_BYTES = BPS * SPC


def _apply_fit_gate_if_enabled() -> None:
    """FIT_GATE=1 → reescreve PACK_LLM com degraus que cabem na RAM host."""
    if os.environ.get("FIT_GATE", "").strip() not in ("1", "true", "yes", "on"):
        return
    tools_dir = os.path.join(ROOT, "tools")
    if tools_dir not in sys.path:
        sys.path.insert(0, tools_dir)
    try:
        import llmfit_pack_filter as fit

        code = fit.run_fit_gate_from_env()
        print(f"[FIT_GATE] applied exit={code} PACK_LLM={os.environ.get('PACK_LLM')}")
    except Exception as e:
        print(f"[FIT_GATE] ERROR skipped: {e}")


def pack_llm_set() -> set[str]:
    """PACK_LLM=850|13|2b|3b|all — default 850 (primeiro degrau). FIT_GATE=1 filtra."""
    _apply_fit_gate_if_enabled()
    raw = os.environ.get("PACK_LLM", "850").strip().lower()
    if not raw or raw in ("none", "0", "off"):
        return set()
    if raw in ("all", "*"):
        return {"850", "13", "2b", "3b"}
    out: set[str] = set()
    for tok in raw.replace(";", ",").split(","):
        t = tok.strip().lower().replace(" ", "")
        if t in ("850", "850m", "fast", "large"):
            out.add("850")
        elif t in ("13", "1.3", "1p3", "1.5", "xl", "1.58", "158"):
            out.add("13")
        elif t in ("2b", "2", "2.0"):
            out.add("2b")
        elif t in ("3b", "3", "pro"):
            out.add("3b")
    return out


def find_file(name: str):
    for d in [
        ROOT,
        os.path.join(ROOT, "target"),
        os.path.join(ROOT, "firmware"),
        os.path.join(ROOT, "crates/neural-kernel"),
        os.path.join(ROOT, "tools/target"),
    ]:
        p = os.path.join(d, name)
        if os.path.exists(p):
            return p
    return None


def find_large(name: str, min_bytes: int = 1_000_000):
    p = find_file(name)
    if p and os.path.getsize(p) >= min_bytes:
        return p
    return None


def find_bitnet_13():
    return find_large("BITNET13.BIN") or find_large("bitnet_1p3b.bitnet")


def find_bitnet_850():
    return (
        find_large("BITNET850.BIN")
        or find_large("bitnet_850m.bitnet")
        or find_large("MICRO.BIN")
    )


def find_bitnet_3b():
    return find_large("BITNET3B.BIN") or find_large("bitnet_3B.bitnet")


def align_up(v: int, a: int) -> int:
    return (v + a - 1) // a * a


def collect_files() -> list[tuple[str, bytes | str | None]]:
    llm = pack_llm_set()
    print(f"[PACK_LLM] {sorted(llm) or 'none'} (env PACK_LLM; default=850)")
    files: list[tuple[str, bytes | str | None]] = [
        ("BGE.BIN", find_file("bge-small.bitnet") or find_file("bge.bin")),
        # ADR-0083 §5.3: roteador MoE treinado (tools/train_router.py). Opcional.
        ("ROUTER.BITNET", find_file("ROUTER.BITNET")),
        ("RUSTCDR.BITNET", find_file("rust_coder.bitnet") or find_file("RUSTCDR.BITNET")),
        ("HW_EXPERT.BITNET", find_file("hw_expert_tf.bitnet") or find_file("hw_expert_v3.bitnet")),
        ("HWEXPRT.BIN", find_file("hw_expert_v3.bitnet") or find_file("hw_expert_tf.bitnet")),
        ("PIPER.BIN", find_file("PIPER_PT_BR.BIN") or find_file("PIPER.BIN")),
        ("PIPER_EN.BIN", find_file("PIPER_EN.BIN")),
        ("STT.BIN", find_file("STT.BIN")),
        ("BPE.BIN", find_file("bpe_vocab.bin") or find_file("BPE.BIN")),
        ("BITNET13.BIN", find_bitnet_13() if "13" in llm else None),
        ("BITNET850.BIN", find_bitnet_850() if "850" in llm else None),
        (
            "BITNET2B.BIN",
            (find_file("bitnet_2B.bitnet")
            or find_file("BITNET2B.BIN")
            or find_file("BITNET-2B.BITNET")
            or find_file("bitnet-BitNet-b1_58-2B-4T.bitnet"))
            if "2b" in llm else None,
        ),
        ("BITNET.BIN", None),
        ("BITNET3B.BIN", find_bitnet_3b() if "3b" in llm else None),
        ("MICRO.BITNET", None if ("850" in llm or "13" in llm) else find_file("MICRO.BITNET")),
    ]
    # ADR-0056: LEGOs cedo no root (antes do walk firmware) — evita "sem slot dir"
    _inject_device_legos(files)
    fw_root = os.path.join(ROOT, "firmware")
    if os.path.isdir(fw_root):
        for root, _dirs, fnames in os.walk(fw_root):
            for name in fnames:
                ext = os.path.splitext(name)[1].lower()
                if ext not in (".bin", ".fw", ".ucode"):
                    continue
                rel = os.path.relpath(root, fw_root)
                prefix = rel.upper().replace("\\", "_").replace("/", "_")
                fw_path = os.path.join(root, name)
                if prefix == "NVIDIA_GP108":
                    fw_name = "FW_" + name.upper().replace(".", "_")
                else:
                    fw_name = f"FW_{prefix}_{name.upper().replace('.', '_')}"
                files.append((fw_name, fw_path))
    boot_mode = os.environ.get("BOOT_MODE", "hw").strip().lower()
    if boot_mode not in ("qemu", "hw"):
        boot_mode = "hw"
    platform = "virtio-qemu" if boot_mode == "qemu" else "baremetal"
    config = (
        f"BOOT_MODE={boot_mode}\nPLATFORM={platform}\nGPU=auto\nLOG_TO_EXFAT=1\n"
        f"NEURALFS_USB_FORMAT=0\n"
        f"EXFAT_WRITE=0\n"
        f"USB_TRUST_ENFORCE=0\n"
    ).encode()
    files.append(("CONFIG.TXT", config))
    return files


def _inject_device_legos(files: list) -> None:
    tools_dir = os.path.join(ROOT, "tools")
    if tools_dir not in sys.path:
        sys.path.insert(0, tools_dir)
    try:
        from pack_device_legos import append_lego_files

        append_lego_files(files)
    except Exception as e:
        print(f"[LEGO] ERROR inject skipped: {e}")


def utf16le(s: str) -> bytes:
    return s.encode("utf-16le")


def make_file_entries(name: str, first_cluster: int, size: int) -> bytes:
    """File(0x85) + Stream(0xC0) + Name(0xC1...) — minimo para ExfatReader."""
    name_chars = list(name)
    name_entries = (len(name_chars) + 14) // 15
    secondary = 1 + name_entries  # stream + name entries
    out = bytearray()
    # 0x85 File
    e = bytearray(32)
    e[0] = 0x85
    e[1] = secondary
    struct.pack_into("<H", e, 4, 0x20)  # archive
    out += e
    # 0xC0 Stream
    e = bytearray(32)
    e[0] = 0xC0
    e[1] = 0x01  # allocation possible / no FAT chain flag clear → use FAT
    e[3] = len(name_chars)  # NameLength (UTF-16 code units)
    struct.pack_into("<Q", e, 8, size)  # valid data length
    struct.pack_into("<I", e, 20, first_cluster)
    struct.pack_into("<Q", e, 24, size)  # data length
    out += e
    # 0xC1 File Name
    for i in range(name_entries):
        e = bytearray(32)
        e[0] = 0xC1
        chunk = name_chars[i * 15 : (i + 1) * 15]
        raw = utf16le("".join(chunk))
        e[2 : 2 + len(raw)] = raw
        out += e
    return bytes(out)



def make_bitmap_entry(first_cluster: int, data_length: int) -> bytes:
    e = bytearray(32)
    e[0] = 0x81
    struct.pack_into("<I", e, 20, first_cluster)
    struct.pack_into("<Q", e, 24, data_length)
    return bytes(e)


def make_upcase_entry(first_cluster: int, data_length: int, table_checksum: int) -> bytes:
    e = bytearray(32)
    e[0] = 0x82
    struct.pack_into("<I", e, 4, table_checksum & 0xFFFFFFFF)
    struct.pack_into("<I", e, 20, first_cluster)
    struct.pack_into("<Q", e, 24, data_length)
    return bytes(e)


def make_volume_label_entry(label: str) -> bytes:
    e = bytearray(32)
    e[0] = 0x83
    chars = list(label[:11])
    e[1] = len(chars)
    raw = utf16le("".join(chars))
    e[2 : 2 + len(raw)] = raw
    return bytes(e)


def exfat_boot_checksum(boot_region: bytes) -> int:
    csum = 0
    for i, b in enumerate(boot_region[: 11 * BPS]):
        if i in (106, 107, 112):
            continue
        csum = ((csum << 31) | (csum >> 1)) & 0xFFFFFFFF
        csum = (csum + b) & 0xFFFFFFFF
    return csum


def write_upcase_identity(f, heap_lba: int, first_cl: int, n_clusters: int, spc: int) -> int:
    table = bytearray(65536 * 2)
    for i in range(65536):
        struct.pack_into("<H", table, i * 2, i)
    csum = 0
    for b in table:
        csum = ((csum << 31) | (csum >> 1)) & 0xFFFFFFFF
        csum = (csum + b) & 0xFFFFFFFF
    for i in range(n_clusters):
        chunk = table[i * CLUSTER_BYTES : (i + 1) * CLUSTER_BYTES]
        if len(chunk) < CLUSTER_BYTES:
            chunk = chunk + b"\x00" * (CLUSTER_BYTES - len(chunk))
        f.seek((heap_lba + (first_cl + i - 2) * spc) * BPS)
        f.write(chunk)
    return csum

def create_exfat(path: str, size_mb: int, label: str) -> dict:
    size = size_mb * 1024 * 1024
    total_sectors = size // BPS
    part_sectors = total_sectors - PART_LBA

    fat_offset = 24
    approx_clusters = max(65536, (part_sectors - fat_offset - 256) // SPC)
    fat_bytes = align_up(approx_clusters * 4, BPS)
    fat_sectors = fat_bytes // BPS
    heap_offset = fat_offset + fat_sectors
    usable = part_sectors - heap_offset
    cluster_count = usable // SPC
    if cluster_count < 4096:
        raise SystemExit(f"imagem pequena demais para exFAT util: {cluster_count} clusters")

    fat_bytes = align_up((cluster_count + 2) * 4, BPS)
    fat_sectors = fat_bytes // BPS
    heap_offset = fat_offset + fat_sectors
    usable = part_sectors - heap_offset
    cluster_count = usable // SPC

    bitmap_bytes = align_up((cluster_count + 7) // 8, CLUSTER_BYTES)
    bitmap_clusters = max(1, bitmap_bytes // CLUSTER_BYTES)
    upcase_clusters = (65536 * 2 + CLUSTER_BYTES - 1) // CLUSTER_BYTES
    bitmap_cl = 2
    upcase_cl = bitmap_cl + bitmap_clusters
    root_cl = upcase_cl + upcase_clusters
    # Recalcular cluster_count apos reservar metadados no heap
    reserved_meta = bitmap_clusters + upcase_clusters + 1
    if cluster_count <= root_cl + 8:
        raise SystemExit("imagem pequena demais apos bitmap/upcase")

    with open(path, "wb") as f:
        f.write(b"\x00" * size)

    mbr = bytearray(512)
    mbr[0x1BE] = 0x00
    mbr[0x1C2] = 0x07
    struct.pack_into("<I", mbr, 0x1C6, PART_LBA)
    struct.pack_into("<I", mbr, 0x1CA, part_sectors)
    mbr[0x1FE], mbr[0x1FF] = 0x55, 0xAA

    vbr = bytearray(512)
    vbr[0], vbr[1], vbr[2] = 0xEB, 0x76, 0x90
    vbr[3:11] = b"EXFAT   "
    struct.pack_into("<Q", vbr, 64, PART_LBA)
    struct.pack_into("<Q", vbr, 72, part_sectors)
    struct.pack_into("<I", vbr, 80, fat_offset)
    struct.pack_into("<I", vbr, 84, fat_sectors)
    struct.pack_into("<I", vbr, 88, heap_offset)
    struct.pack_into("<I", vbr, 92, cluster_count)
    struct.pack_into("<I", vbr, 96, root_cl)
    struct.pack_into("<I", vbr, 100, 0x12345678)
    struct.pack_into("<H", vbr, 104, 0x0100)
    struct.pack_into("<H", vbr, 106, 0)
    vbr[108], vbr[109], vbr[110], vbr[111], vbr[112] = 9, SPC_SHIFT, 1, 0x80, 0
    vbr[0x1FE], vbr[0x1FF] = 0x55, 0xAA

    ext = bytearray(512)
    ext[0x1FE], ext[0x1FF] = 0x55, 0xAA
    boot_region = bytearray(11 * BPS)
    boot_region[0:512] = vbr
    for i in range(1, 9):
        boot_region[i * 512 : (i + 1) * 512] = ext
    csum = exfat_boot_checksum(bytes(boot_region))
    csum_sector = bytearray(512)
    for i in range(128):
        struct.pack_into("<I", csum_sector, i * 4, csum)

    with open(path, "r+b") as f:
        f.write(mbr)
        base = PART_LBA * BPS
        f.seek(base)
        f.write(boot_region)
        f.seek(base + 11 * BPS)
        f.write(csum_sector)
        f.seek(base + 12 * BPS)
        f.write(boot_region)
        f.seek(base + 23 * BPS)
        f.write(csum_sector)

        fat = bytearray(fat_sectors * BPS)
        struct.pack_into("<I", fat, 0, 0xFFFFFFF8)
        struct.pack_into("<I", fat, 4, 0xFFFFFFFF)

        def chain_clusters(first: int, count: int) -> None:
            for i in range(count):
                cl = first + i
                nxt = 0xFFFFFFFF if i + 1 == count else first + i + 1
                struct.pack_into("<I", fat, cl * 4, nxt)

        chain_clusters(bitmap_cl, bitmap_clusters)
        chain_clusters(upcase_cl, upcase_clusters)
        chain_clusters(root_cl, 1)
        f.seek((PART_LBA + fat_offset) * BPS)
        f.write(fat)

        heap_lba = PART_LBA + heap_offset
        bitmap = bytearray(bitmap_bytes)
        for cl in range(2, root_cl + 1):
            bit_index = cl - 2
            bitmap[bit_index // 8] |= 1 << (bit_index % 8)
        for i in range(bitmap_clusters):
            f.seek((heap_lba + (bitmap_cl + i - 2) * SPC) * BPS)
            chunk = bitmap[i * CLUSTER_BYTES : (i + 1) * CLUSTER_BYTES]
            f.write(chunk + b"\x00" * (CLUSTER_BYTES - len(chunk)))

        upcase_csum = write_upcase_identity(f, heap_lba, upcase_cl, upcase_clusters, SPC)

        root = bytearray(CLUSTER_BYTES)
        off = 0
        for ent in (
            make_volume_label_entry(label),
            make_bitmap_entry(bitmap_cl, bitmap_bytes),
            make_upcase_entry(upcase_cl, 65536 * 2, upcase_csum),
        ):
            root[off : off + 32] = ent
            off += 32
        f.seek((heap_lba + (root_cl - 2) * SPC) * BPS)
        f.write(root)

    meta = {
        "part_lba": PART_LBA,
        "fat_offset": fat_offset,
        "fat_sectors": fat_sectors,
        "heap_offset": heap_offset,
        "cluster_count": cluster_count,
        "root_cl": root_cl,
        "bitmap_cl": bitmap_cl,
        "upcase_cl": upcase_cl,
        "heap_lba": PART_LBA + heap_offset,
        "fat_lba": PART_LBA + fat_offset,
        "spc": SPC,
        "first_data_cl": root_cl + 1,
    }
    print(
        f"[OK] {path}: {size_mb}MB exFAT, {cluster_count} clusters, "
        f"FAT {fat_sectors}s, heap@{heap_offset}, boot_csum=0x{csum:08X}"
    )
    return meta


def fat_get(f, fat_lba: int, cl: int) -> int:
    f.seek(fat_lba * BPS + cl * 4)
    return struct.unpack("<I", f.read(4))[0]


def fat_set(f, fat_lba: int, cl: int, val: int) -> None:
    f.seek(fat_lba * BPS + cl * 4)
    f.write(struct.pack("<I", val))


def bitmap_set(f, heap_lba: int, bitmap_cl: int, cl: int) -> None:
    """Marca cluster `cl` (2-based) no bitmap (pode span multiplos clusters)."""
    bit_index = cl - 2
    byte_i = bit_index // 8
    bit = bit_index % 8
    cluster_off = byte_i // CLUSTER_BYTES
    within = byte_i % CLUSTER_BYTES
    lba = heap_lba + (bitmap_cl + cluster_off - 2) * SPC
    f.seek(lba * BPS + within)
    b = f.read(1)[0]
    b |= 1 << bit
    f.seek(lba * BPS + within)
    f.write(bytes([b]))


def alloc_clusters(f, meta: dict, n: int) -> list[int]:
    fat_lba = meta["fat_lba"]
    heap_lba = meta["heap_lba"]
    bitmap_cl = meta["bitmap_cl"]
    max_cl = meta["cluster_count"] + 1
    free: list[int] = []
    start_cl = meta.get("first_data_cl", meta["root_cl"] + 1)
    for cl in range(start_cl, max_cl + 2):
        if fat_get(f, fat_lba, cl) == 0:
            free.append(cl)
            if len(free) >= n:
                break
    if len(free) < n:
        return []
    for i, cl in enumerate(free):
        nxt = free[i + 1] if i + 1 < len(free) else 0xFFFFFFFF
        fat_set(f, fat_lba, cl, nxt)
        bitmap_set(f, heap_lba, bitmap_cl, cl)
    return free


def append_root_entries(f, meta: dict, entries: bytes) -> bool:
    fat_lba = meta["fat_lba"]
    heap_lba = meta["heap_lba"]
    root = meta["root_cl"]
    spc = meta["spc"]
    dir_cl = root
    while dir_cl >= 2 and dir_cl < 0xFFFFFFF0:
        cl_lba = heap_lba + (dir_cl - 2) * spc
        f.seek(cl_lba * BPS)
        buf = bytearray(f.read(CLUSTER_BYTES))
        # find free 32-byte slots
        need = len(entries)
        for off in range(0, CLUSTER_BYTES - need + 1, 32):
            if buf[off] == 0x00:
                buf[off : off + need] = entries
                f.seek(cl_lba * BPS)
                f.write(buf)
                return True
        nxt = fat_get(f, fat_lba, dir_cl)
        if nxt >= 0xFFFFFFF8 or nxt < 2:
            # extend root
            extra = alloc_clusters(f, meta, 1)
            if not extra:
                return False
            fat_set(f, fat_lba, dir_cl, extra[0])
            fat_set(f, fat_lba, extra[0], 0xFFFFFFFF)
            f.seek((heap_lba + (extra[0] - 2) * spc) * BPS)
            f.write(b"\x00" * CLUSTER_BYTES)
            dir_cl = extra[0]
        else:
            dir_cl = nxt
    return False


def populate(path: str, meta: dict) -> None:
    with open(path, "r+b") as f:
        for name, src in collect_files():
            if isinstance(src, bytes):
                data = src
            elif src and os.path.exists(src):
                with open(src, "rb") as sf:
                    data = sf.read()
            else:
                print(f"  [--] {name} — nao encontrado")
                continue
            ncl = (len(data) + CLUSTER_BYTES - 1) // CLUSTER_BYTES
            if ncl == 0:
                ncl = 1
            chain = alloc_clusters(f, meta, ncl)
            if not chain:
                print(f"  [--] {name}: sem espaco ({ncl} cls)")
                continue
            heap_lba = meta["heap_lba"]
            spc = meta["spc"]
            for idx, cl in enumerate(chain):
                chunk = data[idx * CLUSTER_BYTES : (idx + 1) * CLUSTER_BYTES]
                pad = CLUSTER_BYTES - len(chunk)
                f.seek((heap_lba + (cl - 2) * spc) * BPS)
                f.write(chunk + (b"\x00" * pad))
            ents = make_file_entries(name, chain[0], len(data))
            ok = append_root_entries(f, meta, ents)
            status = "OK" if ok else "sem slot dir"
            print(f"  [{status}] {name} ({len(data) // 1024}K, {ncl} cls)")


def main():
    p = argparse.ArgumentParser(description="Gera disk_*.raw em exFAT para boot de dados")
    p.add_argument("--size", default=str(DEFAULT_SIZE_MB))
    p.add_argument("--label", default="NEURAL-OS")
    p.add_argument("--output", default=None)
    args = p.parse_args()
    size_mb = int(str(args.size).replace("M", "").replace("m", ""))
    target_dir = os.path.join(ROOT, "target")
    os.makedirs(target_dir, exist_ok=True)
    out = args.output
    if out is None:
        out = os.path.join(target_dir, "disk_qemu.raw")
    elif not os.path.isabs(out):
        out = os.path.join(ROOT, out)
    if "BOOT_MODE" not in os.environ:
        base = os.path.basename(out).lower()
        os.environ["BOOT_MODE"] = "hw" if "disk_hw" in base else "qemu"
    print(f"\n=== {os.path.basename(out)} ({size_mb}MB exFAT) BOOT_MODE={os.environ.get('BOOT_MODE')} ===")
    if os.path.exists(out):
        os.remove(out)
    meta = create_exfat(out, size_mb, args.label)
    populate(out, meta)


if __name__ == "__main__":
    main()
