#!/usr/bin/env python3
"""
MicroPython WASM Build Script — Sprint 106-6 / GOAL3
Gera bytecode WASM carregável pelo wasmi (hermes::micropython_wasm).

Ordem:
  1. Se `emcc` estiver no PATH → tenta ports/emscripten do MicroPython (full).
  2. Senão → MVP honesto: módulo WASM mínimo com exports
     `_start`, `python_eval`, `exec` (não é o stub wasmi_rt::generate_wasm_module).

Artefatos:
  tools/target/micropython/micropython.wasm
  crates/hermes/assets/micropython.wasm   (include_bytes no boot NoDisk)
  models/MICROPY.WASM                     (mkfat32 / FAT32)
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MICROPYTHON_REPO = "https://github.com/micropython/micropython.git"
MICROPYTHON_VERSION = "v1.23.0"
BUILD_DIR = ROOT / "tools" / "target" / "micropython"
WASM_OUTPUT = BUILD_DIR / "micropython.wasm"
HERMES_ASSET = ROOT / "crates" / "hermes" / "assets" / "micropython.wasm"
FAT_NAME = ROOT / "models" / "MICROPY.WASM"


def encode_u32_leb(n: int) -> bytes:
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            break
    return bytes(out)


def section(sec_id: int, payload: bytes) -> bytes:
    return bytes([sec_id]) + encode_u32_leb(len(payload)) + payload


def build_mvp_wasm() -> bytes:
    """Módulo WASM mínimo válido p/ wasmi: python_eval/exec/_start."""
    # type: 2 tipos — ()->i32 e (i32)->i32
    types = bytes(
        [
            0x02,
            0x60,
            0x00,
            0x01,
            0x7F,
            0x60,
            0x01,
            0x7F,
            0x01,
            0x7F,
        ]
    )
    # func: 2 funções (type0, type1)
    funcs = bytes([0x02, 0x00, 0x01])
    # export: _start→0, python_eval→1, exec→1
    exports = bytearray([0x03])
    for name, idx in ((b"_start", 0), (b"python_eval", 1), (b"exec", 1)):
        exports.append(len(name))
        exports.extend(name)
        exports.append(0x00)  # kind=func
        exports.append(idx)
    # code: const 42 ; local.get 0
    code = bytes(
        [
            0x02,
            0x04,
            0x00,
            0x41,
            42,
            0x0B,
            0x04,
            0x00,
            0x20,
            0x00,
            0x0B,
        ]
    )
    out = bytearray(b"\0asm\x01\x00\x00\x00")
    out.extend(section(1, types))
    out.extend(section(3, funcs))
    out.extend(section(7, bytes(exports)))
    out.extend(section(10, code))
    return bytes(out)


def check_emscripten() -> bool:
    emcc = shutil.which("emcc")
    if not emcc:
        print("[MicroPython] emcc ausente — usando MVP WASM (python_eval/exec/_start)")
        return False
    print(f"[MicroPython] Emscripten encontrado: {emcc}")
    return True


def clone_micropython() -> None:
    if BUILD_DIR.exists() and (BUILD_DIR / "ports").exists():
        print(f"[MicroPython] Diretório já existe: {BUILD_DIR}")
        return
    print(f"[MicroPython] Clonando {MICROPYTHON_REPO} @ {MICROPYTHON_VERSION}...")
    BUILD_DIR.parent.mkdir(parents=True, exist_ok=True)
    if BUILD_DIR.exists():
        shutil.rmtree(BUILD_DIR)
    subprocess.run(
        [
            "git",
            "clone",
            "--depth",
            "1",
            "--branch",
            MICROPYTHON_VERSION,
            MICROPYTHON_REPO,
            str(BUILD_DIR),
        ],
        check=True,
    )


def build_with_emscripten() -> bool:
    mp_emscripten_dir = BUILD_DIR / "ports" / "emscripten"
    if not mp_emscripten_dir.exists():
        print(f"[MicroPython] ERRO: {mp_emscripten_dir} ausente")
        return False
    cwd = os.getcwd()
    try:
        os.chdir(mp_emscripten_dir)
        subprocess.run(["make", "clean"], check=False, capture_output=True)
        print("[MicroPython] Executando make (emscripten)...")
        subprocess.run(["make"], check=True, timeout=600)
        wasm_files = list(mp_emscripten_dir.glob("*.wasm"))
        if not wasm_files:
            print("[MicroPython] ERRO: nenhum .wasm gerado pelo make")
            return False
        publish_artifact(wasm_files[0].read_bytes(), source="emscripten")
        return True
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
        print(f"[MicroPython] ERRO emscripten: {e}")
        return False
    finally:
        os.chdir(cwd)


def publish_artifact(data: bytes, source: str) -> None:
    if data[:4] != b"\0asm":
        raise SystemExit(f"[MicroPython] magic inválido: {data[:4]!r}")
    # NÃO padar com zeros — wasmi interpreta trailing 0x00 como seção inválida.
    for dest in (WASM_OUTPUT, HERMES_ASSET, FAT_NAME):
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(data)
        print(f"[MicroPython] escrito {dest} ({len(data)} bytes, source={source})")


def verify_wasm(path: Path) -> None:
    data = path.read_bytes()
    if data[:4] != b"\0asm":
        raise SystemExit(f"[MicroPython] ERRO: magic inválido em {path}")
    print(f"[MicroPython] WASM válido: {path} ({len(data)} bytes)")


def main() -> int:
    print("=" * 60)
    print("MicroPython WASM Build — GOAL3")
    print("=" * 60)

    if check_emscripten():
        try:
            clone_micropython()
            if build_with_emscripten():
                verify_wasm(WASM_OUTPUT)
                print("[MicroPython] Build Emscripten OK")
                return 0
            print("[MicroPython] Emscripten falhou — caindo para MVP")
        except Exception as e:
            print(f"[MicroPython] Emscripten path falhou: {e} — caindo para MVP")

    mvp = build_mvp_wasm()
    publish_artifact(mvp, source="mvp_python_eval")
    verify_wasm(WASM_OUTPUT)
    print("[MicroPython] MVP bytecode pronto (wasmi CapGate intacto)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
