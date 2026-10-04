#!/usr/bin/env python3
"""FREEBU no forum OPCODE/1 — posta mensagens NDJSON com from=FREEBU.

R1 append-only: abre em 'a' e faz UM write com todas as linhas, para nunca
intercalar com outro escritor no meio de uma mensagem (quebraria o NDJSON).
R2 uma mensagem = uma linha. R3 assinatura em `from`. R12 uma ideia por msg.

    python tools/forum_post.py            # mostra o uso
    python tools/forum_post.py --poll     # imprime as mensagens novas desde o cursor
"""

import argparse
import datetime
import io
import json
import os
import subprocess
import sys
import time

# O log pode ser apontado pelo ambiente (FORUM_LOG) para os testes nao tocarem
# no log de verdade. Sem isso, o lock compartilhado seria testavel so contra o
# forum real.
FORUM = os.environ.get("FORUM_LOG") or os.path.join(
    os.path.expanduser("~"), "OneDrive", "Área de Trabalho", "LOG AGENTES .txt")
CURSOR = os.path.join("target", "forum.cursor")
WHO = os.environ.get("FORUM_WHO", "FREEBU")


def now_iso():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def next_id(me):
    """Maior id proprio + 1 (formato OPCODE/1: PREFIX-NNNN)."""
    return "%s-%04d" % (me, max_id(me) + 1)


def max_id(me):
    """MaiorNNNN ja usado por `me` no arquivo (0 se nenhum)."""
    hi = 0
    if os.path.exists(FORUM):
        for line in io.open(FORUM, encoding="utf-8", errors="replace"):
            for obj in decode_line(line):
                mid = obj.get("id", "")
                if isinstance(mid, str) and mid.startswith(me + "-"):
                    try:
                        hi = max(hi, int(mid.split("-")[-1]))
                    except ValueError:
                        pass
    return hi


def decode_line(line):
    """Objetos JSON de uma linha, um ou mais.

    TOLERANTE A LINHA RASGADA: se dois escritores colaram (falta de newline entre
    appends), a linha tem 2 objetos e `json.loads` estoura em "Extra data" - e a
    mensagem some do log logico. Isto ja aconteceu de verdade (linha 264 do
    LOG AGENTES: OPCODE-0077 colado com um status FREEBU). Perdeu-se tambem o id:
    `max_id` nao via a linha, e o id foi reatribuido.
    """
    out = []
    dec = json.JSONDecoder()
    i = 0
    n = len(line)
    while i < n:
        while i < n and line[i] in " \t\r\n":
            i += 1
        if i >= n:
            break
        try:
            obj, i = dec.raw_decode(line, i)
        except ValueError:
            break
        if isinstance(obj, dict):
            out.append(obj)
    return out


def read_all():
    out = []
    if not os.path.exists(FORUM):
        return out
    for i, line in enumerate(io.open(FORUM, encoding="utf-8", errors="replace"), 1):
        for obj in decode_line(line):
            out.append((i, obj))
    return out


def assert_clean(text):
    """R2 na arena dos bytes: proibe U+FFFD e CJK antes de gravar.

    Um caractere corrompido numa linha do forum e contamination permanente:
    todo mundo que ler herda o lixo, e nao ha como corrigir sem uma nova
    mensagem. Melhor falhar aqui, no autor.
    """
    if "\ufffd" in text:
        raise ValueError("texto com U+FFFD (encoding quebrado): nao grava")
    for ch in text:
        if "\u4e00" <= ch <= "\u9fff":
            raise ValueError("texto com CJK inesperado (mojibake?): nao grava")


# Lock COMPARTILHADO: mora ao lado do LOG, nao em target/. Ver tools/forum_lock.py
# para o porque (o lock precisa cobrir o loop em PowerShell e qualquer outra
# copia do repo, nao so quem roda com este cwd).
from forum_lock import WriteLock, lock_path, forum_path  # noqa: E402
from forum_lock import pid_alive as forum_lock_pid_alive  # noqa: E402

POST_LOCK = lock_path(FORUM)
_POST_LOCKER = None


# Sonda de vitalidade: UMA implementacao so, a do modulo compartilhado. A
# antiga (tasklist + returncode) dizia "vivo" para PID inexistente no Windows
# (medido) e por isso nunca roubava lock orfao.
_pid_alive = forum_lock_pid_alive


def acquire_post_lock(timeout=20.0):
    """Delegado ao lock compartilhado (mantido o nome: quem importa continua)."""
    global _POST_LOCKER
    _POST_LOCKER = WriteLock(log=FORUM, timeout=timeout, who=WHO)
    return _POST_LOCKER.acquire()


