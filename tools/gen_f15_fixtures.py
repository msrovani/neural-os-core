#!/usr/bin/env python3
"""Fixtures de log para testar tools/f15_parse.ps1 sem QEMU.

Incluem o DECOY: a primeira chamada de reload (pre-mount) imprimindo durable=0.

Morava em target/ (gitignored) -- a suite chamava um gerador que nao estava no
repo, entao um `git clean` tirava a suite inteira. Agora e tools/.

Secao 14: o sidecar de identidade aponta para uma imagem DETERMINISTICA criada
aqui (target/f15_fixtures/fake_uefi.img), com bytes/mtime/sha reais lidos do
arquivo. O sidecar de um caso negativo altera um campo para provar que o parser
reprova (falso-negativo e' o que machuca: um veredito PASS sobre imagem errada).
"""
import hashlib
import io
import os
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
D = os.path.join(ROOT, "target", "f15_fixtures")
os.makedirs(D, exist_ok=True)
H = "0x1a2b3c4d5e6f7081"
H2 = "0xdead0000dead0000"
NAME = "oracle_rt_expr_v1"
OUTRO = "hw_pnp_pci_bridge_observe_only_agent_platformage"

PRE = "[T+12000] SKILL ok [skills][ok] reload n=0 model-born=0 template=0 dummy=0 imported=0 reloaded=0 durable=0 durable_unknown=false"
POST_OK = "[T+24000] SKILL ok [skills][ok] reload n=1 model-born=1 template=0 dummy=0 imported=0 reloaded=0 durable=1 durable_unknown=false"
POST_BAD = "[T+24000] SKILL ok [skills][ok] reload n=0 model-born=0 template=0 dummy=0 imported=0 reloaded=0 durable=0 durable_unknown=false"
TICKV = "[T+30000] TICKV ok backend=file"

b1 = [
    PRE,
    "[T+18000] SKILL_LAB ok act=gen name=%s prov=model-born bytes=118 hash=%s" % (NAME, H),
    "[T+18001] SKILL_LAB ok run name=%s a=6 b=7 result=52" % NAME,
    POST_OK, TICKV,
]
b2 = [
    PRE,
    "[T+18000] SKILL_LAB ok reuse name=%s has_skill=1 hash=%s" % (NAME, H),
    "[T+18001] SKILL_LAB ok act=reuse name=%s a=9 b=2 result=82" % NAME,
    POST_OK, TICKV,
]

# Linhas REAIS do boot 1 que reprovaram o parser quando ele contava
# palavra-chave no log inteiro. O arrow e UTF-8 de proposito: o PS 5.1 le o log
# como cp1252 e vira mojibake, e o parser tem de seguir imune a isso.
FORJA_OUTRO = [
    "[T+58] [R3] [hermes] [PnP] [ok] - skill '%s' uso rotineiro \u2192 pedido de gera\u00e7\u00e3o WASM (FORJA)" % OUTRO,
    "[T+71] [R3] [hermes] [Skill] [ok] - skill_gen '%s' \u2192 LLM op-IR (FORJA WASM)" % OUTRO,
]

