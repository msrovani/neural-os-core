#!/usr/bin/env python3
"""relabel_hw_expert_v4_class.py — RELABEL the HW Expert v4 dataset with
INDEPENDENT ground truth (device class from pci.ids / usb.ids names), breaking
the circularity of the old classify_by_vendor labels.

Why name-based class:
  The raw pci.ids (v2.2) class section is ONLY a 22-entry global class-name
  table ("C 02  Network controller"); it attaches NO class code to individual
  devices and has NO subclass lines. tools/download_hw_databases.py drops the
  section entirely. SDIO hwids in tools/target/sdio_hwids.json have
  class="unknown" for all 16,126 entries; WDM classes are mostly generic
  ("pci"/"usb"/"system"). The only independent, per-device ground truth left
  is the OFFICIAL DEVICE NAME from pci.ids / usb.ids — classified here with an
  explicit, documented keyword table (+ a small curated override table for
  names that lack class vocabulary, mirroring the kernel's table_lookup).

Taxonomy (12 families; id 0 = unknown, never trained):
  see FAMILY_CLASS in vocab_class_v2.json.

Outputs (NEW files; originals untouched):
  models/hw_expert/v4/dataset_class_v2.json
  models/hw_expert/v4/vocab_class_v2.json

Usage:  python tools/relabel_hw_expert_v4_class.py
"""
from __future__ import annotations

import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
V4 = ROOT / "models" / "hw_expert" / "v4"
TGT = ROOT / "tools" / "target"

# ─── New family taxonomy (id → name) ──────────────────────────────────────
# Mirrors the kernel's class-byte dispatch (hw_capability.rs heuristic_card:
# 0x02 net, 0x0D wifi, 0x03 display, 0x01 storage, 0x04 audio/mm, 0x0C/0x03 usb,
# 0x0C/0x00 serial, 0x06 bridge, 0x09 input) at GENERIC class granularity.
FAMILY_CLASS = [
    "unknown",       # 0  no ground truth / unclassified (excluded from data)
    "network",       # 1  Ethernet NICs (PCI class 0x02, non-0x80)
    "wifi",          # 2  wireless/WLAN (PCI 0x02/0x80, 0x0D, bluetooth)
    "display",       # 3  VGA/GPU (PCI 0x03)
    "storage",       # 4  IDE/SATA/AHCI/NVMe/SCSI (PCI 0x01)
    "audio",         # 5  HDA/sound (PCI 0x04/0x03)
    "usb",           # 6  USB host controllers (PCI 0x0C/0x03)
    "serial_io",     # 7  serial/SMBus/I2C/FireWire/modem (PCI 0x0C/0x00, 0x07)
    "bridge",        # 8  PCI/ISA/ACPI bridges (PCI 0x06)
    "multimedia",    # 9  video capture/webcam/encoder (PCI 0x04/0x00)
    "input",         # 10 keyboard/mouse/gamepad (PCI 0x09)
    "other",         # 11 memory/processor/encryption/signal/etc (classified,
    #                 #    but no family signal in the name)
]
FAM2ID = {n: i for i, n in enumerate(FAMILY_CLASS)}

# Reused heads (same vocab as v4 — model heads unchanged for these).
FW = ["-", "intel/iwlwifi", "rtlwifi", "ath9k", "brcmfmac", "nvidia/gp108", "i915", "amdgpu"]
AGENT = ["HwBridgeAgent", "NetAgent", "WifiAgent", "DisplayAgent", "GpuBackend",
         "UsbDriverAgent", "HdaAudioAgent", "DiskAgent", "PlatformAgent"]
NEXT = ["ready", "load_firmware", "bind_network", "bind_wifi_scan", "bind_gpu_compute",
        "bind_usb_host", "bind_audio", "bind_storage", "observe_only"]
CAPS = ["NET", "WIFI", "DISPLAY", "COMPUTE", "AUDIO", "USB_HOST",
        "STORAGE", "NEEDS_FW", "SCAN", "CAPTURE"]

# ─── caps bit indices (same order as CAPS) ────────────────────────────────
NET, WIFI, DISPLAY, COMPUTE, AUDIO, USB_HOST, STORAGE, NEEDS_FW, SCAN, CAPTURE = range(10)

