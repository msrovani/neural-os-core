#!/usr/bin/env python3
"""fix_bitnet_header.py — neural-os-core Sprint 107 Part B #8

Repara .bitnet v4 gerados pelo `write_bitnet()` (BUGADO) de train_gpu_full.py
antes do fix: vocab_size e num_medusa foram escritos como u16 (2 bytes) em vez
de u32 (4 bytes) — o kernel (`crates/neural-kernel/src/cortex.rs::load_model`)
sempre le vocab_size/num_medusa como u32, entao o parse desalinhava e
`[HWEXPERT] parse FAILED` (vocab lido como lixo, ex.: 4194368).

Este script NAO retreina — apenas re-escreve o cabecalho no layout correto
(igual a `train_models_gpu.py::write_header()` / `download_and_train.py::
write_bitnet_header()`) e copia os bytes de pesos (inalterados) apos o novo
cabecalho.

Uso:
    python tools/fix_bitnet_header.py target/hw_expert_v3.bitnet
    python tools/fix_bitnet_header.py target/hw_expert_v3.bitnet target/hw_expert_tf.bitnet
    python tools/fix_bitnet_header.py --dry-run target/hw_expert_v3.bitnet

Saida: sobrescreve o arquivo original (com backup `.bak-header`) por padrao;
use --no-backup para pular o backup, ou --out PATH para escrever em outro lugar
(so valido com um unico arquivo de entrada).
"""
import argparse
import shutil
import struct
import sys
from pathlib import Path

MAGIC = 0xBE11BE11


def read_u16(data, off):
    return struct.unpack_from("<H", data, off)[0], off + 2


def read_u32(data, off):
    return struct.unpack_from("<I", data, off)[0], off + 4


def parse_buggy_header(data):
    """Le um header no layout BUGADO (9x u16 para h,nl,nh,vcb,seq,ff,nkv,qd,medusa)."""
    off = 0
    magic, off = read_u32(data, off)
    if magic != MAGIC:
        raise ValueError(f"magic invalido: {hex(magic)} (esperado {hex(MAGIC)})")
    version, off = read_u16(data, off)
    num_params, off = read_u32(data, off)
    h, off = read_u16(data, off)
    n_l, off = read_u16(data, off)
    n_h, off = read_u16(data, off)
    vcb, off = read_u16(data, off)
    seq, off = read_u16(data, off)
    ff, off = read_u16(data, off)
    nkv, off = read_u16(data, off)
    qd, off = read_u16(data, off)
    medusa, off = read_u16(data, off)
    tie = data[off:off + 4]; off += 4
    tok_type = data[off]; off += 1
    tok_len, off = read_u32(data, off)
    tok = data[off:off + tok_len]; off += tok_len
    layer_features = data[off]; off += 1
    return {
        "version": version, "num_params": num_params, "hidden": h,
        "num_layers": n_l, "num_heads": n_h, "vocab_size": vcb, "max_seq": seq,
        "intermediate_size": ff, "num_kv_heads": nkv, "q_dim": qd,
        "num_medusa": medusa, "tie": tie, "tok_type": tok_type, "tok": tok,
        "layer_features": layer_features, "header_end": off,
    }


def looks_already_fixed(data):
    """Heuristica: tenta ler vocab_size como u32 no offset correto do layout
    CORRETO (14 + 2 = 16). Se o valor bater com uma faixa plausivel (<= 2^20)
    E os bytes em torno formarem um header valido terminando em tok+0x07,
    assumimos que ja esta no formato correto."""
    try:
        off = 4 + 2 + 4  # magic+version+num_params
        _h, off = read_u16(data, off)
        _nl, off = read_u16(data, off)
        _nh, off = read_u16(data, off)
        vs, off = read_u32(data, off)
        if vs > (1 << 20):
            return False
        _ms, off = read_u16(data, off)
        _isz, off = read_u16(data, off)
        _nkv, off = read_u16(data, off)
        _qd, off = read_u16(data, off)
        _medusa, off = read_u32(data, off)
        tie = data[off:off + 4]; off += 4
        if tie not in (b"TIED", b"\x00\x00\x00\x00"):
            return False
        off += 1  # tok_type
        tok_len, off = read_u32(data, off)
        if off + tok_len + 1 > len(data):
            return False
        return True
    except Exception:
        return False


