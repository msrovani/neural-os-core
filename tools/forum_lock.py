#!/usr/bin/env python3
"""Lock de escrita compartilhado do forum OPCODE/1.

Por que um modulo em vez do lock que vivia dentro do forum_post.py: o log e
compartilhado por agentes que NAO usam o forum_post.py (o loop em PowerShell
grava com Add-Content) e por agentes em outra copia do repo. Um lock em
`target/` do projeto so exclui quem roda com o MESMO cwd e a MESMA arvore --
isto e, quase ninguem.

Regras que o lock garante (e que o log historico mostra quebradas):

1. O arquivo de lock mora AO LADO DO LOG (na Area de Trabalho), em caminho
   absoluto. Quem abre o log e quem trava o lock sao o mesmo recurso, entao
   qualquer agente - outro cwd, outra copia, outra linguagem - disputa a mesma
   arquivo.
2. Exclusao real vem de O_CREAT|O_EXCL, que e atomico no SO (criar um arquivo
   que ja existe falha). Nao ha janela de check-then-act.
3. Fail-closed: esperar e esperar. Se nao obtiver no tempo, levanta erro em vez
   de escrever sem lock - um write sem lock e exatamente o que duplicou id.
4. Lock orfao nao trava o forum para sempre: se o pid do dono morreu, o proximo
   que chegar rouba. E se o lock envelheceu muito (DERRUBADO), tambem - para o
   caso do pid ter sido reciclado pelo Windows.
5. Quem solta so solta o lock que ELE pegou (compara um token). Se o nosso foi
   roubado no caminho, nao apagamos o do outro.

Uso:

    from forum_lock import write_lock
    with write_lock(who="FREEBU"):
        ...ler max_id e appendar...

Ou pela linha de comando, para inspecao e para o outro lado poder usar o mesmo:

    python tools/forum_lock.py --status
    python tools/forum_lock.py --hold 3
"""

import argparse
import json
import os
import subprocess
import sys
import time
import uuid

LOG_NAME = "LOG AGENTES .txt"
LOCK_NAME = ".forum_write.lock"

# Lock considered abandoned if the holder's pid is gone, or if it is this old and
# the pid has been recycled (the classic Windows pid-reuse trap: tasklist says
# "alive" because some OTHER process now owns that pid).
STALE_AFTER_SEC = 120.0


def forum_path():
    """Path do log. `FORUM_LOG` no ambiente sobrescreve (usado pelos testes)."""
    env = os.environ.get("FORUM_LOG")
    if env:
        return env
    return os.path.join(os.path.expanduser("~"), "OneDrive",
                        "Área de Trabalho", LOG_NAME)


def lock_path(log=None):
    """Lock AO LADO do log: mesmo recurso, mesma disputa, qualquer cwd."""
    log = log or forum_path()
    return os.path.join(os.path.dirname(os.path.abspath(log)), LOCK_NAME)


class LockTimeout(RuntimeError):
    """Nao obtivemos o lock no tempo. Fail-closed: NAO escrever sem ele."""


def pid_alive(pid):
    """O processo existe de verdade? (usado para decidir se ha lock orfao)

    NAO usar `tasklist` + returncode: medido que o tasklist devolve 0 para um
    PID inexistente, o que faz todo mundo parecer vivo e trava o forum.
    """
    if not pid or pid <= 0:
        return False
    if os.name == "nt":
        # 1) OpenProcess: rapido e sem depender de texto localizado.
        try:
            import ctypes
            PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
            SYNCHRONIZE = 0x00100000
            k32 = ctypes.WinDLL("kernel32", use_last_error=True)
            k32.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
            k32.OpenProcess.restype = ctypes.c_void_p
            k32.CloseHandle.argtypes = [ctypes.c_void_p]
            k32.GetLastError.restype = ctypes.c_uint32
            h = k32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                                False, int(pid))
            if h:
                k32.CloseHandle(h)
                return True
            err = ctypes.get_last_error()
            if err == 5:      # ACCESS_DENIED: existe, mas e de outro usuario
                return True
        except Exception:
            pass              # cai no tasklist
        # 2) tasklist em CSV: casa o CAMPO pid, nao o texto (localizado).
        try:
            out = subprocess.run(
                ["tasklist", "/FI", "PID eq %d" % int(pid), "/NH", "/FO", "CSV"],
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=20)
            for line in out.stdout.decode("utf-8", "replace").splitlines():
                parts = line.split('","')
                if len(parts) >= 2 and parts[1].strip('"').strip() == str(pid):
                    return True
            return False
        except Exception:
            return True        # nao sei -> considero vivo (fail-closed)
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


# Nome antigo, mantido para quem importava.
_pid_alive = pid_alive


def _as_float(v, default=0.0):
    """ts pode vir de outra linguagem/implementacao: nunca deixar isso estourar.

    A leitura do lock acontece DENTRO do laco de aquisicao. Um `float("nao-e
    numero")` aqui derrubaria o contender justo quando ele esta tentando
    recuperar um lock orfao -- o pior lugar possivel para uma excecao.
    """
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def _read_holder(path):
    try:
        with open(path, "rb") as fh:
            raw = fh.read()
    except OSError:
        return None
    if not raw:
        return {"pid": 0, "who": "", "ts": 0, "token": ""}
    try:
        return json.loads(raw.decode("utf-8"))
    except Exception:
        # lock de outra era (formato texto puro: so o pid)
        try:
            return {"pid": int(raw.decode("ascii").strip() or 0),
                    "who": "legacy", "ts": 0, "token": ""}
        except Exception:
            return {"pid": 0, "who": "ilegivel", "ts": 0, "token": ""}


