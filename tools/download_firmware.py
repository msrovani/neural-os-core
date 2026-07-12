#!/usr/bin/env python3
"""Baixa firmwares do linux-firmware.git (NVIDIA GPU, Intel GPU, Realtek NIC/WiFi).
Uso: python tools/download_firmware.py

Firmware:
  NVIDIA GP108 (GTX 1050): fecs_*.bin + gpccs_*.bin (~40KB)
  Intel i915 SKL/KBL:       GuC + HuC + DMC (~3.8MB)
  Realtek NIC:              rtl8168/8125/etc (~200KB)
  Realtek WiFi:             rtlwifi blobs (~1MB)
"""
import os, subprocess, sys, shutil
from pathlib import Path

TARGET = Path(__file__).parent.parent / "target" / "firmware"
GIT_URL = "https://gitlab.com/kernel-firmware/linux-firmware.git"
GIT_DIR = TARGET / "linux-firmware"

FW_BLOBS = {
    "nvidia/gp108": [
        "fecs_bl.bin", "fecs_data.bin", "fecs_inst.bin", "fecs_sig.bin",
        "gpccs_bl.bin", "gpccs_data.bin", "gpccs_inst.bin", "gpccs_sig.bin",
    ],
    "i915": [
        "skl_dmc_ver1_27.bin", "skl_guc_33.0.0.bin", "skl_guc_49.0.1.bin",
        "skl_guc_62.0.0.bin", "skl_guc_69.0.3.bin", "skl_guc_70.1.1.bin",
        "skl_huc_2.0.0.bin", "skl_huc_ver01_07_1398.bin",
        "kbl_dmc_ver1_04.bin", "kbl_guc_33.0.0.bin", "kbl_guc_49.0.1.bin",
        "kbl_guc_62.0.0.bin", "kbl_guc_69.0.3.bin", "kbl_guc_70.1.1.bin",
        "kbl_huc_4.0.0.bin", "kbl_huc_ver02_00_1810.bin",
    ],
    "rtl_nic": [
        "rtl8168d-1.fw", "rtl8168d-2.fw", "rtl8168e-1.fw", "rtl8168e-2.fw",
        "rtl8168e-3.fw", "rtl8168f-1.fw", "rtl8168f-2.fw", "rtl8168fp-3.fw",
        "rtl8168g-1.fw", "rtl8168g-2.fw", "rtl8168g-3.fw", "rtl8168h-1.fw",
        "rtl8168h-2.fw", "rtl8125a-3.fw", "rtl8125b-1.fw", "rtl8125b-2.fw",
        "rtl8125bp-2.fw", "rtl8125cp-1.fw", "rtl8125d-1.fw", "rtl8125d-2.fw",
        "rtl8125k-1.fw", "rtl8126a-2.fw", "rtl8126a-3.fw", "rtl8127a-1.fw",
        "rtl8105e-1.fw", "rtl8106e-1.fw", "rtl8106e-2.fw", "rtl8107e-1.fw",
        "rtl8107e-2.fw", "rtl8153a-2.fw", "rtl8153a-3.fw", "rtl8153a-4.fw",
        "rtl8153b-2.fw", "rtl8153c-1.fw", "rtl8156a-2.fw", "rtl8156b-2.fw",
        "rtl8261c.bin", "rtl8402-1.fw", "rtl8411-1.fw", "rtl8411-2.fw",
        "rtl9151a-1.fw",
    ],
}

def main():
    print("=== Download Firmwares ===")
    TARGET.mkdir(parents=True, exist_ok=True)

    git_bin = shutil.which("git")
    if not git_bin:
        print("[ERRO] git nao encontrado.")
        sys.exit(1)

    if not GIT_DIR.exists():
        print("[GIT] Clonando linux-firmware (shallow)...")
        r = subprocess.run([git_bin, "clone", "--depth", "1", GIT_URL, str(GIT_DIR)],
                           capture_output=True, text=True)
        if r.returncode != 0:
            print(f"[ERRO] git clone falhou: {r.stderr[:200]}")
            sys.exit(1)
    else:
        print(f"[GIT] ja existe em {GIT_DIR}")

    ok, total = 0, 0
    for category, blobs in FW_BLOBS.items():
        total += len(blobs)
        for name in blobs:
            src = GIT_DIR / category / name
            dst = TARGET / category / name
            if src.exists():
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dst)
                print(f"  [OK] {category}/{name}")
                ok += 1
            else:
                print(f"  [--] {category}/{name}")

    print(f"\n[OK] {ok}/{total} firmwares copiados para {TARGET}")

if __name__ == "__main__":
    main()
