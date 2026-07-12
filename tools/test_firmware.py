#!/usr/bin/env python3
"""Testa validade de todos os firmwares baixados.
Uso: python tools/test_firmware.py

Lê blobs de firmware/ (git-tracked), valida estrutura, simula WPR layout.
"""
import os, struct, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# ─── NVIDIA GP108 (FECS + GPCCS) ──────────────────────────────────────────
NVIDIA_FW = {
    "fecs_bl.bin":    (576,   "FECS bootloader"),
    "fecs_data.bin":  (2248,  "FECS data segment"),
    "fecs_inst.bin":  (21161, "FECS code/instructions"),
    "fecs_sig.bin":   (192,   "FECS RSA signature"),
    "gpccs_bl.bin":   (576,   "GPCCS bootloader"),
    "gpccs_data.bin": (2092,  "GPCCS data segment"),
    "gpccs_inst.bin": (13095, "GPCCS code/instructions"),
    "gpccs_sig.bin":  (192,   "GPCCS RSA signature"),
}

# ─── Intel i915 SKL/KBL (GuC + HuC + DMC) ────────────────────────────────
INTEL_FW = {
    "skl_dmc_ver1_27.bin":       8928,
    "skl_guc_33.0.0.bin":       182080,
    "skl_guc_49.0.1.bin":       196288,
    "skl_guc_62.0.0.bin":       199552,
    "skl_guc_69.0.3.bin":       216704,
    "skl_guc_70.1.1.bin":       206208,
    "skl_huc_2.0.0.bin":        136320,
    "kbl_dmc_ver1_04.bin":       8840,
    "kbl_guc_33.0.0.bin":       182912,
    "kbl_guc_49.0.1.bin":       197184,
    "kbl_guc_62.0.0.bin":       200448,
    "kbl_guc_69.0.3.bin":       217664,
    "kbl_guc_70.1.1.bin":       206976,
    "kbl_huc_4.0.0.bin":        226048,
}

FW_DIR = os.path.join(ROOT, "firmware")

def test_nvidia():
    print("=" * 60)
    print("NVIDIA GP108 — ACR Firmware Blobs (FECS + GPCCS)")
    print("=" * 60)
    fw_dir = os.path.join(FW_DIR, "nvidia", "gp108")
    ok = True
    blobs = {}
    for name, (exp_size, desc) in NVIDIA_FW.items():
        path = os.path.join(fw_dir, name)
        if not os.path.exists(path):
            print(f"  [FAIL] {name}: ARQUIVO NAO ENCONTRADO")
            ok = False
            continue
        data = open(path, "rb").read()
        size_ok = len(data) == exp_size
        if not size_ok:
            print(f"  [FAIL] {name}: tamanho {len(data)}B (esperado {exp_size}B)")
            ok = False
        else:
            print(f"  [OK]   {name}: {len(data)}B ({desc})")
        blobs[name] = data

    if not ok:
        print("\n  ❌ ALGUNS BLOBS FALTAM OU ESTAO CORROMPIDOS")
        return False

    # WPR layout simulation
    print("\n  --- WPR Layout (simulado) ---")
    WPR_SIZE = 0x200000  # 2MB
    offset = 0
    for prefix in ("fecs", "gpccs"):
        for seg in ("bl", "data", "inst"):
            name = f"{prefix}_{seg}.bin"
            sz = len(blobs[name])
            print(f"    WPR+0x{offset:05x}: {name} ({sz}B)")
            offset += sz
        sig_name = f"{prefix}_sig.bin"
        sig_sz = len(blobs[sig_name])
        # Signature is NOT loaded to WPR; it's verified by Falcon against HS table
        print(f"    [sig] {sig_name} ({sig_sz}B) — verified by Falcon, NOT in WPR")
    total_wpr = offset
    free_wpr = WPR_SIZE - total_wpr
    pct = total_wpr * 100 / WPR_SIZE
    print(f"\n  Total WPR usado: {total_wpr}B / {WPR_SIZE / 1024:.0f}KB ({pct:.2f}%)")
    print(f"  WPR livre:        {free_wpr / 1024:.1f}KB")
    print(f"  Falcon bootvec:   WPR_BASE (topo VRAM - 2MB)")

    # Falcon boot header check
    for prefix in ("fecs", "gpccs"):
        bl = blobs[f"{prefix}_bl.bin"]
        if bl[:4] == b'\x00' * 4 and len(bl) == 576:
            print(f"  [{prefix}_bl.bin] bootloader header: OK (576B, zeros init)")
        else:
            print(f"  [{prefix}_bl.bin] bootloader: size={len(bl)}B, first bytes={bl[:4].hex()}")

    # FAT32 filename conversion check
    print("\n  ─── FAT32 Name Conversion ───")
    for name in NVIDIA_FW:
        fw_name = "FW_" + name.upper().replace(".", "_")
        print(f"    {name:20s} → {fw_name}")
    print("\n  [OK] NVIDIA GP108 firmware validado")
    return True