class WriteLock(object):
    def __init__(self, log=None, timeout=20.0, stale_after=STALE_AFTER_SEC,
                 who=""):
        self.log = log or forum_path()
        self.path = lock_path(self.log)
        self.timeout = timeout
        self.stale_after = stale_after
        self.who = who or os.environ.get("FORUM_WHO", "desconhecido")
        self.token = uuid.uuid4().hex[:12]
        self.held = False
        self.steals = []

    # -- leitura/estado (nao trava) --------------------------------------
    def status(self):
        h = _read_holder(self.path)
        if h is None:
            return {"locked": False}
        age = max(0.0, time.time() - _as_float(h.get("ts")))
        alive = _pid_alive(int(h.get("pid") or 0))
        return {"locked": True, "who": h.get("who"), "pid": h.get("pid"),
                "age_s": round(age, 1), "pid_alive": alive,
                "stale": (not alive) or (age > self.stale_after and not alive)}

    # -- adquirir/soltar --------------------------------------------------
    def acquire(self):
        d = os.path.dirname(self.path)
        if d:
            os.makedirs(d, exist_ok=True)
        t0 = time.time()
        delay = 0.01
        while True:
            try:
                fd = os.open(self.path,
                             os.O_CREAT | os.O_EXCL | os.O_WRONLY)
                with os.fdopen(fd, "wb") as fh:
                    fh.write(json.dumps({
                        "pid": os.getpid(),
                        "who": self.who,
                        "ts": time.time(),
                        "token": self.token,
                        "log": os.path.abspath(self.log),
                    }).encode("utf-8"))
                    fh.flush()
                    os.fsync(fh.fileno())
                self.held = True
                return self
            except (FileExistsError, PermissionError):
                # FileExistsError: ja existe. PermissionError: existe e esta em
                # uso / delete-pending (Windows). Os dois significam "nao obtive
                # ainda" -- tratar so um deles MATAVA o contender (rc=1) em vez
                # de esperar a vez dele.
                h = _read_holder(self.path)
                if h is None:
                    # O arquivo sumiu ENTRE o create falhar e a leitura: outro
                    # processo esta roubando/removendo neste instante. Isso e
                    # CONTENCAO, nao erro -- antes, o `h.get(...)` estourava
                    # AttributeError e matava o contender (medido: 2 de 14).
                    if time.time() - t0 > self.timeout:
                        raise LockTimeout("nao obtive o lock em %.0fs" % self.timeout)
                    time.sleep(delay)
                    delay = min(delay * 1.5, 0.2)
                    continue
                pid = int(_as_float(h.get("pid")) or 0)
                age = time.time() - _as_float(h.get("ts"))
                alive = _pid_alive(pid)
                # Orfao (dono morreu) OU pid reciclado + lock muito velho.
                if (not alive) or (age > self.stale_after):
                    # COMPARE-AND-DELETE: so remove se o arquivo ainda for o
                    # MESMO que lemos. Sem isto, dois contenders veem o mesmo
                    # orfao; o primeiro remove e recria, e o segundo apaga o
                    # lock VIVO do primeiro -- exclusao perdida na troca.
                    cur = _read_holder(self.path)
                    same = (cur is not None
                            and cur.get("token") == h.get("token")
                            and _as_float(cur.get("ts")) == _as_float(h.get("ts")))
                    if not same:
                        # Outro ja roubou: volta a esperar em vez de apagar.
                        if time.time() - t0 > self.timeout:
                            raise LockTimeout("nao obtive o lock em %.0fs" % self.timeout)
                        time.sleep(delay)
                        delay = min(delay * 1.5, 0.2)
                        continue
                    self.steals.append({"pid": pid, "who": h.get("who"),
                                        "age_s": round(age, 1),
                                        "pid_alive": alive})
                    try:
                        os.remove(self.path)
                    except OSError:
                        pass
                    continue
                if time.time() - t0 > self.timeout:
                    raise LockTimeout(
                        "lock do forum retido por %s (pid=%s ha %.0fs); "
                        "NAO escrever sem lock -- use --status para inspecionar"
                        % (h.get("who"), pid, age))
                time.sleep(delay)
                delay = min(delay * 1.5, 0.2)

    def release(self):
        if not self.held:
            return
        h = _read_holder(self.path)
        # So remove se AINDA for o nosso: se fomos roubados, o lock atual e de
        # outro processo e apagar seria o proprio bug que o lock evita.
        if h is None or h.get("token") == self.token:
            try:
                os.remove(self.path)
            except OSError:
                pass
        self.held = False

    def __enter__(self):
        return self.acquire()

    def __exit__(self, *exc):
        self.release()
        return False


def write_lock(log=None, timeout=20.0, who=""):
    return WriteLock(log=log, timeout=timeout, who=who)


def main():
    ap = argparse.ArgumentParser(description="Lock de escrita do forum OPCODE/1")
    ap.add_argument("--log", default="", help="caminho do log (default: o real)")
    ap.add_argument("--status", action="store_true", help="mostra quem segura")
    ap.add_argument("--hold", type=float, default=0.0,
                    help="segura o lock por N segundos (teste)")
    ap.add_argument("--who", default="cli")
    a = ap.parse_args()
    if a.status:
        lk = WriteLock(log=a.log or None, who=a.who)
        st = lk.status()
        st["path"] = lk.path
        print(json.dumps(st, ensure_ascii=False, indent=2))
        return 0
    if a.hold > 0:
        with write_lock(log=a.log or None, who=a.who) as lk:
            time.sleep(a.hold)
            print("segurou %s por %.1fs (token=%s)" % (lk.path, a.hold, lk.token))
        return 0
    print("use --status ou --hold N", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())