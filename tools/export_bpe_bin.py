#!/usr/bin/env python3
"""Exporta vocab HF compacto (id→UTF-8) para o kernel BitNet 2B.

Formato BPB1 (little-endian):
  magic: b"BPB1"
  version: u16 = 1
  bos: u32
  eos: u32
  eot: u32          # <|eot_id|> (128009) — fim de turno chat
  vocab_n: u32      # tipicamente 128256
  offsets: vocab_n × u32  (byte offset no heap; sentinel final = heap_len)
  heap: bytes UTF-8 concatenados

Não inclui merges (encode full BPE fica para depois). Decode + encode
aproximado (greedy over pieces) ou prompts pré-tokenizados no kernel.
"""
from __future__ import annotations

import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOK_JSON = ROOT / "target" / "tokenizer.json"
OUT = ROOT / "target" / "bpe_vocab.bin"


def main() -> int:
    src = Path(sys.argv[1]) if len(sys.argv) > 1 else TOK_JSON
    dst = Path(sys.argv[2]) if len(sys.argv) > 2 else OUT
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
    bos = next((i for i, s in id2.items() if s == "<|begin_of_text|>"), 128000)
    eos = next((i for i, s in id2.items() if s == "<|end_of_text|>"), 128001)
    eot = next((i for i, s in id2.items() if s == "<|eot_id|>"), 128009)

    pieces: list[bytes] = []
    offsets: list[int] = []
    off = 0
    for i in range(vocab_n):
        offsets.append(off)
        raw = id2.get(i, "")
        # HF BPE: Ġ (U+0120) = espaço no início da peça
        raw = raw.replace("\u0120", " ")
        b = raw.encode("utf-8", errors="replace")
        if len(b) > 65535:
            b = b[:65535]
        pieces.append(b)
        off += len(b)
    offsets.append(off)  # sentinel

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

    size = dst.stat().st_size
    print(f"OK {dst} size={size} ({size/1024:.1f}KB) vocab_n={vocab_n} bos={bos} eos={eos} eot={eot}")
    # sanity: weather-ish pieces
    for tid in (24108, 30081, 39298, 1788):
        s = id2.get(tid, "?")
        print(f"  id {tid} = {s!r}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
