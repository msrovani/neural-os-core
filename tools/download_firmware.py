#!/usr/bin/env python3
"""Baixa firmwares GPU do linux-firmware.git para secure boot.
Uso: python tools/download_firmware.py

Os blobs são MIT license, redistribuíveis.
Usa GitLab mirror (kernel.org gitweb bloqueia scrape, clone lento).

Firmwares necessários por GPU:
  NVIDIA GP108 (GTX 1050): fecs_*.bin + gpccs_*.bin (~40KB)
  Intel Gen9+ (HD 530):      GuC + HuC (~500KB)
"""
import os, subprocess, sys, shutil
from pathlib import Path

TARGET = Path(__file__).parent.parent / "target" / "firmware"
GIT_URL = "https://gitlab.com/kernel-firmware/linux-firmware.git"
GIT_DIR = TARGET / "linux-firmware"

NVIDIA_BLOBS = [
    "nvidia/gp108/fecs_bl.bin", "nvidia/gp108/fecs_data.bin",
    "nvidia/gp108/fecs_inst.bin", "nvidia/gp108/fecs_sig.bin",
    "nvidia/gp108/gpccs_bl.bin", "nvidia/gp108/gpccs_data.bin",
    "nvidia/gp108/gpccs_inst.bin", "nvidia/gp108/gpccs_sig.bin",
]

def main():
    print("=== Download GPU Firmwares ===")
    TARGET.mkdir(parents=True, exist_ok=True)

    git_bin = shutil.which("git")
    if not git_bin:
        print("[ERRO] git nao encontrado. Instale git para Windows.")
        print("  https://git-scm.com/downloads/win")
        sys.exit(1)

    if not GIT_DIR.exists():
        print(f"[GIT] Clonando linux-firmware (shallow, ~50MB)...")
        r = subprocess.run([git_bin, "clone", "--depth", "1", GIT_URL, str(GIT_DIR)],
                           capture_output=True, text=True)
        if r.returncode != 0:
            print(f"[ERRO] git clone falhou: {r.stderr[:200]}")
            sys.exit(1)
        print("[GIT] Clone OK")
    else:
        print(f"[GIT] linux-firmware ja existe em {GIT_DIR}")

    ok = 0
    for blob in NVIDIA_BLOBS:
        src = GIT_DIR / blob
        dst = TARGET / blob
        if src.exists():
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)
            sz = dst.stat().st_size // 1024
            print(f"  [OK] {blob} ({sz}KB)")
            ok += 1
        else:
            print(f"  [--] {blob} NAO ENCONTRADO")

    print(f"\n[OK] {ok}/{len(NVIDIA_BLOBS)} firmwares copiados para {TARGET}")
    print(f"\nPara carregar: implementar WPR loading em gpu/firmware.rs")
    print(f"  Referencia: drivers/gpu/drm/nouveau/nvkm/subdev/acr/ (Linux kernel)")

if __name__ == "__main__":
    main()
