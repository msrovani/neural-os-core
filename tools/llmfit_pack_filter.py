#!/usr/bin/env python3
"""FitPolicy host gate — scoring Neural inspirado em llmfit (sem depender do binário).

Uso:
  python tools/llmfit_pack_filter.py --dry-run --ram-mb 4096 --pack all
  FIT_GATE=1 PACK_LLM=all python tools/mkfat32.py ...

Exit: 0 = ≥1 Good+; 2 = só Marginal; 1 = Deny total / erro.
Nunca sobe degrau além do PACK_LLM pedido — só filtra para baixo.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from typing import Any

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Degraus canônicos alinhados a mkfat32 / mkexfat
TOKENS = ("850", "13", "2b", "3b", "falcon3")

# Footprints estáticos (MB) se blob ausente — ordem de grandeza BitNet
FALLBACK_BLOB_MB = {
    "850": 220,
    "13": 320,
    "2b": 590,
    "3b": 700,
    "falcon3": 771,
}

HEAP_FLOOR_MB = 128
KV_RATIO = 0.15  # ~15% do blob como overhead KV/heap auxiliar
AIRLLM_HINT_MB = 400  # >PIO prefer AirLLM


def find_file(name: str) -> str | None:
    for d in [
        ROOT,
        os.path.join(ROOT, "target"),
        os.path.join(ROOT, "firmware"),
        os.path.join(ROOT, "crates", "neural-kernel"),
    ]:
        p = os.path.join(d, name)
        if os.path.isfile(p):
            return p
    return None


def find_large(name: str, min_bytes: int = 1_000_000) -> str | None:
    p = find_file(name)
    if p and os.path.getsize(p) >= min_bytes:
        return p
    return None


def resolve_blob(token: str) -> tuple[str | None, int]:
    """Retorna (path|None, size_mb). size_mb=0 se ausente e sem fallback útil."""
    path: str | None = None
    if token == "850":
        path = (
            find_large("BITNET850.BIN")
            or find_large("bitnet_850m.bitnet")
            or find_large("MICRO.BIN")
        )
    elif token == "13":
        path = find_large("BITNET13.BIN") or find_large("bitnet_1p3b.bitnet")
    elif token == "2b":
        path = (
            find_file("bitnet_2B.bitnet")
            or find_file("BITNET2B.BIN")
            or find_file("BITNET-2B.BITNET")
            or find_file("bitnet-BitNet-b1_58-2B-4T.bitnet")
        )
    elif token == "3b":
        path = find_large("BITNET3B.BIN") or find_large("bitnet_3B.bitnet")
    elif token == "falcon3":
        path = find_large("FALCON3.V6") or find_large("FALCON3.BIN") or find_large("falcon3.v6")

    if path and os.path.isfile(path):
        mb = max(1, os.path.getsize(path) // (1024 * 1024))
        return path, mb
    fb = FALLBACK_BLOB_MB.get(token, 0)
    return None, fb


def parse_pack(raw: str) -> set[str]:
    raw = (raw or "850").strip().lower()
    if not raw or raw in ("none", "0", "off"):
        return set()
    if raw in ("all", "*"):
        return set(TOKENS)
    out: set[str] = set()
    for tok in raw.replace(";", ",").split(","):
        t = tok.strip().lower().replace(" ", "")
        if t in ("850", "850m", "fast", "large"):
            out.add("850")
        elif t in ("13", "1.3", "1p3", "1.5", "xl", "1.58", "158"):
            out.add("13")
        elif t in ("2b", "2", "2.0"):
            out.add("2b")
        elif t in ("3b", "3", "pro"):
            out.add("3b")
        elif t in ("falcon3", "falcon", "f3", "falcon-3b", "falcon3b"):
            out.add("falcon3")
    return out


def detect_ram_mb() -> int:
    try:
        import psutil  # type: ignore

        return int(psutil.virtual_memory().total // (1024 * 1024))
    except Exception:
        pass
    if sys.platform == "win32":
        try:
            out = subprocess.check_output(
                [
                    "powershell",
                    "-NoProfile",
                    "-Command",
                    "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
                ],
                text=True,
                timeout=15,
            ).strip()
            return max(1, int(out) // (1024 * 1024))
        except Exception:
            pass
    try:
        with open("/proc/meminfo", encoding="utf-8") as f:
            for line in f:
                if line.startswith("MemTotal:"):
                    kb = int(line.split()[1])
                    return max(1, kb // 1024)
    except Exception:
        pass
    return 8192  # fallback host típico


def detect_vram_mb() -> int:
    smi = shutil.which("nvidia-smi")
    if not smi:
        return 0
    try:
        out = subprocess.check_output(
            [smi, "--query-gpu=memory.total", "--format=csv,noheader,nounits"],
            text=True,
            timeout=10,
        ).strip()
        vals = [int(x.strip()) for x in out.splitlines() if x.strip().isdigit()]
        return max(vals) if vals else 0
    except Exception:
        return 0


def classify(usage: float) -> str:
    if usage <= 0.50:
        return "Perfect"
    if usage <= 0.80:
        return "Good"
    if usage <= 0.95:
        return "Marginal"
    if usage <= 1.05:
        return "TooTight"
    return "Deny"


def tok_s_est(needed_mb: int, ram_mb: int, vram_mb: int) -> float:
    """Proxy bandwidth → tok/s (só informativo)."""
    bw = float(vram_mb if vram_mb > 0 else ram_mb)
    if bw <= 0 or needed_mb <= 0:
        return 0.0
    # ~GB/s proxy / model size → rough tok/s
    return round(max(0.1, (bw / 1024.0) * 40.0 / max(1.0, needed_mb / 256.0)), 2)


def score_token(token: str, ram_mb: int, vram_mb: int) -> dict[str, Any]:
    path, blob_mb = resolve_blob(token)
    if blob_mb <= 0:
        return {
            "token": token,
            "class": "Deny",
            "reason": "blob_absent",
            "blob_mb": 0,
            "needed_mb": 0,
            "usage": 1.0,
            "path": None,
            "prefer_airllm": False,
            "tok_s_est": 0.0,
        }
    kv_mb = max(8, int(blob_mb * KV_RATIO))
    heap_mb = HEAP_FLOOR_MB
    needed = blob_mb + kv_mb + heap_mb
    pool = ram_mb  # honesty: VRAM só informativo até inventário guest
    usage = needed / float(pool) if pool > 0 else 2.0
    cls = classify(usage)
    if path is None and cls in ("Perfect", "Good", "Marginal"):
        # fallback footprint sem arquivo — ainda Deny para pack real
        cls = "Deny"
        reason = "blob_missing_on_disk"
    else:
        reason = "ok" if cls != "Deny" else "too_large"
    return {
        "token": token,
        "class": cls,
        "reason": reason,
        "blob_mb": blob_mb,
        "needed_mb": needed,
        "usage": round(usage, 4),
        "path": path,
        "prefer_airllm": blob_mb >= AIRLLM_HINT_MB,
        "tok_s_est": tok_s_est(needed, ram_mb, vram_mb),
    }


def filter_pack(
    requested: set[str], ram_mb: int, vram_mb: int
) -> tuple[list[str], list[dict[str, Any]], list[dict[str, Any]]]:
    recs: list[dict[str, Any]] = []
    for t in TOKENS:
        if t not in requested:
            continue
        recs.append(score_token(t, ram_mb, vram_mb))

    good = [r for r in recs if r["class"] in ("Perfect", "Good")]
    marginal = [r for r in recs if r["class"] == "Marginal"]
    denied = [r for r in recs if r["class"] in ("TooTight", "Deny")]

    if good:
        pack = [r["token"] for r in good]
    elif marginal:
        pack = [r["token"] for r in marginal]
    else:
        pack = []

    return pack, recs, denied


def try_llmfit_advisory() -> Any:
    exe = shutil.which("llmfit")
    if not exe:
        return None
    try:
        out = subprocess.check_output(
            [exe, "recommend", "--json"],
            text=True,
            timeout=60,
            stderr=subprocess.DEVNULL,
        )
        return json.loads(out)
    except Exception as e:
        return {"error": str(e)}


def apply_fit_gate_env(pack_csv: str) -> None:
    """Usado por mkfat32/mkexfat: reescreve PACK_LLM no ambiente do processo."""
    os.environ["PACK_LLM"] = pack_csv
    os.environ["PACK_LLM_FIT"] = pack_csv


def run_fit_gate_from_env() -> int:
    """Chamado com FIT_GATE=1: filtra PACK_LLM in-place. Retorna exit code."""
    requested = parse_pack(os.environ.get("PACK_LLM", "850"))
    ram = int(os.environ.get("FIT_RAM_MB") or detect_ram_mb())
    vram = int(os.environ.get("FIT_VRAM_MB") or detect_vram_mb())
    pack, recs, denied = filter_pack(requested, ram, vram)
    report = {
        "host": {"ram_mb": ram, "vram_mb": vram},
        "requested": sorted(requested),
        "recommendations": recs,
        "pack_llm": ",".join(pack) if pack else "",
        "denied": denied,
    }
    print(f"[FIT_GATE] ram={ram}MB vram={vram}MB requested={sorted(requested)} → pack={pack or 'none'}")
    for r in recs:
        print(
            f"[FIT_GATE] {r['token']}: {r['class']} usage={r['usage']:.0%} "
            f"needed={r['needed_mb']}MB blob={r['blob_mb']}MB reason={r['reason']}"
        )
    if pack:
        apply_fit_gate_env(",".join(pack))
    else:
        apply_fit_gate_env("none")
    # exit semantics for callers that care
    if any(r["class"] in ("Perfect", "Good") for r in recs):
        return 0
    if any(r["class"] == "Marginal" for r in recs):
        return 2
    return 1


def main() -> int:
    ap = argparse.ArgumentParser(description="Neural FitPolicy pack filter (llmfit-inspired)")
    ap.add_argument("--dry-run", action="store_true", help="Só JSON / relatório")
    ap.add_argument("--ram-mb", type=int, default=0, help="Override RAM host (MB)")
    ap.add_argument("--vram-mb", type=int, default=-1, help="Override VRAM (MB); -1=auto")
    ap.add_argument(
        "--pack",
        default=os.environ.get("PACK_LLM", "850"),
        help="PACK_LLM pedido (default env ou 850)",
    )
    ap.add_argument("--json", action="store_true", help="Stdout só JSON")
    ap.add_argument("--apply", action="store_true", help="Imprime export PACK_LLM=...")
    ap.add_argument("--advisory-llmfit", action="store_true", help="Tenta llmfit no PATH")
    args = ap.parse_args()

    ram = args.ram_mb if args.ram_mb > 0 else detect_ram_mb()
    vram = detect_vram_mb() if args.vram_mb < 0 else args.vram_mb
    requested = parse_pack(args.pack)
    pack, recs, denied = filter_pack(requested, ram, vram)

    report: dict[str, Any] = {
        "host": {"ram_mb": ram, "vram_mb": vram},
        "requested": sorted(requested),
        "recommendations": recs,
        "pack_llm": ",".join(pack) if pack else "",
        "denied": [d["token"] for d in denied],
        "quant_ladder": [".bitnet", "airllm_gguf", "deny"],
    }
    if args.advisory_llmfit or shutil.which("llmfit"):
        adv = try_llmfit_advisory()
        if adv is not None:
            report["advisory_llmfit"] = adv

    if args.json or args.dry_run:
        print(json.dumps(report, indent=2))
    else:
        print(f"host ram={ram}MB vram={vram}MB")
        for r in recs:
            print(
                f"  {r['token']}: {r['class']} usage={r['usage']:.0%} "
                f"needed={r['needed_mb']}MB tok_s_est={r['tok_s_est']}"
            )
        print(f"pack_llm={report['pack_llm'] or 'none'}")

    if args.apply and pack:
        print(f"export PACK_LLM={','.join(pack)}")
        print(f"export PACK_LLM_FIT={','.join(pack)}")

    if any(r["class"] in ("Perfect", "Good") for r in recs):
        return 0
    if any(r["class"] == "Marginal" for r in recs):
        return 2
    return 1


if __name__ == "__main__":
    sys.exit(main())