# ─── PCI class code → family (documented; used for decisions + Windows maps)
# ─── (pci.ids only lists these as class NAMES, no device mapping — kept as
# ───  the documented class-byte dispatch for reference & WDM fallback)
PCI_CLASS_FAMILY = {
    0x00: "unknown",      # Unclassified device
    0x01: "storage",      # Mass storage controller
    0x02: "network",      # Network controller (0x80 subclass → wifi via name)
    0x03: "display",      # Display controller
    0x04: "audio",        # Multimedia controller (0x03 sub → audio; 0x00 → mm)
    0x05: "other",        # Memory controller
    0x06: "bridge",       # Bridge
    0x07: "serial_io",    # Communication controller
    0x08: "other",        # Generic system peripheral
    0x09: "input",        # Input device controller
    0x0A: "other",        # Docking station
    0x0B: "other",        # Processor
    0x0C: "serial_io",    # Serial bus controller (0x03 sub → usb via name)
    0x0D: "wifi",         # Wireless controller
    0x0E: "other",        # Intelligent controller
    0x0F: "other",        # Satellite communications controller
    0x10: "other",        # Encryption controller
    0x11: "other",        # Signal processing controller
    0x12: "other",        # Processing accelerators
    0x13: "other",        # Non-Essential Instrumentation
    0x40: "other",        # Coprocessor
    0xFF: "unknown",      # Unassigned class
}

# ─── Windows class name → family (SDIO/WDM fallback; explicit dict) ───────
WIN_CLASS_FAMILY = {
    "network adapters": "network", "net": "network", "network": "network",
    "display adapters": "display", "display": "display", "video": "display",
    "usb": "usb", "usbdevice": "usb",
    "system": "bridge",          # chipset/system devices ≈ bridges
    "bluetooth": "wifi",
    "camera": "multimedia", "image": "multimedia",
    "audio": "audio", "media": "audio", "sound": "audio",
    "scsiadapter": "storage", "hdc": "storage", "diskdrive": "storage",
    "modem": "serial_io", "ports": "serial_io",
    "keyboard": "input", "mouse": "input", "hidclass": "input",
}

# ─── Name keyword → family (PRIMARY ground truth; priority-ordered) ───────
# First match wins. Deliberately ordered: wifi before network ("Wireless
# Network Adapter"), display before storage ("video" contains "ide"),
# bridge before serial (PIIX ISA/PIIX4 ACPI are 0x06 bridges).
NAME_RULES = [
    ("wifi",  re.compile(r"wi-?fi|wireless|wlan|802\.11|bluetooth|wifi adapter", re.I)),
    ("display", re.compile(r"\bvga\b|svga|display adapter|display controller|graphics|"
                           r"\bgpu\b|framebuffer|geforce|\brtx\b|\bgtx\b|quadro|tesla|"
                           r"radeon|vesa|video controller", re.I)),
    ("audio", re.compile(r"audio|sound|codec|ac97|azalia|\bhda\b", re.I)),
    ("storage", re.compile(r"sata|ahci|\bnvme\b|\bscsi\b|\braid\b|mass storage|"
                           r"ide controller|ide interface|disk controller|floppy|"
                           r"smart storage|\bi2o\b|\bnand\b|\bsd\b host|emmc", re.I)),
    ("usb",   re.compile(r"\buhci\b|\bohci\b|\behci\b|\bxhci\b|usb host|usb3|"
                         r"usb 3|usb controller|\busb\b", re.I)),
    ("network", re.compile(r"ethernet|network|nic adapter|\blan\b|gigabit|10/100|"
                           r"100base|1000base|fast ethernet|e1000|rtl81|net device|"
                           r"net adapter|ethernet adapter", re.I)),
    ("bridge", re.compile(r"bridge|pci-to-pci|pcie root|root port|isa bridge|host bridge|"
                          r"southbridge|northbridge|cardbus|pcmcia|\bpii\b|isa$|\bisa\b", re.I)),
    ("serial_io", re.compile(r"serial|uart|smbus|\bi2c\b|firewire|1394|parallel|"
                             r"16550|communication controller|modem|\blpt\b", re.I)),
    ("multimedia", re.compile(r"multimedia|webcam|\bcamera\b|capture|tv tuner|"
                              r"video device|encoder|decoder|\bdsp\b", re.I)),
    ("input", re.compile(r"keyboard|mouse|touchpad|gamepad|joystick|digitizer|"
                         r"\bpen\b|input controller|\bhid\b", re.I)),
]

