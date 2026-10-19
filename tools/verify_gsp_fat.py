#!/usr/bin/env python3
"""S0 baseline: verifica GSP.BIN/GSPBOOT/GSPLOAD na partição de dados do usb_hw.img.

Uso:
  python tools/verify_gsp_fat.py
  python tools/verify_gsp_fat.py --img target/usb_hw.img

Exit 0 se os 3 nomes da recipe ga107 estão presentes com sizes plausíveis.
Metal slog 3/3 ainda exige boot real (AWAITING até reflash+log).
"""
from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NEED = {
    "GSP.BIN": (20_000_000, 50_000_000),  # ~38MB pin 535
    "GSPBOOT.BIN": (4_000, 100_000),
    "GSPLOAD.BIN": (20_000, 200_000),
}


def gpt_data_lba(img: Path) -> int:
    """LBA start da partição de dados (tipo basic data) — fallback 262144."""
    with img.open("rb") as f:
        f.seek(512)  # primary GPT header @ LBA1
        hdr = f.read(92)
        if hdr[0:8] != b"EFI PART":
            return 262144
        entry_lba = struct.unpack_from("<Q", hdr, 72)[0]
        num = struct.unpack_from("<I", hdr, 80)[0]
        esz = struct.unpack_from("<I", hdr, 84)[0]
        f.seek(entry_lba * 512)
        basic = bytes.fromhex("A2A0D0EBE5B9334487C068B6B72699C7")
        for _ in range(num):
            e = f.read(esz)
            if len(e) < 56:
                break
            if e[0:16] == basic:
                return struct.unpack_from("<Q", e, 32)[0]
    return 262144


def list_fat_root(img: Path, part_lba: int) -> dict[str, int]:
    """Parse FAT32 root dirent names → size (primeiro cluster walk skip)."""
    with img.open("rb") as f:
        f.seek(part_lba * 512)
        boot = f.read(512)
        if boot[510:512] != b"\x55\xaa":
            raise SystemExit(f"no FAT boot signature @ LBA {part_lba}")
        bps = struct.unpack_from("<H", boot, 11)[0] or 512
        spc = boot[13]
        reserved = struct.unpack_from("<H", boot, 14)[0]
        fats = boot[16]
        fatsz = struct.unpack_from("<I", boot, 36)[0]
        root_clus = struct.unpack_from("<I", boot, 44)[0]
        fat_start = part_lba * 512 + reserved * bps
        data_start = fat_start + fats * fatsz * bps
        # Read first root cluster only (enough for short names we need)
        clus = root_clus
        names: dict[str, int] = {}
        for _ in range(64):  # bound
            off = data_start + (clus - 2) * spc * bps
            f.seek(off)
            chunk = f.read(spc * bps)
            for i in range(0, len(chunk), 32):
                d = chunk[i : i + 32]
                if not d or d[0] == 0x00:
                    return names
                if d[0] == 0xE5 or d[11] in (0x0F, 0x08):
                    continue
                name = d[0:8].decode("ascii", "ignore").strip()
                ext = d[8:11].decode("ascii", "ignore").strip()
                full = f"{name}.{ext}" if ext else name
                size = struct.unpack_from("<I", d, 28)[0]
                if full and full not in names:
                    names[full] = size
            # next cluster via FAT
            f.seek(fat_start + clus * 4)
            nxt = struct.unpack("<I", f.read(4))[0] & 0x0FFFFFFF
            if nxt < 2 or nxt >= 0x0FFFFFF8:
                break
            clus = nxt
        return names


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--img", type=Path, default=ROOT / "target" / "usb_hw.img")
    args = ap.parse_args()
    if not args.img.is_file():
        print(f"[FAIL] missing {args.img}")
        return 1
    lba = gpt_data_lba(args.img)
    print(f"[IMG] {args.img} size={args.img.stat().st_size} data_lba={lba}")
    names = list_fat_root(args.img, lba)
    ok = True
    for n, (lo, hi) in NEED.items():
        sz = names.get(n)
        if sz is None:
            print(f"[MISS] {n}")
            ok = False
            continue
        good = lo <= sz <= hi
        print(f"[{'OK' if good else 'SIZE'}] {n} = {sz}B (expect {lo}..{hi})")
        ok = ok and good
    # Optional
    if "GSPUNLD.BIN" in names:
        print(f"[OK] GSPUNLD.BIN = {names['GSPUNLD.BIN']}B (optional)")
    if ok:
        print("[PASS] S0 FAT contract GSP.BIN/GSPBOOT/GSPLOAD — metal slog 3/3 AWAITING reflash+boot")
        return 0
    print("[FAIL] S0 incomplete")
    return 1


if __name__ == "__main__":
    sys.exit(main())
