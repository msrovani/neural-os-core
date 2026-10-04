#!/usr/bin/env python3
"""Braco de ablacao do F1.5: o disco de que o boot parte e uma VARIAVEL.

Tres operacoes, todas imprimindo `key=value` (o launcher consome como sidecar):

    ensure  --disk D --pristine P [--force]
        Cria o snapshot do disco em P **se ele nao existir**, e so aceita uma
        fonte LIMPA: se o disco ja contem a skill do lab, o snapshot seria
        "pristine" com o resultado dentro -- e o boot 1 passaria a provar nada.
        Com --force, aceita mesmo assim e diz que aceitou (nao esconde).

    restore --pristine P --disk D
        Copia P -> D e **verifica byte a byte** (mmap/memcmp). Sem verificacao,
        um restore parcial deixa o boot rodando sobre um disco que ninguem knows.

    scan    --disk D
        Diz se o disco ja contem a skill do lab (dirents FAT32
        `skill/wasm/<nome>.asm` e `skill/wasm_prov/<nome>`). E o falsificador do
        boot 1: se a skill ja estava la antes, `act=gen` nao prova geracao.

    check   --disk D
        Alias de scan + metadados (bytes/mtime/sha16). Diagnostico manual.

Medido neste host: varrer 3 GB custa ~1,6-4,9 s (mmap + find), entao o custo por
boot e irrelevante frente ao orcamento de 900 s. Por que mmap e nao `read()`: 3 GB
na cabem na memoria, e o fatiavel precisaria de 64 MB por vez para na estourar.

Medido no disco real (2026-10-04): `target/disk_qemu.pristine.raw` (22:47) esta
LIMPO; `target/disk_qemu.raw` tem `skill/wasm/oracle_rt_expr_v1` e
`skill/wasm_prov/oracle_rt_expr_v1` -- ou seja, o disco do lab ja foi sujo por um
boot anterior. E exatamente o confound que este braco existe para expor.
"""
import argparse
import mmap
import os
import sys
import time

# Os dois caminhos que o skill_lab grava no volume. Nomes curtos FAT32 (8.3)
# cortam a busca, mas o LFN vem logo em seguida no mesmo dirent: achei o
# caminho completo e uso ele.
LAB_PATHS = (b"skill/wasm/oracle_rt_expr_v1", b"skill/wasm_prov/oracle_rt_expr_v1")
CHUNK = 1 << 26  # 64 MiB


def scan_lab(path):
    """(lab_state, hits, bytes_scanned, seconds). lab_state = 1 se achou."""
    t0 = time.time()
    hits = 0
    total = 0
    if not os.path.exists(path):
        return 0, 0, 0, time.time() - t0
    with open(path, "rb") as fh:
        with mmap.mmap(fh.fileno(), 0, access=mmap.ACCESS_READ) as mm:
            total = len(mm)
            pos = 0
            while pos < total:
                end = min(pos + CHUNK, total)
                block = mm[pos:end]
                for n in LAB_PATHS:
                    if n in block:
                        hits += 1
                pos = end
    return (1 if hits else 0), hits, total, time.time() - t0


def same_bytes(a, b):
    """memcmp por blocos: True se os dois arquivos tem o MESMO conteudo."""
    sa, sb = os.stat(a).st_size, os.stat(b).st_size
    if sa != sb:
        return False, sa, sb
    if sa == 0:
        return True, sa, sb
    with open(a, "rb") as fa, open(b, "rb") as fb:
        ma = mmap.mmap(fa.fileno(), 0, access=mmap.ACCESS_READ)
        mb = mmap.mmap(fb.fileno(), 0, access=mmap.ACCESS_READ)
        try:
            pos = 0
            while pos < sa:
                end = min(pos + CHUNK, sa)
                if ma[pos:end] != mb[pos:end]:
                    return False, sa, sb
                pos = end
        finally:
            ma.close()
            mb.close()
    return True, sa, sb


def cmd_scan(args):
    state, hits, total, sec = scan_lab(args.disk)
    print("disk=%s" % args.disk)
    print("disk_bytes=%d" % (os.stat(args.disk).st_size if os.path.exists(args.disk) else 0))
    print("lab_state=%d" % state)
    print("lab_hits=%d" % hits)
    print("lab_probe=%s" % ",".join(p.decode() for p in LAB_PATHS))
    print("scan_sec=%.2f" % sec)
    return 0


