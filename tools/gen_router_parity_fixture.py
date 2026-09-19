#!/usr/bin/env python3
"""gen_router_parity_fixture.py — fixture de paridade kernel x trainer do roteador.

Problema (SESSION_362): o encode do kernel divergia do trainer em TRÊS eixos —
mapeamento (`b+2` para todo byte vs `(b-32)+3` só para 32..=126), truncagem
(64 vs 32) e vocabulário (clamp em 255 vs tabela de 99 linhas do arquivo).
Medido: 82.9% -> 17.1% de acerto com o encode do kernel.

Este script congela a REFERÊNCIA (validate_router_v6.py, que parseia o artefato
byte-exato) numa fixture de texto que o teste host em
`crates/cortex/src/trinity.rs` consome com `include_str!`:

    <top1_expert> \\t <ids csv> \\t <probs csv> \\t <texto>

O teste exige (a) `super::encode(text)` == ids, (b) a decisão do kernel == top1
e (c) rota neural (stats_neural +1), tudo sobre o MESMO ROUTER.BITNET.

Uso:
    python tools/gen_router_parity_fixture.py                    # target1/ROUTER.BITNET
    python tools/gen_router_parity_fixture.py outro/ROUTER.BITNET
"""
from __future__ import annotations
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))

import numpy as np
from validate_router_v6 import VOCAB, load_router_v6, forward, encode as ref_encode

# Mesma ordem de registro do kernel (cortex::trinity::init_trinity) e do trainer.
EXPERT_NAMES = ["generator", "hw_control", "hw_identify", "rust_coder",
                "disk_diag", "security", "speech_synth"]

BOS, EOS, CHAR_OFFSET, MAX_TOKENS = 0, 1, 3, 32

CASES = [
    "aumenta o volume",
    "diminui o volume",
    "mute o audio",
    "bom dia jarvis",
    "que horas sao agora",
    "quais dispositivos pci estao no barramento",
    "escreve uma funcao rust para ler um arquivo",
    "o disco esta com erro de leitura no setor",
    "verifique se ha algum ataque de rede suspeito",
    "leia em voz alta o relatorio de status",
    # truncagem: 32 tokens = BOS + 30 chars + EOS -> esta frase corta
    "configura o brilho do monitor para cinquenta por cento agora mesmo",
    # nao-ASCII: bytes > 126 sao PULADOS (acento nao vira token)
    "configuração de segurança na rede",
]


def encode_ids(text: str):
    """Espelho exato do encode do trainer/kernel (a única verdade agora)."""
    toks = [BOS]
    for b in text.encode("utf-8"):
        if 32 <= b <= 126:
            toks.append((b - 32) + CHAR_OFFSET)
    toks.append(EOS)
    return toks[:MAX_TOKENS]


def fnv1a64(data: bytes) -> int:
    """FNV-1a 64 — barato, determinístico e sem dependência (o teste Rust usa o
    mesmo algoritmo para travar o par artefato+fixture)."""
    h = 0xCBF29CE484222325
    for b in data:
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def counts_from_ids(ids):
    c = np.zeros(VOCAB, dtype=np.float32)
    for t in ids:
        c[min(t, VOCAB - 1)] += 1.0
    return c


def main():
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass

    path = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target1" / "ROUTER.BITNET"
    res, err = load_router_v6(path)
    if res is None:
        print(f"[FAIL] {path}: {err}")
        return 1
    embed, Wq = res["embed"], res["Wq"]

    raw = path.read_bytes()
    # Cópia RASTREADA do artefato: target1/ é gitignored, então o teste host não
    # pode usar include_bytes! nele (quebraria o compile num clone limpo).
    ref = ROOT / "tools" / "router_reference.BITNET"
    ref.write_bytes(raw)
    try:
        src_str = str(path.resolve().relative_to(ROOT))  # caminho relativo no header
    except ValueError:
        src_str = str(path)
    lines = [
        f"# router parity fixture (referencia: tools/validate_router_v6.py)",
        f"# origem: {src_str}",
        f"# copia: tools/router_reference.BITNET size={len(raw)} fnv1a64={fnv1a64(raw):016x} vocab={VOCAB}",
        f"# formato: top1_expert<TAB>ids<TAB>probs<TAB>text",
    ]
    mismatched_counts = 0
    for text in CASES:
        ids = encode_ids(text)
        # self-check: ids (token list) tem de gerar o MESMO histograma da referencia
        counts = counts_from_ids(ids)
        if not np.array_equal(counts, ref_encode(text)):
            mismatched_counts += 1
        probs = forward(counts[None, :], embed, Wq)[0]
        top1 = int(probs.argmax())
        name = EXPERT_NAMES[top1]
        lines.append("\t".join([
            name,
            ",".join(str(i) for i in ids),
            ",".join(f"{p:.6f}" for p in probs),
            text,
        ]))
        print(f"  {name:<11} n_tok={len(ids):<3} max={probs[top1]:.3f} {text!r}")

    out = ROOT / "tools" / "router_parity_fixture.txt"
    out.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"\n[ok] {out} ({len(CASES)} casos) histograma-divergente={mismatched_counts}")
    print(f"[ok] {ref} ({ref.stat().st_size} bytes, fnv1a64={fnv1a64(raw):016x})")
    return 0 if mismatched_counts == 0 else 2


if __name__ == "__main__":
    sys.exit(main())