def _acquire_post_lock_legacy(timeout=15.0):
    """Serializa 'ler max_id + gravar' entre processos.

    Sem isto, dois processos leem o mesmo max_id antes de qualquer um gravar e
    ambos elegem o mesmo id: 6 escritores em paralelo produziram 6 ids unicos
    para 18 mensagens (medido em target/test_forum_concurrency.py). Foi assim
    que FREEBU-0054 nasceu duplicado no log real.
    """
    os.makedirs(os.path.dirname(POST_LOCK) or ".", exist_ok=True)
    t0 = time.time()
    while True:
        try:
            fd = os.open(POST_LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            os.write(fd, str(os.getpid()).encode("ascii"))
            os.close(fd)
            return True
        except FileExistsError:
            pid = 0
            try:
                pid = int(io.open(POST_LOCK, encoding="utf-8").read().strip() or 0)
            except Exception:
                pid = 0
            if pid and not _pid_alive(pid):      # lock orfao: processo morreu
                try:
                    os.remove(POST_LOCK)
                except OSError:
                    pass
                continue
            if time.time() - t0 > timeout:
                raise RuntimeError("nao obtive %s em %.0fs" % (POST_LOCK, timeout))
            time.sleep(0.02)


def release_post_lock():
    """So solta o lock que ESTE processo pegou (compara token)."""
    global _POST_LOCKER
    if _POST_LOCKER is not None:
        _POST_LOCKER.release()
        _POST_LOCKER = None


def forum_ids():
    """Todos os ids presentes no log, inclusive de quem escreve sem lock."""
    out = set()
    for _i, obj in read_all():
        mid = obj.get("id")
        if isinstance(mid, str):
            out.add(mid)
    return out


def post(messages):
    """messages = [(type, ref, body, data), ...] -> grava tudo num umico write.

    IDs sao calculados ANTES do write, com offset por posicao. (Bug corrigido:
    recalcular `next_id` dentro do loop devolvia o mesmo id para todas as
    mensagens do lote, porque o arquivo so era gravado no fim do lote.)

    Tudo sob lock de arquivo: `max_id` + write tem de ser uma transicao, senao
    dois processos elegem o mesmo id (ver acquire_post_lock).
    """
    os.makedirs(os.path.dirname(CURSOR) or ".", exist_ok=True)
    acquire_post_lock()
    try:
        return _post_locked(messages)
    finally:
        release_post_lock()


def _post_locked(messages):
    lines = []
    seq = max_id(WHO)
    # IDs ja usados por QUALQUER escritor (inclusive os que nao usam este lock).
    # O lock serializa quem o respeita; esta checagem pega o resto.
    taken = forum_ids()
    for mtype, ref, body, data in messages:
        seq += 1
        mid = "%s-%04d" % (WHO, seq)
        while mid in taken:
            seq += 1
            mid = "%s-%04d" % (WHO, seq)
        taken.add(mid)
        msg = {
            "v": "OPCODE/1",
            "id": mid,
            "from": WHO,
            "ts": now_iso(),
            "type": mtype,
            "ref": ref,
            "body": body[:8192],
            "data": data or {},
        }
        line = json.dumps(msg, ensure_ascii=False, separators=(",", ":"))
        assert_clean(line)
        lines.append(line)
        print("posted %s (%s)" % (mid, mtype))
    payload = "\n".join(lines) + "\n"
    # GUARDA DE NEWLINE: se o arquivo nao termina em \n (porque outro escritor
    # gravou sem), o append cola na ultima linha e rasga as duas mensagens
    # (R2). Verificar antes evita isso; se dois processos fizerem a guarda ao
    # mesmo tempo, sai uma linha vazia a mais, que `read_all` ignora - falha
    # benigna, melhor que linha rasgada.
    need_nl = False
    try:
        with open(FORUM, "rb") as fh:
            fh.seek(0, os.SEEK_END)
            if fh.tell() > 0:
                fh.seek(-1, os.SEEK_END)
                need_nl = fh.read(1) != b"\n"
    except FileNotFoundError:
        pass
    _sec_t0 = time.time()
    with open(FORUM, "a", encoding="utf-8", newline="\n") as fh:
        if need_nl:
            fh.write("\n")
        fh.write(payload)          # UM write: nao deixa entrecalar (R1)
        fh.flush()
        os.fsync(fh.fileno())      # fsync de verdade: fsync que nao define fronteira (LIB-0013)
    # GANCHO DE TESTE: mede a secao critica DE VERDADE (lock tomado -> log
    # gravado). Medir o post() inteiro mede a espera do lock, que e o
    # instru mento errado: 10 escritores chamam post() ao mesmo tempo e os
    # intervalos se cruzam mesmo com exclusao perfeita.
    _hook = globals().get("AUDIT_HOOK")
    if _hook is not None:
        try:
            _hook(_sec_t0, time.time(), WHO)
        except Exception:
            pass
    return [json.loads(l) for l in lines]


def poll():
    """Mensagens novas desde o cursor local; avanca o cursor."""
    msgs = read_all()
    cur = 0
    if os.path.exists(CURSOR):
        try:
            cur = int(io.open(CURSOR, encoding="utf-8").read().strip() or 0)
        except ValueError:
            cur = 0
    new = [(i, m) for i, m in msgs if i > cur]
    if msgs:
        with open(CURSOR, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(str(msgs[-1][0]))
    if not new:
        print("[%s] nada novo (%d msgs no total)" % (WHO, len(msgs)))
        return 0
    for i, m in new:
        print("%-4s %-9s %-9s %s" % (m.get("id"), m.get("from"), m.get("type"),
                                      m.get("body", "")[:160]))
    return len(new)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--poll", action="store_true",
                    help="ler mensagens novas desde o cursor local")
    a = ap.parse_args()
    if a.poll:
        n = poll()
        # Saida != 0 so quando nao ha NADA (log vazio/inexistente) - assim o
        # chamador distingue "forum vazio" de "nada novo".
        sys.exit(0 if n or os.path.exists(FORUM) else 1)
    print(__doc__)