fixtures = {
    "b1_ok": b1,
    "b2_ok": b2,
    # --- negativos que a v1 do parser nao pegava ---
    "b1_escalate0_noise": [PRE,
        "[T+0] [k-ai] [Boot] [ok] - BOOT_OBSERVE:devices=3:...:escalate=0:trust=true",
        "[T+18000] SKILL_LAB ok act=gen name=%s prov=model-born bytes=118 hash=%s" % (NAME, H),
        "[T+18001] SKILL_LAB ok run name=%s a=6 b=7 result=52" % NAME, POST_OK],
    "b2_noreuse": [PRE, POST_OK],                       # skill nao sobreviveu
    "b2_has0": [PRE, "[T+18000] SKILL_LAB ok reuse name=%s has_skill=0 hash=%s" % (NAME, H),
                "[T+18001] SKILL_LAB ok act=reuse name=%s a=9 b=2 result=82" % NAME, POST_OK],
    "b2_hashdiff": [PRE, "[T+18000] SKILL_LAB ok reuse name=%s has_skill=1 hash=%s" % (NAME, H2),
                    "[T+18001] SKILL_LAB ok act=reuse name=%s a=9 b=2 result=82" % NAME, POST_OK],
    "b2_regen": b2 + ["[T+18500] SKILL_LAB ok act=gen name=%s prov=model-born bytes=118 hash=%s" % (NAME, H)],
    "b2_forja": b2 + ["[T+18600] SKILL_LAB warn escalate=gen name=%s err=sandbox" % NAME,
                      "[T+18601] HERMES warn skill_gen_request name=%s" % NAME],
    "b2_result99": [PRE, "[T+18000] SKILL_LAB ok reuse name=%s has_skill=1 hash=%s" % (NAME, H),
                    "[T+18001] SKILL_LAB ok act=reuse name=%s a=9 b=2 result=99" % NAME, POST_OK],
    "b1_unknown_true": [PRE, "[T+18000] SKILL_LAB ok act=gen name=%s prov=model-born bytes=118 hash=%s" % (NAME, H), "[T+18001] SKILL_LAB ok run name=%s a=6 b=7 result=52" % NAME, "[T+24000] SKILL ok [skills][ok] reload n=0 model-born=0 template=0 dummy=0 imported=0 reloaded=0 durable=0 durable_unknown=true"],
    "b1_durable0": [PRE, "[T+18000] SKILL_LAB ok act=gen name=%s prov=model-born bytes=118 hash=%s" % (NAME, H),
                    "[T+18001] SKILL_LAB ok run name=%s a=6 b=7 result=52" % NAME, POST_BAD],
    # --- escopo por nome (a linha que faltava) ---
    # FORJA de OUTRA skill nao reprova o lab: e o caso real do boot 1.
    "b1_forja_outro_nome": b1 + FORJA_OUTRO,
    # ... mas FORJA da skill do LAB reprova (sensibilidade preservada).
    "b1_forja_lab": b1 + ["[T+18600] SKILL_LAB warn escalate=gen name=%s err=sandbox" % NAME,
                          "[T+18601] HERMES warn skill_gen_request name=%s" % NAME],
    # act=gen com o nome ERRADO: escopar por nome nao pode abrir esse buraco.
    "b1_wrong_name": [PRE, "[T+18000] SKILL_LAB ok act=gen name=outra_coisa prov=model-born bytes=118 hash=%s" % H,
                      "[T+18001] SKILL_LAB ok run name=outra_coisa a=6 b=7 result=52", POST_OK],
    # reproducao do boot 2 REAL: o Tickv nao devolveu o blob.
    "b2_notfound": [PRE, "[T+18600] SKILL_LAB warn escalate=reuse name=%s reason=not_found" % NAME, POST_OK],
}

# --- [14] identidade do artefato bootado ------------------------------------
# Imagem deterministica criada aqui: o sidecar aponta para um arquivo que existe
# e cujo bytes/mtime/sha sao lidos de verdade, para o veredito dos casos nao
# depender da checagem de identidade.
FAKE = os.path.join(D, "fake_uefi.img")
_blob = (b"NEURAL-OS-UEFI-FIXTURE\n" * 256)
io.open(FAKE, "wb").write(_blob)
_st = os.stat(FAKE)


def _iso_utc(ts):
    frac = int(round((ts - int(ts)) * 1e7))
    if frac > 9999999:
        frac = 9999999
    return time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime(int(ts))) + ".%07dZ" % frac


def _imgid(restore, lab_state, motivo):
    return [
        "uefi=" + FAKE.replace("\\", "\\"),
        "uefi_bytes=%d" % _st.st_size,
        "uefi_mtime=" + _iso_utc(_st.st_mtime),
        "uefi_epoch=%d" % int(_st.st_mtime),
        "uefi_sha=" + hashlib.sha256(_blob).hexdigest()[:16].upper(),
        "disk=" + os.path.join(D, "fake_disk.raw"),
        "disk_bytes=4096",
        "disk_mtime=" + _iso_utc(_st.st_mtime),
        "probe=SKILL_LAB",
        "probe_na_fonte=True",
        "probe_na_imagem=True",
        "restore=%d" % restore,
        "restore_motivo=" + motivo,
        "disk_lab_state_before=%d" % lab_state,
        "pristine=" + os.path.join(D, "fake_pristine.raw"),
        "pristine_bytes=4096",
    ]


