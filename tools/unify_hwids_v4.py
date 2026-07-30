#!/usr/bin/env python3
"""Unify ALL HWID sources into v4 training dataset.

Sources:
  1. Windows DriverStore (WDM) — models/WDM/hwids.json
  2. SDIO DriverPacks — models/pci_usb/sdio_hwids.json
  3. PCI.IDS + USB.IDS unified — models/pci_usb/hw_all_unified.csv
  4. Kernel seed table — embedded (train_hw_expert_v4.py seed_table_rows)
  5. HW Expert v3 — models/hw_expert/hw_expert_v3.bitnet (future: extract logits)

Output:
  models/hw_expert/v4/dataset.json       — unified labeled dataset
  models/hw_expert/v4/vocab.json          — vocabulary maps
  models/hw_expert/v4/stats.json          — dataset statistics
"""

import csv
import json
import re
import sys
from collections import defaultdict, Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MODELS = ROOT / "models"
TARGET = MODELS / "hw_expert" / "v4"

# ─── Import vocab from training script ──────────────────────────────────
sys.path.insert(0, str(ROOT / "tools"))
from train_hw_expert_v4 import (
    FAMILY, FW, AGENT, NEXT, CAPS, pack_vid_did, seed_table_rows, row_to_sample
)

# ─── Sources ─────────────────────────────────────────────────────────────

SOURCES = {
    "wdm": MODELS / "WDM" / "hwids.json",
    "sdio": MODELS / "pci_usb" / "sdio_hwids.json",
    "unified_csv": MODELS / "pci_usb" / "hw_all_unified.csv",
}


def load_wdm(path: Path) -> list[dict]:
    """Load Windows DriverStore HWIDs."""
    if not path.exists():
        print(f"  [SKIP] WDM not found: {path}")
        return []
    with open(path) as f:
        data = json.load(f)
    print(f"  [WDM] {len(data)} entries")
    for entry in data:
        entry["_source"] = "wdm"
        # WDM has bus, vid, did, class
        entry["bus"] = entry.get("bus", "pci")
    return data


def load_sdio(path: Path) -> list[dict]:
    """Load SDIO DriverPack HWIDs."""
    if not path.exists():
        print(f"  [SKIP] SDIO not found: {path}")
        return []
    with open(path) as f:
        data = json.load(f)
    print(f"  [SDIO] {len(data)} entries")

    results = []
    for item in data:
        hwid = item.get("hwid", "")
        cls = item.get("class", "unknown")
        # Parse PCI\VEN_XXXX&DEV_XXXX
        m = re.search(r'VEN_([0-9A-Fa-f]{4})&DEV_([0-9A-Fa-f]{4})', hwid)
        if m:
            vid, did = int(m.group(1), 16), int(m.group(2), 16)
            results.append({
                "bus": "pci", "vid": vid, "did": did,
                "class": cls, "_source": "sdio",
                "raw_hwid": hwid,
            })
        # Also try USB\VID_XXXX&PID_XXXX
        m = re.search(r'VID_([0-9A-Fa-f]{4})&PID_([0-9A-Fa-f]{4})', hwid)
        if m:
            vid, pid = int(m.group(1), 16), int(m.group(2), 16)
            results.append({
                "bus": "usb", "vid": vid, "did": pid,
                "class": cls, "_source": "sdio",
                "raw_hwid": hwid,
            })
    print(f"  [SDIO] parsed {len(results)} VID/DID pairs")
    return results