# ─── Curated (vid,did) → family overrides ─────────────────────────────────
# Devices whose official pci.ids NAME lacks class vocabulary, resolved by
# known silicon (mirrors kernel table_lookup's curated pairs). Documented.
OVERRIDE = {
    # Intel 440FX/PIIX chipset (QEMU default) — class 0x06 / 0x01
    (0x8086, 0x1237): "bridge",   # 440FX - 82441FX PMC [Natoma] (host bridge)
    (0x8086, 0x7000): "bridge",   # 82371SB PIIX3 ISA [Natoma/Triton II]
    (0x8086, 0x7010): "storage",  # 82371SB PIIX3 IDE [Natoma/Triton II]
    (0x8086, 0x7113): "bridge",   # 82371AB/EB/MB PIIX4 ACPI (class 0x0680)
    # QEMU virtio (1af4:1000 resolves via name anyway — kept for stability)
    (0x1AF4, 0x1000): "network",  # Virtio network device
    # QEMU Bochs VGA (absent from official pci.ids; known silicon)
    (0x1234, 0x1111): "display",  # Virtual SVGA / Bochs VGA
}


def classify_name(name: str) -> str | None:
    """Name → family via NAME_RULES (first match). None if no rule hits."""
    if not name:
        return None
    for fam, rx in NAME_RULES:
        if rx.search(name):
            return fam
    return None


def build_ground_truth():
    """(vid,did) → (family, source) for every device we can classify.
    Primary: official name from pci.ids / usb.ids. Fallback: WDM class.
    Returns dict + per-source coverage counters."""
    pci = {}
    for e in json.load(open(TGT / "pci_ids.json", encoding="utf-8")):
        try:
            pci[(int(e["vid"], 16), int(e["did"], 16))] = e["name"].strip()
        except (KeyError, ValueError):
            continue
    usb = {}
    for e in json.load(open(TGT / "usb_ids.json", encoding="utf-8")):
        try:
            usb[(int(e["vid"], 16), int(e["did"], 16))] = e["name"].strip()
        except (KeyError, ValueError):
            continue
    wdm = {}
    for e in json.load(open(ROOT / "models" / "WDM" / "hwids.json", encoding="utf-8")):
        cls = str(e.get("class", "")).lower()
        if cls in WIN_CLASS_FAMILY:
            wdm[(int(e["vid"]), int(e["did"]))] = WIN_CLASS_FAMILY[cls]

    gt = {}
    src_cnt = Counter()
    for dev in sorted(set(pci) | set(usb) | set(wdm) | set(OVERRIDE)):
        vid, did = dev
        if dev in OVERRIDE:
            gt[dev] = (OVERRIDE[dev], "override")
            src_cnt["override"] += 1
            continue
        if dev in pci:
            fam = classify_name(pci[dev])
            if fam:
                gt[dev] = (fam, "pci_ids")
                src_cnt["pci_ids"] += 1
                continue
        if dev in usb:
            fam = classify_name(usb[dev])
            if fam:
                gt[dev] = (fam, "usb_ids")
                src_cnt["usb_ids"] += 1
                continue
        if dev in wdm:
            gt[dev] = (wdm[dev], "wdm")
            src_cnt["wdm"] += 1
    # devices in pci/usb with a name but NO family signal → "other" (still
    # classified, real ground truth of the generic kind)
    for dev in set(pci) | set(usb):
        if dev in gt:
            continue
        name = pci.get(dev) or usb.get(dev)
        if name:
            gt[dev] = ("other", "pci_ids" if dev in pci else "usb_ids")
            src_cnt["other_named"] += 1
    return gt, src_cnt


