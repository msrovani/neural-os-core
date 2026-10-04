#!/usr/bin/env python3
"""FREEBU - rotina do forum: 1 ciclo por minuto, com trabalho de verdade.

    python tools/forum_watch.py --once          # um ciclo
    python tools/forum_watch.py --watch 60      # 60 ciclos de 60s

LIMITE HONESTO (regra do projeto: nao afirmar o que nao e verdade):
o script faz a parte MECANICA. Ele NAO compoe resposta de fundo - isso exige
que o agente seja invocado. O que ele entrega a cada minuto:

  1. DETECCAO   - o que apareceu desde o cursor (id/from/tipo/resumo).
  2. DIGESTO    - uma entrada `status` no fórum com o lote novo, para ninguem
                  precisar reler 160 linhas. So posta se houve novidade
                  (nada de encher o fórum de "silêncio").
  3. INTEGRIDADE- watchdog do próprio forum: id duplicado, linha NDJSON invalida,
                  mojibake (U+FFFD) ou linha de outro participante alterada.
                  Alerta UNA vez por assinatura de problema (nao spamma).
  4. FILA       - tudo que referencia FREEBU vai para target/freebu_pending.md,
                  que e o que eu leio na minha proxima rodada para responder.

O heartbeat NAO substitui resposta: ele so garante que nada passou em descuido.
"""

import argparse
import io
import json
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from forum_post import post, read_all, decode_line, CURSOR

LOCK = os.path.join("target", "forum_watch.lock")
PENDING = os.path.join("target", "freebu_pending.md")
ALERTS = os.path.join("target", "forum_alerts.json")
ALERTED = os.path.join("target", "forum_alerted.json")   # dedup (cap 200, FIFO)
CYCLE_STATUS = os.path.join("target", "forum_cycle.txt")


def io_read(path):
    return io.open(path, encoding="utf-8").read()


def pid_alive(pid):
    """Vitalidade de PID confiavel POR PLATAFORMA.

    `os.kill(pid, 0)` NAO serve no Windows: levanta WinError 87 (parametro
    incorreto) mesmo com o processo vivo, o que faria este lock roubar o lock de
    um watcher em execucao e duplicar heartbeat. La usa-se tasklist.
    """
    if pid <= 0:
        return False
    if os.name == "nt":
        try:
            out = subprocess.run(["tasklist", "/FI", "PID eq %d" % pid, "/NH"],
                                 capture_output=True, text=True, timeout=10).stdout
        except (OSError, subprocess.SubprocessError):
            return False            # nao sabemos => NAO roube o lock
        return str(pid) in out
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def acquire_lock():
    """Lock de instancia unica; lock ORFAO e retomado (restart de sessao)."""
    os.makedirs("target", exist_ok=True)
    for _ in range(2):
        try:
            fd = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            os.write(fd, str(os.getpid()).encode())
            os.close(fd)
            return True
        except FileExistsError:
            try:
                pid = int(io_read(LOCK).strip() or 0)
            except (ValueError, OSError):
                pid = 0
            if pid_alive(pid):
                return False
            print("lock orfao (pid %d nao existe) - tomando de volta" % pid)
            try:
                os.remove(LOCK)
            except OSError:
                pass
    return False


def release_lock():
    try:
        os.remove(LOCK)
    except OSError:
        pass


def cursor_pos():
    if os.path.exists(CURSOR):
        try:
            return int(io_read(CURSOR).strip() or 0)
        except ValueError:
            return 0
    return 0


def check_integrity():
    """Watchdog do forum: o log e append-only e precisa continuar parseavel."""
    import collections
    p = sys.modules["forum_post"].FORUM
    raw = io.open(p, encoding="utf-8", newline="").read().split("\n")
    problems = []
    ids = collections.Counter()
    for i, l in enumerate(raw, 1):
        if not l.strip():
            continue
        # DECODE TOLERANTE: `json.loads` estoura em "Extra data" numa linha que tem
        # 2 mensagens, e o watchdog reportava isso como log quebrado - quando a
        # linha na verdade esta legivel por um reader que decoda objeto a objeto.
        # Pior: contando so o primeiro objeto, o segundo ficava invisivel para o
        # detector de id duplicado (foi assim que FREEBU-0054 passou batido).
        # Agora: 0 objetos = lixo real; >1 = violacao do R2 mas legivel.
        objs = decode_line(l)
        if not objs:
            problems.append(("json", i, "sem objeto JSON recuperavel"))
            continue
        if len(objs) > 1:
            problems.append(("multi_obj", i, "%d mensagens na mesma linha (R2)"
                             % len(objs)))
        for m in objs:
            ids[m.get("id")] += 1
            # Normaliza escapes: um U+FFFD gravado com ensure_ascii=True vira a
            # sequencia literal "\ufffd" no arquivo e escapava da checagem crua.
            # Re-serializar com ensure_ascii=False traz o caractere de volta.
            if "\ufffd" in json.dumps(m, ensure_ascii=False):
                problems.append(("mojibake", i, m.get("id")))
    for mid, n in ids.items():
        if n > 1:
            problems.append(("dup_id", mid, "%d linhas" % n))
    return problems


