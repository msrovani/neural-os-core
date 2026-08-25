#!/usr/bin/env python3
"""Baixa tiiuae/Falcon3-3B-Base (ou --variant 1.58bit) via huggingface_hub.

Salva em target/falcon3/ (--output). Trata licenca Falcon 3 terms.
Uso:
  python tools/download_falcon3.py
  python tools/download_falcon3.py --variant 1.58bit
  python tools/download_falcon3.py --variant base --output target/falcon3
"""
from __future__ import annotations
import argparse
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUT = ROOT / "target" / "falcon3"

REPOS = {
    "1b": "tiiuae/Falcon3-1B-Instruct-1.58bit",
    "3b": "tiiuae/Falcon3-3B-Instruct-1.58bit",
    "7b": "tiiuae/Falcon3-7B-Instruct-1.58bit",
    "10b": "tiiuae/Falcon3-10B-Instruct-1.58bit",
    "base": "tiiuae/Falcon3-3B-Base",
    "1.58bit": "tiiuae/Falcon3-3B-Instruct-1.58bit",
    "instruct": "tiiuae/Falcon3-3B-Instruct",
}

LICENSE_NOTE = """\
[Falcon 3 License] https://huggingface.co/tiiuae/Falcon3-3B-Base
  Falcon 3 sujeito a Falcon LLM License / TII terms. Uso comercial permitido
  com atribuicao. Ao baixar voce aceita os termos do repo HF.
  Veja LICENSE e Falcon3-LICENSE no repo HF antes de distribuir.
"""


def main():
    ap = argparse.ArgumentParser(description="Baixa Falcon3 3B via huggingface_hub")
    ap.add_argument("--variant", default="7b",
                    choices=["1b", "3b", "7b", "10b", "base", "1.58bit", "instruct", "1_58bit", "158"],
                    help="Falcon3 size (default 7b = alvo Neural OS)")
    ap.add_argument("--output", type=Path, default=DEFAULT_OUT, help="dir destino (target/falcon3)")
    ap.add_argument("--revision", default="main", help="branch/revision HF")
    ap.add_argument("--token", default=None, help="HF token se repo gated (ou env HF_TOKEN)")
    args = ap.parse_args()

    var = args.variant.replace("_", ".").replace("158", "1.58bit")
    if var == "1_58bit":
        var = "1.58bit"
    repo_id = REPOS.get(var, REPOS["base"])
    out = args.output
    out.mkdir(parents=True, exist_ok=True)

    print(LICENSE_NOTE)
    print(f"[DL] {repo_id} -> {out} (revision={args.revision})")

    try:
        from huggingface_hub import snapshot_download
    except ImportError:
        print("[ERRO] huggingface_hub nao instalado. pip install huggingface_hub")
        sys.exit(1)

    token = args.token or os.environ.get("HF_TOKEN") or os.environ.get("HUGGING_FACE_HUB_TOKEN")
    try:
        dst = snapshot_download(
            repo_id=repo_id,
            local_dir=str(out),
            local_dir_use_symlinks=False,
            revision=args.revision,
            token=token,
        )
    except Exception as e:
        print(f"[ERRO] snapshot_download falhou: {e}")
        print("  Dica: HF_TOKEN env se gated; ou huggingface-cli login")
        sys.exit(1)

    # lista rapida
    files = sorted(p.name for p in out.glob("*") if p.is_file())
    print(f"[OK] {repo_id} em {dst}")
    print(f"  arquivos: {', '.join(files[:8])}{' ...' if len(files)>8 else ''}")
    # sanity
    if not (out / "config.json").exists():
        print("[WARN] config.json ausente no snapshot")
    if not list(out.glob("*.safetensors")):
        print("[WARN] nenhum .safetensors encontrado")


if __name__ == "__main__":
    main()
