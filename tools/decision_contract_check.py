#!/usr/bin/env python3
"""ADR-0106 — decision_contract_check.py

Verifica alinhamento mecânico entre opções Rust (intent_decide / Trinity)
e artefatos de treino. Fail-closed: exit 1 se divergir.

Uso:
  python tools/decision_contract_check.py
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

INTENT_DECIDE = ROOT / "crates" / "cortex" / "src" / "intent_decide.rs"
TRINITY = ROOT / "crates" / "cortex" / "src" / "trinity.rs"
DECISION = ROOT / "crates" / "cortex" / "src" / "decision.rs"
TRAIN_ROUTER = ROOT / "tools" / "train_router.py"


def extract_intent_options(src: str) -> list[str]:
    m = re.search(r"INTENT_OPTIONS:\s*\[Intent;\s*INTENT_N\]\s*=\s*\[(.*?)\];", src, re.S)
    if not m:
        raise SystemExit("INTENT_OPTIONS not found in intent_decide.rs")
    body = m.group(1)
    return re.findall(r"Intent::(\w+)", body)


def extract_contract_version(src: str) -> int:
    m = re.search(r"DECISION_CONTRACT_VERSION:\s*u16\s*=\s*(\d+)", src)
    if not m:
        raise SystemExit("DECISION_CONTRACT_VERSION missing")
    return int(m.group(1))


def extract_trinity_expert_order(src: str) -> list[str]:
    # init_trinity / Expert { name: "..." } order in first big block
    names = re.findall(r'name:\s*"(.*?)"', src)
    # Keep unique preserving order (router experts)
    seen = set()
    out = []
    for n in names:
        if n not in seen and n not in ("FALLBACK",):
            seen.add(n)
            out.append(n)
        if len(out) >= 7:
            break
    return out


def main() -> int:
    errors: list[str] = []

    decision_src = DECISION.read_text(encoding="utf-8")
    ver = extract_contract_version(decision_src)
    if ver != 1:
        errors.append(f"unexpected DECISION_CONTRACT_VERSION={ver} (want 1)")

    intents = extract_intent_options(INTENT_DECIDE.read_text(encoding="utf-8"))
    if "Unknown" not in intents:
        errors.append("Intent::Unknown missing from INTENT_OPTIONS (ADR-0106 escape hatch)")
    if len(intents) != 15:
        errors.append(f"INTENT_OPTIONS len={len(intents)} want 15")

    # train_router must mention ORDER of experts if present
    if TRAIN_ROUTER.exists():
        tr = TRAIN_ROUTER.read_text(encoding="utf-8", errors="replace")
        if "ORDER" not in tr and "order" not in tr.lower():
            errors.append("train_router.py missing ORDER reminder (ADR-0106 §0.8)")

    experts = extract_trinity_expert_order(TRINITY.read_text(encoding="utf-8"))
    if len(experts) < 7:
        errors.append(f"trinity experts found={len(experts)} want >=7: {experts}")

    print(f"DECISION_CONTRACT_VERSION={ver}")
    print(f"INTENT_OPTIONS ({len(intents)}): {', '.join(intents)}")
    print(f"TRINITY experts (first 7): {', '.join(experts[:7])}")

    if errors:
        print("FAIL:")
        for e in errors:
            print(f"  - {e}")
        return 1
    print("OK — contract check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
