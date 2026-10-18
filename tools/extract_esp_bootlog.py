#!/usr/bin/env python3
"""Extrai BOOT.LOG da ESP FAT32 (s493/s494) para leitura na IDE.

Aceita:
  - target/uefi.img / limine-esp.img (disco ESP só)
  - target/usb_hw.img (GPT: part ESP)
  - letra de drive montada (ex.: D:) — lê raiz e NEURAL\\

Preferência (s494): o ficheiro com mais bytes úteis entre raiz e NEURAL\\.
Strip de NULs; marca [TRUNCATED] se o payload enche o CAP.

Uso:
  python tools/extract_esp_bootlog.py target/uefi.img -o logs/BOOT.LOG
  python tools/extract_esp_bootlog.py target/usb_hw.img
  python tools/extract_esp_bootlog.py D:
"""
from __future__ import annotations

import argparse
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BOOT_LOG_CAP = 1024 * 1024


def read_bpb(img, lba: int):
    img.seek(lba * 512)
    return img.read(512)


def _fat_next(img, fat_lba: int, bps: int, cluster: int) -> int:
    fat_off = cluster * 4
    img.seek(fat_lba * 512 + (fat_off // bps) * bps)
    fsec = img.read(bps)
    bo = fat_off % bps
    return struct.unpack_from("<I", fsec, bo)[0] & 0x0FFFFFFF


def _read_chain(img, fat_lba, data_lba, spc, bps, start_cl, size: int) -> bytes:
    out = bytearray()
    c = start_cl
    while c >= 2 and c < 0x0FFFFFF8 and len(out) < size:
        cba = data_lba + (c - 2) * spc
        img.seek(cba * 512)
        chunk = img.read(spc * bps)
        need = size - len(out)
        out.extend(chunk[:need])
        c = _fat_next(img, fat_lba, bps, c)
    return bytes(out)


def find_boot_logs_in_fat(img, part_lba: int) -> list[tuple[str, bytes]]:
    """Devolve [(path, raw_bytes), ...] para BOOT.LOG na raiz e em NEURAL/."""
    bpb = read_bpb(img, part_lba)
    if len(bpb) < 512 or bpb[0:3] == b"\x00\x00\x00":
        return []
    if bpb[3:11] == b"EXFAT   ":
        return []
    bps = struct.unpack_from("<H", bpb, 0x0B)[0]
    spc = bpb[0x0D]
    reserved = struct.unpack_from("<H", bpb, 0x0E)[0]
    fats = bpb[0x10]
    root_ent = struct.unpack_from("<H", bpb, 0x11)[0]
    if root_ent != 0 or bps < 512 or spc == 0:
        return []
    spf = struct.unpack_from("<I", bpb, 0x24)[0]
    root_cl = struct.unpack_from("<I", bpb, 0x2C)[0]
    fat_lba = part_lba + reserved
    data_lba = fat_lba + fats * spf
    found: list[tuple[str, bytes]] = []

    def walk_dir(cluster: int, prefix: str, depth: int = 0) -> None:
        if depth > 4:
            return
        walked = 0
        while 2 <= cluster < 0x0FFFFFF8 and walked < 64:
            walked += 1
            clba = data_lba + (cluster - 2) * spc
            img.seek(clba * 512)
            dirbuf = img.read(spc * bps)
            for off in range(0, len(dirbuf), 32):
                e = dirbuf[off : off + 32]
                if not e or e[0] == 0:
                    return
                if e[0] == 0xE5 or (e[11] & 0x0F) == 0x0F:
                    continue
                name83 = e[0:11]
                attr = e[11]
                size = struct.unpack_from("<I", e, 28)[0]
                fc = struct.unpack_from("<H", e, 26)[0] | (
                    struct.unpack_from("<H", e, 20)[0] << 16
                )
                if attr & 0x10:
                    # subdir NEURAL
                    label = name83[0:8].decode("ascii", "replace").strip()
                    if label.upper() == "NEURAL" and fc >= 2:
                        walk_dir(fc, prefix + "NEURAL/", depth + 1)
                    continue
                if name83 == b"BOOT    LOG":
                    path = prefix + "BOOT.LOG"
                    raw = _read_chain(img, fat_lba, data_lba, spc, bps, fc, size)
                    found.append((path, raw))
            cluster = _fat_next(img, fat_lba, bps, cluster)

    walk_dir(root_cl, "")
    return found


def gpt_esp_lba(img) -> int | None:
    img.seek(512)
    hdr = img.read(92)
    if hdr[0:8] != b"EFI PART":
        return None
    ent_lba = struct.unpack_from("<Q", hdr, 72)[0]
    num = struct.unpack_from("<I", hdr, 80)[0]
    esz = struct.unpack_from("<I", hdr, 84)[0]
    img.seek(ent_lba * 512)
    esp_guid = bytes.fromhex("28732ac11ff8d211ba4b00a0c93ec93b")
    for _ in range(min(num, 128)):
        e = img.read(esz)
        if len(e) < 56:
            break
        if e[0:16] == esp_guid:
            return struct.unpack_from("<Q", e, 32)[0]
    return None


def usable_len(raw: bytes) -> int:
    """Bytes úteis (sem cauda de NULs)."""
    end = len(raw)
    while end > 0 and raw[end - 1] == 0:
        end -= 1
    return end


def clean_text(raw: bytes) -> str:
    # Remove NULs (setores parcial / pad) e BOM.
    text = raw.replace(b"\x00", b"").decode("utf-8", errors="replace")
    if text.startswith("\ufeff"):
        text = text[1:]
    used = usable_len(raw)
    if used >= BOOT_LOG_CAP - 64 or b"[TRUNCATED" in raw:
        if "[TRUNCATED" not in text:
            text = text.rstrip() + "\n[TRUNCATED at BOOT_LOG_CAP]\n"
    return text


def pick_best(candidates: list[tuple[str, bytes]]) -> tuple[str, bytes] | None:
    if not candidates:
        return None
    return max(candidates, key=lambda x: usable_len(x[1]))


def from_drive_letter(letter: str) -> list[tuple[str, bytes]]:
    root = letter.rstrip("\\/") + "\\"
    out: list[tuple[str, bytes]] = []
    for rel in ("BOOT.LOG", os.path.join("NEURAL", "BOOT.LOG")):
        p = os.path.join(root, rel)
        if os.path.isfile(p):
            with open(p, "rb") as f:
                out.append((rel.replace("\\", "/"), f.read()))
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "image",
        nargs="?",
        default=os.path.join(ROOT, "target", "uefi.img"),
        help="imagem (uefi/usb_hw) ou letra de drive (D:)",
    )
    ap.add_argument("-o", "--output", default=None)
    args = ap.parse_args()
    src = args.image
    candidates: list[tuple[str, bytes]] = []

    if len(src) == 2 and src[1] == ":":
        candidates = from_drive_letter(src)
    else:
        path = src if os.path.isabs(src) else os.path.join(ROOT, src)
        if not os.path.isfile(path):
            print(f"missing {path}", file=sys.stderr)
            return 1
        with open(path, "rb") as img:
            for lba in (0, 2048):
                candidates.extend(find_boot_logs_in_fat(img, lba))
            if not candidates:
                esp = gpt_esp_lba(img)
                if esp is not None:
                    candidates.extend(find_boot_logs_in_fat(img, int(esp)))

    best = pick_best(candidates)
    if best is None:
        print("BOOT.LOG not found in ESP/drive", file=sys.stderr)
        return 2
    name, data = best
    text = clean_text(data)
    out = args.output or os.path.join(ROOT, "logs", "BOOT.LOG")
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    lines = [ln for ln in text.splitlines() if ln.strip()]
    print(
        f"[OK] {out}: source={name} raw={len(data)} used={usable_len(data)} "
        f"lines={len(lines)} (candidates={len(candidates)})"
    )
    for ln in lines[:5]:
        print(" ", ln[:120])
    if lines:
        print(" ...")
        for ln in lines[-3:]:
            print(" ", ln[:120])
    return 0


if __name__ == "__main__":
    sys.exit(main())