# Boot 1 parte de um disco limpo (restore=1); boot 2 parte do estado que o boot 1
# deixou -- que e justamente o que o experimento precisa provar que persistiu.
IMGID_B1 = _imgid(1, 0, "pristine restaurado e verificado byte a byte antes do boot")
IMGID_B2 = _imgid(0, 1, "herdado do boot 1: e o estado que o experimento precisa persistir")

# Negativos de proposito (o veredito tem de CAIR):
#   b1_imgid_stale  -> probe_na_imagem=False (imagem sem o literal da fonte)
#   b1_sem_imgid    -> nenhum sidecar (veredito sobre artefato desconhecido)
#   b1_imgid_mudou  -> identidade do sidecar NAO bate com o arquivo agora
#                      (medido na s447: sidecar dizia mtime 00:36, o arquivo era
#                      de 00:40 -- o probe lia a imagem nova e "provava" o codigo
#                      novo num log antigo)
#   b1_lab_state1   -> o disco JA tinha a skill antes do boot 1 (o act=gen nao
#                      prova geracao) -- confound REAL medido: em 04/10 o
#                      disk_qemu.raw tinha skill/wasm/oracle_rt_expr_v1
#   b2_restore1     -> restore no boot 2 apaga o estado a persistir (rigged)
#   b1_sem_ablacao  -> sidecar sem restore=/disk_lab_state_before=
IMGID_STALE = [l if not l.startswith("probe_na_imagem") else "probe_na_imagem=False"
               for l in IMGID_B1]
# muda SO o epoch (caminho preferido): a string ISO continua a mesma, entao o
# caso prova que o epoch e o que decide e nao um detalhe cosmetico.
IMGID_MUDOU = ["uefi_epoch=946684800" if l.startswith("uefi_epoch") else l for l in IMGID_B1]
IMGID_LAB1 = ["disk_lab_state_before=1" if l.startswith("disk_lab_state_before") else l
              for l in IMGID_B1]
IMGID_RESTORE_B2 = ["restore=1" if l.startswith("restore=") else l for l in IMGID_B2]
IMGID_SEM_ABLACAO = [l for l in IMGID_B1
                     if not l.startswith(("restore=", "restore_motivo=", "disk_lab_state_before="))]
# imagem travada por outro QEMU/build no momento do carimbo (medido na s448:
# Get-FileHash falha com "usado por outro processo"). O launcher grava
# uefi_sha=ERRO:lido-em-uso e o parser tem de reprovar: identidade nao estabelecida.
IMGID_SHA_ERRO = ["uefi_sha=ERRO:lido-em-uso" if l.startswith("uefi_sha") else l
                  for l in IMGID_B1]

SIDECAR = {"b1_imgid_stale": IMGID_STALE, "b1_sem_imgid": None,
           "b1_imgid_mudou": IMGID_MUDOU, "b1_lab_state1": IMGID_LAB1,
           "b2_restore1": IMGID_RESTORE_B2, "b1_sem_ablacao": IMGID_SEM_ABLACAO,
           "b1_sha_erro": IMGID_SHA_ERRO}
fixtures["b1_imgid_stale"] = fixtures["b1_ok"]
fixtures["b1_sem_imgid"] = fixtures["b1_ok"]
fixtures["b1_imgid_mudou"] = fixtures["b1_ok"]
fixtures["b1_lab_state1"] = fixtures["b1_ok"]
fixtures["b2_restore1"] = fixtures["b2_ok"]
fixtures["b1_sem_ablacao"] = fixtures["b1_ok"]
fixtures["b1_sha_erro"] = fixtures["b1_ok"]


def sidecar_for(name):
    if name in SIDECAR:
        return SIDECAR[name]
    return IMGID_B1 if name.startswith("b1") else IMGID_B2


for name in sorted(fixtures):
    lines = fixtures[name]
    p = os.path.join(D, name + ".log")
    io.open(p, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    sc = sidecar_for(name)
    if sc is not None:
        io.open(os.path.splitext(p)[0] + ".imgid", "w", encoding="utf-8",
                newline="\n").write("\n".join(sc) + "\n")
    print("%-20s %d linhas%s" % (name, len(lines),
          "  [SEM imgid]" if sc is None else ""))
print("fixtures em %s (imagem de identidade: %s, %d bytes)" % (D, FAKE, _st.st_size))