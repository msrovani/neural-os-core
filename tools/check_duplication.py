#!/usr/bin/env python3
"""Detecta arquivos .rs duplicados entre crates (mesmo caminho relativo em >=2 crates).

Fonte unica na crate base (licao SESSION_237/243: correcao numa copia nao chega as
outras, e nada avisa). Ignora facades legitimas:
  - `mod.rs` (facade de modulo), `lib.rs`/`main.rs` (raiz de crate)
  - arquivos que so tem `pub use` + comentarios (re-export fino, padrao documentado)

Qualquer outro `.rs` com o mesmo caminho relativo em mais de uma crate e bandeira
de duplicacao real (duas implementacoes que podem divergir silenciosamente).

Uso: python tools/check_duplication.py   (exit 1 se achar duplicatas)
"""
import os
import sys
from collections import defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRATES = os.path.join(ROOT, "crates")
ALWAYS_FACADE = {"mod.rs", "lib.rs", "main.rs"}


def is_facade(path: str) -> bool:
    """True se o arquivo so re-exporta (pub use) + comentarios."""
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            text = f.read()
    except OSError:
        return False
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("//") or stripped.startswith("/*"):
            continue
        if stripped.startswith("pub use ") or stripped.startswith("pub(crate) use "):
            continue
        return False
    return True


def main() -> int:
    seen = defaultdict(list)  # rel_path -> [(crate, size)]
    for crate in sorted(os.listdir(CRATES)):
        src = os.path.join(CRATES, crate, "src")
        if not os.path.isdir(src):
            continue
        for dirpath, _, files in os.walk(src):
            for f in files:
                if not f.endswith(".rs") or f in ALWAYS_FACADE:
                    continue
                full = os.path.join(dirpath, f)
                if is_facade(full):
                    continue
                rel = os.path.relpath(full, src)
                seen[rel].append((crate, os.path.getsize(full)))

    bad = {rel: hits for rel, hits in seen.items() if len(hits) > 1}
    if not bad:
        print("OK: nenhum .rs duplicado entre crates")
        return 0
    for rel, hits in sorted(bad.items()):
        detail = ", ".join(f"{crate} ({size}B)" for crate, size in hits)
        print(f"DUP {rel}: {detail}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
