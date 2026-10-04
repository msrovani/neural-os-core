#!/usr/bin/env python3
"""Teste de concorrencia do lock compartilhado do forum.

Roda N escritores em Python e M em PowerShell AO MESMO TEMPO, todos disputando
o MESMO arquivo de lock, e prova tres coisas que o log historico mostra quebradas:

  1. NENHUM id duplicado. Todos os escritores Python usam o MESMO prefixo de
     proposito: sem lock, todos leem o mesmo max_id e elegem o mesmo id (foi
     assim que FREEBU-0054 e OPKIMI-0007 nasceram duplicados).
  2. NENHUMA linha rasgada. Cada linha tem de parsear como exatamente 1 objeto.
  3. EXCLUSAO REAL: os intervalos de "estou dentro da secao critica" gravados por
     cada escritor NAO se sobrepoem. Isso e a prova de que o lock e msmutuo --
     nao apenas que o resultado ficou bonito.

Mais dois casos de borda:
  4. Escritor ROGUE (sem lock)pre-grava um id que o proximo writer usaria: o
     post tem de elevar o id em vez de colidir.
  5. Lock ORFAO (dono morreu): tem de ser roubado, nao travar o forum.

    python tools\\test_forum_lock.py
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOLS = os.path.join(ROOT, "tools")
sys.path.insert(0, TOOLS)

PY_WRITER = r'''
import os, sys, json, time
os.environ["FORUM_LOG"] = sys.argv[1]
os.environ["FORUM_WHO"] = sys.argv[2]
sys.path.insert(0, sys.argv[3])
import forum_post
audit = sys.argv[4]
sec = []
def _hook(a, b, who):
    sec.append((a, b))
forum_post.AUDIT_HOOK = _hook
forum_post.post([("status", "t", "py writer %s #%d" % (sys.argv[2], i), {})
                 for i in range(int(sys.argv[5]))])
assert len(sec) == 1, "gancho nao disparou"
json.dump({"start": sec[0][0], "end": sec[0][1]}, open(audit, "w"))
'''

PS_WRITER = r'''
param([string]$Log, [string]$Who, [string]$Tools, [string]$Audit,
      [int]$Count, [int]$Tag)
. (Join-Path $Tools 'forum_lock.ps1')
$msg = @(
  for ($i = 0; $i -lt $Count; $i++) {
    $b = @{v='OPCODE/1';id="PSWRITER-$Tag-$i";from=$Who;ts=(Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ");
          type='status';ref='t';body="ps writer $Tag #$i";data=@{}} | ConvertTo-Json -Compress
    $h = Enter-ForumWriteLock -Log $Log -Who $Who
    $t0 = Get-ForumEpoch
    try {
      $needNl = $false
      if (Test-Path -LiteralPath $Log) {
        $fs = [System.IO.File]::Open($Log, 'Open', 'Read', 'ReadWrite')
        try { if ($fs.Length -gt 0) { [void]$fs.Seek(-1,'End'); if ($fs.ReadByte() -ne 10) { $needNl = $true } } }
        finally { $fs.Dispose() }
      }
      $p = ''; if ($needNl) { $p = "`n" }
      $enc = New-Object System.Text.UTF8Encoding($false)
      [System.IO.File]::AppendAllText($Log, $p + $b + "`n", $enc)
      Start-Sleep -Milliseconds 5
    } finally { Exit-ForumWriteLock -Handle $h }
    Set-Content -LiteralPath $Audit -Value ('{"start":' + $t0 + ',"end":' + (Get-ForumEpoch) + '}')
  }
)
'''


def parse_lines(path):
    """(json validos, ids, linhas que nao sao exatamente 1 objeto)."""
    ok, ids, torn = 0, [], []
    with open(path, "rb") as fh:
        raw = fh.read().decode("utf-8", errors="replace")
    for i, line in enumerate(raw.split("\n"), 1):
        if not line.strip():
            continue
        try:
            obj = json.loads(line)
            ok += 1
            ids.append(obj.get("id"))
        except Exception:
            torn.append((i, line[:90]))
    return ok, ids, torn


def intervals(audit_dir):
    out = []
    for f in os.listdir(audit_dir):
        try:
            with open(os.path.join(audit_dir, f)) as fh:
                d = json.load(fh)
            out.append((d["start"], d["end"], f))
        except Exception:
            pass
    return sorted(out)


def overlaps(iv):
    bad = []
    for (s1, e1, a), (s2, e2, b) in zip(iv, iv[1:]):
        if s2 < e1 - 1e-6:
            bad.append("%s [%.4f..%.4f] cruza %s [%.4f..%.4f]" % (a, s1, e1, b, s2, e2))
    return bad


def main():
    tmp = tempfile.mkdtemp(prefix="forumlock_")
    log = os.path.join(tmp, "LOG AGENTES .txt")
    audit = os.path.join(tmp, "audit")
    os.makedirs(audit)
    open(log, "w").close()
    env = dict(os.environ, FORUM_LOG=log)
    failures = []

    pyw = os.path.join(tmp, "w.py")
    open(pyw, "w").write(PY_WRITER)
    psw = os.path.join(tmp, "w.ps1")
    open(psw, "w", newline="\r\n").write(PS_WRITER)

    # --- 1..3: 10 python + 4 powershell, TODOS ao mesmo tempo ---------------
    procs = []
    for i in range(10):
        a = os.path.join(audit, "py%02d.json" % i)
        # mesmo prefixo de proposito: e a corrida que duplica id sem lock
        procs.append((subprocess.Popen(
            [sys.executable, pyw, log, "FREEBU", TOOLS, a, "3"], env=env,
            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE), a))
    for i in range(4):
        a = os.path.join(audit, "ps%02d.json" % i)
        procs.append((subprocess.Popen(
            ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", psw,
             "-Log", log, "-Who", "PSWRITER", "-Tools", TOOLS, "-Audit", a,
             "-Count", "3", "-Tag", str(i)],
            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE), a))

    rcs = []
    for p, a in procs:
        out, err = p.communicate(timeout=300)
        rcs.append((p.returncode, a, err.decode("utf-8", "replace")[-400:]))

    crash = [(rc, a, e) for rc, a, e in rcs if rc != 0]
    ok, ids, torn = parse_lines(log)
    dup = sorted({i for i in ids if ids.count(i) > 1})

    print("linhas validas : %d" % ok)
    print("ids duplicados : %d %s" % (len(dup), dup[:5]))
    print("linhas rasgadas: %d %s" % (len(torn), torn[:2]))
    iv = intervals(audit)
    cross = overlaps(iv)
    print("secoes criticas: %d registros, %d sobreposicoes" % (len(iv), len(cross)))
    for c in cross[:3]:
        print("   CRUZOU: %s" % c)
    for rc, a, e in crash[:3]:
        print("   processo rc=%d %s\n     %s" % (rc, os.path.basename(a), e.strip()[-300:]))

    if crash:
        failures.append("%d processo(s) sairam != 0" % len(crash))
    if dup:
        failures.append("%d id(s) duplicado(s)" % len(dup))
    if torn:
        failures.append("%d linha(s) rasgada(s)" % len(torn))
    if cross:
        failures.append("%d sobreposicao(oes) de secao critica" % len(cross))
    if len(iv) != 14:
        failures.append("esperava 14 registros de secao critica, veio %d" % len(iv))

    # --- 4: escritor ROGUE sem lockinsere o id que o proximo writer usaria ----
    rogue = json.dumps({"v": "OPCODE/1", "id": "FREEBU-9999", "from": "ROGUE",
                        "ts": "2026-01-01T00:00:00Z", "type": "status",
                        "ref": "t", "body": "rogue", "data": {}})
    with open(log, "a", encoding="utf-8", newline="\n") as fh:
        fh.write(rogue + "\n")
    import forum_post
    forum_post.FORUM = log
    forum_post.POST_LOCK = forum_post.lock_path(log)
    forum_post.post([("status", "t", "depois do rogue", {})])
    _, ids2, _ = parse_lines(log)
    reused = [i for i in ids2 if i == "FREEBU-9999"]
    print("rogue: FREEBU-9999 aparece %d vez (esperado 1 = nao reusado)" % len(reused))
    if len(reused) != 1:
        failures.append("id do rogue foi reusado")

    # --- 5: lock orfao (dono morto) tem de ser roubado, nao travar ----------
    lp = forum_post.lock_path(log)
    with open(lp, "w") as fh:
        fh.write(json.dumps({"pid": 999999, "who": "morto", "ts": time.time(),
                             "token": "z"}))
    t0 = time.time()
    import forum_lock
    try:
        with forum_lock.write_lock(log=log, timeout=10, who="teste"):
            got = True
    except Exception as e:
        got = False
        failures.append("lock orfao nao foi roubado: %s" % e)
    dt = time.time() - t0
    print("lock orfao: roubado=%s em %.2fs" % (got, dt))
    if not os.path.exists(lp):
        pass  # solto corretamente
    else:
        failures.append("lock sobrou no disco depois de sair")

    shutil.rmtree(tmp, ignore_errors=True)
    print("")
    if failures:
        for f in failures:
            print("FALHOU: %s" % f)
        return 1
    print("OK: lock compartilhado exclui Python e PowerShell, sem id duplicado")
    return 0


if __name__ == "__main__":
    sys.exit(main())