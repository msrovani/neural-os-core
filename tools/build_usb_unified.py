#!/usr/bin/env python3
"""Gera imagem USB unificada (boot UEFI + dados FAT32/exFAT) para Rufus DD / um pendrive.

Layout (GPT + MBR amigavel a Windows removable):
  Part GPT 0 — ESP (conteudo de target/uefi.img / bootloader 0.11) — FAT
  Part GPT 1 — dados (FAT32 default — monta no Explorer; --exfat opcional)
  MBR slot0 — dados 0x0C/0x07 PRIMEIRO (Windows em pendrive ignora GPT e
    so monta MBR; 0xEE-only = nenhuma letra; 0xEE-primeiro = \"FS nao
    reconhecido\"). Slot1 — ESP 0xEF. UEFI boot usa a GPT.

Uso:
  python tools/build_usb_unified.py
  python tools/build_usb_unified.py --size 1536 --output target/usb_hw.img
  python tools/build_image.py --hw --unified
  python tools/build_usb_unified.py --fat32   # dados FAT32 (BOOT.LOG / modelos)

Requer: target/uefi.img (cargo build --release -p boot) e modelos/firmware
        como em mkexfat.py / build_image.py --hw.
        Device LEGO (ADR-0056): pack_device_legos.py + inject LEGO*.MD no volume.
"""
from __future__ import annotations

import argparse
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import uuid
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SECTOR = 512
DEFAULT_SIZE_MB = 3072  # 3GB partição dados (FAT32 file limit ≈4GB-1)

# GPT type GUIDs (mixed-endian as stored on disk)
GUID_ESP = bytes.fromhex("28732ac11ff8d211ba4b00a0c93ec93b")  # C12A7328-F81F-11D2-BA4B-00A0C93EC93B
GUID_BASIC = bytes.fromhex("a2a0d0ebb9e5334487c068b6b72699c7")  # EBD0A0A2-B9E5-4433-87C0-68B6B72699C7


def align_up(v: int, a: int) -> int:
    return (v + a - 1) // a * a


def crc32(data: bytes) -> int:
    return zlib.crc32(data) & 0xFFFFFFFF


def parse_uefi_esp(uefi_path: str) -> tuple[int, int, bytes]:
    """Retorna (esp_start_lba, esp_sectors, esp_raw_bytes) a partir de uefi.img."""
    size = os.path.getsize(uefi_path)
    with open(uefi_path, "rb") as f:
        mbr = f.read(SECTOR)
        if mbr[0x1FE:0x200] != b"\x55\xaa":
            raise SystemExit(f"[ERRO] {uefi_path}: MBR sem assinatura 55AA")
        f.seek(SECTOR)
        hdr = f.read(92)
        if hdr[:8] != b"EFI PART":
            raise SystemExit(f"[ERRO] {uefi_path}: GPT header ausente (bootloader 0.11 esperado)")
        entries_lba = struct.unpack_from("<Q", hdr, 72)[0]
        entry_count = struct.unpack_from("<I", hdr, 80)[0]
        entry_size = struct.unpack_from("<I", hdr, 84)[0]
        f.seek(entries_lba * SECTOR)
        esp_start = esp_end = None
        for _ in range(entry_count):
            e = f.read(entry_size)
            if len(e) < entry_size:
                break
            if e[:16] == b"\x00" * 16:
                continue
            start = struct.unpack_from("<Q", e, 32)[0]
            end = struct.unpack_from("<Q", e, 40)[0]
            # Prefer ESP GUID; senao primeira particao nao-vazia
            if e[:16] == GUID_ESP or esp_start is None:
                esp_start, esp_end = start, end
                if e[:16] == GUID_ESP:
                    break
        if esp_start is None or esp_end is None:
            raise SystemExit(f"[ERRO] {uefi_path}: nenhuma particao GPT")
        sectors = int(esp_end - esp_start + 1)
        f.seek(esp_start * SECTOR)
        raw = f.read(sectors * SECTOR)
        if len(raw) != sectors * SECTOR:
            raise SystemExit(f"[ERRO] {uefi_path}: ESP truncado")
    print(f"[OK] ESP de {uefi_path}: LBA {esp_start}..{esp_end} ({sectors} setores, {len(raw)//1024}KB)")
    print(f"     uefi.img total={size} bytes")
    return int(esp_start), sectors, raw


def encode_utf16le_name(name: str, nbytes: int = 72) -> bytes:
    b = name.encode("utf-16-le")
    return (b + b"\x00" * nbytes)[:nbytes]


