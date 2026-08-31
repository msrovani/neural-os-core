#!/usr/bin/env python3
"""Gate AIOS: usb_hw.img ESP deve ter kernel.elf identico ao ESP tree.

Limine-only no metal = kernel ausente/stale na ESP (SESSION_295).
Observe o artefato, nao o log do cargo.

Uso:
  python tools/verify_usb_esp.py [target/usb_hw.img]
Exit 0 OK / 1 FAIL
"""
from __future__ import annotations

import hashlib
import os
import struct
import sys
import uuid

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ESP_GUID = uuid.UUID("C12A7328-F81F-11D2-BA4B-00A0C93EC93B").bytes_le
SECTOR = 512


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def find_esp(f) -> tuple[int, int]:
    f.seek(512)
    if f.read(8) != b"EFI PART":
        raise SystemExit("FAIL: GPT ausente")
    f.seek(2 * 512)
    for _ in range(4):
        e = f.read(128)
        ptype = e[0:16]
        s, en = struct.unpack_from("<QQ", e, 32)
        name = e[56:88].decode("utf-16-le", "replace").split("\x00")[0]
        if ptype == ESP_GUID or name.upper().startswith("EFI"):
            return int(s), int(en - s + 1)
    raise SystemExit("FAIL: particao ESP ausente")


def read_vol(f, start: int, nsec: int) -> bytes:
    f.seek(start * SECTOR)
    return f.read(nsec * SECTOR)


def fat_ctx(vol: bytes):
    bps = struct.unpack_from("<H", vol, 11)[0] or 512
    spc = vol[13]
    reserved = struct.unpack_from("<H", vol, 14)[0]
    fats = vol[16]
    spf = struct.unpack_from("<I", vol, 36)[0]
    root = struct.unpack_from("<I", vol, 44)[0]
    fat_off = reserved * bps
    data_off = (reserved + fats * spf) * bps
    return bps, spc, fat_off, data_off, root


def fat_next(vol: bytes, fat_off: int, cl: int) -> int:
    o = fat_off + cl * 4
    if o + 4 > len(vol):
        return 0x0FFFFFFF
    return struct.unpack_from("<I", vol, o)[0] & 0x0FFFFFFF


def walk_dir(vol: bytes, start_cl: int):
    bps, spc, fat_off, data_off, _ = fat_ctx(vol)
    lfn: list[str] = []
    cl = start_cl
    walked = 0
    while 2 <= cl < 0x0FFFFFF8 and walked < 128:
        walked += 1
        off = data_off + (cl - 2) * spc * bps
        raw = vol[off : off + spc * bps]
        for i in range(0, len(raw), 32):
            ent = raw[i : i + 32]
            if len(ent) < 32 or ent[0] == 0:
                return
            if ent[0] == 0xE5:
                lfn = []
                continue
            if ent[11] == 0x0F:
                chunk = ent[1:11] + ent[14:26] + ent[28:32]
                chars = []
                for j in range(0, 26, 2):
                    w = struct.unpack_from("<H", chunk, j)[0]
                    if w in (0, 0xFFFF):
                        break
                    chars.append(chr(w))
                lfn.insert(0, "".join(chars))
                continue
            long_name = "".join(lfn).rstrip("\x00")
            lfn = []
            size = struct.unpack_from("<I", ent, 28)[0]
            lo = struct.unpack_from("<H", ent, 26)[0]
            hi = struct.unpack_from("<H", ent, 20)[0]
            start = (hi << 16) | lo
            is_dir = bool(ent[11] & 0x10)
            name = long_name or ent[0:11].decode("ascii", "replace").strip()
            yield name, start, size, is_dir
        cl = fat_next(vol, fat_off, cl)


def read_chain(vol: bytes, start_cl: int, size: int) -> bytes:
    bps, spc, fat_off, data_off, _ = fat_ctx(vol)
    out = bytearray()
    c = start_cl
    while 2 <= c < 0x0FFFFFF8 and len(out) < size:
        off = data_off + (c - 2) * spc * bps
        chunk = vol[off : off + spc * bps]
        need = size - len(out)
        out.extend(chunk[:need])
        c = fat_next(vol, fat_off, c)
    return bytes(out)


def find_file(vol: bytes, want: str) -> bytes | None:
    want_u = want.upper()
    # root
    subdirs = []
    for name, cl, size, is_dir in walk_dir(vol, fat_ctx(vol)[4]):
        if is_dir and name not in (".", ".."):
            subdirs.append((name, cl))
            continue
        if name.upper() == want_u:
            return read_chain(vol, cl, size)
    # EFI/BOOT (BOOTX64.EFI)
    for dname, dcl in subdirs:
        if dname.upper() != "EFI":
            continue
        for n2, cl2, sz2, is2 in walk_dir(vol, dcl):
            if is2 and n2.upper() == "BOOT":
                for n3, cl3, sz3, is3 in walk_dir(vol, cl2):
                    if not is3 and n3.upper() == want_u:
                        return read_chain(vol, cl3, sz3)
            if not is2 and n2.upper() == want_u:
                return read_chain(vol, cl2, sz2)
    return None


def main() -> int:
    img_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "usb_hw.img")
    if not os.path.isfile(img_path):
        print(f"FAIL: imagem ausente {img_path}")
        return 1
    with open(img_path, "rb") as f:
        start, nsec = find_esp(f)
        vol = read_vol(f, start, nsec)
    print(f"ESP LBA {start} sectors={nsec}")
    root_names = [n for n, *_ in walk_dir(vol, fat_ctx(vol)[4])]
    print("ESP root:", root_names)
    kernel = find_file(vol, "kernel.elf")
    efi = find_file(vol, "BOOTX64.EFI")
    if kernel is None:
        print("FAIL: kernel.elf ausente na ESP — metal fica so no splash Limine")
        return 1
    if kernel[:4] != b"\x7fELF":
        print("FAIL: kernel.elf nao e ELF")
        return 1
    ksha = sha256(kernel)
    print(f"kernel.elf {len(kernel)} sha256={ksha[:16]}")
    if efi is None:
        print("FAIL: BOOTX64.EFI ausente")
        return 1
    print(f"BOOTX64.EFI {len(efi)}")
    tree = os.path.join(ROOT, "target", "limine-esp-tree", "kernel.elf")
    if os.path.isfile(tree):
        tsha = sha256(open(tree, "rb").read())
        print(f"tree kernel sha256={tsha[:16]} match={tsha == ksha}")
        if tsha != ksha:
            print("FAIL: ESP kernel != limine-esp-tree (ESP stale)")
            return 1
    print("OK: ESP kernel.elf + BOOTX64.EFI")
    return 0


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    raise SystemExit(main())
