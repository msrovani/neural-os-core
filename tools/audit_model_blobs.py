#!/usr/bin/env python3
"""Auditoria de blobs BitNet em target1/ + models/ vs validate_for_slot (s489).

Espelha cortex::model::{parse_model_header, validate_for_slot} o suficiente
para classificar: Ok / REJECT size_mismatch / not_llm / usable metal|qemu.
"""
from __future__ import annotations

import struct
import sys
from pathlib import Path

MAGIC = 0xBE11BE11
TYPE = {0: "LLM", 1: "HwExpert", 2: "Router"}
ROOT = Path(__file__).resolve().parents[1]


def slot_guess(name: str) -> str:
    n = name.upper()
    if "HWEXPRT" in n or "HW_EXPERT" in n or "HW_EXPERT" in n or n.startswith("HW_EXPERT"):
        return "HwExpert"
    if "HW" in n and "EXPERT" in n:
        return "HwExpert"
    if "RUSTCDR" in n:
        return "RustCoder"
    if "VISION" in n or "SIGLIP" in n or "VIT" in n:
        return "Vision"
    if "RERANK" in n:
        return "Reranker"
    if "AGENT" in n:
        return "Agent"
    if "LEARNER" in n:
        return "Learner"
    if "PRO." in n or "FALCON7" in n or "FALCON3_7" in n:
        return "GeneratorPro"
    if "FALCON10" in n or "FALCON3_10" in n:
        return "GeneratorPro"
    if "FALCON" in n or "BITNET2" in n or n.startswith("BITNET"):
        return "Active"
    if "ROUTER" in n:
        return "Router(MoE)"
    if "PIPER" in n:
        return "TTS(out)"
    if "STT" in n:
        return "STT(out)"
    if "BGE" in n or "E5" in n:
        return "Embed(out)"
    if "BPE" in n or "VOCAB" in n:
        return "Tokenizer"
    return "?"


