#!/usr/bin/env python3
"""Fetch third-party model weights (host tool) com pinning SHA-256.

Uso:
  python tools/download_models.py --all            # baixa tudo que tem pin sha256
  python tools/download_models.py --file PIPER_PT_BR.BIN  # só uma entrada
  python tools/download_models.py --verify         # confere arquivos existentes vs pins

Fail-closed: enquanto `sha256` for None o script NÃO baixa nada — imprime o
comando de conversão manual. Arquivo nunca é salvo sem hash verificado.
"""
from __future__ import annotations

import argparse
import hashlib
import sys
import urllib.request
from pathlib import Path

# Windows cp1252 console não imprime —/ã; forçar UTF-8 (errors=replace)
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

ROOT = Path(__file__).resolve().parent.parent
MODELS_DIR = ROOT / "models" / "tokenizer"

# TODO: compute after first trusted download, then commit the pin.
# O primeiro `--all` (com pins=None) só imprime instruções — intencional.
# Converter é o comando que regenera o .BIN a partir da fonte (None = sem
# converter dedicado em tools/; o .BIN é artefato convertido).
MANIFEST = {
    "PIPER_PT_BR.BIN": {
        "url": (
            "https://huggingface.co/rhasspy/piper-voices/resolve/main/"
            "pt/pt_BR/cadu/medium/pt_BR-cadu-medium.onnx"
        ),
        "sha256": None,  # TODO: pin after first trusted download
        "convert": "python tools/convert_piper_to_bitnet.py --voice pt_BR-cadu-medium",
    },
    "E5_MULTI.BIN": {
        "url": (
            "https://huggingface.co/intfloat/multilingual-e5-small/resolve/main/"
            "model.safetensors"
        ),
        "sha256": None,  # TODO: pin after first trusted download
        "convert": None,  # sem converter E5 em tools/; padrão BGE em convert_bgem3.py
    },
}


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def download_verified(name: str, entry: dict) -> int:
    pin = entry["sha256"]
    if pin is None:
        print(f"[SKIP] {name}: sem pin sha256 — nada baixado (fail-closed).")
        print(f"       Fonte: {entry['url']}")
        if entry.get("convert"):
            print(f"       Converter: {entry['convert']}")
        else:
            print(f"       {name} é artefato convertido (sem converter dedicado em tools/);")
            print("       use a fonte acima + padrão BGE (tools/convert_bgem3.py).")
        return 0

    dest = MODELS_DIR / name
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = dest.with_suffix(dest.suffix + ".part")
    print(f"[DL] {name} <- {entry['url']}")
    h = hashlib.sha256()
    try:
        with urllib.request.urlopen(entry["url"], timeout=120) as resp, open(tmp, "wb") as fh:
            while True:
                chunk = resp.read(1 << 20)
                if not chunk:
                    break
                h.update(chunk)
                fh.write(chunk)
    except Exception as e:  # noqa: BLE001 — host tool, report and bail
        tmp.unlink(missing_ok=True)
        print(f"[ERRO] download {name}: {e}")
        return 1

    got = h.hexdigest()
    if got != pin:
        tmp.unlink(missing_ok=True)
        print(f"[FAIL] {name}: sha256 {got} != pin {pin} — arquivo descartado")
        return 1
    tmp.rename(dest)
    print(f"[OK] {name}: {dest} ({dest.stat().st_size} B) sha256={got}")
    return 0


def verify(name: str, entry: dict) -> int:
    dest = MODELS_DIR / name
    if not dest.is_file():
        print(f"FAIL {name} (missing)")
        return 1
    pin = entry["sha256"]
    if pin is None:
        print(f"[?] {name}: sem pin — não verificável (fail-closed => FAIL)")
        return 1
    got = sha256_file(dest)
    if got == pin:
        print(f"OK {name}")
        return 0
    print(f"FAIL {name} (sha256 {got} != {pin})")
    return 1


def main() -> None:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--verify", action="store_true", help="conferir arquivos existentes contra os pins")
    ap.add_argument("--all", action="store_true", help="baixar todas as entradas com pin sha256")
    ap.add_argument("--file", metavar="NAME", help="baixar/verificar só uma entrada do manifesto")
    args = ap.parse_args()

    names = [args.file] if args.file else list(MANIFEST)
    unknown = [n for n in names if n not in MANIFEST]
    if unknown:
        print(f"[ERRO] entradas desconhecidas: {', '.join(unknown)}")
        print(f"       conhecidas: {', '.join(MANIFEST)}")
        sys.exit(2)

    if args.verify:
        action = verify
    elif args.all or args.file:
        action = download_verified
    else:
        ap.print_help()
        print("\nUse --all (baixar tudo com pin), --verify (conferir) ou --file NAME (um só).")
        sys.exit(2)

    code = 0
    for n in names:
        code |= action(n, MANIFEST[n])
    sys.exit(code)


if __name__ == "__main__":
    main()
