#!/usr/bin/env python3
"""test_v6_suite.py — Suite de coerencia p/ modelos .v6 (host, Windows/IDE).

3 perguntas do aceite:
  Q1 natural:      "Qual e o seu nome?" (chat frame) -> linguagem natural,
                  sem loop degenerado.
  Q2 aritmetica:   "1+1=" (chat frame) -> "2" aparece na saida (assert real).
  Q3 estruturada:  instrucao JSON estilo Hermes->Cortex (OutputGrammar::Json,
                  crates/cortex/src/structured_decode.rs) -> saida parece
                  JSON ({...}/[...]) e sem loop. Bonus informativo: probe
                  machine-path (MachineCtx, infer_queue.rs: ids diretos
                  [bos, intent, slots...], sem framed PT-BR).

Heuristicas = port de bpe::gibberish_stop (cortex/src/bpe.rs):
  janela de 8 toks: distinct-2<=1 OU rep-4gram>0.6 OU peca media<3 chars
  => DEGENERADO.

Gate: exit 0 sse Q1+Q2+Q3 verdes (p/ rodar apos reconversao).
Baseline no artefato quebrado atual: esperado 3x DEGENERADO, exit != 0.

Uso:
  python tools/test_v6_suite.py --model target1/FALCON3_1B.V6 [--max-tokens 48]
"""
from __future__ import annotations
import argparse
import sys
from pathlib import Path

import torch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from chat_v6 import V6, BPE, ensure_tokenizer, build_forbidden_ids  # noqa: E402
import chat_v6  # noqa: E402 (p/ flags globais: W2A8)


def degenerate(ids, bpe, use_piece=True) -> tuple[bool, str]:
    """Port de bpe::gibberish_stop. (ok, motivo). use_piece=False p/ JSON
    (pecas de 1-2 chars sao legitimas em dados estruturados)."""
    if len(ids) < 8:
        return True, "curto (<8 toks)"
    w = ids[len(ids) - 8:]
    dist2 = 0
    for i in range(7):
        if not any(w[k] == w[i] and w[k + 1] == w[i + 1] for k in range(i)):
            dist2 += 1
    if dist2 * 5 < 7:
        return True, f"distinct-2={dist2}/7"
    dist4 = 0
    for i in range(5):
        if not any(w[k] == w[i] and w[k + 1] == w[i + 1]
                   and w[k + 2] == w[i + 2] and w[k + 3] == w[i + 3]
                   for k in range(i)):
            dist4 += 1
    if (5 - dist4) * 10 > 5 * 6:
        return True, f"rep-4gram={(5 - dist4) / 5:.2f}"
    if bpe is not None and use_piece:
        tot, n = 0, 0
        for t in w:
            s = bpe.id2.get(t, "")
            tot += len(s)
            n += 1
        if n and tot * 10 < n * 30:
            return True, f"peca-media={tot / n:.2f} chars"
    return False, "ok"


def looks_like_json(s: str) -> bool:
    s = s.strip()
    # fences markdown (```json ... ```) envolvem JSON valido — o kernel gera
    # com fence e a referencia tambem; julga o conteudo, nao a cerca.
    if s.startswith("```"):
        s = s[3:]
        if s.startswith("json"):
            s = s[4:]
        s = s.strip()
    if s.endswith("```"):
        s = s[:-3].strip()
    return len(s) >= 2 and ((s[0] == "{" and s[-1] == "}") or (s[0] == "[" and s[-1] == "]"))


