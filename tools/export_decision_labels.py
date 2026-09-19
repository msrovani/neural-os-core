#!/usr/bin/env python3
"""ADR-0106 D4 — valida decision_labels.jsonl (sites + examples).

Uso:
  python tools/export_decision_labels.py data/decision_labels.jsonl
  python tools/export_decision_labels.py --write-demo data/decision_labels.jsonl
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

EXPERTS = {
    "generator",
    "hw_control",
    "hw_identify",
    "rust_coder",
    "disk_diag",
    "security",
    "speech_synth",
}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("path", nargs="?", default="data/decision_labels.jsonl")
    ap.add_argument("--write-empty", action="store_true")
    ap.add_argument("--write-demo", action="store_true",
                    help="header + 1 hitl example for train_router smoke")
    args = ap.parse_args()
    path = Path(args.path)

    if args.write_empty or args.write_demo:
        path.parent.mkdir(parents=True, exist_ok=True)
        lines = ['{"type":"header","contract":1,"provenance":"demo_stub"}']
        if args.write_demo:
            lines.append(
                '{"type":"example","text":"aumenta o volume do alto-falante",'
                '"expert":"hw_control","provenance":"hitl"}'
            )
            lines.append(
                '{"type":"site","k":"intent.think","ok":3,"bad":0,"abs":1,'
                '"esc":0,"trusted":0,"provenance":"hitl_or_auto"}'
            )
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"wrote {path}")
        return 0

    if not path.exists():
        print(f"missing {path} — use --write-demo or /decisions export")
        return 1

    sites = examples = 0
    bad = 0
    header = None
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        obj = json.loads(line)
        if obj.get("type") == "header":
            header = obj
            if int(obj.get("contract", -1)) != 1:
                print(f"FAIL contract={obj.get('contract')} want 1")
                return 1
        elif obj.get("type") == "site":
            sites += 1
            print(f"  site={obj.get('k')} ok={obj.get('ok')} esc={obj.get('esc')}")
        elif obj.get("type") == "example":
            examples += 1
            ex = obj.get("expert")
            if ex not in EXPERTS:
                print(f"FAIL unknown expert={ex}")
                bad += 1
            else:
                print(f"  example expert={ex} prov={obj.get('provenance')} text={obj.get('text')!r}")
    print(f"header={header}")
    print(f"sites={sites} examples={examples}")
    if bad:
        return 1
    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