def load_unified_csv(path: Path) -> list[dict]:
    """Load unified CSV (pci_ids + usb_ids)."""
    if not path.exists():
        print(f"  [SKIP] Unified CSV not found: {path}")
        return []
    results = []
    with open(path, newline="", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for row in reader:
            try:
                vid = int(row.get("vid_hex", "0"), 16) if row.get("vid_hex") else int(row["vid"])
                did = int(row.get("did_hex", "0"), 16) if row.get("did_hex") else int(row["did"])
                family = row.get("family", "GenericPCI")
                source = row.get("source", "unknown")
                results.append({
                    "bus": "pci" if "pci" in source.lower() else "usb",
                    "vid": vid, "did": did,
                    "class": family,
                    "family_hint": family,
                    "_source": f"csv_{source}",
                })
            except (ValueError, KeyError):
                pass
    print(f"  [CSV] {len(results)} entries")
    return results


# ─── Label heuristics (espelha kernel heuristic_card) ───────────────────

def classify_by_vendor(vid: int, did: int, bus: str = "pci") -> dict:
    """Heuristic classification — same logic as kernel heuristic_card()."""
    # Known vendors
    NVIDIA = 0x10DE
    AMD = 0x1002
    INTEL = 0x8086
    REALTEK = 0x10EC
    BROADCOM = 0x14E4
    ATHEROS = 0x168C
    QUALCOMM = 0x17CB
    MARVELL = 0x11AB
    VIRTIO = 0x1AF4
    REDHAT = 0x1B36
    QEMU = 0x1234

    # Network vendors
    net_vendors = {REALTEK, BROADCOM, MARVELL, QUALCOMM, 0x10B7, 0x10E8,
                   0x1022, 0x11AD, 0x14E4, 0x16C6, 0x1A56, 0x1D6A, 0x8086}

    if bus == "usb":
        # USB devices → usually no PCI family
        return {
            "family": "usb_xhci",
            "fw": "-",
            "agent": "UsbDriverAgent",
            "caps": ["USB_HOST"],
            "next": "bind_usb_host",
        }

    if vid == INTEL:
        if did & 0xFFFC == 0x1000 or did & 0xFF00 == 0x1500:
            return {"family": "intel_e1000", "fw": "-", "agent": "NetAgent",
                    "caps": ["NET"], "next": "bind_network"}
        if did & 0xFF00 == 0x2400 or did & 0xF000 == 0xA000:
            return {"family": "intel_iwlwifi", "fw": "intel/iwlwifi", "agent": "WifiAgent",
                    "caps": ["WIFI", "NET", "NEEDS_FW", "SCAN"], "next": "load_firmware"}
        if did & 0xFF00 == 0x2600:
            return {"family": "intel_hda", "fw": "-", "agent": "HdaAudioAgent",
                    "caps": ["AUDIO"], "next": "bind_audio"}
        if did & 0xFF00 == 0x2200:
            return {"family": "usb_xhci", "fw": "-", "agent": "UsbDriverAgent",
                    "caps": ["USB_HOST", "CAPTURE"], "next": "bind_usb_host"}
        if did & 0xFF00 == 0x1900 or did == 0x1912:
            return {"family": "intel_i915", "fw": "i915", "agent": "DisplayAgent",
                    "caps": ["DISPLAY", "COMPUTE", "NEEDS_FW"], "next": "load_firmware"}
        if did & 0xFF00 == 0x1A00:
            return {"family": "storage_ata", "fw": "-", "agent": "DiskAgent",
                    "caps": ["STORAGE"], "next": "bind_storage"}

    if vid == NVIDIA:
        return {"family": "nvidia_gpu", "fw": "nvidia/gp108", "agent": "GpuBackend",
                "caps": ["DISPLAY", "COMPUTE", "NEEDS_FW"], "next": "load_firmware"}

    if vid == AMD:
        return {"family": "amd_gpu", "fw": "amdgpu", "agent": "GpuBackend",
                "caps": ["DISPLAY", "COMPUTE", "NEEDS_FW"], "next": "load_firmware"}

    if vid == REALTEK:
        return {"family": "realtek_eth", "fw": "-", "agent": "NetAgent",
                "caps": ["NET"], "next": "bind_network"}

    if vid in {ATHEROS, 0x0CF3}:
        return {"family": "atheros_wifi", "fw": "ath9k", "agent": "WifiAgent",
                "caps": ["WIFI", "NET", "NEEDS_FW", "SCAN"], "next": "load_firmware"}

    if vid == BROADCOM:
        return {"family": "broadcom_wifi", "fw": "brcmfmac", "agent": "WifiAgent",
                "caps": ["WIFI", "NET", "NEEDS_FW", "SCAN"], "next": "load_firmware"}

    if vid in {VIRTIO, REDHAT}:
        if did == 0x1041:
            return {"family": "virtio_net", "fw": "-", "agent": "NetAgent",
                    "caps": ["NET"], "next": "bind_network"}
        if did == 0x1050:
            return {"family": "virtio_gpu", "fw": "-", "agent": "DisplayAgent",
                    "caps": ["DISPLAY", "COMPUTE"], "next": "ready"}

    if vid == QEMU:
        return {"family": "qemu_vga", "fw": "-", "agent": "DisplayAgent",
                "caps": ["DISPLAY"], "next": "ready"}

    if vid in net_vendors:
        return {"family": "realtek_eth", "fw": "-", "agent": "NetAgent",
                "caps": ["NET"], "next": "bind_network"}

    # Default fallback: use bus to guess
    if bus == "usb":
        return {"family": "usb_xhci", "fw": "-", "agent": "UsbDriverAgent",
                "caps": ["USB_HOST"], "next": "bind_usb_host"}

    return {"family": "pci_bridge", "fw": "-", "agent": "PlatformAgent",
            "caps": [], "next": "observe_only"}


# ─── Main unifier ───────────────────────────────────────────────────────

def unify() -> list[dict]:
    """Load all sources, deduplicate, classify, return unified dataset."""
    print("=" * 60)
    print("  HW Expert v4 — Dataset Unifier")
    print("=" * 60)

    # Load seed table (kernel's known devices — gold standard labels)
    seed = [row_to_sample(r) for r in seed_table_rows()]
    print(f"\n  [SEED] {len(seed)} kernel-known devices (gold labels)")

    # Build lookup for exact VID/DID → seed label (prevents duplicates)
    known = {}
    for s in seed:
        vid = s["meta"]["vid"]
        did = s["meta"]["did"]
        s["meta"]["source"] = "kernel_seed"
        known[(vid, did)] = s

    # Load and classify all sources
    sources_data = []
    wdm = load_wdm(SOURCES["wdm"])
    sources_data.extend(wdm)
    sdio = load_sdio(SOURCES["sdio"])
    sources_data.extend(sdio)
    csv_data = load_unified_csv(SOURCES["unified_csv"])
    sources_data.extend(csv_data)

    # ZERO dedup — cada HWID string vira uma amostra de treino.
    # O v4 usa (vid,did) como entrada, mas diferentes subsistemas/revisões
    # no dataset ajudam o modelo a generalizar melhor.
    # As únicas repetições removidas são strings HWID idênticas dentro da mesma fonte.
    seen_exact = set()
    unified = []
    for item in sources_data:
        bus = item.get("bus", "pci")
        vid = item.get("vid", 0)
        did = item.get("did", 0)
        source = item.get("_source", "unknown")
        raw = item.get("raw_hwid", item.get("hwid", ""))

        # Remove apenas strings literalmente idênticas (ex: 2 .inf com o mesmo HWID)
        exact_key = (source, raw, bus, vid, did)
        if exact_key in seen_exact:
            continue
        seen_exact.add(exact_key)

        # Skip seed devices (kernel gold standard não é fonte raw)
        if (vid, did) in known and source != "kernel_seed":
            continue

        # Classify
        label = classify_by_vendor(vid, did, bus)

        # Build sample
        caps_bits = 0
        for c in label.get("caps", []):
            caps_bits |= CAPS.get(c, 0)

        sample = {
            "x": pack_vid_did(vid, did),
            "y": {
                "family": FAMILY.get(label["family"], 0),
                "fw_id": FW.get(label["fw"], 0),
                "agent_id": AGENT.get(label["agent"], 0),
                "caps_bits": caps_bits,
                "next_action": NEXT.get(label["next"], 8),
            },
            "meta": {
                "vid": vid,
                "did": did,
                "bus": bus,
                "class": label["family"],
                "source": item.get("_source", "unknown"),
            }
        }
        unified.append(sample)

    # Combine seed + unified
    all_samples = seed + unified
    print(f"\n  [UNIFIED] {len(unified)} HWIDs classified by heuristics")
    print(f"  [TOTAL]   {len(all_samples)} training samples")

    return all_samples


def main():
    TARGET.mkdir(parents=True, exist_ok=True)

    dataset = unify()

    # Write dataset
    ds_path = TARGET / "dataset.json"
    with open(ds_path, "w", encoding="utf-8") as f:
        json.dump({"samples": dataset}, f, indent=1)
    print(f"\n  Wrote {ds_path}")

    # Write vocab
    vocab = {
        "family": list(FAMILY.keys()),
        "fw": list(FW.keys()),
        "agent": list(AGENT.keys()),
        "next": list(NEXT.keys()),
        "caps": list(CAPS.keys()),
    }
    vocab_path = TARGET / "vocab.json"
    with open(vocab_path, "w", encoding="utf-8") as f:
        json.dump(vocab, f, indent=2)
    print(f"  Wrote {vocab_path}")

    # Stats
    source_counts = Counter(s["meta"].get("source", "unknown") for s in dataset)
    class_counts = Counter(s["meta"].get("class", "unknown") for s in dataset)
    print(f"\n  Stats:")
    print(f"  By source: {dict(source_counts.most_common(10))}")
    print(f"  By class (top 20):")
    for cls, count in class_counts.most_common(20):
        print(f"    {cls}: {count}")


if __name__ == "__main__":
    main()