def write_fixed_header(hdr):
    f_out = bytearray()
    f_out += struct.pack("<I", MAGIC)
    f_out += struct.pack("<H", hdr["version"])
    f_out += struct.pack("<I", hdr["num_params"])
    f_out += struct.pack("<H", hdr["hidden"])
    f_out += struct.pack("<H", hdr["num_layers"])
    f_out += struct.pack("<H", hdr["num_heads"])
    f_out += struct.pack("<I", hdr["vocab_size"])       # u32 (fix)
    f_out += struct.pack("<H", hdr["max_seq"])
    f_out += struct.pack("<H", hdr["intermediate_size"])
    f_out += struct.pack("<H", hdr["num_kv_heads"])
    f_out += struct.pack("<H", hdr["q_dim"])
    f_out += struct.pack("<I", hdr["num_medusa"])       # u32 (fix)
    f_out += hdr["tie"]
    f_out += struct.pack("B", hdr["tok_type"])
    f_out += struct.pack("<I", len(hdr["tok"]))
    f_out += hdr["tok"]
    f_out += struct.pack("B", hdr["layer_features"])
    return bytes(f_out)


def fix_file(path: Path, out_path: Path, backup: bool, dry_run: bool):
    data = path.read_bytes()
    if looks_already_fixed(data):
        print(f"[SKIP] {path} — header ja parece correto (vocab_size u32 plausivel)")
        return False
    hdr = parse_buggy_header(data)
    print(f"[{path.name}] buggy header: hidden={hdr['hidden']} layers={hdr['num_layers']} "
          f"heads={hdr['num_heads']} vocab={hdr['vocab_size']} seq={hdr['max_seq']} "
          f"ff={hdr['intermediate_size']} nkv={hdr['num_kv_heads']} qd={hdr['q_dim']} "
          f"medusa={hdr['num_medusa']} tok={hdr['tok']!r} header_end={hdr['header_end']}")
    new_header = write_fixed_header(hdr)
    payload = data[hdr["header_end"]:]
    new_data = new_header + payload
    print(f"  old_header_size={hdr['header_end']}B new_header_size={len(new_header)}B "
          f"(+{len(new_header) - hdr['header_end']}B) payload={len(payload)}B "
          f"total_old={len(data)}B total_new={len(new_data)}B")
    if dry_run:
        print("  [DRY-RUN] nenhum arquivo escrito")
        return True
    if backup and out_path == path:
        bak = path.with_suffix(path.suffix + ".bak-header")
        shutil.copy2(path, bak)
        print(f"  backup: {bak}")
    out_path.write_bytes(new_data)
    print(f"  [OK] escrito {out_path} ({len(new_data)} bytes)")
    return True


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("files", nargs="+", help=".bitnet file(s) a corrigir")
    ap.add_argument("--out", help="Caminho de saida (so com 1 arquivo de entrada)")
    ap.add_argument("--no-backup", action="store_true", help="Nao criar .bak-header")
    ap.add_argument("--dry-run", action="store_true", help="So mostra o diagnostico, nao escreve")
    args = ap.parse_args()

    if args.out and len(args.files) != 1:
        print("[ERRO] --out so pode ser usado com um unico arquivo de entrada")
        sys.exit(1)

    any_fixed = False
    for fp in args.files:
        path = Path(fp)
        if not path.exists():
            print(f"[ERRO] nao encontrado: {path}")
            continue
        out_path = Path(args.out) if args.out else path
        fixed = fix_file(path, out_path, backup=not args.no_backup, dry_run=args.dry_run)
        any_fixed = any_fixed or fixed
    sys.exit(0 if any_fixed or args.dry_run else 1)


if __name__ == "__main__":
    main()