def write_gpt(
    f,
    *,
    total_sectors: int,
    disk_guid: bytes,
    parts: list[tuple[bytes, int, int, str, bytes]],
) -> None:
    """parts: list of (type_guid, start_lba, end_lba, name, part_guid)."""
    entry_size = 128
    entry_count = 128
    entries = bytearray(entry_count * entry_size)
    for i, (type_guid, start, end, name, part_guid) in enumerate(parts):
        off = i * entry_size
        entries[off : off + 16] = type_guid
        entries[off + 16 : off + 32] = part_guid
        struct.pack_into("<Q", entries, off + 32, start)
        struct.pack_into("<Q", entries, off + 40, end)
        struct.pack_into("<Q", entries, off + 48, 0)  # attrs
        entries[off + 56 : off + 56 + 72] = encode_utf16le_name(name)

    entries_crc = crc32(bytes(entries))
    first_usable = 34
    last_usable = total_sectors - 34
    entries_lba = 2
    backup_entries_lba = total_sectors - 33
    backup_header_lba = total_sectors - 1

    def make_header(current_lba: int, alt_lba: int, my_entries_lba: int) -> bytearray:
        hdr = bytearray(SECTOR)
        hdr[0:8] = b"EFI PART"
        struct.pack_into("<I", hdr, 8, 0x00010000)  # revision
        struct.pack_into("<I", hdr, 12, 92)  # header size
        struct.pack_into("<I", hdr, 16, 0)  # crc32 placeholder
        struct.pack_into("<Q", hdr, 24, current_lba)
        struct.pack_into("<Q", hdr, 32, alt_lba)
        struct.pack_into("<Q", hdr, 40, first_usable)
        struct.pack_into("<Q", hdr, 48, last_usable)
        hdr[56:72] = disk_guid
        struct.pack_into("<Q", hdr, 72, my_entries_lba)
        struct.pack_into("<I", hdr, 80, entry_count)
        struct.pack_into("<I", hdr, 84, entry_size)
        struct.pack_into("<I", hdr, 88, entries_crc)
        struct.pack_into("<I", hdr, 16, crc32(hdr[:92]))
        return hdr

    primary = make_header(1, backup_header_lba, entries_lba)
    backup = make_header(backup_header_lba, 1, backup_entries_lba)

    f.seek(1 * SECTOR)
    f.write(primary)
    f.seek(entries_lba * SECTOR)
    f.write(entries)
    f.seek(backup_entries_lba * SECTOR)
    f.write(entries)
    f.seek(backup_header_lba * SECTOR)
    f.write(backup)


def write_protective_mbr(f, *, total_sectors: int) -> None:
    """MBR protetor GPT padrao (uma unica 0xEE). So para disco fixo; em USB removable
    o Windows ignora GPT e nao cria letra — use write_removable_mbr."""
    mbr = bytearray(SECTOR)
    mbr[0x1BE] = 0x00
    mbr[0x1BE + 4] = 0xEE
    struct.pack_into("<I", mbr, 0x1BE + 8, 1)
    struct.pack_into("<I", mbr, 0x1BE + 12, min(max(1, total_sectors - 1), 0xFFFFFFFF))
    mbr[0x1FE], mbr[0x1FF] = 0x55, 0xAA
    f.seek(0)
    f.write(mbr)


def write_removable_mbr(
    f,
    *,
    esp_start: int,
    esp_sectors: int,
    data_start: int,
    data_sectors: int,
    data_mbr_type: int = 0x0C,
) -> None:
    """MBR para pendrive: só partição de dados (Windows Explorer).
    
    Slot 0 = FAT32/exFAT (letra no Explorer). Sem ESP no MBR — UEFI
    boot usa a GPT. Sem 0xEE: Windows ignora GPT em media removable.
    """
    mbr = bytearray(SECTOR)
    # Part 0 — dados (Explorer)
    mbr[0x1BE] = 0x00
    mbr[0x1BE + 4] = data_mbr_type & 0xFF
    struct.pack_into("<I", mbr, 0x1BE + 8, data_start)
    struct.pack_into("<I", mbr, 0x1BE + 12, min(data_sectors, 0xFFFFFFFF))
    mbr[0x1FE], mbr[0x1FF] = 0x55, 0xAA
    f.seek(0)
    f.write(mbr)