def cmd_check(args):
    cmd_scan(args)
    if os.path.exists(args.disk):
        import hashlib
        h = hashlib.sha256()
        with open(args.disk, "rb") as fh:
            with mmap.mmap(fh.fileno(), 0, access=mmap.ACCESS_READ) as mm:
                for pos in range(0, len(mm), CHUNK):
                    h.update(mm[pos:pos + CHUNK])
        print("disk_sha16=%s" % h.hexdigest()[:16].upper())
    return 0


def cmd_ensure(args):
    if not os.path.exists(args.disk):
        print("ERRO disco ausente: %s" % args.disk)
        return 2
    state, hits, _, _ = scan_lab(args.disk)
    if os.path.exists(args.pristine) and not args.force:
        print("pristine_exists=1 pristine=%s" % args.pristine)
        print("pristine_bytes=%d" % os.stat(args.pristine).st_size)
        pstate, _, _, _ = scan_lab(args.pristine)
        print("pristine_lab_state=%d" % pstate)
        print("aviso=use --force para recriar")
        return 0
    if state == 1 and not args.force:
        print("ERRO o disco de origem JA tem a skill do lab (lab_state=1, %d hits):" % hits)
        print("     um snapshot tirado agora nao e 'pristine' -- o boot 1 passaria a")
        print("     provar nada. Use --force se isso e mesmo o que voce quer medir.")
        return 3
    print("copiando %s -> %s" % (args.disk, args.pristine))
    t0 = time.time()
    with open(args.disk, "rb") as src, open(args.pristine, "wb") as dst:
        while True:
            chunk = src.read(CHUNK)
            if not chunk:
                break
            dst.write(chunk)
    ok, sa, sb = same_bytes(args.disk, args.pristine)
    print("pristine_bytes=%d" % sb)
    print("verified=%d" % (1 if ok else 0))
    print("copy_sec=%.2f" % (time.time() - t0))
    print("force=%d origem_lab_state=%d" % (1 if args.force else 0, state))
    if not ok:
        print("ERRO snapshot divergente do original (%d vs %d bytes)" % (sa, sb))
        return 4
    return 0


def cmd_restore(args):
    if not os.path.exists(args.pristine):
        print("ERRO pristine ausente: %s" % args.pristine)
        return 2
    before, _, _, _ = scan_lab(args.disk) if os.path.exists(args.disk) else (0, 0, 0, 0)
    t0 = time.time()
    with open(args.pristine, "rb") as src, open(args.disk, "wb") as dst:
        while True:
            chunk = src.read(CHUNK)
            if not chunk:
                break
            dst.write(chunk)
    ok, sa, sb = same_bytes(args.pristine, args.disk)
    after, hits, _, _ = scan_lab(args.disk)
    print("restore=1")
    print("disk_bytes=%d" % sb)
    print("verified=%d" % (1 if ok else 0))
    print("lab_state_antes=%d lab_state_depois=%d" % (before, after))
    print("restore_sec=%.2f" % (time.time() - t0))
    if not ok:
        print("ERRO disco restaurado difere do pristine (%d vs %d bytes)" % (sa, sb))
        return 4
    if after == 1:
        # Restore "verificado" e ainda assim com a skill dentro = o PRESTINO nao
        # era limpo. Nao e falha do restore: e falha do snapshot, e precisa dizer.
        print("AVISO o pristine restaurado JA contem a skill do lab: o snapshot nao era limpo")
        return 5
    return 0


def main():
    ap = argparse.ArgumentParser(description="braco de ablacao do F1.5 (disco de partida)")
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("scan", "check"):
        p = sub.add_parser(name)
        p.add_argument("--disk", required=True)
        p.set_defaults(fn=cmd_scan if name == "scan" else cmd_check)
    p = sub.add_parser("ensure")
    p.add_argument("--disk", required=True)
    p.add_argument("--pristine", required=True)
    p.add_argument("--force", action="store_true")
    p.set_defaults(fn=cmd_ensure)
    p = sub.add_parser("restore")
    p.add_argument("--disk", required=True)
    p.add_argument("--pristine", required=True)
    p.set_defaults(fn=cmd_restore)
    a = ap.parse_args()
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main())