#!/usr/bin/env python3
"""Export SDIO/HWID index → LEGO stubs (draft Escalate, nunca Auto bind).

Uso:
  python tools/export_sdio_lego_index.py --input tools/target/sdio_hwids.json --out target/lego_index
  python tools/export_sdio_lego_index.py --dry-run

Não gera centenas de RECIPE ativas. Só CSV/JSON de roteamento + opcional stub .md unsigned.
"""
from __future__ import annotations

import argparse
import csv
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

HWID_PCI = re.compile(
    r"VEN_([0-9A-Fa-f]{4}).*?DEV_([0-9A-Fa-f]{4})", re.I
)


def parse_vid_did(hwid: str) -> tuple[int, int] | None:
    m = HWID_PCI.search(hwid.replace("\\", "\\"))
    if not m:
        return None
    return int(m.group(1), 16), int(m.group(2), 16)


def guess_family(vid: int, did: int, hint: str) -> str:
    h = hint.lower()
    if vid == 0x168C:
        if did in (0x003E, 0x0041):
            return "ath10k_qca6174"
        return "atheros_wifi"
    if vid == 0x8086 and ("wifi" in h or "wlan" in h or did in (0x2723, 0x2725)):
        return "intel_iwlwifi"
    if vid == 0x10DE:
        return "nvidia_gpu"
    if "wlan" in h or "wifi" in h:
        return "wifi_unknown"
    if "video" in h or "vga" in h:
        return "gpu_unknown"
    return "unknown"


def load_entries(path: Path) -> list[dict]:
    if not path.exists():
        return []
    data = json.loads(path.read_text(encoding="utf-8", errors="replace"))
    if isinstance(data, dict) and "hwids" in data:
        raw = data["hwids"]
    elif isinstance(data, list):
        raw = data
    else:
        raw = []
    out = []
    for item in raw:
        if isinstance(item, str):
            hwid, cat = item, ""
        elif isinstance(item, dict):
            hwid = item.get("hwid") or item.get("id") or ""
            cat = item.get("class") or item.get("category") or ""
        else:
            continue
        vd = parse_vid_did(hwid)
        if not vd:
            continue
        vid, did = vd
        out.append(
            {
                "vendor_id": vid,
                "device_id": did,
                "family_guess": guess_family(vid, did, cat),
                "class_hint": cat,
                "needs_fw": guess_family(vid, did, cat)
                in ("ath10k_qca6174", "atheros_wifi", "intel_iwlwifi", "nvidia_gpu"),
                "trust": "escalate",
                "provenance": "imported",
                "note": "draft stub — not an active DeviceRecipe",
            }
        )
    # dedupe
    seen = set()
    uniq = []
    for e in out:
        k = (e["vendor_id"], e["device_id"])
        if k in seen:
            continue
        seen.add(k)
        uniq.append(e)
    return uniq


def write_stub(out_dir: Path, e: dict) -> None:
    name = f"{e['family_guess']}_{e['vendor_id']:04x}_{e['device_id']:04x}"
    path = out_dir / "stubs" / f"{name}.recipe.md"
    path.parent.mkdir(parents=True, exist_ok=True)
    body = f"""---
schema: 1
kind: device-recipe
name: {name[:64]}
package_id: draft.{name[:48]}
description: SDIO index stub — Escalate only
goal: Rotear HWID; preencher RegMap/stages com fonte
contexto: Gerado por export_sdio_lego_index.py — NAO bind Auto
acionaveis: [on_demand]
required_tokens: []
capabilities: []
class: Wifi
layer: L2
bind:
  vendor_id: {e['vendor_id']}
  device_id: {e['device_id']}
  rev_mask: null
firmware: []
trust_class: escalate
provenance: imported
sandbox_status: pending
honesty: no_fake_ready
requires: [PciEnumerated]
provides: []
content_hash: ""
signature: ""
---

## Contexto

Stub do índice SDIO. family_guess={e['family_guess']}.

## Bind

VID/DID acima.

## Firmware

TODO + blob_hash.

## RegMap

TODO com cite Linux.

## Stages / UnlockDAG

TODO.

## HalOffer Port

TODO.

## Internal edges

TODO.

## Pre-Flight

Unsigned — Escalate.

## Success Criteria

Nao promover sem signature + HW serial.

## Failure Policy

Deny Auto bind.

## Anti-Patterns

Nao inventar MMIO; nao Ready falso.

## Test Plan

Issue danger se family_guess errado.
"""
    path.write_text(body, encoding="utf-8")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--input",
        type=Path,
        default=ROOT / "tools" / "target" / "sdio_hwids.json",
    )
    ap.add_argument("--out", type=Path, default=ROOT / "target" / "lego_index")
    ap.add_argument("--stubs", action="store_true", help="escrever RECIPE stubs Escalate")
    ap.add_argument("--max-stubs", type=int, default=32)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    entries = load_entries(args.input)
    print(f"[LEGO] entries={len(entries)} from {args.input}")
    if args.dry_run:
        for e in entries[:10]:
            print(
                f"  {e['vendor_id']:04x}:{e['device_id']:04x} "
                f"family={e['family_guess']} needs_fw={e['needs_fw']}"
            )
        return

    args.out.mkdir(parents=True, exist_ok=True)
    json_path = args.out / "hwid_index.json"
    json_path.write_text(json.dumps(entries, indent=2), encoding="utf-8")
    csv_path = args.out / "hwid_index.csv"
    with csv_path.open("w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(
            f,
            fieldnames=[
                "vendor_id",
                "device_id",
                "family_guess",
                "class_hint",
                "needs_fw",
                "trust",
            ],
        )
        w.writeheader()
        for e in entries:
            w.writerow(
                {
                    "vendor_id": f"{e['vendor_id']:04x}",
                    "device_id": f"{e['device_id']:04x}",
                    "family_guess": e["family_guess"],
                    "class_hint": e["class_hint"],
                    "needs_fw": e["needs_fw"],
                    "trust": e["trust"],
                }
            )
    print(f"[LEGO] wrote {json_path} + {csv_path}")

    if args.stubs:
        # priorizar wifi/gpu
        prio = [e for e in entries if e["needs_fw"]][: args.max_stubs]
        for e in prio:
            write_stub(args.out, e)
        print(f"[LEGO] stubs={len(prio)} under {args.out / 'stubs'} (Escalate only)")


if __name__ == "__main__":
    main()
