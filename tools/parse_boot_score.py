#!/usr/bin/env python3
"""ADR-0092: extract BOOT SCORE from a serial log (QEMU or metal).

Exit 0: bloco presente, nenhum campo fail, attention=none
Exit 1: fail ou attention real
Exit 2: bloco ausente
"""
from __future__ import annotations

import argparse
import re
import sys


KEYS = (
    "phase_0_7",
    "cpu",
    "net",
    "storage",
    "llm",
    "audio_stt_tts",
    "gpu",
    "wifi",
    "attention",
)

# ADR-0103 s321 — homes/facades (informativo; não falha o placar)
TOPOLOGY_KEYS = (
    "k3chj",
    "net_nic",
    "usb_msc",
    "fat32",
    "storage_bus",
    "slog",
)


_ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")


def _payload(line: str) -> str:
    """Strip ANSI + slog prefix (`… [ok] - `) → body do placar."""
    s = _ANSI.sub("", line).strip()
    if " - " in s:
        tail = s.rsplit(" - ", 1)[-1].strip()
        if (
            tail.startswith("===")
            or tail.startswith("---")
            or any(tail.startswith(k) for k in KEYS + TOPOLOGY_KEYS)
        ):
            return tail
    return s


def extract_block(text: str) -> str | None:
    text = _ANSI.sub("", text)
    m = re.search(
        r"=== BOOT SCORE .+?===\n(?:.*\n)*?(?:^|\n)[^\n]*===",
        text,
        re.MULTILINE,
    )
    return m.group(0) if m else None


def parse(text: str) -> dict[str, str]:
    block = extract_block(text)
    if not block:
        return {}
    out: dict[str, str] = {}
    # Mais longos primeiro — evita net ⊂ net_nic, storage ⊂ storage_bus
    score_keys = sorted(KEYS, key=len, reverse=True)
    topo_keys = sorted(TOPOLOGY_KEYS, key=len, reverse=True)
    for line in block.splitlines():
        s = _payload(line)
        if "=== BOOT SCORE" in s:
            for key in ("qemu", "ram_mb", "smp_online"):
                mm = re.search(rf"{key}=(\S+)", s)
                if mm:
                    out[key] = mm.group(1)
            continue
        matched = False
        for key in topo_keys:
            if s.startswith(key) and (len(s) == len(key) or s[len(key)].isspace()):
                out[f"topo.{key}"] = s[len(key) :].strip()
                matched = True
                break
        if matched:
            continue
        for key in score_keys:
            if s.startswith(key) and (len(s) == len(key) or s[len(key)].isspace()):
                out[key] = s[len(key) :].strip()
                break
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("log", nargs="?", help="serial log (stdin if omitted)")
    args = ap.parse_args()
    raw = (
        sys.stdin.read()
        if not args.log
        else open(args.log, encoding="utf-8", errors="replace").read()
    )
    fields = parse(raw)
    if not fields:
        print("NO_SCORE", file=sys.stderr)
        return 2
    for k, v in fields.items():
        print(f"{k}={v}")
    att = fields.get("attention", "none").strip()
    for k in KEYS:
        if k == "attention":
            continue
        val = fields.get(k, "")
        token = val.split()[0] if val else ""
        if token == "fail":
            print(f"FAIL_FIELD {k}={val}", file=sys.stderr)
            return 1
    if att and att != "none":
        print(f"ATTENTION {att}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
