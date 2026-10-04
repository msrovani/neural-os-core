#!/usr/bin/env python3
# Suite do parser F1.5 sem QEMU: roda tools/f15_parse.ps1 sobre cada fixture e
# confere o veredito esperado E o exit code esperado (o veredito que nao vira
# status de processo volta a ser opiniao - CURAIX-0046/0048).
#
#   python tools\run_f15_fixtures.py
#
# Por que existe: o parser ganhou regra nova (escopo por nome da skill) e a
# unica prova de que ela NAO perdeu sensibilidade sao os negativos - FORJA da
# propria skill tem de continuar reprovando. Um "todos os positivos passam" nao
# distingue uma regra correta de uma regra que nunca reprova nada.
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
D = os.path.join(ROOT, "target", "f15_fixtures")
PARSER = os.path.join(ROOT, "tools", "f15_parse.ps1")
PS = "powershell"

# fixture -> (boot, exit esperado, token esperado na saida, substring obrigatoria)
# boot 0 = nao aplica (so compare).
CASES = [
    # ---- positivos ----
    ("b1_ok", 1, 0, "PASS", ""),
    ("b2_ok", 2, 0, "PASS", ""),
    ("b1_escalate0_noise", 1, 0, "PASS", ""),   # escalate=0 do k-ai nao e escalate
    ("b1_durable0", 1, 0, "PASS", ""),          # durable_unknown=false -> zero legitimo
    ("b1_forja_outro_nome", 1, 0, "PASS", ""),  # FORJA de OUTRA skill nao reprova
    ("b2_hashdiff", 2, 0, "PASS", ""),          # hash so e julgar no Compare
    # ---- negativos ----
    # ---- secao 14: identidade do artefato bootado (fail-closed) ----
    ("b1_imgid_stale", 1, 1, "FALSIFIED", "NAO contem o literal da fonte"),
    ("b1_sem_imgid", 1, 1, "FALSIFIED", "sem identidade do artefato bootado"),
    ("b1_imgid_mudou", 1, 1, "FALSIFIED", "mudou DEPOIS do boot"),
    ("b1_sha_erro", 1, 1, "FALSIFIED", "identidade NAO foi estabelecida"),
    # ---- secao 7: braco de ablacao (o disco de partida e uma variavel) ----
    ("b1_lab_state1", 1, 1, "FALSIFIED", "JA continha a skill do lab"),
    ("b1_sem_ablacao", 1, 1, "FALSIFIED", "sem o registro de ablacao"),
    ("b2_restore1", 2, 1, "FALSIFIED", "experimento rigged"),
    ("b1_unknown_true", 1, 1, "FALSIFIED", "UNKNOWN"),
    ("b1_forja_lab", 1, 1, "FALSIFIED", "escalate/FORJA"),
    ("b1_wrong_name", 1, 1, "FALSIFIED", "act=gen"),
    ("b2_noreuse", 2, 1, "FALSIFIED", "nenhuma linha reuse"),
    ("b2_notfound", 2, 1, "FALSIFIED", "reason=not_found"),
    ("b2_has0", 2, 1, "FALSIFIED", "has_skill=0"),
    ("b2_regen", 2, 1, "FALSIFIED", "regenerou"),
    ("b2_forja", 2, 1, "FALSIFIED", "escalate/FORJA"),
    ("b2_result99", 2, 1, "FALSIFIED", "result=99"),
]

# (log1, log2, exit esperado, substring obrigatoria)
COMPARE = [
    ("b1_ok", "b2_ok", 0, "hash identico"),
    ("b1_ok", "b1_ok", 0, "hash identico"),
    ("b1_ok", "b2_hashdiff", 1, "divergente"),
    ("b1_ok", "b2_notfound", 1, "reason=not_found"),
]


def run(args):
    p = subprocess.run([PS, "-NoProfile", "-File", PARSER] + args,
                       cwd=ROOT, capture_output=True, text=True,
                       errors="replace")
    return p.returncode, (p.stdout or "") + (p.stderr or "")


def main():
    # O gerador mora em tools/ (versionado): em target/ um `git clean` apagava a
    # suite inteira, porque o runner chamava um arquivo que nao estava no repo.
    subprocess.run([sys.executable, os.path.join(ROOT, "tools", "gen_f15_fixtures.py")],
                   cwd=ROOT, check=True, capture_output=True)
    bad = 0
    for name, boot, want_exit, want_tok, want_sub in CASES:
        log = os.path.join(D, name + ".log")
        if not os.path.exists(log):
            print("FALTA   %-20s fixture inexistente" % name)
            bad += 1
            continue
        rc, out = run(["-Log", log, "-Boot", str(boot)])
        why = []
        if rc != want_exit:
            why.append("exit %d != %d" % (rc, want_exit))
        if want_tok not in out:
            why.append("sem token %r" % want_tok)
        if want_sub and want_sub not in out:
            why.append("sem causa %r" % want_sub)
        if why:
            bad += 1
            print("FALHOU  %-20s %s" % (name, "; ".join(why)))
            for l in out.splitlines():
                print("          | " + l)
        else:
            print("ok      %-20s exit=%d" % (name, rc))
    for a, b, want_exit, want_sub in COMPARE:
        rc, out = run(["-Compare", "%s,%s" % (os.path.join(D, a + ".log"),
                                             os.path.join(D, b + ".log"))])
        why = []
        if rc != want_exit:
            why.append("exit %d != %d" % (rc, want_exit))
        if want_sub not in out:
            why.append("sem token %r" % want_sub)
        tag = "%s,%s" % (a, b)
        if why:
            bad += 1
            print("FALHOU  compare %s: %s" % (tag, "; ".join(why)))
        else:
            print("ok      compare %s exit=%d" % (tag, rc))

    total = len(CASES) + len(COMPARE)
    print("\n%d/%d casos OK" % (total - bad, total))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())