def parse_v6_size(data: bytes):
    """Port de parse_model_header (só LLM mtype=0). Retorna (size, info) ou (None, why)."""
    if len(data) < 52:
        return None, "trunc"
    ver = struct.unpack_from("<H", data, 4)[0]
    if ver not in (6, 7):
        return None, f"ver={ver}"
    if data[14] != 0:
        return None, f"mtype={data[14]}"
    hidden, layers, heads = struct.unpack_from("<HHH", data, 18)
    vocab = struct.unpack_from("<I", data, 24)[0]
    max_seq, inter, kv, qdim = struct.unpack_from("<HHHH", data, 28)
    medusa = struct.unpack_from("<I", data, 36)[0]
    tie = data[40:44] == b"TIED"
    tok_len = struct.unpack_from("<I", data, 45)[0]
    tok_cap = 8 * 1024 * 1024 if ver == 7 else 65536
    if tok_len > tok_cap:
        return None, "tok_len_cap"
    hdr_end = 49 + tok_len + 3
    if hdr_end > len(data):
        return None, f"need>{hdr_end}"
    embed_type = data[hdr_end - 2]
    feat = data[hdr_end - 1]
    if ver == 7:
        if hdr_end + 2 > len(data):
            return None, "need_v7"
        rl = struct.unpack_from("<H", data, hdr_end)[0]
        if rl > 4096:
            return None, "rl"
        p = hdr_end + 2 + rl + 40
        if p + 2 > len(data):
            return None, "need_v7b"
        cl = struct.unpack_from("<H", data, p)[0]
        if cl > 4096:
            return None, "cl"
        p = p + 2 + cl + 40 + 4
        if p > len(data):
            return None, "need_v7c"
        hdr_end = p
    hd = qdim // max(heads, 1)
    k_dim = kv * hd
    ffn_group = inter * qdim // max(hidden, 1)
    size = hdr_end
    if embed_type == 0:
        size += (hidden * vocab + 3) // 4 + 4
    elif embed_type == 1:
        size += ((hidden * vocab + 255) // 256) * 210 + 4
    else:
        size += hidden * vocab * 2 + 4
    per_layer_norms = hidden * 4 * 2
    if feat & 1:
        per_layer_norms += hidden * 4
    if feat & 2:
        per_layer_norms += inter * 4
    q_term = (hidden * qdim + 3) // 4
    kv_term = (hidden * k_dim + 3) // 4
    o_term = (qdim * hidden + 3) // 4
    ff_term = (hidden * ffn_group + 3) // 4
    down_term = (inter * qdim + 3) // 4
    per_layer_tern = q_term + kv_term * 2 + o_term + ff_term * 2 + down_term + 7 * 4
    size += layers * (per_layer_norms + per_layer_tern)
    size += hidden * 4
    if not tie:
        if feat & 0x08:
            size += ((hidden * vocab + 255) // 256) * 210 + 4
        else:
            size += (hidden * vocab + 3) // 4 + 4
    size += medusa * ((hidden * vocab + 3) // 4 + 4)
    if feat & 4:
        size += 4
    info = {
        "ver": ver,
        "hidden": hidden,
        "L": layers,
        "heads": heads,
        "kv": kv,
        "vocab": vocab,
        "inter": inter,
        "qdim": qdim,
        "tok_len": tok_len,
        "emb": embed_type,
        "feat": feat,
        "tie": tie,
        "max_seq": max_seq,
    }
    return size, info


def read_head(path: Path, want: int) -> bytes:
    with open(path, "rb") as f:
        return f.read(min(path.stat().st_size, want))


def audit(path: Path) -> dict:
    sz = path.stat().st_size
    try:
        rel = str(path.relative_to(ROOT))
    except ValueError:
        rel = str(path)
    mb = round(sz / (1024 * 1024), 2)
    qemu_ok = sz <= 8 * 1024 * 1024
    slot = slot_guess(path.name)
    head = read_head(path, 9 * 1024 * 1024)
    out: dict = {
        "file": rel,
        "mb": mb,
        "bytes": sz,
        "slot_guess": slot,
        "qemu_loo": qemu_ok,
    }
    if len(head) < 4:
        out["slot_ok"] = "too_short"
        out["usable"] = "NO"
        return out
    magic = struct.unpack_from("<I", head, 0)[0]
    if magic != MAGIC:
        out["slot_ok"] = "bad_magic"
        out["magic"] = hex(magic)
        out["usable"] = "NO"
        return out
    ver = struct.unpack_from("<H", head, 4)[0]
    mtype = head[14] if len(head) > 14 else None
    out["ver"] = ver
    out["mtype"] = mtype
    out["mtype_name"] = TYPE.get(mtype, "?")

    if slot == "HwExpert":
        if 1 <= ver <= 5:
            out["slot_ok"] = "Ok(legacy)"
            out["usable"] = "metal+qemu"
        elif ver in (6, 7):
            if mtype == 1:
                out["slot_ok"] = "Ok"
                out["usable"] = "metal+qemu(AlwaysTiny)"
                if len(head) >= 22:
                    h, L = struct.unpack_from("<HH", head, 18)
                    out["shape"] = f"h={h} L={L}"
            elif mtype == 0:
                out["slot_ok"] = "REJECT llm_in_hwexpert"
                out["usable"] = "NO"
            else:
                out["slot_ok"] = "REJECT not_hwexpert"
                out["usable"] = "NO"
        else:
            out["slot_ok"] = "REJECT unsupported"
            out["usable"] = "NO"
        return out

    if slot in ("Vision", "Reranker"):
        if 1 <= ver <= 7:
            out["slot_ok"] = "Ok(magic+ver)"
            out["usable"] = "metal+qemu" if qemu_ok else "metal"
        else:
            out["slot_ok"] = "REJECT"
            out["usable"] = "NO"
        if ver <= 5:
            out["note"] = "v1-5 legacy encoder (load path dedicado)"
        elif ver in (6, 7) and mtype == 0:
            exp, info = parse_v6_size(head)
            if exp is None and isinstance(info, str) and info.startswith("need"):
                head = read_head(path, 16 * 1024 * 1024)
                exp, info = parse_v6_size(head)
            if isinstance(info, dict):
                out["shape"] = f"h={info['hidden']} L={info['L']}"
                out["delta"] = sz - (exp or 0)
                if exp and exp != sz:
                    out["note"] = "size!=LLM probe (Vision/Reranker gate ignora)"
        return out

    if slot in ("Active", "GeneratorPro", "Agent", "RustCoder", "Learner"):
        if 1 <= ver <= 5:
            out["slot_ok"] = "Ok(legacy)"
            out["usable"] = "metal" if not qemu_ok else "metal+qemu"
            out["warn"] = "v1-5: LOO validate_for_slot Ok; loader pode migrar"
            return out
        if ver in (6, 7):
            if mtype != 0:
                out["slot_ok"] = f"REJECT not_llm(mtype={mtype})"
                out["usable"] = "NO"
                return out
            exp, info = parse_v6_size(head)
            if exp is None and isinstance(info, str) and info.startswith("need"):
                head = read_head(path, 16 * 1024 * 1024)
                exp, info = parse_v6_size(head)
            if exp is None:
                out["slot_ok"] = f"REJECT {info}"
                out["usable"] = "NO"
                return out
            out["exp_mb"] = round(exp / (1024 * 1024), 2)
            out["delta"] = sz - exp
            out["shape"] = (
                f"h={info['hidden']} L={info['L']} heads={info['heads']}/{info['kv']} "
                f"vocab={info['vocab']} tok={info['tok_len']}"
            )
            if exp == sz:
                out["slot_ok"] = "Ok(size_match)"
                if qemu_ok:
                    out["usable"] = "metal+qemu"
                elif slot == "RustCoder":
                    out["usable"] = "metal only (QEMU miss)"
                else:
                    out["usable"] = "metal (QEMU: loader/FAT grande)"
            else:
                out["slot_ok"] = "REJECT size_mismatch"
                out["usable"] = "NO"
            return out
        out["slot_ok"] = "REJECT unsupported"
        out["usable"] = "NO"
        return out

    # fora do hub
    out["slot_ok"] = "n/a (fora hub)"
    out["usable"] = "path dedicado"
    if ver in (6, 7) and mtype == 0:
        exp, info = parse_v6_size(head)
        if isinstance(info, dict):
            out["shape"] = f"h={info['hidden']} L={info['L']}"
            if exp:
                out["delta"] = sz - exp
    elif ver in (6, 7) and mtype == 1:
        if len(head) >= 22:
            h, L = struct.unpack_from("<HH", head, 18)
            out["shape"] = f"h={h} L={L}"
        out["note"] = "HwExpert-shaped (ok p/ AlwaysTiny)"
    elif ver in (1, 2, 3, 4, 5):
        out["note"] = f"legacy v{ver}"
    return out


def main() -> int:
    rows = []
    for root_name in ("target1", "models"):
        root = ROOT / root_name
        if not root.is_dir():
            continue
        for p in sorted(root.iterdir()):
            if not p.is_file():
                continue
            su = p.suffix.upper()
            if su not in (".BITNET", ".BIN", ".V6", ".V7", ".GGUF"):
                continue
            try:
                rows.append(audit(p))
            except Exception as e:
                rows.append({"file": str(p), "slot_ok": "ERROR", "usable": "NO", "err": str(e)})

    hdr = f"{'file':<42} {'MB':>8} {'ver':>3} {'type':>8} {'slot':<14} {'validate':<24} usable"
    print(hdr)
    print("-" * len(hdr))
    ok = rej = outhub = 0
    for r in rows:
        v = r.get("ver", "-")
        t = r.get("mtype_name", r.get("status", "?"))
        so = str(r.get("slot_ok", "?"))
        print(
            f"{r.get('file','?'):<42} {r.get('mb',0):>8} {v:>3} {str(t):>8} "
            f"{r.get('slot_guess','?'):<14} {so:<24} {r.get('usable','?')}"
        )
        if r.get("shape"):
            print(f"  shape: {r['shape']}  delta={r.get('delta', 'n/a')}")
        if r.get("warn"):
            print(f"  WARN: {r['warn']}")
        if r.get("note"):
            print(f"  note: {r['note']}")
        if r.get("err"):
            print(f"  ERR: {r['err']}")
        if so.startswith("Ok"):
            ok += 1
        elif so.startswith("REJECT"):
            rej += 1
        else:
            outhub += 1

    print()
    print(f"resumo: Ok={ok} REJECT={rej} n/a|outro={outhub} total={len(rows)}")
    print("QEMU LOO (<=8MB):", ", ".join(r["file"] for r in rows if r.get("qemu_loo") and r.get("slot_ok", "").startswith("Ok")) or "(nenhum hub mid)")
    return 0 if rej == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
