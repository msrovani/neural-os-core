#!/usr/bin/env python3
"""compress_boot_evidence.py — reduz um log de boot (evidência) a um digest.

Uso:
  python3 tools/compress_boot_evidence.py docs/evidence/<log>.txt
      Imprime o digest no stdout (sem alterar arquivos).
  python3 tools/compress_boot_evidence.py --archive docs/evidence/<log>.txt
      Escreve o digest NO caminho original (referências de docs continuam
      válidas) e move o log bruto gzipado para docs/evidence/archive/.

Motivação (auditoria 10 itens #6): logs de boot crus (ex.: 1036 linhas de
serial) poluem o repo. Logs completos vivem em CI artifacts (boot.yml já
sobe boot.txt como artifact); o repo guarda só o digest: cabeçalho de
aceite do autor (#) + contagens das assinaturas-chave do boot.
"""

import gzip
import re
import shutil
import sys
from pathlib import Path

SIGNATURES = {
    "OK/PASS/Allow/Deny": re.compile(r"OK|PASS|Allow|Deny"),
    "[EXC] (exceções)": re.compile(r"\[EXC\]"),
    "[SELF-HEAL]": re.compile(r"\[SELF-HEAL\]"),
    "triple fault (CR2=0xfffffffffffffff8)": re.compile(r"CR2=0xfffffffffffffff8"),
    "Phase 6": re.compile(r"Phase 6"),
    "tick=": re.compile(r"tick="),
    "[BOOT:Runtime]": re.compile(r"\[BOOT:Runtime\]"),
    "AgentFleet": re.compile(r"AgentFleet"),
}


def digest(path: Path) -> str:
    text = path.read_text(encoding="utf-8-sig", errors="replace")
    lines = text.splitlines()
    header = [ln for ln in lines if ln.startswith("#")]
    counts = {name: sum(1 for ln in lines if pat.search(ln)) for name, pat in SIGNATURES.items()}
    exc = counts["[EXC] (exceções)"]
    trip = counts["triple fault (CR2=0xfffffffffffffff8)"]
    runtime = counts["[BOOT:Runtime]"]
    verdict = "PASS" if runtime > 0 and trip == 0 and exc == 0 else "REVIEW"
    out = []
    out.append(f"# Digest de {path.name} — gerado por tools/compress_boot_evidence.py")
    out.append(f"# Fonte bruta: {len(lines)} linhas, arquivada em docs/evidence/archive/")
    out.append(f"# Veredito: {verdict}")
    out.append("")
    out.append("## Cabeçalho de aceite (autor)")
    out.extend(header or ["(sem cabeçalho #)"])
    out.append("")
    out.append("## Assinaturas")
    for name, n in counts.items():
        out.append(f"- {name}: {n}")
    out.append("")
    return "\n".join(out) + "\n"


def main() -> int:
    argv = sys.argv[1:]
    archive = False
    if argv and argv[0] == "--archive":
        archive = True
        argv = argv[1:]
    if not argv:
        print(__doc__)
        return 1
    src = Path(argv[0])
    if not src.exists():
        print(f"ERRO: {src} não encontrado", file=sys.stderr)
        return 1
    d = digest(src)
    if not archive:
        sys.stdout.write(d)
        return 0
    # Bruto gzipado no archive/ ANTES de sobrescrever com o digest.
    archive_dir = src.parent / "archive"
    archive_dir.mkdir(exist_ok=True)
    gz = archive_dir / (src.name + ".gz")
    with src.open("rb") as f_in, gzip.open(gz, "wb", compresslevel=9) as f_out:
        shutil.copyfileobj(f_in, f_out)
    # Digest no caminho ORIGINAL (referências de docs continuam válidas).
    src.write_text(d, encoding="utf-8")
    print(f"OK: digest escrito em {src}")
    print(f"OK: bruto gzipado em {gz} ({src.name})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
