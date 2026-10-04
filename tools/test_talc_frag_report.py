#!/usr/bin/env python3
"""s443 — Teste de CONTRATO do formato de telemetria (lição SESSION_411:
"formato de export é contrato com o loader" — o teste tem que usar o MESMO
byte que o produtor emite, não uma cópia da mão).

Este teste extrai o template REAL do slog do kernel
(`crates/hermes/src/hub_triage.rs`), formata uma linha a partir dele e prova
que `tools/talc_frag_report.py` entende a linha: contagem de amostras, veredito
de fragmentação, buckets do histograma e a coerência entre o total de gaps que o
kernel publica e a soma dos buckets.

    python tools/test_talc_frag_report.py     # exit 0 = contrato intacto
"""

import io
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TEMPLATE_RE = r'"(ok talc u\{\}M[^"]*)"'
LOG = os.path.join(ROOT, "target", "_wire_contract.log")


def kernel_template():
    """O template do slog, lido do fonte do kernel (fonte única da verdade)."""
    src = io.open(os.path.join(ROOT, "crates", "hermes", "src", "hub_triage.rs"),
                  encoding="utf-8").read()
    m = re.search(TEMPLATE_RE, src)
    assert m, "template 'ok talc ...' nao encontrado em hub_triage.rs"
    return m.group(1)


def run_tool(path):
    r = subprocess.run(
        [sys.executable, os.path.join(ROOT, "tools", "talc_frag_report.py"), path, "--json"],
        capture_output=True, text=True, cwd=ROOT)
    assert r.returncode == 0, "talc_frag_report falhou: %s" % r.stderr
    return json.loads(r.stdout)


def test_wire_contract():
    fmt = kernel_template()
    assert fmt.count("{}") == 10, (
        "template com %d placeholders (esperado 10: 4 totais + 6 do histograma) "
        "- o kernel e o tool estao fora de sincronia" % fmt.count("{}"))

    # Mesmos numeros usados na s443: 64 gaps, 40 deles de poeira <8KB.
    used, free, largest, gaps = 1200, 5700, 200, 64
    hist = [40, 12, 8, 3, 1, 0]
    assert sum(hist) == gaps, "fixture incoerente: buckets != gaps"
    line = "[HubTriage] info " + fmt.format(used, free, largest, gaps, *hist)

    os.makedirs(os.path.dirname(LOG), exist_ok=True)
    io.open(LOG, "w", encoding="utf-8", newline="\n").write(line + "\n")

    d = run_tool(LOG)
    assert d["amostras"] == 1, "tool leu %d amostras (esperado 1)" % d["amostras"]
    assert d["ultimo_veredito"] == "FRAGMENTADO", (
        "regua de fragmentacao divergiu: %s" % d["ultimo_veredito"])
    got = d["ultimo_histograma"]
    assert got["<8KB"] == 40, "bucket <8KB veio %s" % got["<8KB"]
    assert sum(got.values()) == gaps, (
        "soma dos buckets (%d) != gaps publicado pelo kernel (%d) - o histograma "
        "esta mentindo sobre a distribuicao" % (sum(got.values()), gaps))
    assert abs(d["poeira_frac"] - 40.0 / 64.0) < 1e-9, "fracao de poeira errada"


def test_log_antigo_sem_histograma_ainda_e_lido():
    """Linha pre-s443 (sem `hist=`) nao pode fazer o tool quebrar: e um log
    antigo, nao um erro — so nao tem o dado novo."""
    old = "HubTriage ok talc u100M f6800M lg6800M g1\n"
    io.open(LOG, "w", encoding="utf-8", newline="\n").write(old)
    r = subprocess.run(
        [sys.executable, os.path.join(ROOT, "tools", "talc_frag_report.py"), LOG, "--json"],
        capture_output=True, text=True, cwd=ROOT)
    assert r.returncode == 0, "log pre-s443 deveria ser lido: %s" % r.stderr
    d = json.loads(r.stdout)
    assert d["amostras"] == 1
    assert d["com_histograma"] == 0
    assert "poeira_frac" not in d, "nao pode inventar fracao sem histograma"


def test_log_sem_telemetria_falha_explicitamente():
    io.open(LOG, "w", encoding="utf-8", newline="\n").write("nada aqui\n")
    r = subprocess.run(
        [sys.executable, os.path.join(ROOT, "tools", "talc_frag_report.py"), LOG],
        capture_output=True, text=True, cwd=ROOT)
    assert r.returncode == 1, "sem dados o tool tem que sair != 0 (nao fingir relatorio)"


if __name__ == "__main__":
    tests = [test_wire_contract, test_log_antigo_sem_histograma_ainda_e_lido,
             test_log_sem_telemetria_falha_explicitamente]
    failed = 0
    for t in tests:
        try:
            t()
            print("ok   %s" % t.__name__)
        except AssertionError as e:
            failed += 1
            print("FAIL %s: %s" % (t.__name__, e))
    if os.path.exists(LOG):
        os.remove(LOG)
    print("%d/%d ok" % (len(tests) - failed, len(tests)))
    sys.exit(1 if failed else 0)