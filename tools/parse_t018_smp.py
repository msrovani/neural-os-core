#!/usr/bin/env python3
"""ADR-0100 T-018: valida aceite SMP metal a partir de log serial.

Critério pass:
  - Checkpoint K23 presente (ou posterior Runtime)
  - `online == madt_enabled - 1` (APs acordados, BSP excluído)
  - Sem loop INFINITO de INIT-SIPI-SIPI (heurística: <= 20 linhas SIPI)

Exit 0: PASS
Exit 1: FAIL (critério não atendido)
Exit 2: log insuficiente (sem linhas SMP)

Uso:
  python tools/parse_t018_smp.py logs/boot_metal.txt
  Get-Content E:\\BOOT.LOG | python tools/parse_t018_smp.py
"""
from __future__ import annotations

import argparse
import re
import sys

SMP_MARKERS = (
    re.compile(r"K23\b", re.I),
    re.compile(r"SMP.*AP_READY", re.I),
    re.compile(r"AP\s+\d+\s+entrou", re.I),
    re.compile(r"ap_pollable\s*=\s*true", re.I),
    re.compile(r"All\s+\d+\s+APs\s+IDT\s+ready", re.I),
)

SIPI_RE = re.compile(r"INIT-SIPI-SIPI", re.I)
MADT_RE = re.compile(r"madt[_\s]*(?:enabled|lapics?)\s*[=:]\s*(\d+)", re.I)
ONLINE_RE = re.compile(
    r"(?:online|AP_ONLINE|total_cores|cores_online)\s*[=:]\s*(\d+)", re.I
)
AP_ENTRY_RE = re.compile(r"AP_ENTRY|ap_entry_count\s*[=:]\s*(\d+)", re.I)
K_CHECK_RE = re.compile(r"K(\d{2,3})\b")


def parse(text: str) -> dict:
    lines = text.splitlines()
    sipi_count = sum(1 for ln in lines if SIPI_RE.search(ln))
    has_k23 = any("K23" in ln for ln in lines)
    has_runtime = any("Runtime" in ln or "SCHEDULER" in ln for ln in lines)
    madt: int | None = None
    online: int | None = None
    ap_entries: int | None = None
    smp_hits = 0

    for ln in lines:
        for pat in SMP_MARKERS:
            if pat.search(ln):
                smp_hits += 1
                break
        if madt is None:
            m = MADT_RE.search(ln)
            if m:
                madt = int(m.group(1))
        if online is None:
            m = ONLINE_RE.search(ln)
            if m:
                online = int(m.group(1))
        m = AP_ENTRY_RE.search(ln)
        if m and m.lastindex:
            ap_entries = int(m.group(1))

    last_k = 0
    for ln in lines:
        for m in K_CHECK_RE.finditer(ln):
            k = int(m.group(1))
            if k > last_k:
                last_k = k

    expected_aps: int | None = None
    if madt is not None and madt > 0:
        expected_aps = madt - 1

    return {
        "sipi_count": sipi_count,
        "has_k23": has_k23,
        "has_runtime": has_runtime,
        "madt": madt,
        "online": online,
        "ap_entries": ap_entries,
        "expected_aps": expected_aps,
        "smp_hits": smp_hits,
        "last_k": last_k,
    }


def verdict(fields: dict) -> tuple[str, list[str]]:
    reasons: list[str] = []
    if fields["smp_hits"] == 0 and not fields["has_k23"]:
        return "INSUFFICIENT", ["sem evidência SMP no log"]

    if fields["sipi_count"] > 20:
        reasons.append(f"SIPI excessivo ({fields['sipi_count']}>20) — possível hang")

    if not fields["has_k23"] and not fields["has_runtime"]:
        reasons.append("K23/Runtime ausente — boot não chegou ao gate SMP")

    exp = fields["expected_aps"]
    on = fields["online"]
    if exp is not None and on is not None:
        if on != exp:
            reasons.append(f"online={on} != madt-1={exp}")
    elif fields["ap_entries"] is not None and exp is not None:
        if fields["ap_entries"] != exp:
            reasons.append(
                f"ap_entry_count={fields['ap_entries']} != madt-1={exp}"
            )
    elif exp is not None and fields["has_k23"]:
        reasons.append(
            f"madt={fields['madt']} mas online/ap_entry não parseado — revisar log"
        )

    if reasons:
        return "FAIL", reasons
    if fields["has_k23"] or (fields["has_runtime"] and fields["smp_hits"] > 0):
        return "PASS", ["critérios T-018 atendidos ou parcial forte+"]
    return "INSUFFICIENT", ["dados insuficientes para PASS/FAIL"]


def main() -> int:
    ap = argparse.ArgumentParser(description="Validador T-018 SMP metal")
    ap.add_argument("log", nargs="?", help="arquivo serial (stdin se omitido)")
    args = ap.parse_args()
    raw = (
        sys.stdin.read()
        if not args.log
        else open(args.log, encoding="utf-8", errors="replace").read()
    )
    fields = parse(raw)
    status, reasons = verdict(fields)

    for k, v in fields.items():
        print(f"{k}={v}")
    print(f"verdict={status}")
    for r in reasons:
        print(f"reason={r}", file=sys.stderr)

    if status == "PASS":
        return 0
    if status == "FAIL":
        return 1
    return 2


if __name__ == "__main__":
    sys.exit(main())