@torch.no_grad()
def run_case(m, bpe, prompt_ids, max_tokens, eos, label, rep=1.0, fbd=None):
    import time
    t0 = time.time()
    out, _ = m.generate(prompt_ids, max_tokens, eos, stream=None,
                        repetition_penalty=rep, forbidden_ids=fbd)
    dt = time.time() - t0
    text = bpe.decode(out) if bpe else f"<{len(out)} ids, sem vocab>"
    deg, why = degenerate(out, bpe)
    tps = len(out) / dt if dt > 0 else 0.0
    eos_hit = eos >= 0 and out and out[-1] == eos
    print(f"--- {label} ---")
    print(f"output: {text[:300]!r}")
    print(f"toks={len(out)} tok/s={tps:.2f} eos={eos_hit} degenerado={deg} ({why})")
    return out, text, deg


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True)
    ap.add_argument("--tokenizer", default=None)
    ap.add_argument("--max-tokens", type=int, default=48)
    ap.add_argument("--repetition-penalty", type=float, default=1.15,
                    help="1.0=greedy puro (kernel); 1.15=recomendado shippavel")
    ap.add_argument("--no-mask", action="store_true",
                    help="desliga mascara de especiais (puro p/ paridade)")
    ap.add_argument("--w2a8", action="store_true",
                    help="matmul int8 em lineares (experimental; default fp32)")
    ap.add_argument("--frame", default="auto", choices=["auto", "falcon", "bitnet"],
                    help="chat template do prompt (auto: bos <|begin_of_text|>->bitnet)")
    args = ap.parse_args()
    chat_v6.W2A8 = args.w2a8
    print(f"[suite] w2a8={args.w2a8}")

    m = V6(ROOT / args.model)
    bpe = None
    if getattr(m, "tok_type", 0) == 1 and getattr(m, "tok_blob", b""):
        bpe = BPE.from_bpb1(m.tok_blob)
        print(f"[suite] tok embutido BPB1: vocab={len(bpe.vocab)} "
              f"bos={bpe.bos} eos={bpe.eos} merges={len(bpe.ranks)}")
    if bpe is None:
        tp = Path(args.tokenizer) if args.tokenizer else ensure_tokenizer(ROOT / args.model)
        bpe = BPE(tp) if tp and tp.is_file() else None
    if bpe is None:
        sys.exit("suite exige tokenizer (texto); sem vocab nao ha como julgar coerencia")
    eos = bpe.eos
    fbd = None if args.no_mask else build_forbidden_ids(bpe, eos)
    print(f"[suite] policy rep={args.repetition_penalty} mask={'off' if fbd is None else f'on({len(fbd)})'}")
    _is_bitnet = bpe.id2.get(bpe.bos, "") == "<|begin_of_text|>"
    _frame = args.frame if args.frame != "auto" else ("bitnet" if _is_bitnet else "falcon")
    print(f"[suite] frame={_frame} (bos={bpe.id2.get(bpe.bos, '?')!r})")

    def frame(prompt):
        if _frame == "bitnet":
            # HF bitnet-b1.58-2B-4T (sem system — mede o modelo, nao o prompt).
            ids = [bpe.bos] + bpe.encode(f"User: {prompt}<|eot_id|>Assistant: ")
        else:
            ids = [bpe.bos] + bpe.encode(f"<|user|>\n{prompt}\n<|assistant|>\n")
        return [i for i in ids if i < m.vocab] or [0]

    results = {}

    _, t1, d1 = run_case(m, bpe, frame("Qual e o seu nome?"), args.max_tokens, eos, "Q1 nome",
                         rep=args.repetition_penalty, fbd=fbd)
    results["Q1"] = not d1 and len(t1.strip()) >= 3

    o2, t2, d2 = run_case(m, bpe, frame("1+1="), 24, eos, "Q2 1+1=",
                         rep=args.repetition_penalty, fbd=fbd)
    has2 = "2" in t2
    eos2 = bool(o2) and o2[-1] == eos
    print(f"contem '2': {has2} eos={eos2}")
    # aritmetica curta: vale conteudo + stop (eos) — a janela de 8 toks
    # reprovaria ate '1+1=2'+eos correto; loop sem eos continua reprovado.
    results["Q2"] = has2 and (eos2 or not d2)

    q3prompt = ('Retorne apenas JSON valido, sem texto extra: '
                '{"resultado": <quanto e 2+2>}')
    o3, t3, _ = run_case(m, bpe, frame(q3prompt), args.max_tokens, eos, "Q3 json",
                         rep=args.repetition_penalty, fbd=fbd)
    isj = looks_like_json(t3)
    d3, why3 = degenerate(o3, bpe, use_piece=False)
    t3l = t3.lower()
    import re as _re
    # "4" standalone (10.56437 contem digito 4 sem SER 4 — falso-positivo
    # documentado; \b4\b exige o valor). "quatro" por extenso vale.
    has4 = (_re.search(r"(?:^|\D)4(?:\D|$)", t3) is not None) or ("quatro" in t3l)
    print(f"looks_like_json: {isj} degenerado-relaxed={d3} ({why3}) contem-4: {has4}")
    results["Q3"] = isj and not d3 and has4

    # Q4: completamento factual canonico ("azul" e a resposta).
    _, t4, d4 = run_case(m, bpe, frame("Complete a frase: O céu é"), 30, eos, "Q4 ceu",
                         rep=args.repetition_penalty, fbd=fbd)
    results["Q4"] = ("azul" in t4.lower()) and not d4

    # Q5: tarefa real do kernel (clima). Vale palavra climatica NOVA
    # (fora do prompt — eco puro nao pontua) + sem degeneracao + folego.
    _wx = {"sol", "chuva", "chove", "nublado", "nublad", "ensolarado", "chuvoso",
           "frio", "calor", "quente", "ceu", "céu", "celsius", "graus", "bom",
           "claro", "tempo"}
    _pq5 = "Como está o tempo hoje?"
    o5, t5, d5 = run_case(m, bpe, frame(_pq5), 40, eos, "Q5 clima",
                         rep=args.repetition_penalty, fbd=fbd)
    _new5 = set(t5.lower().split()) - set(_pq5.lower().split())
    _hit5 = any(w == x or w in x for w in _wx for x in _new5)
    print(f"clima novo: {_hit5}")
    results["Q5"] = _hit5 and not d5 and len(o5) >= 8

    # Q6: saudacao (boot greeting). Vale frase fluente nao-ecoica com folego.
    _pq6 = "Olá, como vai você?"
    o6, t6, d6 = run_case(m, bpe, frame(_pq6), 40, eos, "Q6 saudacao",
                         rep=args.repetition_penalty, fbd=fbd)
    _echo6 = t6.strip().lower() == _pq6.strip().lower()
    print(f"eco: {_echo6}")
    results["Q6"] = (not d6) and (5 <= len(o6) <= 60) and not _echo6

    # Bonus informativo: machine-path (MachineCtx: ids diretos, sem frame).
    # intent/slots sinteticos de vocab real; julga so colapso, sem assert.
    mids = [bpe.bos] + bpe.encode("status")[:4]
    mids = [i for i in mids if i < m.vocab] or [0]
    out4, _, d4 = run_case(m, bpe, mids, 24, eos, "Q3b machine-path (info)")
    print(f"machine-path colapsou: {d4}")

    print("\n== VEREDITO ==")
    ok = True
    for k, v in results.items():
        print(f"{k}: {'COERENTE' if v else 'DEGENERADO'}")
        ok = ok and v
    print("SUITE VERDE" if ok else "SUITE VERMELHA")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
