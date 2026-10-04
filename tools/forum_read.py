#!/usr/bin/env python3
"""FREEBU - leitura manual do forum (uso do agente, 1x por minuto).

    python tools/forum_read.py            # imprime o que ha de novo e avanca o cursor

    python tools/forum_read.py --all      # relê tudo, sem mexer no cursor

Cursor PROPRIO (target/freebu_read.cursor), separado do watcher: se dividissem o
mesmo arquivo, um advancing pularia a leitura do outro e o loop perderia
mensagens sem nunca perceber.
"""

import io
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from forum_post import FORUM, decode_line

CURSOR = os.path.join("target", "freebu_read.cursor")


def load():
    lines = [l for l in io.open(FORUM, encoding="utf-8", newline="").read().split("\n")
             if l.strip()]
    out = []
    lixas = 0
    for i, l in enumerate(lines, 1):
        # decode_line: uma linha pode render mais de um objeto (writer rasgado).
        # Com json.loads por linha, essa linha era DESCARTADA inteira - e com
        # ela as mensagens que ninguem ia ler. Falha visivel, nao silenciosa.
        objs = decode_line(l)
        if not objs:
            lixas += 1
            print("LINHA SEM OBJETO JSON %d: %s" % (i, l[:80]))
            continue
        if len(objs) > 1:
            print("LINHA %d: %d mensagens na mesma linha (R2)" % (i, len(objs)))
        for o in objs:
            out.append((i, o))
    if lixas:
        print("ATENCAO: %d linha(s) sem objeto JSON recuperavel" % lixas)
    return out


def main():
    show_all = "--all" in sys.argv
    msgs = load()
    if show_all:
        cur = 0
    else:
        cur = 0
        if os.path.exists(CURSOR):
            try:
                cur = int(io.open(CURSOR, encoding="utf-8").read().strip() or 0)
            except ValueError:
                cur = 0
    new = [(i, m) for i, m in msgs if i > cur]
    if not show_all and msgs:
        os.makedirs(os.path.dirname(CURSOR) or ".", exist_ok=True)
        with open(CURSOR, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(str(msgs[-1][0]))
    if not new:
        print("nada novo (%d msgs no total, cursor %d)" % (len(msgs), cur))
        return 0
    for i, m in new:
        print("[L%d] %-9s %-9s ref=%s" % (i, m.get("from"), m.get("type"), m.get("ref") or "-"))
        print("     %s" % m.get("body", "")[:600])
    print("--- %d nova(s) de %d ---" % (len(new), len(msgs)))
    return 0


if __name__ == "__main__":
    sys.exit(main())