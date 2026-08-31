#!/usr/bin/env python3
"""AIOS: atualiza so a ESP de usb_hw.img a partir de uefi.img (sem regravar 3GB de dados).

Uso (depois de cargo build --release -p boot):
  python tools/patch_usb_esp.py
"""
from __future__ import annotations

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))
from build_usb_unified import parse_uefi_esp  # noqa: E402
from verify_usb_esp import SECTOR, find_esp  # noqa: E402


def main() -> int:
    uefi = os.path.join(ROOT, "target", "uefi.img")
    usb = os.path.join(ROOT, "target", "usb_hw.img")
    if not os.path.isfile(uefi) or not os.path.isfile(usb):
        print("FAIL: falta target/uefi.img ou target/usb_hw.img")
        return 1
    _start, sectors, raw = parse_uefi_esp(uefi)
    with open(usb, "r+b") as f:
        usb_lba, usb_secs = find_esp(f)
        if sectors > usb_secs:
            print(f"FAIL: ESP nova {sectors} setores > slot USB {usb_secs}")
            return 1
        f.seek(usb_lba * SECTOR)
        f.write(raw)
    print(f"[OK] usb_hw.img ESP <- uefi.img ({len(raw)//1024} KB @ LBA {usb_lba})")
    r = subprocess.run(
        [sys.executable, os.path.join(ROOT, "tools", "verify_usb_esp.py"), usb],
        cwd=ROOT,
    )
    return r.returncode


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    raise SystemExit(main())
