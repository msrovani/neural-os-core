#!/usr/bin/env python3
"""
MicroPython WASM Build Script — Sprint 106-6
Compila MicroPython para WebAssembly (WASM) usando Emscripten.
Objetivo: Sandbox dentro de sandbox para execução Python no hermes.
SEM FALLBACK STUB - compilação real obrigatória.
"""

import os
import subprocess
import sys
import shutil
from pathlib import Path

# Configurações
MICROPYTHON_REPO = "https://github.com/micropython/micropython.git"
MICROPYTHON_VERSION = "v1.23.0"
BUILD_DIR = Path("tools/target/micropython")
WASM_OUTPUT = Path("tools/target/micropython/micropython.wasm")
EMSCRIPTEN_PATH = os.environ.get("EMSCRIPTEN_PATH", "/opt/emsdk")

def check_emscripten():
    """Verifica se Emscripten está instalado e configurado"""
    emcc = shutil.which("emcc")
    if not emcc:
        print("[MicroPython] ERRO: emcc não encontrado no PATH")
        print("[MicroPython] Instale Emscripten:")
        print("  1. Baixe: https://emscripten.org/docs/getting_started/downloads.html")
        print("  2. Extraia e execute: ./emsdk install latest")
        print("  3. Ative: ./emsdk activate latest")
        print("  4. Source: source ./emsdk_env.sh")
        print("  5. Ou export EMSCRIPTEN_PATH=/caminho/para/emsdk")
        return False
    
    print(f"[MicroPython] Emscripten encontrado: {emcc}")
    return True

def clone_micropython():
    """Clona repositório MicroPython se não existir"""
    if BUILD_DIR.exists():
        print(f"[MicroPython] Diretório já existe: {BUILD_DIR}")
        return
    
    print(f"[MicroPython] Clonando repositório {MICROPYTHON_REPO} (branch {MICROPYTHON_VERSION})...")
    BUILD_DIR.parent.mkdir(parents=True, exist_ok=True)
    
    try:
        subprocess.run([
            "git", "clone", "--depth", "1", "--branch", MICROPYTHON_VERSION,
            MICROPYTHON_REPO, str(BUILD_DIR)
        ], check=True, capture_output=True, text=True)
        print(f"[MicroPython] Clonado com sucesso em {BUILD_DIR}")
    except subprocess.CalledProcessError as e:
        print(f"[MicroPython] ERRO ao clonar: {e}")
        print(f"[MicroPython] stderr: {e.stderr}")
        sys.exit(1)

def build_micropython_wasm():
    """Compila MicroPython para WASM usando Emscripten"""
    print("[MicroPython] Iniciando compilação WASM com Emscripten...")
    
    # MicroPython já tem suporte WASM via port emscripten
    mp_emscripten_dir = BUILD_DIR / "ports" / "emscripten"
    
    if not mp_emscripten_dir.exists():
        print(f"[MicroPython] ERRO: Diretório emscripten não encontrado: {mp_emscripten_dir}")
        print("[MicroPython] MicroPython pode não ter suporte WASM nesta versão")
        sys.exit(1)
    
    os.chdir(mp_emscripten_dir)
    
    try:
        # Limpa build anterior
        subprocess.run(["make", "clean"], check=True, capture_output=True)
        
        # Compila para WASM
        print("[MicroPython] Executando 'make'...")
        result = subprocess.run(
            ["make"],
            check=True,
            capture_output=True,
            text=True,
            timeout=600  # 10 minutos timeout
        )
        
        print("[MicroPython] Build output:")
        print(result.stdout)
        
        # Verifica se o arquivo WASM foi gerado
        wasm_files = list(mp_emscripten_dir.glob("*.wasm"))
        if not wasm_files:
            print("[MicroPython] ERRO: Nenhum arquivo .wasm gerado")
            sys.exit(1)
        
        # Copia para output
        source_wasm = wasm_files[0]
        WASM_OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(source_wasm, WASM_OUTPUT)
        
        print(f"[MicroPython] Compilação concluída: {WASM_OUTPUT}")
        return True
        
    except subprocess.TimeoutExpired:
        print("[MicroPython] ERRO: Timeout na compilação (10 minutos)")
        sys.exit(1)
    except subprocess.CalledProcessError as e:
        print(f"[MicroPython] ERRO na compilação: {e}")
        print(f"[MicroPython] stderr: {e.stderr}")
        sys.exit(1)
    finally:
        os.chdir("../../../..")

def verify_wasm():
    """Verifica se o arquivo WASM é válido"""
    if not WASM_OUTPUT.exists():
        print(f"[MicroPython] ERRO: Arquivo WASM não encontrado: {WASM_OUTPUT}")
        sys.exit(1)
    
    with open(WASM_OUTPUT, "rb") as f:
        magic = f.read(4)
        if magic != b'\0asm':
            print(f"[MicroPython] ERRO: Magic number inválido: {magic}")
            sys.exit(1)
    
    size = WASM_OUTPUT.stat().st_size
    print(f"[MicroPython] WASM válido: {WASM_OUTPUT} ({size} bytes)")
    return True

def main():
    print("=" * 60)
    print("MicroPython WASM Build — Sprint 106-6")
    print("SEM FALLBACK STUB - compilação real obrigatória")
    print("=" * 60)
    
    # Verifica Emscripten
    if not check_emscripten():
        sys.exit(1)
    
    # Clona MicroPython
    clone_micropython()
    
    # Compila
    build_micropython_wasm()
    
    # Verifica
    verify_wasm()
    
    print("[MicroPython] Build concluído com sucesso!")
    return 0

if __name__ == "__main__":
    sys.exit(main())
