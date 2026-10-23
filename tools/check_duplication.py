#!/usr/bin/env python3
"""Detecta arquivos .rs duplicados entre crates (mesmo caminho relativo em >=2 crates).

Fonte unica na crate base (licao SESSION_237/243: correcao numa copia nao chega as
outras, e nada avisa). Ignora facades legitimas:
  - `mod.rs` (facade de modulo), `lib.rs`/`main.rs` (raiz de crate)
  - arquivos que so tem `pub use` + comentarios (re-export fino, padrao documentado)

Qualquer outro `.rs` com o mesmo caminho relativo em mais de uma crate e bandeira
de duplicacao real (duas implementacoes que podem divergir silenciosamente).

A1 (HUMAN-0010): tambem varre `nixaios/packages/nosd/crates/*/src` e compara
caminhos relativos contra `crates/*/src`. Hits na lista EXCLUDE_NIXAIOS sao
bancada intencional (reportados como INFO, nao falham). Outros hits cross-tree
falham (exit 1) — segunda verdade de semantica.

Uso: python tools/check_duplication.py   (exit 1 se achar duplicatas)
"""
import os
import sys
from collections import defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRATES = os.path.join(ROOT, "crates")
NIXAIOS_CRATES = os.path.join(ROOT, "nixaios", "packages", "nosd", "crates")
ALWAYS_FACADE = {"mod.rs", "lib.rs", "main.rs"}

# Bancada NixAIOS: paths relativos (sob src/) permitidos como espelho/stub.
# Manter curto; preferir adaptador das crates canonicas (forum COMPOSER-0014).
EXCLUDE_NIXAIOS = {
    # stubs de bancada medidos 2026-10-22 (CROSS) — preferir adaptador depois
    "gguf.rs",
    "hub.rs",
}


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
        if stripped.startswith("#!["):
            continue
        if stripped.startswith("use ") or stripped.startswith("pub use ") or stripped.startswith("pub(crate) use "):
            continue
        return False
    return True


def collect_tree(src_root: str, label_prefix: str, into: dict) -> None:
    if not os.path.isdir(src_root):
        return
    for crate in sorted(os.listdir(src_root)):
        src = os.path.join(src_root, crate, "src")
        if not os.path.isdir(src):
            continue
        for dirpath, _, files in os.walk(src):
            for f in files:
                if not f.endswith(".rs") or f in ALWAYS_FACADE:
                    continue
                full = os.path.join(dirpath, f)
                if is_facade(full):
                    continue
                rel = os.path.relpath(full, src).replace("\\", "/")
                into[rel].append((f"{label_prefix}/{crate}", os.path.getsize(full)))


def main() -> int:
    strict_crates = "--strict-crates" in sys.argv[1:]
    seen = defaultdict(list)  # rel_path -> [(crate, size)]
    collect_tree(CRATES, "crates", seen)

    bad = {rel: hits for rel, hits in seen.items() if len(hits) > 1}
    rc = 0
    if bad:
        tag = "DUP" if strict_crates else "LEGACY_DUP"
        for rel, hits in sorted(bad.items()):
            detail = ", ".join(f"{crate} ({size}B)" for crate, size in hits)
            print(f"{tag} {rel}: {detail}")
        if strict_crates:
            rc = 1
        else:
            print(
                "NOTE: %d LEGACY_DUP entre crates/ (pre-existente; "
                "use --strict-crates para falhar). A1 falha so em CROSS."
                % len(bad)
            )
    else:
        print("OK: nenhum .rs duplicado entre crates/")

    # Cross-tree: crates vs nixaios (bancada)
    nix_seen = defaultdict(list)
    collect_tree(NIXAIOS_CRATES, "nixaios", nix_seen)
    if not nix_seen:
        print("OK: nixaios/packages/nosd/crates ausente ou vazio (skip cross)")
        return rc

    cross = []
    info = []
    for rel, nix_hits in sorted(nix_seen.items()):
        if rel not in seen:
            continue
        if rel in EXCLUDE_NIXAIOS:
            info.append((rel, seen[rel], nix_hits))
        else:
            cross.append((rel, seen[rel], nix_hits))

    for rel, a, b in info:
        detail_a = ", ".join(f"{c} ({s}B)" for c, s in a)
        detail_b = ", ".join(f"{c} ({s}B)" for c, s in b)
        print(f"INFO bancada-exclude {rel}: {detail_a} | {detail_b}")

    if cross:
        rc = 1
        for rel, a, b in cross:
            detail_a = ", ".join(f"{c} ({s}B)" for c, s in a)
            detail_b = ", ".join(f"{c} ({s}B)" for c, s in b)
            print(f"CROSS {rel}: {detail_a} | {detail_b}")
        print(
            "HINT: se for bancada intencional, adicione o path relativo a "
            "EXCLUDE_NIXAIOS em tools/check_duplication.py; preferivel adaptar "
            "crates canonicas (HUMAN-0010 NixAIOS=bancada)."
        )
    else:
        print("OK: nenhum CROSS inesperado crates/ vs nixaios")

    return rc


if __name__ == "__main__":
    sys.exit(main())
