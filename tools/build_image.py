#!/usr/bin/env python3
"""build_image.py — neural-os-core v1.0
Cria imagem de disco FAT32 com modelos .bitnet para boot em HW real.

Uso: python build_image.py [--disk disk.raw] [--size 64M] [--kernel target/.../bootimage-neural-os-core.bin]

O disco gerado contem:
  - RUSTCDR.BITNET  - Expert RustCoder (Trinity MoE)
  - HW_EXPERT.BITNET - Expert hardware identification
  - BGE_SMALL.BITNET - BGE embedding (se disponivel)
  - MICRO.BITNET    - Modelo de teste micro (built-in)
  - CONFIG.TXT      - Configuracao de boot

Em QEMU, use: -drive format=raw,file=disk.raw,if=ide
"""

import argparse
import os
import shutil
import subprocess
import struct
import sys
import tempfile

def find_file(name, search_paths):
    for p in search_paths:
        path = os.path.join(p, name)
        if os.path.exists(path):
            return os.path.abspath(path)
    return None

def build_image(disk_path, size, kernel_path, model_dir):
    # Coleta modelos disponiveis
    search_paths = [model_dir, ".", "target", "crates/neural-kernel"]
    if model_dir:
        search_paths.insert(0, model_dir)

    models = {
        "RUSTCDR.BITNET": find_file("rust_coder.bitnet", search_paths) or find_file("RUSTCDR.BITNET", search_paths),
        "HW_EXPERT.BITNET": find_file("hw_expert.bitnet", search_paths) or find_file("HW_EXPERT.BITNET", search_paths),
        "MICRO.BITNET": find_file("micro.bitnet", search_paths),
        "BGE_SMALL.BITNET": find_file("bge-small.bitnet", search_paths) or find_file("BGE_SMALL.BITNET", search_paths) or find_file("target/bge-small.bitnet", search_paths),
        "YOLO26N.BITNET": find_file("yolo26n.bitnet", search_paths) or find_file("target/yolo26n.bitnet", search_paths),
    }

    # Modelos opcionais para HW real (carregados via FAT32 em vez de QEMU loader)
    llm_models = {
        "BITNET-2B.BITNET": find_file("bitnet-BitNet-b1_58-2B-4T.bitnet", search_paths),
        "BITNET-L.BITNET": find_file("bitnet-bitnet_b1_58-large.bitnet", search_paths),
        "POCKET-TTS.BITNET": find_file("pocket-tts.bitnet", search_paths) or find_file("target/pocket-tts.bitnet", search_paths),
    }

    print(f"[BUILD] Criando {disk_path} ({size})...")

    # Cria imagem vazia
    # Usa qemu-img se disponivel, senao fallback para dd via Python
    qemu_img = shutil.which("qemu-img")
    if qemu_img:
        subprocess.run([qemu_img, "create", "-f", "raw", disk_path, size], check=True)
    else:
        # Cria arquivo vazio do tamanho especificado
        size_bytes = parse_size(size)
        with open(disk_path, "wb") as f:
            f.seek(size_bytes - 1)
            f.write(b"\x00")

    # Formata como FAT32
    # Opcao 1: mkfs.fat (Linux/WSL)
    # Opcao 2: mkdosfs (MSYS2/Git Bash)
    # Opcao 3: Python manual com biblioteca fat32
    mkfs = shutil.which("mkfs.fat") or shutil.which("mkdosfs") or shutil.which("mkfs.vfat")
    if mkfs:
        subprocess.run([mkfs, "-F", "32", disk_path], check=True)
        print(f"[BUILD] Formatado FAT32 via {mkfs}")
    else:
        # Fallback: usar Python mbrfat32
        try:
            from mbrfat32 import MbrFat32
            fs = MbrFat32()
            fs.create(disk_path, size_bytes=parse_size(size))
            print("[BUILD] Formatado FAT32 via mbrfat32 Python")
        except ImportError:
            print("[WARN] Nenhuma ferramenta FAT32 encontrada. Instale mkfs.fat ou: pip install mbrfat32")
            print("[WARN] A imagem sera criada VAZIA. Popule manualmente.")
            return

    # Monta e copia arquivos
    # Precisamos de um ponto de montagem ou ferramenta para copiar
    # Opcao: usar mtool (mtools) se disponivel
    mcopy = shutil.which("mcopy")
    if mcopy:
        for name, src_path in models.items():
            if src_path:
                size_kb = os.path.getsize(src_path) // 1024
                try:
                    subprocess.run(["mcopy", "-i", disk_path, src_path, f"::{name}"], check=True, capture_output=True)
                    print(f"[BUILD] {name} ({size_kb}K) - OK")
                except subprocess.CalledProcessError as e:
                    print(f"[WARN] {name}: falha ao copiar ({e.stderr.decode().strip()})")
            else:
                print(f"[BUILD] {name} - NAO ENCONTRADO (pulei)")

        # Copia tambem LLM models (se disponiveis e couberem)
        for name, src_path in llm_models.items():
            if src_path:
                size_kb = os.path.getsize(src_path) // 1024
                try:
                    subprocess.run(["mcopy", "-i", disk_path, src_path, f"::{name}"], check=True, capture_output=True)
                    print(f"[BUILD] {name} ({size_kb}K) - OK")
                except subprocess.CalledProcessError as e:
                    print(f"[WARN] {name}: sem espaco ou falha ({e.stderr.decode().strip()})")
            else:
                print(f"[BUILD] {name} - NAO ENCONTRADO (pulei)")
    else:
        # Fallback: montagem em Windows (se disponivel)
        # Tenta usar a biblioteca fat32doppio
        try:
            from fat32doppio import FAT32
            fs = FAT32(disk_path)
            for name, src_path in models.items():
                if src_path:
                    with open(src_path, "rb") as f:
                        fs.write_file(name, f.read())
                    print(f"[BUILD] {name} - OK (via fat32doppio)")
        except ImportError:
            print("[WARN] mcopy nao encontrado e sem fallback Python. Instale mtools:")
            print("  Linux:  apt install mtools")
            print("  Windows: pacman -S mtools (MSYS2) ou instale pip install fat32doppio")
            print("[INFO] Copie manualmente os modelos para a FAT32")

    # Cria CONFIG.TXT com info de boot
    print(f"\n[BUILD] Resumo:")
    print(f"  Disco:     {disk_path} ({size})")
    print(f"  Kernel:    {kernel_path or '(usar bootimage)'}")
    found = sum(1 for v in models.values() if v)
    total = len(models)
    print(f"  Modelos:   {found}/{total} encontrados")

    size_bytes = os.path.getsize(disk_path)
    print(f"  Tamanho:   {size_bytes // (1024*1024)} MB")
    print(f"[BUILD] Pronto! Use: .\\run-qemu-whpx.ps1")


def parse_size(s):
    s = s.upper()
    if s.endswith("G"):
        return int(s[:-1]) * 1024 * 1024 * 1024
    elif s.endswith("M"):
        return int(s[:-1]) * 1024 * 1024
    elif s.endswith("K"):
        return int(s[:-1]) * 1024
    else:
        return int(s)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Gera disco FAT32 para neural-os-core")
    parser.add_argument("--disk", default="disk.raw", help="Caminho do disco (default: disk.raw)")
    parser.add_argument("--size", default="64M", help="Tamanho (default: 64M, min: 16M para FAT32)")
    parser.add_argument("--kernel", default=None, help="Caminho do kernel (opcional)")
    parser.add_argument("--model-dir", default="target", help="Diretorio com modelos .bitnet (default: target/)")
    args = parser.parse_args()

    build_image(args.disk, args.size, args.kernel, args.model_dir)
