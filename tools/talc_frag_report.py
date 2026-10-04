#!/usr/bin/env python3
"""s443 — Relatório de FRAGMENTAÇÃO DO TALC a partir de um log real de runtime.

A telemetria do kernel (s435/s443) publica uma linha por amostragem:

    ok talc u1200M f5700M lg200M g64 hist=40/12/8/3/1/0

`u`=usado `f`=livre `lg`=maior gap `g`=nº de gaps, e `hist` é a contagem de
gaps por faixa: <8KB / 8-64KB / 64-256KB / 256KB-1MB / 1-16MB / >=16MB.

Totais dizem QUE o TALC está fragmentado; o histograma diz DE QUE JEITO — e é
isso que decide a ação certa:
  - muitos gaps pequenos  -> "poeira": memoria livre existe, mas aloc médio falha;
                             comprimir/limpar consumers pequenos resolve;
  - poucos gaps grandes  -> fragmentação benigna; forçar compactação é
                             trabalho jogado fora.

Uso:
    python tools/talc_frag_report.py qemu_serial.log [--csv frag.csv] [--json]

ASCII puro de propósito: o console Windows (cp1252) quebra em `→`/`—`, e um
relatório que não imprime não é dado.
"""

import argparse
import json
import re
import sys

# `hist=a/b/c/d/e/f` só existe a partir da s443; linhas antigas (sem hist) são
# aceitas e reportadas como "sem histograma" em vez de serem descartadas.
LINE_RE = re.compile(
    r"ok talc u(?P<used>\d+)M f(?P<free>\d+)M lg(?P<largest>\d+)M g(?P<gaps>\d+)"
    r"(?: hist=(?P<h>\d+)/(?P<h1>\d+)/(?P<h2>\d+)/(?P<h3>\d+)/(?P<h4>\d+)/(?P<h5>\d+))?"
)

BUCKET_LABELS = ["<8KB", "8-64KB", "64-256KB", "256KB-1MB", "1-16MB", ">=16MB"]


def parse(path):
    """Extrai as amostras de telemetria do log. Tolera log parcial/UTF-8 torto."""
    samples = []
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        for lineno, line in enumerate(fh, 1):
            m = LINE_RE.search(line)
            if not m:
                continue
            hist = None
            if m.group("h") is not None:
                hist = [int(m.group(k)) for k in ("h", "h1", "h2", "h3", "h4", "h5")]
            samples.append({
                "lineno": lineno,
                "used_mb": int(m.group("used")),
                "free_mb": int(m.group("free")),
                "largest_mb": int(m.group("largest")),
                "gaps": int(m.group("gaps")),
                "hist": hist,
            })
    return samples


def classify(s):
    """Veredito de fragmentacao da amostra (mesma regra do kernel: s435)."""
    if s["free_mb"] >= 256 and s["largest_mb"] * 4 < s["free_mb"]:
        return "FRAGMENTADO"
    return "ok"


def dust_ratio(hist):
    """Fracao de gaps na faixa <8KB — a 'poeira' que nao serve a alloc nenhum."""
    if not hist:
        return None
    total = sum(hist)
    return (hist[0] / total) if total else None


def main():
    ap = argparse.ArgumentParser(description="Relatorio de fragmentacao do TALC (s443)")
    ap.add_argument("log", help="log de serial do QEMU (ou qualquer log com linhas 'ok talc')")
    ap.add_argument("--csv", help="exporta as amostras em CSV (para planilha/plot)")
    ap.add_argument("--json", action="store_true", help="saida em JSON")
    args = ap.parse_args()

    samples = parse(args.log)
    if not samples:
        print("Nenhuma linha 'ok talc' encontrada em", args.log, file=sys.stderr)
        print("  - a telemetria so aparece quando o TALC tem claim E a triagem roda",
              file=sys.stderr)
        return 1

    with_hist = [s for s in samples if s["hist"]]
    frag = [s for s in samples if classify(s) == "FRAGMENTADO"]
    worst = max(samples, key=lambda s: s["gaps"])
    smallest = min(samples, key=lambda s: s["largest_mb"])
    last = samples[-1]

    report = {
        "log": args.log,
        "amostras": len(samples),
        "com_histograma": len(with_hist),
        "fragmentadas": len(frag),
        "pior_por_gaps": worst,
        "pior_por_largest": smallest,
        "ultima": last,
        "ultimo_veredito": classify(last),
    }
    if with_hist:
        last_h = with_hist[-1]["hist"]
        report["ultimo_histograma"] = dict(zip(BUCKET_LABELS, last_h))
        report["poeira_frac"] = dust_ratio(last_h)

    if args.json:
        print(json.dumps(report, indent=2))
        return 0

    print("=== TALC fragmentation report (s443) ===")
    print("log            :", args.log)
    print("samples        :", len(samples), "(with histogram:", len(with_hist), ")")
    print("fragmented     :", len(frag))
    print()
    print("worst by gaps  : u%uM f%uM lg%uM g%u  (line %u)"
          % (worst["used_mb"], worst["free_mb"], worst["largest_mb"], worst["gaps"], worst["lineno"]))
    print("worst by lg    : u%uM f%uM lg%uM g%u  (line %u)"
          % (smallest["used_mb"], smallest["free_mb"], smallest["largest_mb"],
             smallest["gaps"], smallest["lineno"]))
    print()
    print("last sample    : u%uM f%uM lg%uM g%u -> %s"
          % (last["used_mb"], last["free_mb"], last["largest_mb"], last["gaps"],
             classify(last)))
    if with_hist:
        last_h = with_hist[-1]["hist"]
        total = sum(last_h) or 1
        print()
        print("last histogram (gap count by size bucket):")
        for label, n in zip(BUCKET_LABELS, last_h):
            bar = "#" * min(40, (n * 40) // total)
            print("  %-10s %6u  %s" % (label, n, bar))
        dr = dust_ratio(last_h)
        print()
        print("dust (<8KB) share: %.1f%%" % (100.0 * dr))
        if dr is not None and dr >= 0.6:
            print("  -> DUST-DOMINANT: many tiny gaps. Free memory exists but a medium")
            print("     alloc keeps failing. Compact/small consumers first (see IDEA #630).")
        elif dr is not None and dr <= 0.2 and last["gaps"] > 8:
            print("  -> FEW BIG GAPS: fragmentation is benign; forcing compaction is wasted")
            print("     work. Look at consumers that pin LARGE chunks instead.")
        else:
            print("  -> MIXED: no single dominant bucket; read the series over time.")
    else:
        print()
        print("(no histogram in this log: it predates s443 -- totals only)")

    if args.csv:
        with open(args.csv, "w", encoding="utf-8", newline="") as fh:
            fh.write("lineno,used_mb,free_mb,largest_mb,gaps,verdict,h0,h1,h2,h3,h4,h5\n")
            for s in samples:
                h = s["hist"] or [""] * 6
                fh.write("%d,%d,%d,%d,%d,%s,%s\n" % (
                    s["lineno"], s["used_mb"], s["free_mb"], s["largest_mb"],
                    s["gaps"], classify(s), ",".join(str(x) for x in h)))
        print()
        print("csv written:", args.csv)

    return 0


if __name__ == "__main__":
    sys.exit(main())