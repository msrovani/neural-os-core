#!/usr/bin/env python3
"""bitnet_fwd_parity.py — compara forward BitNet kernel vs PyTorch/HF.

Fase 0/1 do plano coerência: carrega o mesmo modelo (ex. 850M), faz 1 forward
pass com HF transformers, e compara top-N logits com dump serial do kernel.

Uso:
  # 1) Gerar dump kernel: boot QEMU com BITNET850.BIN, prompt "ola"
  #    Capturar linha [FWD] logits_top_n=... do serial
  # 2) Comparar:
  python tools/bitnet_fwd_parity.py \
    --model 1bitLLM/bitnet_b1_58-xl \
    --prompt "ola" \
    --kernel-dump "[FWD] logits_top_n=16 ids=[...] logits_bits=[...]"

  # Modo offline: usar dump pré-salvo
  python tools/bitnet_fwd_parity.py --dump-file logs/boot_850_*.txt
"""
from __future__ import annotations

import argparse
import json
import math
import re
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def parse_kernel_dump(text: str) -> dict | None:
    """Extrai logits_top_n do dump serial do kernel."""
    m = re.search(
        r"\[FWD\].*?logits_top_n=(\d+)\s+ids=\[([^\]]+)\]\s+logits_bits=\[([^\]]+)\]",
        text,
    )
    if not m:
        return None
    n = int(m.group(1))
    ids = [int(x.strip()) for x in m.group(2).split(",") if x.strip()]
    bits = [int(x.strip()) for x in m.group(3).split(",") if x.strip()]
    logits_f32 = [struct.unpack("<f", struct.pack("<I", b))[0] for b in bits]
    return {"n": n, "ids": ids[:n], "logits": logits_f32[:n]}


def kernel_dump_from_file(path: Path, prompt: str = "ola") -> dict | None:
    """Procura o primeiro dump [FWD] logits_top no log que corresponde ao prompt."""
    text = path.read_text(encoding="utf-8", errors="replace")
    # Se houver multiplos, pegar o primeiro apos o prompt
    prompt_escaped = re.escape(prompt)
    for block in re.split(rf"prompt='{prompt_escaped}'|Generating for:|LLM-TEST", text):
        d = parse_kernel_dump(block)
        if d:
            return d
    # fallback: primeiro dump no arquivo
    return parse_kernel_dump(text)


# ── HF forward reference ──────────────────────────────────────────────

def hf_forward(model_name: str, prompt: str, device: str = "cpu") -> dict:
    """Carrega modelo HF e retorna top-N logits do primeiro token gerado."""
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer

    tok = AutoTokenizer.from_pretrained(model_name, trust_remote_code=True, use_fast=True)
    model = AutoModelForCausalLM.from_pretrained(
        model_name,
        trust_remote_code=True,
        torch_dtype=torch.float32,
        device_map=device,
    )
    model.eval()
    inputs = tok(prompt, return_tensors="pt")
    with torch.no_grad():
        outputs = model(
            input_ids=inputs["input_ids"],
            attention_mask=inputs.get("attention_mask"),
            return_dict=True,
        )
    logits = outputs.logits[0, -1, :]  # (vocab_size,)
    vocab_size = logits.shape[0]
    top_n = min(64, vocab_size)
    vals, ids = torch.topk(logits, top_n)
    return {
        "model": model_name,
        "prompt": prompt,
        "vocab_size": vocab_size,
        "ids": ids.tolist(),
        "logits": vals.tolist(),
        "input_ids": inputs["input_ids"][0].tolist(),
    }


# ── Comparação ────────────────────────────────────────────────────────

def overlap_pct(kernel: dict, host: dict, k: int = 5) -> float:
    """Overlap nos top-k IDs."""
    k_ids = set(kernel["ids"][:k])
    h_ids = set(host["ids"][:k])
    if not h_ids:
        return 0.0
    common = k_ids & h_ids
    return 100.0 * len(common) / k


def compare(kernel: dict, host: dict) -> dict:
    """Compara dumps kernel vs HF host."""
    out = {
        "kernel_n": kernel["n"],
        "host_n": len(host["ids"]),
        "overlap_top1": overlap_pct(kernel, host, 1),
        "overlap_top5": overlap_pct(kernel, host, 5),
        "overlap_top16": overlap_pct(kernel, host, 16),
    }
    # top-5 side-by-side
    top_k = []
    for i in range(min(5, kernel["n"], len(host["ids"]))):
        ki = kernel["ids"][i] if i < len(kernel["ids"]) else None
        hi = host["ids"][i] if i < len(host["ids"]) else None
        kv = kernel["logits"][i] if i < len(kernel["logits"]) else None
        hv = host["logits"][i] if i < len(host["logits"]) else None
        match = "✓" if (ki is not None and hi is not None and ki == hi) else "✗"
        top_k.append({"rank": i + 1, "kernel_id": ki, "host_id": hi, "kernel_logit": kv, "host_logit": hv, "match": match})
    out["top5_detail"] = top_k
    return out