def family_y(fam: str) -> dict:
    """family → (fw_id, agent_id, caps_bits, next_action) — explicit port of
    kernel prediction_to_card / heuristic_card semantics for GENERIC families.
    Caps bit order = CAPS list above."""
    if fam == "network":
        return {"fw_id": 0, "agent_id": 1,  # NetAgent
                "caps_bits": 1 << NET, "next_action": 2}        # bind_network
    if fam == "wifi":
        return {"fw_id": 0, "agent_id": 2,  # WifiAgent
                "caps_bits": (1 << WIFI) | (1 << NET) | (1 << NEEDS_FW) | (1 << SCAN),
                "next_action": 1}                                 # load_firmware
    if fam == "display":
        return {"fw_id": 0, "agent_id": 3,  # DisplayAgent
                "caps_bits": 1 << DISPLAY, "next_action": 0}      # ready
    if fam == "storage":
        return {"fw_id": 0, "agent_id": 7,  # DiskAgent
                "caps_bits": 1 << STORAGE, "next_action": 7}      # bind_storage
    if fam == "audio":
        return {"fw_id": 0, "agent_id": 6,  # HdaAudioAgent
                "caps_bits": 1 << AUDIO, "next_action": 6}        # bind_audio
    if fam == "usb":
        return {"fw_id": 0, "agent_id": 5,  # UsbDriverAgent
                "caps_bits": (1 << USB_HOST) | (1 << CAPTURE),
                "next_action": 5}                                 # bind_usb_host
    if fam == "multimedia":
        return {"fw_id": 0, "agent_id": 0,  # HwBridgeAgent
                "caps_bits": 1 << CAPTURE, "next_action": 8}      # observe_only
    # bridge / serial_io / input / other: kernel default (observe-only)
    agent = 8 if fam == "bridge" else 0   # bridge → PlatformAgent
    return {"fw_id": 0, "agent_id": agent,
            "caps_bits": 0, "next_action": 8}                     # observe_only


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    gt, src_cnt = build_ground_truth()
    print(f"ground truth devices classified : {len(gt)}")

    data = json.load(open(V4 / "dataset.json", encoding="utf-8"))
    samples = data["samples"] if isinstance(data, dict) else data
    print(f"input samples                  : {len(samples)}")

    dev_all = set()
    dev_lab = set()
    fam_cnt = Counter()
    src_sample = Counter()
    kept = []
    dropped = 0
    for s in samples:
        dev = (s["meta"]["vid"], s["meta"]["did"])
        dev_all.add(dev)
        fam = gt.get(dev)
        if fam is None:
            dropped += 1
            continue
        dev_lab.add(dev)
        y_fam, gt_src = fam
        src_sample[gt_src] += 1
        fam_cnt[y_fam] += 1
        y = family_y(y_fam)
        ns = {
            "x": list(s["x"][:4]),
            "y": {"family": FAM2ID[y_fam], "fw_id": y["fw_id"], "agent_id": y["agent_id"],
                  "caps_bits": y["caps_bits"], "next_action": y["next_action"]},
            "meta": {"vid": s["meta"]["vid"], "did": s["meta"]["did"],
                     "source": s["meta"].get("source", "?"),
                     "family": y_fam, "gt_source": gt_src,
                     "fw": FW[y["fw_id"]], "agent": AGENT[y["agent_id"]],
                     "caps": [CAPS[b] for b in range(10) if y["caps_bits"] & (1 << b)],
                     "next": NEXT[y["next_action"]]},
        }
        kept.append(ns)

    print(f"unique devices in dataset       : {len(dev_all)}")
    print(f"unique devices WITH class label : {len(dev_lab)}  "
          f"({len(dev_lab) / max(len(dev_all), 1) * 100:.1f}%)")
    print(f"samples kept                    : {len(kept)}  dropped: {dropped}")
    print("ground-truth source (samples)   :", dict(src_sample))
    print("\nnew family distribution (samples):")
    for i, n in enumerate(FAMILY_CLASS):
        print(f"  {i:2d} {n:12s} {fam_cnt[n]:6d}  {fam_cnt[n] / max(len(kept), 1) * 100:5.1f}%")

    # self-check: canonical devices must resolve as expected
    canon = {
        (0x8086, 0x100E): "network", (0x1234, 0x1111): "display",
        (0x8086, 0x1237): "bridge", (0x8086, 0x7000): "bridge",
        (0x8086, 0x7010): "storage", (0x8086, 0x7113): "bridge",
        (0x1AF4, 0x1000): "network", (0x8086, 0x2723): "wifi",
        (0x168C, 0x003E): "wifi", (0x10EC, 0x8139): "network",
    }
    bad = [f"{v:04x}:{d:04x} -> {gt.get((v, d))} (want {w})"
           for (v, d), w in canon.items() if gt.get((v, d), (None,))[0] != w]
    assert not bad, f"canonical ground truth mismatch: {bad}"
    print("\ncanonical devices ground truth  : OK")

    out = {"samples": kept}
    V4.mkdir(parents=True, exist_ok=True)
    (V4 / "dataset_class_v2.json").write_text(
        json.dumps(out, ensure_ascii=False), encoding="utf-8")
    vocab = {"family": FAMILY_CLASS, "fw": FW, "agent": AGENT, "next": NEXT, "caps": CAPS}
    (V4 / "vocab_class_v2.json").write_text(
        json.dumps(vocab, ensure_ascii=False, indent=2), encoding="utf-8")
    print("\nwrote:")
    print("  models/hw_expert/v4/dataset_class_v2.json "
          f"({len(kept)} samples)")
    print("  models/hw_expert/v4/vocab_class_v2.json")
    print(f"\nunique devices with class: {len(dev_lab)} / {len(dev_all)} "
          f"= {len(dev_lab) / max(len(dev_all), 1) * 100:.1f}%")


if __name__ == "__main__":
    main()
