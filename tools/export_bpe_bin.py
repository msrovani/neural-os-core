#!/usr/bin/env python3
"""Exporta vocab HF compacto (id→UTF-8) para o kernel BitNet.

Formato BPB1 (little-endian):
  magic: b"BPB1"
  version: u16 = 1
  bos: u32
  eos: u32
  eot: u32
  vocab_n: u32
  offsets: (vocab_n+1) × u32
  heap: bytes UTF-8 concatenados
  [opcional SP32] magic b"MRG1" + merge_n u32 + merges:
      for each: len_a u16, a bytes, len_b u16, b bytes

Modos:
  default / --llama : BitNet 2B Llama-3 (Ġ→espaço; bos/eos/eot 128000+)
  --sp32            : BitNet 850/1.3/3B SentencePiece BPE 32k (mantém ▁ + MRG1)

Uso:
  python tools/export_bpe_bin.py
  python tools/export_bpe_bin.py --sp32
"""
from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOK_JSON = ROOT / "target" / "tokenizer.json"
OUT = ROOT / "target" / "bpe_vocab.bin"
SP32_DEFAULT_SRC = ROOT / "target" / "hf_cache" / "1bitLLM__bitnet_b1_58-xl" / "tokenizer.json"
SP32_DEFAULT_DST = ROOT / "target" / "bpe_vocab_sp32.bin"


def export(src: Path, dst: Path, *, sp32: bool) -> int:
    if not src.is_file():
        print(f"FAIL: missing {src}", file=sys.stderr)
        return 1

    tok = json.loads(src.read_text(encoding="utf-8"))
    vocab = tok["model"]["vocab"]
    id2: dict[int, str] = {}
    for s, i in vocab.items():
        id2[int(i)] = s
    for a in tok.get("added_tokens", []):
        id2[int(a["id"])] = a["content"]

    vocab_n = max(id2.keys()) + 1
    if sp32:
        bos = next((i for i, s in id2.items() if s == "<s>"), 1)
        eos = next((i for i, s in id2.items() if s == "</s>"), 2)
        eot = eos
    else:
        bos = next((i for i, s in id2.items() if s == "<|begin_of_text|>"), 128000)
        eos = next((i for i, s in id2.items() if s == "<|end_of_text|>"), 128001)
        eot = next((i for i, s in id2.items() if s == "<|eot_id|>"), 128009)

    pieces: list[bytes] = []
    offsets: list[int] = []
    off = 0
    for i in range(vocab_n):
        offsets.append(off)
        raw = id2.get(i, "")
        if not sp32:
            raw = raw.replace("\u0120", " ")
        b = raw.encode("utf-8", errors="replace")
        if len(b) > 65535:
            b = b[:65535]
        pieces.append(b)
        off += len(b)
    offsets.append(off)  # sentinel

    merges_raw = tok["model"].get("merges") or [] if sp32 else []
    merge_pairs: list[tuple[bytes, bytes]] = []
    for m in merges_raw:
        if isinstance(m, str):
            a, b = m.split(" ", 1)
        else:
            a, b = m[0], m[1]
        ab = a.encode("utf-8")
        bb = b.encode("utf-8")
        if len(ab) > 65535 or len(bb) > 65535:
            continue
        merge_pairs.append((ab, bb))

    dst.parent.mkdir(parents=True, exist_ok=True)
    with dst.open("wb") as f:
        f.write(b"BPB1")
        f.write(struct.pack("<H", 1))
        f.write(struct.pack("<I", bos))
        f.write(struct.pack("<I", eos))
        f.write(struct.pack("<I", eot))
        f.write(struct.pack("<I", vocab_n))
        for o in offsets:
            f.write(struct.pack("<I", o))
        for p in pieces:
            f.write(p)
        if sp32 and merge_pairs:
            f.write(b"MRG1")
            f.write(struct.pack("<I", len(merge_pairs)))
            for a, b in merge_pairs:
                f.write(struct.pack("<H", len(a)))
                f.write(a)
                f.write(struct.pack("<H", len(b)))
                f.write(b)

    size = dst.stat().st_size
    mode = "sp32" if sp32 else "llama"
    print(
        f"OK [{mode}] {dst} size={size} ({size/1024:.1f}KB) "
        f"vocab_n={vocab_n} bos={bos} eos={eos} eot={eot} merges={len(merge_pairs)}"
    )
    if sp32:
        for tid in (1, 2, 288, 433, 18600, 15043):
            print(f"  id {tid} = {id2.get(tid, '?')!r}")
    else:
        for tid in (24108, 30081, 39298, 1788):
            print(f"  id {tid} = {id2.get(tid, '?')!r}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="Export BPB1 vocab for Neural OS BitNet")
    ap.add_argument("src", nargs="?", help="tokenizer.json path")
    ap.add_argument("dst", nargs="?", help="output .bin path")
    ap.add_argument(
        "--sp32",
        action="store_true",
        help="SentencePiece BPE 32k (BitNet 850/xl/3B); keeps U+2581 + MRG1",
    )
    args = ap.parse_args()
    if args.sp32:
        src = Path(args.src) if args.src else SP32_DEFAULT_SRC
        dst = Path(args.dst) if args.dst else SP32_DEFAULT_DST
    else:
        src = Path(args.src) if args.src else TOK_JSON
        dst = Path(args.dst) if args.dst else OUT
    return export(src, dst, sp32=args.sp32)


if __name__ == "__main__":
    raise SystemExit(main())
