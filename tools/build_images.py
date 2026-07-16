#!/usr/bin/env python3
"""build_images.py — neural-os-core v1.0
Gera duas imagens: QEMU (otimizada VirtIO) e HW Real (fisica).
Uso: python build_images.py
"""

import os
import struct
import shutil
import subprocess
import sys

WORKSPACE = os.path.dirname(os.path.abspath(__file__))

def create_mbr_fat32(path, size_mb, label):
    """Cria imagem com MBR + particao FAT32 usando Python puro."""
    size = size_mb * 1024 * 1024
    bytes_per_sector = 512
    sectors_per_track = 63
    heads = 255
    sectors = size // bytes_per_sector

    with open(path, "wb") as f:
        f.seek(size - 1)
        f.write(b"\x00")

    # MBR (setor 0)
    mbr = bytearray(512)
    mbr[0x1BE] = 0x00     # status: bootable
    mbr[0x1BF] = 0x01     # CHS start head
    mbr[0x1C0] = 0x01     # CHS start sector
    mbr[0x1C1] = 0x00     # CHS start cylinder
    mbr[0x1C2] = 0x0C     # type: FAT32 LBA
    mbr[0x1C3] = 0xFE     # CHS end head
    mbr[0x1C4] = 0xFF     # CHS end sector
    mbr[0x1C5] = 0xFF     # CHS end cylinder
    mbr[0x1C6] = struct.pack("<I", 2048)[0]  # LBA start
    mbr[0x1CA] = struct.pack("<I", (sectors - 2048) // 2)[0]  # sectors in partition
    mbr[0x1FE] = 0x55
    mbr[0x1FF] = 0xAA
    with open(path, "r+b") as f:
        f.write(mbr)

    print(f"[BUILD] {path}: {size_mb}MB FAT32 com MBR, label='{label}'")

def copy_files(disk_path, files):
    """Tenta copiar arquivos via mcopy ou fallback."""
    mcopy = shutil.which("mcopy")
    copied = 0
    for name, src in files.items():
        if src and os.path.exists(src):
            size_kb = os.path.getsize(src) // 1024
            if mcopy:
                try:
                    subprocess.run(["mcopy", "-i", disk_path, src, f"::{name}"],
                                   check=True, capture_output=True)
                    print(f"  [OK] {name} ({size_kb}K)")
                    copied += 1
                except:
                    print(f"  [--] {name} sem espaco ou erro")
            else:
                print(f"  [--] {name} ({size_kb}K) — sem mcopy, copie manualmente")
        else:
            print(f"  [--] {name} — nao encontrado")
    return copied

def find_file(name):
    for d in [WORKSPACE, os.path.join(WORKSPACE, "target"),
              os.path.join(WORKSPACE, "crates/neural-kernel")]:
        p = os.path.join(d, name)
        if os.path.exists(p):
            return p
    return None

def main():
    # 1. QEMU image (VirtIO otimizada, 64MB)
    print("=" * 60)
    print("  DISCO 1: QEMU (VirtIO + WHPX)")
    print("=" * 60)
    qemu_disk = os.path.join(WORKSPACE, "disk_qemu.raw")
    if os.path.exists(qemu_disk):
        os.remove(qemu_disk)
    create_mbr_fat32(qemu_disk, 64, "NEURAL-QEMU")
    qemu_files = {
        "BITNET2B.BIN": find_file("bitnet_2B.bitnet"),
        "RUSTCDR.BITNET": find_file("rust_coder.bitnet") or find_file("RUSTCDR.BITNET"),
        "HWEXPRT.BIN": find_file("hw_expert_v3.bitnet") or find_file("hw_expert_tf.bitnet") or find_file("hw_expert.bitnet"),
        "HW_EXPERT.BITNET": find_file("hw_expert_v3.bitnet") or find_file("hw_expert_tf.bitnet") or find_file("hw_expert.bitnet"),
        "BGE.BIN": find_file("bge-small.bitnet") or find_file("target/bge-small.bitnet"),
        "STT.BIN": find_file("STT.BIN"),
        "CONFIG.TXT": os.path.join(WORKSPACE, "qemu_config.txt"),
    }
    # Cria CONFIG.TXT
    with open(qemu_files["CONFIG.TXT"], "w") as f:
        f.write("BOOT_MODE=qemu\nPLATFORM=virtio-whpx\n")
    copy_files(qemu_disk, qemu_files)
    sz = os.path.getsize(qemu_disk)
    print(f"\n[FEITO] disk_qemu.raw: {sz // (1024*1024)}MB")
    print(f"        .\\run-qemu-whpx.ps1 -disk disk_qemu.raw\n")

    # 2. HW Real image (fisica, 256MB para LLM models)
    print("=" * 60)
    print("  DISCO 2: HW REAL (i5-6400, GTX 1050)")
    print("=" * 60)
    hw_disk = os.path.join(WORKSPACE, "disk_hw.raw")
    if os.path.exists(hw_disk):
        os.remove(hw_disk)
    create_mbr_fat32(hw_disk, 256, "NEURAL-HW")
    hw_files = {
        "BITNET2B.BIN": find_file("bitnet_2B.bitnet"),
        "RUSTCDR.BITNET": find_file("rust_coder.bitnet"),
        "HWEXPRT.BIN": find_file("hw_expert_v3.bitnet") or find_file("hw_expert_tf.bitnet") or find_file("hw_expert.bitnet"),
        "HW_EXPERT.BITNET": find_file("hw_expert_v3.bitnet") or find_file("hw_expert_tf.bitnet") or find_file("hw_expert.bitnet"),
        "BGE.BIN": find_file("bge-small.bitnet"),
        "STT.BIN": find_file("STT.BIN"),
        "POCKETTTS.BITNET": find_file("target/pocket-tts.bitnet"),
        "CONFIG.TXT": os.path.join(WORKSPACE, "hw_config.txt"),
    }
    with open(hw_files["CONFIG.TXT"], "w") as f:
        f.write("BOOT_MODE=hw\nPLATFORM=baremetal-i5-6400\nGPU=GTX1050\n")
    copy_files(hw_disk, hw_files)
    sz = os.path.getsize(hw_disk)
    print(f"\n[FEITO] disk_hw.raw: {sz // (1024*1024)}MB")
    print(f"        Use: qemu -drive format=raw,file=disk_hw.raw,if=ide")
    print(f"        Ou copie para pendrive: dd if=disk_hw.raw of=/dev/sdb")

    print("\n" + "=" * 60)
    print("  RESUMO")
    print("=" * 60)
    print(f"  Kernel: target/x86_64-neural_os/release/bootimage-neural-os-core.bin")
    print(f"  QEMU:   disk_qemu.raw (64MB)")
    print(f"  HW:     disk_hw.raw (256MB)")
    print(f"  Serial bridge: python serial_bridge.py")
    print()

if __name__ == "__main__":
    main()
