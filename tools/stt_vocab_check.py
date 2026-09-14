#!/usr/bin/env python3
"""stt_vocab_check.py — contrato de vocabulário STT (kernel ↔ trainer).

O kernel (`crates/jarbas/src/audio/stt.rs::VOCAB_CHARS`) e o trainer
(`tools/train_stt.py::VOCAB_CHARS`) precisam listar os MESMOS caracteres na MESMA
ordem, porque o `out.bias` do modelo é indexado por essa ordem. Um desalinhamento
silencioso faz o modelo "falar" caracteres trocados — o tipo de bug que só aparece
em produção, nunca no loss do treino.

Uso:
    python tools/stt_vocab_check.py        # exit 0 se casar
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
RUST = ROOT / "crates" / "jarbas" / "src" / "audio" / "stt.rs"
PY = ROOT / "tools" / "train_stt.py"


def rust_vocab() -> list[str]:
    src = RUST.read_text(encoding="utf-8")
    m = re.search(r"pub const VOCAB_CHARS:\s*&\[char\]\s*=\s*&\[(.*?)\];", src, re.S)
    if not m:
        raise SystemExit("[FAIL] VOCAB_CHARS não encontrado em stt.rs")
    return re.findall(r"'(.)'", m.group(1))


def py_vocab() -> list[str]:
    src = PY.read_text(encoding="utf-8")
    m = re.search(r"^VOCAB_CHARS\s*=\s*\[(.*?)\]", src, re.S | re.M)
    if not m:
        raise SystemExit("[FAIL] VOCAB_CHARS não encontrado em train_stt.py")
    return re.findall(r"'(.)'", m.group(1))


def main() -> int:
    r = rust_vocab()
    p = py_vocab()
    print(f"[STT] rust  : {len(r)} chars -> {''.join(r)!r}")
    print(f"[STT] python: {len(p)} chars -> {''.join(p)!r}")
    if r != p:
        print("[FAIL] vocabulários divergem (o modelo indexa por esta ordem!)")
        for i, (a, b) in enumerate(zip(r, p)):
            if a != b:
                print(f"       índice {i}: rust={a!r} python={b!r}")
                break
        if len(r) != len(p):
            print(f"       tamanho: rust={len(r)} python={len(p)}")
        return 1
    print(f"[OK] vocabulário idêntico — vocab={len(r)+1} (blank={len(r)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
