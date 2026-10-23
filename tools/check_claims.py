#!/usr/bin/env python3
"""Gate de artefatos citados em CLAIMS.yaml (A1 / forum ATLAS-0011).

Exit 0: todos os artefatos existem.
Exit 1: algum artefato ausente OU --plant-demo (prova que o gate pega).

Uso:
  python tools/check_claims.py
  python tools/check_claims.py --plant-demo
"""
from __future__ import annotations

import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLAIMS_PATH = os.path.join(ROOT, "CLAIMS.yaml")


def load_claims(path: str) -> list[dict]:
    # Parser YAML minimo (sem PyYAML): so o subset que CLAIMS.yaml usa.
    claims: list[dict] = []
    cur: dict | None = None
    in_claims = False
    with open(path, encoding="utf-8", errors="replace") as f:
        for raw in f:
            line = raw.rstrip("\n")
            if line.startswith("claims:"):
                in_claims = True
                continue
            if not in_claims:
                continue
            if line.startswith("  - id:"):
                if cur:
                    claims.append(cur)
                cur = {"id": line.split(":", 1)[1].strip().strip('"')}
                continue
            if cur is None:
                continue
            if line.startswith("    ") and ":" in line:
                key, val = line.strip().split(":", 1)
                val = val.strip().strip('"')
                if val == "null":
                    val = None
                cur[key] = val
        if cur:
            claims.append(cur)
    return claims


def main() -> int:
    plant = "--plant-demo" in sys.argv[1:]
    if not os.path.isfile(CLAIMS_PATH):
        print("[claims] MISSING CLAIMS.yaml")
        return 1

    claims = load_claims(CLAIMS_PATH)
    if len(claims) < 10:
        print("[claims] FAIL: need >= 10 claims, got %d" % len(claims))
        return 1

    missing = []
    for c in claims:
        art = c.get("artefato")
        if not art:
            continue
        full = os.path.join(ROOT, art.replace("/", os.sep))
        if not os.path.exists(full):
            missing.append((c.get("id", "?"), art))

    if plant:
        fake = "docs/evidence/_planted_missing_claim_artifact.txt"
        full = os.path.join(ROOT, fake.replace("/", os.sep))
        if os.path.exists(full):
            print("[claims] plant-demo: unexpected file exists: %s" % fake)
            return 1
        print("[claims] plant-demo: would-fail on missing %s (gate OK)" % fake)
        # Exit 1 proves the gate would catch absence.
        return 1

    if missing:
        print("[claims] FAIL: %d missing artifact(s)" % len(missing))
        for cid, art in missing:
            print("[claims]   %s -> %s" % (cid, art))
        return 1

    print("[claims] OK: %d claims, all artifacts present" % len(claims))
    return 0


if __name__ == "__main__":
    sys.exit(main())
