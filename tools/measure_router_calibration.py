#!/usr/bin/env python3
"""measure_router_calibration.py — calibração (confiança x acerto) do roteador Trinity.

Responde com número, não opinião, três perguntas:

  1. O `ROUTER.BITNET` exportado acerta o rótulo, e a confiança (max softmax)
     prediz esse acerto? (tabela de reliableza + ECE)
  2. O rótulo do dataset É a semântica do classifier de keyword (ordem de
     verificação) — logo "acerto" aqui é **fidelidade da destilação**, não acerto
     contra fala real. Este script diz isso no output, para não virar overclaim.
  3. O que o KERNEL realmente computa? O `encode()` do Rust (`b+2`, truncate 64,
     VOCAB=256, tabela de 99 linhas) difere do `encode()` do trainer
     (`(b-32)+3`, truncate 32) — aqui os dois são medidos lado a lado.

Uso:
    python tools/measure_router_calibration.py [target1/ROUTER.BITNET]
"""
from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

try:  # console Windows (cp1252) não encoda setas/≈ — força UTF-8 com fallback
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tools"))

from validate_router_v6 import load_router_v6  # noqa: E402
from train_router import CURATED, EXPERT_NAMES, stratified_split  # noqa: E402

VOCAB, HIDDEN, N_EXPERTS = 99, 64, 7
BOS, EOS, CHAR_OFFSET = 0, 1, 3
# ATENÇÃO (s362): os dois abaixo descrevem o kernel ANTES do alinhamento — são
# o contraste histórico. Depois do fix o kernel usa o MESMO mapa/truncagem/vocab
# do arquivo (ver encode_trainer). A seção 2 mede o "antes".
KERNEL_VOCAB = 256          # trinity.rs pré-s362: `const VOCAB: usize = 256`
KERNEL_MAX_TOKENS = 64      # trinity.rs pré-s362: `tokens.truncate(64)`
TRAINER_MAX_TOKENS = 32     # train_router.py: MAX_TOKENS = 32


# ── encode: as duas verdades ────────────────────────────────────────────────

def encode_trainer(text: str) -> np.ndarray:
    """Espelho do trainer (é o que o arquivo conhece): (b-32)+3, truncate 32."""
    toks = [BOS]
    for b in text.encode("utf-8"):
        if 32 <= b <= 126:
            toks.append((b - 32) + CHAR_OFFSET)
    toks.append(EOS)
    toks = toks[:TRAINER_MAX_TOKENS]
    counts = np.zeros(VOCAB, dtype=np.float32)
    for t in toks:
        counts[min(t, VOCAB - 1)] += 1.0
    return counts


def encode_kernel_prev_s362(text: str) -> tuple[np.ndarray, int, int]:
    """PRÉ-s362 (histórico): kernel b+2 (todos os bytes), truncate 64, tabela de 99.

    `idx = tok.min(VOCAB-1)` com VOCAB=256 -> start = idx*64; fora da tabela
    (idx > 98) o `.get()` devolve None e a contribuição é 0.
    Retorna (histograma, tokens, tokens_fora_da_tabela).
    """
    toks = [BOS]
    for b in text.encode("utf-8"):
        toks.append(b + 2)
    toks.append(EOS)
    toks = toks[:KERNEL_MAX_TOKENS]
    counts = np.zeros(VOCAB, dtype=np.float32)
    out_of_table = 0
    for t in toks:
        idx = min(t, KERNEL_VOCAB - 1)
        if idx < VOCAB:
            counts[idx] += 1.0
        else:
            out_of_table += 1
    return counts, len(toks), out_of_table


def forward(X: np.ndarray, embed: np.ndarray, Wq: np.ndarray) -> np.ndarray:
    h = X @ embed
    norms = np.linalg.norm(h, axis=1, keepdims=True) + 1e-8
    h = h / norms
    logits = h @ Wq.astype(np.float32)
    logits -= logits.max(axis=1, keepdims=True)
    ex = np.exp(logits)
    return ex / ex.sum(axis=1, keepdims=True)


def probs_for(embed, Wq, texts, encoder) -> np.ndarray:
    X = np.stack([encoder(t)[0] if encoder is encode_kernel_prev_s362 else encoder(t) for t in texts])
    return forward(X, embed, Wq)


# ── métricas ────────────────────────────────────────────────────────────────

BUCKETS = [0.0, 0.5, 0.7, 0.85, 0.95, 1.0001]

def reliability(probs: np.ndarray, y: np.ndarray):
    conf = probs.max(axis=1)
    pred = probs.argmax(axis=1)
    correct = (pred == y).astype(np.float64)
    rows = []
    ece = 0.0
    n = len(y)
    for lo, hi in zip(BUCKETS[:-1], BUCKETS[1:]):
        m = (conf >= lo) & (conf < hi)
        cnt = int(m.sum())
        if cnt == 0:
            rows.append((lo, hi, 0, None, None))
            continue
        acc = float(correct[m].mean())
        mc = float(conf[m].mean())
        ece += (cnt / n) * abs(acc - mc)
        rows.append((lo, hi, cnt, acc, mc))
    acc_all = float(correct.mean())
    return rows, acc_all, float(conf.mean()), ece, pred, conf, correct