def write_hybrid_mbr(
    f,
    *,
    esp_start: int,
    esp_end: int,
    data_start: int,
    data_sectors: int,
    total_sectors: int,
    data_mbr_type: int = 0x07,
) -> None:
    """Legado 0xEE-primeiro — nao usar em USB (Windows monta EE sem FS)."""
    mbr = bytearray(SECTOR)
    ee_start = 1
    ee_size = max(1, data_start - 1)
    mbr[0x1BE] = 0x00
    mbr[0x1BE + 4] = 0xEE
    struct.pack_into("<I", mbr, 0x1BE + 8, ee_start)
    struct.pack_into("<I", mbr, 0x1BE + 12, min(ee_size, 0xFFFFFFFF))

    mbr[0x1CE] = 0x00
    mbr[0x1CE + 4] = data_mbr_type
    struct.pack_into("<I", mbr, 0x1CE + 8, data_start)
    struct.pack_into("<I", mbr, 0x1CE + 12, min(data_sectors, 0xFFFFFFFF))

    mbr[0x1FE], mbr[0x1FF] = 0x55, 0xAA
    f.seek(0)
    f.write(mbr)
    _ = (esp_start, esp_end, total_sectors)


def patch_exfat_vbr(f, part_lba: int, part_sectors: int) -> None:
    """Ajusta PartitionOffset + VolumeLength do VBR exFAT na particao de dados."""
    f.seek(part_lba * SECTOR)
    vbr = bytearray(f.read(SECTOR))
    if vbr[3:11] != b"EXFAT   ":
        print("[AVISO] VBR sem assinatura EXFAT — patch PartitionOffset/VolumeLength mesmo assim")
    struct.pack_into("<Q", vbr, 64, part_lba)
    struct.pack_into("<Q", vbr, 72, part_sectors)
    vbr[0x1FE], vbr[0x1FF] = 0x55, 0xAA
    f.seek(part_lba * SECTOR)
    f.write(vbr)
    # Recalcular boot checksum nos 11 setores + escrever sector 11 e backup@23
    f.seek(part_lba * SECTOR)
    boot = bytearray(f.read(11 * SECTOR))
    # Patch sector 0 again inside boot buffer
    struct.pack_into("<Q", boot, 64, part_lba)
    struct.pack_into("<Q", boot, 72, part_sectors)
    csum = 0
    for i, b in enumerate(boot):
        if i in (106, 107, 112):
            continue
        csum = ((csum << 31) | (csum >> 1)) & 0xFFFFFFFF
        csum = (csum + b) & 0xFFFFFFFF
    csum_sector = bytearray(SECTOR)
    for i in range(128):
        struct.pack_into("<I", csum_sector, i * 4, csum)
    f.seek(part_lba * SECTOR)
    f.write(boot)
    f.seek((part_lba + 11) * SECTOR)
    f.write(csum_sector)
    # Backup region @ +12
    f.seek((part_lba + 12) * SECTOR)
    f.write(boot)
    f.seek((part_lba + 23) * SECTOR)
    f.write(csum_sector)


def patch_fat_bpb(f, part_lba: int, part_sectors: int) -> None:
    """Ajusta Hidden/Total + jump/OEM do BPB FAT32 (e backup boot @+6)."""
    f.seek(part_lba * SECTOR)
    bpb = bytearray(f.read(SECTOR))
    if bpb[0x52:0x5A] != b"FAT32   " and bpb[0x36:0x3B] != b"FAT32":
        print("[AVISO] BPB sem assinatura FAT32 tipica — patch Hidden/Total mesmo assim")
    # Windows exige jmp + OEM validos para montar
    if bpb[0] not in (0xEB, 0xE9):
        bpb[0], bpb[1], bpb[2] = 0xEB, 0x58, 0x90
    if bpb[3:11] == b"\x00" * 8:
        bpb[3:11] = b"MSWIN4.1"
    struct.pack_into("<I", bpb, 0x1C, part_lba)
    struct.pack_into("<I", bpb, 0x20, part_sectors)
    bpb[0x1FE], bpb[0x1FF] = 0x55, 0xAA
    f.seek(part_lba * SECTOR)
    f.write(bpb)
    # Backup boot sector (BPB_BkBootSec, tipicamente 6)
    bk = struct.unpack_from("<H", bpb, 0x32)[0]
    if 1 <= bk < 32:
        f.seek((part_lba + bk) * SECTOR)
        f.write(bpb)