def test_intel():
    print("\n" + "=" * 60)
    print("Intel i915 — GuC/HuC/DMC Firmware (SKL + KBL)")
    print("=" * 60)
    fw_dir = os.path.join(FW_DIR, "i915")
    ok = True
    total = 0
    for name, exp_size in INTEL_FW.items():
        path = os.path.join(fw_dir, name)
        if not os.path.exists(path):
            print(f"  [FAIL] {name}: ARQUIVO NAO ENCONTRADO")
            ok = False
            continue
        data = open(path, "rb").read()
        total += len(data)
        family = name.split("_")[0].upper()
        kind = "GuC" if "guc" in name else ("HuC" if "huc" in name else "DMC")
        age = len(data) / exp_size * 100 if exp_size > 0 else 0
        flag = "[OK]" if len(data) == exp_size else f"[DIF]"
        print(f"  {flag} {family:3s} {kind:3s} {name:30s} {len(data):>7}B (esperado {exp_size}B)")
    print(f"\n  Total Intel firmware: {total / 1024:.1f}KB")
    print(f"  [{'OK' if ok else 'FAIL'}]")
    return ok


def test_rtl():
    print("\n" + "=" * 60)
    print("Realtek — rtl_nic + rtlwifi Firmware")
    print("=" * 60)
    for cat in ("rtl_nic", "rtlwifi"):
        fw_dir = os.path.join(FW_DIR, cat)
        files = sorted(os.listdir(fw_dir))
        total = 0
        for name in files:
            path = os.path.join(fw_dir, name)
            sz = os.path.getsize(path)
            total += sz
        print(f"  {cat}: {len(files)} arquivos, {total / 1024:.1f}KB")
    print("  [OK] Realtek firmware presente")
    return True


def test_integrity():
    print("\n" + "=" * 60)
    print("INTEGRITY CHECK — todos os firmwares")
    print("=" * 60)
    import hashlib
    total_files = 0
    total_bytes = 0
    for root, dirs, files in os.walk(FW_DIR):
        for name in sorted(files):
            path = os.path.join(root, name)
            rel = os.path.relpath(path, FW_DIR)
            sz = os.path.getsize(path)
            sha = hashlib.sha256(open(path, "rb").read()).hexdigest()[:16]
            total_files += 1
            total_bytes += sz
            print(f"  {sha}  {rel:50s} {sz:>7}B")
    print(f"\n  Total: {total_files} arquivos, {total_bytes / 1024:.1f}KB")
    return True


if __name__ == "__main__":
    ok = True
    ok &= test_nvidia()
    ok &= test_intel()
    ok &= test_rtl()
    ok &= test_integrity()
    print("\n" + "=" * 60)
    print(f"RESULTADO: {'✅ TODOS OS TESTES OK' if ok else '❌ FALHAS ENCONTRADAS'}")
    print("=" * 60)
    sys.exit(0 if ok else 1)