def print_reliability(title, rows, acc, mean_conf, ece, n):
    print(f"\n  {title}  (n={n})")
    print(f"    {'faixa':>12} | {'n':>3} | {'acerto':>7} | {'conf méd':>8} | {'gap':>6}")
    for lo, hi, cnt, a, mc in rows:
        if cnt == 0:
            print(f"    {lo:.2f}–{hi:.2f}   | {0:>3} | {'—':>7} | {'—':>8} | {'—':>6}")
        else:
            print(f"    {lo:.2f}–{hi:.2f}   | {cnt:>3} | {a*100:>6.1f}% | {mc*100:>7.1f}% | {(a-mc)*100:>+5.1f}pp")
    print(f"    acerto={acc*100:.1f}%  conf_média={mean_conf*100:.1f}%  ECE={ece*100:.1f}pp")


def main() -> int:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else (ROOT / "target1" / "ROUTER.BITNET")
    res, err = load_router_v6(path)
    if res is None:
        print(f"[FAIL] {path}: {err}")
        return 2
    embed, Wq = res["embed"], res["Wq"]
    print(f"# Arquivo: {path}")
    print(f"  parse_end={res['parse_end']} size={res['size']} vocab={VOCAB} hidden={HIDDEN} n_exp={N_EXPERTS}")
    nz = int((Wq != 0).sum())
    print(f"  pesos não-zero: {nz}/{Wq.size} ({nz/Wq.size*100:.1f}%) | "
          f"valores={sorted(set(Wq.flatten().tolist()))}")

    # Mesmo split do validator (seed 7 / 0.28) -> holdout nunca visto
    test, _rest = stratified_split(CURATED, 0.28, seed=7)
    tx = [t for t, _ in test]
    ty = np.array([l for _, l in test], dtype=np.int64)
    allx = [t for t, _ in CURATED]
    ally = np.array([l for _, l in CURATED], dtype=np.int64)

    # ── 1. Rótulo = semântica do classifier (fidelidade da destilação)
    print("\n## 1. Com o encode do TRAINER (o que o arquivo conhece)")
    print("   Rótulo = ordem de verificação do classifier de keyword.")
    print("   -> 'acerto' aqui é FIDELIDADE DA DESTILAÇÃO, não acerto em fala real.")

    p_test = probs_for(embed, Wq, tx, encode_trainer)
    rows, acc, mc, ece, pred, conf, correct = reliability(p_test, ty)
    print_reliability("HOLDOUT (CURATED, mesma split do validator)", rows, acc, mc, ece, len(ty))

    p_all = probs_for(embed, Wq, allx, encode_trainer)
    rows2, acc2, mc2, ece2, pred2, conf2, corr2 = reliability(p_all, ally)
    print_reliability("TODO o CURATED (inclui treino — contraste)", rows2, acc2, mc2, ece2, len(ally))

    # ── 2. O que o KERNEL computa
    print("\n## 2. ANTES do fix s362 — encode do kernel (b+2 / truncate 64 / VOCAB=256 vs tabela 99)")
    print("   (o kernel de hoje espelha encode_trainer; esta seção é o contraste histórico)")
    stats = [encode_kernel_prev_s362(t) for t in allx]
    oot = sum(s[2] for s in stats)
    tot = sum(s[1] for s in stats)
    print(f"   tokens fora da tabela: {oot}/{tot} ({oot/tot*100:.1f}%) — contribuem 0 no kernel")
    p_k = probs_for(embed, Wq, allx, encode_kernel_prev_s362)
    rows3, acc3, mc3, ece3, pred3, conf3, corr3 = reliability(p_k, ally)
    print_reliability("TODO o CURATED, encode do KERNEL (pré-s362)", rows3, acc3, mc3, ece3, len(ally))
    print("   hoje (pós-s362) o kernel usa o encode do trainer: acerto=="
          f"{acc2*100:.1f}% — travado pelo teste host "
          "`router_encode_and_decision_match_python_reference`")

    # ── 3. Casos perigosos: confiança alta e rótulo não-confiável
    print("\n## 3. Casos com confiança >= 0.70 (o que passaria de um gate theta=0.7)")
    idx = np.argsort(-conf2)
    shown = 0
    for i in idx:
        if conf2[i] < 0.70:
            break
        mark = "ok " if corr2[i] else "ERRO"
        print(f"   {mark} conf={conf2[i]*100:>5.1f}%  pred={EXPERT_NAMES[pred2[i]]:<12} "
              f"rótulo={EXPERT_NAMES[int(ally[i])]:<12} | {allx[i][:52]!r}")
        shown += 1
        if shown >= 12:
            break
    if shown == 0:
        print("   (nenhuma amostra passaria de 0.70)")

    # ── 4. Sondas OOD (faladas/realistas) — sem score, só comportamento
    print("\n## 4. Sondas OOD (julgamento semântico, não rótulo de dataset)")
    probes = [
        ("por que o volume do disco encheu", "disk_diag (palavra 'volume' sombreia)"),
        ("aumenta o volume do alto-falante", "hw_control"),
        ("me explica como funciona o roteador", "generator"),
        ("tem algum ataque no log de rede", "security"),
        ("o ssd esta com problema", "disk_diag"),
        ("fala mais alto por favor", "hw_control"),
    ]
    pk = probs_for(embed, Wq, [t for t, _ in probes], encode_trainer)
    for (txt, esperado), pr in zip(probes, pk):
        top = int(pr.argmax())
        print(f"   conf={pr[top]*100:>5.1f}%  pred={EXPERT_NAMES[top]:<12} esperado~{esperado:<32} | {txt!r}")

    print("\n# Resumo")
    print(f"  holdout n={len(ty)} acerto={acc*100:.1f}% ECE={ece*100:.1f}pp")
    print(f"  encode do kernel PRÉ-s362: acerto={acc3*100:.1f}% ECE={ece3*100:.1f}pp "
          f"(vs {acc2*100:.1f}% com o encode do trainer, que é o de hoje)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