def build_data_volume(size_mb: int, tmp_path: str, *, use_fat32: bool) -> int:
    """Gera disk via mkexfat (default) ou mkfat32; retorna LBA inicio particao (2048)."""
    env = os.environ.copy()
    env.pop("SKIP_2B", None)
    env["BOOT_MODE"] = "hw"
    env.setdefault("PYTHONIOENCODING", "utf-8")

    # ADR-0056: pack LEGOs antes do volume de dados
    print("=== Device LEGO pack (ecosystem/devices -> target/lego) ===")
    r_lego = subprocess.run(
        [sys.executable, os.path.join(ROOT, "tools", "pack_device_legos.py")],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
    )
    if r_lego.stdout:
        sys.stdout.write(r_lego.stdout)
        if not r_lego.stdout.endswith("\n"):
            sys.stdout.write("\n")
    if r_lego.returncode != 0:
        print(f"[ERRO] pack_device_legos.py exit={r_lego.returncode}: {(r_lego.stderr or '')[:400]}")
        sys.exit(r_lego.returncode)

    maker = "mkfat32.py" if use_fat32 else "mkexfat.py"
    fs_name = "FAT32" if use_fat32 else "exFAT"
    cmd = [
        sys.executable,
        os.path.join(ROOT, "tools", maker),
        "--size",
        str(size_mb),
        "--output",
        tmp_path,
        "--label",
        "NEURAL-OS",
    ]
    print(f"=== Dados {fs_name} {size_mb}MB (BOOT_MODE=hw) + Device LEGO ===")
    r = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True, timeout=1200)
    if r.stdout:
        sys.stdout.write(r.stdout)
        if not r.stdout.endswith("\n"):
            sys.stdout.write("\n")
    if r.returncode != 0:
        print(f"[ERRO] {maker} exit={r.returncode}: {(r.stderr or '')[:500]}")
        sys.exit(r.returncode)
    return 2048


def ensure_uefi_img(uefi_path: str, build_boot: bool) -> None:
    # --build-boot sempre recompila (kernel/features mudam; uefi.img stale engana)
    if build_boot:
        print("=== cargo build --release -p boot (gera uefi.img) ===")
        env = os.environ.copy()
        env.setdefault("CARGO_TARGET_DIR", os.path.join(ROOT, "target"))
        r = subprocess.run(
            ["cargo", "build", "--release", "-p", "boot"],
            cwd=ROOT,
            env=env,
            timeout=3600,
        )
        if r.returncode != 0 or not os.path.exists(uefi_path):
            raise SystemExit("[ERRO] falha ao gerar uefi.img via cargo build -p boot")
        return
    if os.path.exists(uefi_path) and os.path.getsize(uefi_path) > 1024 * 1024:
        return
    raise SystemExit(
        f"[ERRO] {uefi_path} ausente. Rode: cargo build --release -p boot\n"
        "       ou passe --build-boot"
    )

