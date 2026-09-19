#!/usr/bin/env python3
"""Parse serial logs for AutoLearn / SleepCycle METRIC lines (s360 lab)."""
from __future__ import annotations

import argparse
import re
from pathlib import Path

PATTERNS = {
    "learn_probe": re.compile(r"METRIC probe unmatched_injected", re.I),
    "learn_start": re.compile(r"METRIC learn_start", re.I),
    "learn_done": re.compile(r"METRIC learn_done", re.I),
    "sleep_metric": re.compile(r"METRIC phase=", re.I),
    "mesh_role": re.compile(r"mesh role=|MESH_ENGINE|role=Master|role=Worker|role=Memory|role=Compute", re.I),
    "runtime": re.compile(r"PHASE 7|Runtime|SCHEDULER", re.I),
}


def scan(path: Path) -> dict[str, int]:
    counts = {k: 0 for k in PATTERNS}
    samples: dict[str, list[str]] = {k: [] for k in PATTERNS}
    if not path.exists():
        return counts, samples, False
    text = path.read_text(encoding="utf-8", errors="replace")
    for line in text.splitlines():
        for name, pat in PATTERNS.items():
            if pat.search(line):
                counts[name] += 1
                if len(samples[name]) < 3:
                    samples[name].append(line.strip()[:160])
    return counts, samples, True


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--logs", nargs="+", required=True)
    args = ap.parse_args()
    print("=== AutoLearn / SleepCycle / Mesh log measure ===")
    ok_any = False
    for raw in args.logs:
        p = Path(raw)
        counts, samples, exists = scan(p)
        print(f"\n--- {p.name} exists={exists} ---")
        if not exists:
            continue
        for k, n in counts.items():
            mark = "OK" if n > 0 else "--"
            print(f"  [{mark}] {k:14s} = {n}")
            for s in samples[k]:
                print(f"         | {s}")
        if counts["learn_probe"] or counts["learn_done"] or counts["sleep_metric"]:
            ok_any = True
    print("\nVERDICT:", "METRIC visto nos guests" if ok_any else "ainda sem METRIC (aguarde Runtime / rebuild)")
    return 0 if ok_any else 2


if __name__ == "__main__":
    raise SystemExit(main())
