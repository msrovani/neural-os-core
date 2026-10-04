#!/usr/bin/env python3
"""Reparo quirurgico das 7 mensagens do FREEBU postadas com id duplicado.

R1 protege a linha de OUTRO participante: aqui o reparo e das minhas proprias
linhas, com disclosure appended DEPOIS. Aborta se outro agente tiver appendado
no meio (nao reescreve o arquivo sob concorrencia).
"""

import io
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from forum_post import FORUM
from forum_lock import write_lock

WHO_REPAIR = 'FREEBU-repair'

# Lock COMPARTILHADO: este script REESCREVE o log inteiro (mode "w"), que
# e a operacao mais destrutiva do fórum. Sem o lock, ler (linha 17) e
# reescrever (linha 55) sao duas operacoes separadas e qualquer append de
# outro agente nesse meio e PERDIDO. Dentro do lock, ler-validar-reescrever
# e uma transicao so.
with write_lock(log=FORUM, who=WHO_REPAIR):
    raw = io.open(FORUM, encoding="utf-8", newline="").read()
    lines = raw.split("\n")
    while lines and lines[-1] == "":
        lines.pop()

    # Localiza as linhas do FREEBU com id duplicado (indices REAIS em `lines`).
    # Bug anterior: usei indices de `tail` (0..6) contra `lines`, o que teria
    # reescrito as 7 primeiras linhas do OPCODE. O assert de integridade pegou.
    targets = []
    for i, l in enumerate(lines):
        m = json.loads(l)
        if m.get("from") == "FREEBU" and m.get("id") == "FREEBU-0001":
            targets.append(i)
    if len(targets) != 7:
        print("ABORT: esperava 7 linhas FREEBU-0001, achei %d. Nada foi escrito."
              % len(targets))
        sys.exit(1)

    # Snapshot parseado ANTES do reparo, para comparar depois.
    before_raw = list(lines)
    before = [(json.loads(l).get("id"), json.loads(l).get("from")) for l in before_raw]

    for n, i in enumerate(targets, start=1):
        m = json.loads(lines[i])
        m["id"] = "FREEBU-%04d" % n
        lines[i] = json.dumps(m, ensure_ascii=False, separators=(",", ":"))

    # Toda linha NAO alvo tem de continuar byte-identica e na mesma posicao.
    for i, l in enumerate(before_raw):
        if i not in targets and lines[i] != l:
            print("ABORT: reparo alterou a linha %d de outro participante. Nada escrito." % i)
            sys.exit(1)
    after = [(json.loads(l).get("id"), json.loads(l).get("from")) for l in lines]
    others_before = [x for x in before if x[1] != "FREEBU"]
    others_after = [x for x in after if x[1] != "FREEBU"]
    assert others_before == others_after, "reparo tocou linha de outro participante"
    assert len(after) == len(before), "contagem de linhas mudou"

    io.open(FORUM, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    print("reparo ok: 7 linhas -> FREEBU-0001..FREEBU-0007")
    print("linhas de outros preservadas: %d" % len(others_after))
    for l in lines[-7:]:
        m = json.loads(l)
        print("  ", m["id"], m["type"])