# ── Dump .bitnet header info (sem forward) ────────────────────────────

def bitnet_header(path: Path) -> dict:
    """Le o header de um .bitnet (v4) sem carregar pesos."""
    data = path.read_bytes()
    magic = data[:4]
    if magic == b"B1TM":
        off = 4
    elif magic[:2] == b"B1":
        off = 2
    else:
        return {"error": f"unknown magic {magic!r}"}
    fmt = "<IIIIIIII"
    hdr = struct.unpack_from(fmt, data, off)
    vocab_size, hidden, num_layers, num_heads, head_dim, ffn_dim, max_seq, num_medusa = hdr
    return {
        "magic": magic.decode("ascii", errors="replace"),
        "vocab_size": vocab_size,
        "hidden": hidden,
        "layers": num_layers,
        "heads": num_heads,
        "head_dim": head_dim,
        "ffn_dim": ffn_dim,
        "max_seq": max_seq,
        "num_medusa": num_medusa,
        "bytes": len(data),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="BitNet FWD parity: kernel vs HF")
    ap.add_argument("--model", default="1bitLLM/bitnet_b1_58-large", help="HF model name")
    ap.add_argument("--prompt", default="ola", help="Prompt para teste")
    ap.add_argument("--kernel-dump", help="Linha [FWD] logits_top do serial (ou texto contendo)")
    ap.add_argument("--dump-file", type=Path, help="Arquivo de log serial para extrair dump")
    ap.add_argument("--bitnet", type=Path, help="Caminho do .bitnet para ler header")
    ap.add_argument("--device", default="cpu", help="device para HF forward")
    ap.add_argument("--json", action="store_true", help="Saída JSON")
    args = ap.parse_args()

    # 1) Extrair dump kernel
    kernel = None
    if args.kernel_dump:
        kernel = parse_kernel_dump(args.kernel_dump)
    elif args.dump_file and args.dump_file.exists():
        kernel = kernel_dump_from_file(args.dump_file, args.prompt)
    if not kernel:
        print("FAIL: kernel dump nao encontrado. Use --kernel-dump ou --dump-file.", file=sys.stderr)
        return 1

    # 2) Header info
    hdr_info = {}
    if args.bitnet and args.bitnet.exists():
        hdr_info = bitnet_header(args.bitnet)
        print(f"[HEADER] {args.bitnet.name}: {json.dumps(hdr_info, indent=2)}")

    # 3) HF forward
    print(f"[HF] forward {args.model} prompt={args.prompt!r} device={args.device}")
    try:
        host = hf_forward(args.model, args.prompt, device=args.device)
    except ImportError as e:
        print(f"FAIL: HF forward indisponivel — {e}", file=sys.stderr)
        print("  pip install torch transformers", file=sys.stderr)
        return 1
    except Exception as e:
        print(f"FAIL: HF forward error — {e}", file=sys.stderr)
        return 1

    # 4) Comparar
    cmp = compare(kernel, host)
    cmp["kernel"] = kernel
    cmp["host"] = {k: host[k] for k in ("model", "prompt", "vocab_size", "input_ids")}
    cmp["host"]["ids"] = host["ids"][:16]
    cmp["host"]["logits"] = host["logits"][:16]
    cmp["bitnet_header"] = hdr_info

    if args.json:
        print(json.dumps(cmp, indent=2, ensure_ascii=False))
    else:
        print(f"\n{'='*60}")
        print(f"  FWD Parity: {args.prompt!r}")
        print(f"{'='*60}")
        print(f"  Overlap top-1:  {cmp['overlap_top1']:.0f}%")
        print(f"  Overlap top-5:  {cmp['overlap_top5']:.0f}%")
        print(f"  Overlap top-16: {cmp['overlap_top16']:.0f}%")
        print(f"\n  Top-5 comparação:")
        print(f"  {'Rank':>4} {'Kernel ID':>10} {'Host ID':>10} {'Kernel logit':>14} {'Host logit':>14}  Match")
        for t in cmp["top5_detail"]:
            print(f"  {t['rank']:>4} {t['kernel_id'] if t['kernel_id'] is not None else '-':>10} "
                  f"{t['host_id'] if t['host_id'] is not None else '-':>10} "
                  f"{t['kernel_logit'] if t['kernel_logit'] is not None else '-':>14.4f} "
                  f"{t['host_logit'] if t['host_logit'] is not None else '-':>14.4f}  {t['match']}")
        print(f"  Kernel IDs: {kernel['ids']}")
        print(f"  Host IDs:   {host['ids'][:16]}")

    gate_pass = cmp["overlap_top5"] >= 80.0
    print(f"\n  GATE Fase 1: {'✅ PASS' if gate_pass else '❌ FAIL'} (top-5 >= 80%)")
    return 0 if gate_pass else 1


if __name__ == "__main__":
    raise SystemExit(main())
