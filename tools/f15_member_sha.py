#!/usr/bin/env python3
"""f15_member_sha.py — identidade de imagem ESP por MEMBER-SET IMUTAVEL (falsificador do drift imgid).

Por que existe (BUFFY-0025, 2026-10-22):
  O logwriter-efi escreve NEURAL/BOOT.LOG + append em BOOT.LOG na ESP a CADA boot
  (design s424/s445 — canal de evidencia metal). O sidecar .imgid do run-f15 guarda
  o sha do CONTAINER uefi.img e o f15_parse revalida pos-boot -> FALSIFIED garantido
  a cada run, sem anomalia real (prova byte-a-byte: 6/8 membros identicos; 317 B de
  diff = so FAT/dir-entries/logs de escrita do proprio guest).

Fix: identidade = sha256 sobre a lista ordenada (path, sha256, size) de TODOS os
membros IMUTAVEIS. Membros MUTAVEIS (canal de evidencia, excluidos do hash):
  - BOOT.LOG         (root da ESP; head reescrito pelo stage-0)
  - NEURAL/**        (BOOT.LOG novo por boot)
Detecta qualquer swap real (KERNEL.ELF/BOOTX64/LIMINE/NSGDB trocado) — o drift de
log nem mascara nem fabrica identidade. Compativel com a §14b (s447): KERNEL.ELF
muda em TODO build de kernel, igual o container mudava.

Uso:
  python tools/f15_member_sha.py identity <img>            # JSON member_sha
  python tools/f15_member_sha.py compare <imgA> <imgB>     # SAME=0 | MISMATCH=1
  python tools/f15_member_sha.py plant-demo <img>          # prova falsificadora:
                                                           # flip em KERNEL.ELF da copia
                                                           # -> MISMATCH (exit 0 = ok)
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import struct
import sys
import tempfile

EFI_PART_TYPE = bytes.fromhex("28732ac11ff8d211ba4b00a0c93ec93b")  # GUID little-endian
MUTABLE_EXACT = {"BOOT.LOG"}
MUTABLE_PREFIX = ("NEURAL/",)


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def find_esp(img: str) -> int:
    """Offset de bytes da particao ESP (GPT)."""
    with open(img, "rb") as f:
        f.seek(512)
        hdr = f.read(92)
        if hdr[0:8] != b"EFI PART":
            raise ValueError("GPT nao encontrado em LBA1")
        p_entries = struct.unpack_from("<Q", hdr, 72)[0]
        n_entries = struct.unpack_from("<I", hdr, 80)[0]
        f.seek(p_entries * 512)
        for _ in range(n_entries):
            ent = f.read(128)
            if ent[0:16] == EFI_PART_TYPE:
                # layout do entry GPT: type GUID@0, unique GUID@16, first LBA@32
                return struct.unpack_from("<Q", ent, 32)[0] * 512
    raise ValueError("Particao ESP nao encontrada")


class Fat32:
    def __init__(self, img: str):
        self.img = img
        self.base = find_esp(img)
        self.f = open(img, "rb")
        self.f.seek(self.base)
        bpb = self.f.read(90)
        self.ssz = struct.unpack_from("<H", bpb, 11)[0]
        rsvd = struct.unpack_from("<H", bpb, 14)[0]
        nfats = bpb[16]
        spf = struct.unpack_from("<I", bpb, 36)[0]
        self.rootclus = struct.unpack_from("<I", bpb, 44)[0]
        if self.ssz == 0 or spf == 0:
            raise ValueError("BPB invalido")
        self.fat0 = self.base + rsvd * self.ssz
        self.data = self.base + (rsvd + nfats * spf) * self.ssz

    def close(self) -> None:
        self.f.close()

    def _fat_entry(self, n: int) -> int:
        self.f.seek(self.fat0 + n * 4)
        return struct.unpack("<I", self.f.read(4))[0] & 0x0FFFFFFF

    def clus_off(self, n: int) -> int:
        return self.data + (n - 2) * self.ssz

    def read_chain(self, n: int) -> bytes:
        out = bytearray()
        seen: set[int] = set()
        while 2 <= n < 0x0FFFFFF8:
            if n in seen:
                raise ValueError("ciclo na cluster chain")
            seen.add(n)
            self.f.seek(self.clus_off(n))
            out += self.f.read(self.ssz)
            n = self._fat_entry(n)
        return bytes(out)

    def _parse_dir(self, blob: bytes, prefix: str, hit) -> None:
        """Percorre dir; hit(path, clus_no, size) para arquivos; recursa em subdirs."""
        i = 0
        pending = ""
        while i + 32 <= len(blob):
            e = blob[i : i + 32]
            i += 32
            if e[0] == 0x00:
                break
            if e[0] == 0xE5:
                pending = ""
                continue
            attr = e[11]
            if attr == 0x0F:  # LFN: 0x40 = primeiro chunk lido (inicia), demais = prepend
                chunk = "".join(
                    e[off : off + 2].decode("utf-16-le", "ignore")
                    for off in (1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28)
                ).rstrip("\x00")
                pending = chunk if (e[0] & 0x40) else chunk + pending
                continue
            base = (pending or e[0:8].decode("ascii", "ignore").rstrip()) + (
                ("." + e[8:11].decode("ascii", "ignore").rstrip()) if e[8:11].strip() else ""
            )
            pending = ""
            if not base or base in (".", ".."):
                continue
            clus_no = struct.unpack_from("<H", e, 26)[0] | (struct.unpack_from("<H", e, 20)[0] << 16)
            size = struct.unpack_from("<I", e, 28)[0]
            path = (prefix + "/" + base).strip("/")
            if attr & 0x10:
                self._parse_dir(self.read_chain(clus_no), path, hit)
            else:
                hit(path, clus_no, size)

    def files(self) -> list[tuple[str, int, int]]:
        """[(path, clus_no, size)] ordenado por path."""
        out: list[tuple[str, int, int]] = []
        self._parse_dir(self.read_chain(self.rootclus), "", lambda p, c, s: out.append((p, c, s)))
        return sorted(out)

    def file_sha(self, clus_no: int, size: int) -> str:
        data = self.read_chain(clus_no)[:size] if (size and clus_no >= 2) else b""
        return _sha(data)


def identity(img: str) -> dict:
    fat = Fat32(img)
    try:
        members = fat.files()
        immutable = sorted(
            (p.upper(), fat.file_sha(c, s), s)
            for (p, c, s) in members
            if p.upper() not in MUTABLE_EXACT and not p.upper().startswith(MUTABLE_PREFIX)
        )
        excluded = sorted(
            p.upper() for (p, _, _) in members
            if p.upper() in MUTABLE_EXACT or p.upper().startswith(MUTABLE_PREFIX)
        )
    finally:
        fat.close()
    blob = json.dumps(immutable, separators=(",", ":")).encode()
    with open(img, "rb") as f:
        container = _sha(f.read())
    return {
        "img": os.path.basename(img),
        "member_sha": _sha(blob),
        "n_members": len(immutable),
        "excluded_mutable": excluded,
        "container_sha": container,
    }


def compare(a: str, b: str) -> int:
    ia, ib = identity(a), identity(b)
    same = ia["member_sha"] == ib["member_sha"]
    print(f"A {a}: member_sha={ia['member_sha'][:16]}.. n={ia['n_members']} container={ia['container_sha'][:16]}..")
    print(f"B {b}: member_sha={ib['member_sha'][:16]}.. n={ib['n_members']} container={ib['container_sha'][:16]}..")
    print(f"container (identidade ANTIGA):  {'SAME' if ia['container_sha'] == ib['container_sha'] else 'MISMATCH'}")
    print(f"member-set (identidade NOVA):   {'SAME' if same else 'MISMATCH'}")
    print("RESULT:", "SAME" if same else "MISMATCH")
    return 0 if same else 1


def plant_demo(img: str) -> int:
    """Prova falsificadora: copia a imagem, flipa 1 byte de KERNEL.ELF, exige MISMATCH."""
    tmp = os.path.join(tempfile.gettempdir(), "f15_plant.img")
    shutil.copyfile(img, tmp)
    try:
        fat = Fat32(tmp)
        kern = [(p, c, s) for (p, c, s) in fat.files() if p.upper().endswith("KERNEL.ELF")]
        if not kern:
            print("KERNEL.ELF nao encontrado", file=sys.stderr)
            fat.close()
            return 2
        p, clus, size = kern[0]
        pos = fat.clus_off(clus) + size // 2
        fat.close()
        with open(tmp, "r+b") as fh:
            fh.seek(pos)
            b = fh.read(1)
            fh.seek(pos)
            fh.write(bytes([b[0] ^ 0xFF]))
        print(f"planted: flip 1 byte em {p} cluster={clus} off={pos}")
        rc = compare(img, tmp)
        ok = rc == 1
        print("PLANT DEMO:", "MISMATCH detectado (ok)" if ok else "FALHOU: plant nao detectado")
        return 0 if ok else 1
    finally:
        try:
            os.remove(tmp)
        except OSError:
            pass


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("mode", nargs="?", choices=["identity", "compare", "plant-demo"], default="identity")
    ap.add_argument("args", nargs="*")
    ns = ap.parse_args()
    try:
        if ns.mode == "identity" and len(ns.args) == 1:
            print(json.dumps(identity(ns.args[0]), indent=2))
            return 0
        if ns.mode == "compare" and len(ns.args) == 2:
            return compare(ns.args[0], ns.args[1])
        if ns.mode == "plant-demo" and len(ns.args) == 1:
            return plant_demo(ns.args[0])
    except (ValueError, OSError) as e:
        print(f"erro: {e}", file=sys.stderr)
        return 2
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
