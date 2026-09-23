#!/usr/bin/env python3
"""update_tecnologias.py — Re-põe números derivados (LOC/arquivos) no cabeçalho do AGENTS.md.

Rodar após alterações em TECNOLOGIAS.md / estrutura do repo (AGENTS.md regra 6).
Não toca em TECNOLOGIAS.md (linhas de tecnologia são manuais); só mede o repo.

Uso: python tools/update_tecnologias.py [--check]
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def count_rust() -> tuple[int, int]:
    """Retorna (LOC total, nº de arquivos .rs) fora de target/."""
    loc = files = 0
    for f in ROOT.rglob("*.rs"):
        if "target" in f.parts:
            continue
        files += 1
        try:
            loc += sum(1 for _ in open(f, "r", encoding="utf-8", errors="replace"))
        except OSError:
            pass
    return loc, files


def main() -> None:
    check = "--check" in sys.argv
    loc, files = count_rust()
    header = ROOT / "AGENTS.md"
    text = header.read_text(encoding="utf-8")
    pat = re.compile(r"~\d+K LOC, ~\d+ arquivos Rust")
    new = f"~{loc // 1000}K LOC, ~{files} arquivos Rust"
    old = pat.search(text)
    print(f"[MEASURE] {new} (anterior: {old.group(0) if old else 'n/a'})")
    if old is None or old.group(0) == new:
        print("[OK] cabeçalho já em dia.")
        return
    if check:
        print("[DRIFT] rode sem --check para atualizar.")
        sys.exit(1)
    header.write_text(pat.sub(new, text, count=1), encoding="utf-8")
    print("[OK] AGENTS.md atualizado.")


if __name__ == "__main__":
    main()
