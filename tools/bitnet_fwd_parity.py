#!/usr/bin/env python3
"""bitnet_fwd_parity.py — compara forward BitNet kernel vs PyTorch/HF.

Fase 1 (F1, ADR-0085 §5): carrega o modelo .bitnet (default: 2B4T
target/bitnet_2B.bitnet), faz 1 forward pass com HF transformers, e compara
top-N logits com dump serial do kernel. Gate: overlap top-5 >= 80% E max
rel. error dos top-16 logits <= 0.5% (ADR-0084 §11.3.2 — overlap sozinho
nao distingue silu de relu2).

Uso:
  python tools/bitnet_fwd_parity.py [model_path] \
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


def topk_max_rel_err(kernel: dict, host: dict, k: int = 16) -> float | None:
    """Max rel. error |a-b|/max(|a|,|b|,1e-9) sobre os top-k logits rank-a-rank.

    ADR-0084 §11.3.2: overlap sozinho nao distingue silu de relu2 (magnitudes
    muito diferentes) — logits precisam bater em valor, nao so em ordem.
    """
    n = min(k, len(kernel.get("logits", [])), len(host.get("logits", [])))
    if n == 0:
        return None
    errs = []
    for i in range(n):
        a, b = kernel["logits"][i], host["logits"][i]
        errs.append(abs(a - b) / max(abs(a), abs(b), 1e-9))
    return max(errs)


def compare(kernel: dict, host: dict) -> dict:
    """Compara dumps kernel vs HF host."""
    out = {
        "kernel_n": kernel["n"],
        "host_n": len(host["ids"]),
        "overlap_top1": overlap_pct(kernel, host, 1),
        "overlap_top5": overlap_pct(kernel, host, 5),
        "overlap_top16": overlap_pct(kernel, host, 16),
        "max_rel_err_top16": topk_max_rel_err(kernel, host, 16),
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
    """Le o header de um .bitnet (v4/v5/v6 0xBE11BE11 + legado B1TM/B1) sem carregar pesos."""
    data = path.read_bytes()
    magic = data[:4]
    if magic == b"B1TM":
        off = 4
    elif magic[:2] == b"B1":
        off = 2
    elif struct.unpack_from("<I", data, 0)[0] == 0xBE11BE11:
        return _be11_header(data)
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


def _be11_header(data: bytes) -> dict:
    """Header no formato 0xBE11BE11 (v4/v5/v6) — ADR-0085 §2.

    v6 layout: magic u32 @0, version u16 @4, num_params u64 @6,
    model_type u8 @14, reserved @15-17, hidden u16 @18, num_layers u16 @20,
    num_heads u16 @22, vocab_size u32 @24, max_seq u16 @28,
    intermediate_size u16 @30, num_kv_heads u16 @32, q_dim u16 @34,
    num_medusa u32 @36, tie_flag 4B @40, tok_type u8 @44, tok_len u32 @45,
    tokenizer_data[tok_len] @49, act_type u8, embed_type u8, feat u8.
    """
    (version,) = struct.unpack_from("<H", data, 4)
    out = {"magic": "0xBE11BE11", "version": version, "bytes": len(data)}
    if len(data) >= 14:
        (num_params,) = struct.unpack_from("<Q", data, 6)
        out["num_params"] = num_params
        out["model_type"] = data[14]
    if version != 6:
        # Layout completo so documentado p/ v6 (ADR-0085 §2); v4/v5 omitem dims.
        out["note"] = "layout completo so documentado p/ v6; campos dim omitidos"
        return out
    if len(data) < 49:
        out["error"] = "header v6 truncado"
        return out
    out["hidden"], out["layers"], out["heads"] = struct.unpack_from("<HHH", data, 18)
    (out["vocab_size"],) = struct.unpack_from("<I", data, 24)
    out["max_seq"], out["intermediate_size"], out["num_kv_heads"], out["q_dim"] = struct.unpack_from("<HHHH", data, 28)
    (out["num_medusa"],) = struct.unpack_from("<I", data, 36)
    out["tie_flag"] = data[40:44].decode("ascii", errors="replace")
    out["tok_type"] = data[44]
    (tok_len,) = struct.unpack_from("<I", data, 45)
    out["tok_len"] = tok_len
    off = 49 + tok_len
    if len(data) >= off + 3:
        out["act_type"] = data[off]
        out["embed_type"] = data[off + 1]
        out["feat"] = data[off + 2]
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="BitNet FWD parity: kernel vs HF")
    ap.add_argument("model_path", nargs="?", type=Path, default=None,
                    help="Caminho do .bitnet (default: target/bitnet_2B.bitnet se existir)")
    ap.add_argument("--model", default="1bitLLM/bitnet_b1_58-large", help="HF model name")
    ap.add_argument("--prompt", default="ola", help="Prompt para teste")
    ap.add_argument("--kernel-dump", help="Linha [FWD] logits_top do serial (ou texto contendo)")
    ap.add_argument("--dump-file", type=Path, help="Arquivo de log serial para extrair dump")
    ap.add_argument("--bitnet", type=Path, help="Caminho do .bitnet para ler header (legacy; use model_path)")
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

    # 2) Header info — default = 2B4T (target/bitnet_2B.bitnet) quando presente
    hdr_info = {}
    bitnet = args.model_path or args.bitnet
    if bitnet is None:
        cand = ROOT / "target" / "bitnet_2B.bitnet"
        bitnet = cand if cand.exists() else None
    if bitnet is not None and bitnet.exists():
        hdr_info = bitnet_header(bitnet)
        print(f"[HEADER] {bitnet.name}: {json.dumps(hdr_info, indent=2)}")
    elif bitnet is not None:
        print(f"[HEADER] aviso: {bitnet} nao existe; pulando leitura de header", file=sys.stderr)

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
        rel = cmp["max_rel_err_top16"]
        rel_s = f"{rel:.4f}" if rel is not None else "N/A"
        print(f"  Max rel err top-16 logits: {rel_s} (limite 0.5%)")
        print(f"\n  Top-5 comparação:")
        print(f"  {'Rank':>4} {'Kernel ID':>10} {'Host ID':>10} {'Kernel logit':>14} {'Host logit':>14}  Match")
        for t in cmp["top5_detail"]:
            print(f"  {t['rank']:>4} {t['kernel_id'] if t['kernel_id'] is not None else '-':>10} "
                  f"{t['host_id'] if t['host_id'] is not None else '-':>10} "
                  f"{t['kernel_logit'] if t['kernel_logit'] is not None else '-':>14.4f} "
                  f"{t['host_logit'] if t['host_logit'] is not None else '-':>14.4f}  {t['match']}")
        print(f"  Kernel IDs: {kernel['ids']}")
        print(f"  Host IDs:   {host['ids'][:16]}")

    rel = cmp["max_rel_err_top16"]
    # ADR-0084 §11.3.2 / ADR-0085 §5 F1: overlap + metric de logit (distingue silu/relu2)
    gate_pass = cmp["overlap_top5"] >= 80.0 and rel is not None and rel <= 0.005
    print(f"\n  GATE Fase 1: {'✅ PASS' if gate_pass else '❌ FAIL'} "
          f"(top-5 >= 80% E max rel err top-16 <= 0.5%)")
    return 0 if gate_pass else 1


if __name__ == "__main__":
    raise SystemExit(main())