def main() -> None:
    p = argparse.ArgumentParser(description="USB unificado: ESP UEFI + exFAT dados (Rufus DD)")
    p.add_argument("--size", type=int, default=DEFAULT_SIZE_MB, help="Tamanho da particao de dados em MB")
    p.add_argument("--output", default=None, help="Saida (default: target/usb_hw.img)")
    p.add_argument("--uefi", default=None, help="Caminho uefi.img (default: target/uefi.img)")
    p.add_argument("--data-raw", default=None, help="Reusar disk_hw.raw em vez de regenerar")
    p.add_argument("--build-boot", action="store_true", help="Se falta uefi.img, roda cargo build -p boot")
    p.add_argument(
        "--exfat",
        action="store_true",
        help="Particao de dados exFAT (Windows pode marcar RAW; default e FAT32)",
    )
    p.add_argument(
        "--fat32",
        action="store_true",
        help="Particao FAT32 (default; redundante, so documenta)",
    )
    args = p.parse_args()
    use_fat32 = not args.exfat  # default FAT32 — Explorer monta; --exfat opt-in

    target_dir = os.path.join(ROOT, "target")
    os.makedirs(target_dir, exist_ok=True)
    uefi_path = args.uefi or os.path.join(target_dir, "uefi.img")
    out = args.output or os.path.join(target_dir, "usb_hw.img")
    if not os.path.isabs(out):
        out = os.path.join(ROOT, out)

    src_v3 = os.path.join(target_dir, "hw_expert_v3.bitnet")
    dst_v3 = os.path.join(target_dir, "hw_expert_tf.bitnet")
    if os.path.exists(src_v3) and not os.path.exists(dst_v3):
        shutil.copy2(src_v3, dst_v3)

    ensure_uefi_img(uefi_path, args.build_boot)
    esp_src_lba, esp_sectors, esp_raw = parse_uefi_esp(uefi_path)

    esp_start = 2048
    if esp_src_lba != 2048:
        print(f"[AVISO] ESP fonte em LBA {esp_src_lba}; unificado usa LBA {esp_start}")
    esp_end = esp_start + esp_sectors - 1
    data_mbr_type = 0x0C if use_fat32 else 0x07
    fs_name = "FAT32" if use_fat32 else "exFAT"

    with tempfile.TemporaryDirectory(prefix="nk_usb_") as tmp:
        if args.data_raw:
            data_raw = args.data_raw if os.path.isabs(args.data_raw) else os.path.join(ROOT, args.data_raw)
            if not os.path.exists(data_raw):
                raise SystemExit(f"[ERRO] --data-raw nao encontrado: {data_raw}")
            part_lba_in_raw = 2048
        else:
            data_raw = os.path.join(tmp, "disk_hw.raw")
            part_lba_in_raw = build_data_volume(args.size, data_raw, use_fat32=use_fat32)

        data_file_sectors = os.path.getsize(data_raw) // SECTOR
        data_payload_sectors = data_file_sectors - part_lba_in_raw
        min_payload = 65525 if use_fat32 else 4096 * 8  # exFAT: ~clusters min * SPC
        if data_payload_sectors < min_payload:
            raise SystemExit(f"[ERRO] particao de dados muito pequena para {fs_name}")

        data_start = align_up(esp_end + 1, 2048)
        data_sectors = data_payload_sectors
        data_end = data_start + data_sectors - 1
        total_sectors = data_end + 34
        total_bytes = total_sectors * SECTOR

        print(f"=== Layout unificado -> {out} ===")
        print(f"  ESP : LBA {esp_start}..{esp_end} ({esp_sectors * SECTOR // 1024} KB) FAT")
        print(
            f"  DATA: LBA {data_start}..{data_end} "
            f"({data_sectors * SECTOR // (1024*1024)} MB) type=0x{data_mbr_type:02X} {fs_name}"
        )
        print(f"  Total: {total_bytes // (1024*1024)} MB ({total_sectors} setores)")

        os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
        with open(out, "wb") as f:
            f.truncate(total_bytes)

        disk_guid = uuid.uuid4().bytes
        esp_guid = uuid.uuid4().bytes
        data_guid = uuid.uuid4().bytes

        with open(out, "r+b") as f:
            # Pendrive = removable: Windows ignora GPT se MBR for so 0xEE.
            # Dados na slot0 → Explorer ganha letra; GPT permanece para UEFI.
            write_removable_mbr(
                f,
                esp_start=esp_start,
                esp_sectors=esp_sectors,
                data_start=data_start,
                data_sectors=data_sectors,
                data_mbr_type=data_mbr_type,
            )
            write_gpt(
                f,
                total_sectors=total_sectors,
                disk_guid=disk_guid,
                parts=[
                    (GUID_ESP, esp_start, esp_end, "EFI System", esp_guid),
                    (GUID_BASIC, data_start, data_end, "NEURAL-OS", data_guid),
                ],
            )
            f.seek(esp_start * SECTOR)
            f.write(esp_raw)
            with open(data_raw, "rb") as src:
                src.seek(part_lba_in_raw * SECTOR)
                remaining = data_sectors * SECTOR
                f.seek(data_start * SECTOR)
                while remaining > 0:
                    chunk = src.read(min(8 * 1024 * 1024, remaining))
                    if not chunk:
                        break
                    f.write(chunk)
                    remaining -= len(chunk)
                if remaining != 0:
                    raise SystemExit("[ERRO] copia incompleta da particao de dados")
            if use_fat32:
                patch_fat_bpb(f, data_start, data_sectors)
            else:
                patch_exfat_vbr(f, data_start, data_sectors)

    final = os.path.getsize(out)
    alias = os.path.join(target_dir, "disk_hw_unified.raw")
    if os.path.abspath(out) != os.path.abspath(alias):
        shutil.copy2(out, alias)
        print(f"[OK] alias {alias}")

    print(f"\n[OK] {out}: {final // (1024 * 1024)} MB (ESP FAT + dados {fs_name})")
    print("Rufus: modo Imagem DD -> grave no pendrive (Secure Boot OFF).")
    print("Windows: deve aparecer volume NEURAL-OS (FAT32/exFAT) com BOOT.LOG.")
    print("QEMU dois discos: continue com uefi.img + disk_qemu.raw (nao use este como unico).")


if __name__ == "__main__":
    main()
