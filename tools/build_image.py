#!/usr/bin/env python3
"""Gera imagem FAT32 completa para QEMU e HW real.
Inclui: .bitnet, firmware blobs, CONFIG.TXT
Uso: python tools/build_image.py [--size 128] [--output disk.raw]
"""
import os, sys, subprocess
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def main():
    size = sys.argv[2] if len(sys.argv) > 2 and sys.argv[1] == '--size' else '128'
    out = sys.argv[4] if len(sys.argv) > 4 and sys.argv[3] == '--output' else os.path.join(ROOT, 'tools', 'disk_qemu.raw')

    # Ensure v3 model is available
    src_v3 = os.path.join(ROOT, 'target', 'hw_expert_v3.bitnet')
    dst_v3 = os.path.join(ROOT, 'target', 'hw_expert_tf.bitnet')
    if os.path.exists(src_v3) and not os.path.exists(dst_v3):
        import shutil
        shutil.copy2(src_v3, dst_v3)
        print(f"[OK] hw_expert_v3.bitnet ({os.path.getsize(src_v3)//1024}KB) -> hw_expert_tf.bitnet")

    # Run mkfat32
    env = os.environ.copy()
    if 'hw' in out:
        env['SKIP_2B'] = '1'  # pular BITNET-2B para imagem de HW (nao cabe)
    cmd = [sys.executable, os.path.join(ROOT, 'tools', 'mkfat32.py'),
           '--size', str(size), '--output', out]
    print(f"=== Criando imagem {size}MB -> {out} ===")
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=120)
    print(r.stdout)
    if r.returncode != 0:
        print(f"[ERRO] mkfat32: {r.stderr[:300]}")
        return

    final_size = os.path.getsize(out)
    print(f"\n[OK] {out}: {final_size//1024//1024}MB")
    print(f"Para QEMU: qemu-system-x86_64 -drive file={out},format=raw,if=ide")
    print(f"Para pendrive: dd if={out} of=/dev/sdX bs=4M status=progress")

if __name__ == '__main__':
    main()