def already_alerted(sig):
    """Dedup de alerta: 'ja alertei essa assinatura?' (nao spam a cada ciclo).

    Estado em ARQUIVO SEPARADO. Antes este estado vivia em ALERTS, que e o
    artefato dos problemas ATUAIS: o resultado era um arquivo com 4 chaves
    enquanto o contador dizia 2 - ou seja, problema ja resolvido continuava
    'alertado' para sempre e o artefato mentia. Agora:
      ALERTS  = problemas de agora (reescrito a cada ciclo)
      ALERTED = assinaturas ja commendable (dedup, com cap FIFO)
    """
    seen = {}
    if os.path.exists(ALERTED):
        try:
            seen = json.loads(io_read(ALERTED))
        except Exception:
            seen = {}
    if not isinstance(seen, dict):
        seen = {}
    if sig in seen:
        return True
    seen[sig] = 1
    # cap FIFO: o conjunto de assinaturas nao cresce sem limite (higiene de runtime)
    if len(seen) > 200:
        for k in list(seen.keys())[:len(seen) - 200]:
            seen.pop(k, None)
    with open(ALERTED, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(json.dumps(seen))
    return False


def queue_pending(new):
    """Tudo que OUTRO me menciona vai para a fila que eu leio na proxima rodada."""
    mine = [(i, m) for i, m in new
            if m.get("from") != "FREEBU" and "FREEBU" in (m.get("ref") or "")]
    if not mine:
        return 0
    with open(PENDING, "a", encoding="utf-8", newline="\n") as fh:
        for i, m in mine:
            fh.write("## %s (linha %d, %s, %s)\n%s\n\n" % (
                m.get("id"), i, m.get("from"), m.get("type"), m.get("body", "")))
    return len(mine)


def cycle(n):
    msgs = read_all()
    cur = cursor_pos()
    raw_new = [(i, m) for i, m in msgs if i > cur]
    # As mensagens do PROPRIO watcher nao sao noticia. Sem esta exclusao o
    # heartbeat se realimenta: posta status -> proximo ciclo ve o status como
    # novidade -> posta outro. E o loop de feedback que a licao s429-lab proibe
    # (e que a CURAIX-0029 apontou: "FREEBU-0014 e 0015 sao heartbeats").
    new = [(i, m) for i, m in raw_new if m.get("from") != "FREEBU"]
    if msgs:
        os.makedirs(os.path.dirname(CURSOR) or ".", exist_ok=True)
        with open(CURSOR, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(str(msgs[-1][0]))

    # (3) integridade primeiro: problema no forum precede qualquer conversa
    problems = check_integrity()
    for kind, a, b in problems:
        sig = "%s|%s|%s" % (kind, a, b)
        if already_alerted(sig):
            continue
        post([("evidence", str(a),
              "Watchdog de integridade do forum: problema %s em %s (%s). O log e "
              "append-only por design; se isso apareceu, alguem reescreveu linha "
              "alheia ou gravou lixo. Correcao = nova mensagem com ref (R1)."
              % (kind, a, b),
              {"watchdog": kind, "target": str(a), "detail": str(b)})])

    queued = queue_pending(new)
    if not new:
        print("[min %02d] silencio (%d msgs | %d msg(s) propria(s) ignorada(s))"
              % (n, len(msgs), len(raw_new) - len(new)))
    else:
        who = ", ".join(sorted({m.get("from", "?") for _, m in new}))
        ids = ",".join(m.get("id", "?") for _, m in new[:6])
        print("[min %02d] %d nova(s) de %s -> %s" % (n, len(new), who, ids))
        for i, m in new:
            print("    %-13s %-9s %s" % (m.get("from"), m.get("type"),
                                         m.get("body", "")[:130]))
        post([("status", ids,
              "Ciclo %d: %d mensagem(ns) nova(s) de %s lidas. Digesto e o que "
              "chega ate o FREEBU; resposta de fundo exige o agente invocado. "
              "ids: %s" % (n, len(new), who, ids),
              {"cycle": n, "new": len(new), "from_agents": who, "ids": ids})])

    # Artefato de problemas ATUAIS, reescrito a cada ciclo (antes acumulava:
    # o arquivo dizia 4 chaves com o contador em 2 - alerta fantasma).
    with open(ALERTS, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(json.dumps({"%s|%s|%s" % p: 1 for p in problems}))

    with open(CYCLE_STATUS, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("ciclo=%d msgs=%d novas=%d proprias_ignoradas=%d fila=%d "
                 "problemas=%d utc=%s\n" % (
                     n, len(msgs), len(new), len(raw_new) - len(new), queued,
                     len(problems), time.strftime("%H:%M:%S", time.gmtime())))
    return len(new)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--once", action="store_true")
    ap.add_argument("--watch", type=int, default=0)
    a = ap.parse_args()

    if a.once:
        cycle(0)
        return 0

    if not acquire_lock():
        print("ABORT: ja existe um watcher FREEBU rodando (%s)." % LOCK)
        return 1
    try:
        for n in range(1, a.watch + 1):
            cycle(n)
            if n < a.watch:
                time.sleep(60)
    except KeyboardInterrupt:
        print("\nwatcher interrompido; lock liberado")
    finally:
        release_lock()
    return 0


if __name__ == "__main__":
    sys.exit(main())