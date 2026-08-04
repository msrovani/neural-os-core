#!/usr/bin/env python3
"""relabel_hw_expert_v4_vendor.py — RELABEL the HW Expert v4 dataset with a
VENDOR-SPECIFIC DRIVER-FAMILY ground truth derived from the OFFICIAL DEVICE
NAMES in pci.ids / usb.ids (already parsed, no downloads).

Why vendor-specific (v3) instead of generic class (v2):
  The v2 generic-class relabel (dataset_class_v2.json) duplicated a signal the
  kernel already has (the PCI class byte) and left 52.4% in 'other'. The honest
  target for the kernel's agent/fw/caps decode (crates/k_ai/src/hw_capability.rs
  prediction_to_card) is the VENDOR-SPECIFIC DRIVER FAMILY: intel_eth vs
  realtek_eth vs iwlwifi..., the granularity of the 8 firmware labels
  (iwlwifi/rtlwifi/ath9k/brcmfmac/nvidia/gp108/i915/amdgpu) and the 16-entry
  HwFamily enum the kernel actually decodes.

Taxonomy (21 entries; 0 = unknown, never trained; 20 = 'other' fallback):
  see FAMILY in vocab_class_v3.json. 19 specific families:
    1  intel_eth     2  realtek_eth    3  broadcom_eth
    4  virtio        5  intel_wifi     6  realtek_wifi
    7  atheros_wifi  8  broadcom_wifi  9  nvidia_gpu
    10 amd_gpu       11 intel_gpu      12 audio_hda
    13 usb_host      14 storage        15 bridge
    16 qemu_vga      17 chelsio_eth    18 mellanox_eth
    19 marvell_eth
  Deviations from the proposed 16-family list (documented):
    + qemu_vga   — kernel HwFamily::QemuVga (QEMU harness, 1234:1111)
    + chelsio_eth / mellanox_eth / marvell_eth — 433+195+~90 pci_ids devices
      (3.2%) are real, large vendor-specific Ethernet driver families
      (cxgb4/mlx5/mv643xx) that would otherwise all land in 'other'.

Rule architecture (priority-ordered):
  1. OVERRIDE (vid,did) — curated QEMU/chipset pairs (mirrors kernel table)
  2. vendor-specific rules — vendor (pci.ids 'vendor' field) AND device
     keywords; wifi-before-eth per vendor (names like '802.11ac WiFi
     Adapter' must not fall into eth)
  3. vendor-agnostic class rules — USB/storage/audio/bridge keywords
  4. name-only GPU keywords — board partners (Elsa GLoria, etc.) whose names
     carry the NVIDIA/AMD silicon brand
  5. WDM class fallback (vendor-agnostic classes only)
  6. 'other' — classified, but no driver family in the name

Outputs (NEW files; v2 + originals untouched):
  models/hw_expert/v4/dataset_class_v3.json
  models/hw_expert/v4/vocab_class_v3.json

Usage:  python tools/relabel_hw_expert_v4_vendor.py
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
V4 = ROOT / "models" / "hw_expert" / "v4"
TGT = ROOT / "tools" / "target"

# ─── Family taxonomy (id → name) ──────────────────────────────────────────
# Kernel HwFamily correspondence (crates/k_ai/src/hw_capability.rs):
#   intel_eth→IntelE1000, virtio→VirtioNet/VirtioGpu, realtek_eth→RealtekEth,
#   intel_wifi→IntelIwlWifi, realtek_wifi→RealtekWifi, atheros_wifi→AtherosWifi,
#   broadcom_wifi→BroadcomWifi, nvidia_gpu→NvidiaGpu, intel_gpu→IntelI915,
#   amd_gpu→AmdGpu, qemu_vga→QemuVga, usb_host→UsbHostXhci,
#   audio_hda→IntelHda, storage→StorageAta, bridge→PciBridge,
#   chelsio_eth/mellanox_eth/marvell_eth/broadcom_eth→(no enum entry yet;
#   decode = NetAgent via agent head, matching kernel class-0x02 fallback)
FAMILY = [
    "unknown",        # 0  no ground truth / excluded from data
    "intel_eth",      # 1  Intel Ethernet (e1000/PRO/1000/i210/i219/8257x/X540)
    "realtek_eth",    # 2  Realtek Ethernet (RTL8139/8168/8111/8125)
    "broadcom_eth",   # 3  Broadcom Ethernet (NetXtreme/BCM57xx)
    "virtio",         # 4  virtio (net/gpu/blk; agent refined per device)
    "intel_wifi",     # 5  Intel Wireless (Wi-Fi 6 AX/AC 9xxx/iwlwifi)
    "realtek_wifi",   # 6  Realtek Wireless (RTL8723/8821/8812/rtlwifi)
    "atheros_wifi",   # 7  Qualcomm/Atheros Wireless (QCA6174/AR9x/ath9k)
    "broadcom_wifi",  # 8  Broadcom Wireless (BCM43xx/brcmfmac)
    "nvidia_gpu",     # 9  NVIDIA (GeForce/Quadro/RTX/GTX → nvidia/gp108)
    "amd_gpu",        # 10 AMD/ATI (Radeon/FirePro → amdgpu)
    "intel_gpu",      # 11 Intel Graphics (HD/UHD/Iris/Arc → i915)
    "audio_hda",      # 12 audio controllers (Intel HDA/HD Audio/Audio device)
    "usb_host",       # 13 USB host controllers (xHCI/EHCI/UHCI/OHCI)
    "storage",        # 14 storage controllers (ATA/SATA/AHCI/NVMe/SCSI/RAID)
    "bridge",         # 15 bridges (host/PCI/PCIe root/LPC/SMBus/chipset)
    "qemu_vga",       # 16 QEMU/Bochs VGA (1234:1111; kernel QemuVga)
    "chelsio_eth",    # 17 Chelsio Ethernet (T4/T5/T6 Unified Wire → cxgb4)
    "mellanox_eth",   # 18 Mellanox Ethernet/IB (ConnectX → mlx5)
    "marvell_eth",    # 19 Marvell Ethernet (88E1111/Alaska/mv64360)
    "other",          # 20 classified, but no vendor driver family in name
]
FAM2ID = {n: i for i, n in enumerate(FAMILY)}

# Reused heads (IDENTICAL sets to vocab_class_v2.json — only FAMILY changes).
FW = ["-", "intel/iwlwifi", "rtlwifi", "ath9k", "brcmfmac", "nvidia/gp108", "i915", "amdgpu"]
AGENT = ["HwBridgeAgent", "NetAgent", "WifiAgent", "DisplayAgent", "GpuBackend",
         "UsbDriverAgent", "HdaAudioAgent", "DiskAgent", "PlatformAgent"]
NEXT = ["ready", "load_firmware", "bind_network", "bind_wifi_scan", "bind_gpu_compute",
        "bind_usb_host", "bind_audio", "bind_storage", "observe_only"]
CAPS = ["NET", "WIFI", "DISPLAY", "COMPUTE", "AUDIO", "USB_HOST",
        "STORAGE", "NEEDS_FW", "SCAN", "CAPTURE"]
NET, WIFI, DISPLAY, COMPUTE, AUDIO, USB_HOST, STORAGE, NEEDS_FW, SCAN, CAPTURE = range(10)

# ─── vendor regexes (matched against pci.ids 'vendor' field, lowercased) ──
INTEL = r"\bintel\b"
REALTEK = r"realtek"
BROADCOM = r"broadcom"
NVIDIA = r"nvidia"
AMD = r"advanced micro devices|\bamd\b|ati technologies"
ATHEROS = r"atheros|qualcomm"
CHELSIO = r"chelsio"
MELLANOX = r"mellanox"
MARVELL = r"marvell"
ANY = r""

# ─── Rule list (family, vendor_rx, name_rx) — first match wins ────────────
# Priority: vendor-specific before vendor-agnostic; wifi before eth (same
# vendor); nForce chipset before NVIDIA GPU; AMD Data Fabric before amd_gpu.
RULES = [
    # --- NVIDIA: nForce/Ck8x chipset = bridges, not GPUs ---
    ("bridge", NVIDIA, r"nforce|nforce2|mcp\d\d|ck804|nforce 6\d\d"),
    # --- AMD: Data Fabric / NTB = chipset interconnect (class 0x06) ---
    ("bridge", AMD, r"data fabric|\bntb\b|vntb"),
    # --- Intel ---
    ("intel_wifi", INTEL, r"wireless|wi-?fi|iwlwifi|802\.11|8260|8265|9560|9260|9462|"
                          r"ac 9\d{3}|ax\d{3}|\bwifi\b"),
    ("intel_eth", INTEL, r"ethernet|e1000|pro/1000|pro/100|8254\d|8256\d|8257\d|"
                         r"8258\d|8259\d|82562|\bi21\d\b|\bi22\d\b|\bi226\b|"
                         r"x540|x550|x520|x710|gigabit|10/100|10g|1000base"),
    ("intel_gpu", INTEL, r"graphics|\buhd\b|iris|\barc\b|hd graphics|gma\b|810e|"
                         r"815|845g|865g|915g|965g|g35|g45|q35|q45|\bgpu\b|"
                         r"hasswell|broxton|elkhart|gemini lake|skylake-u|kabylake-u"),
    # --- Realtek ---
    ("realtek_wifi", REALTEK, r"8723|8821|8812|8811|8822|8852|8192|wireless|wi-?fi|802\.11"),
    ("realtek_eth", REALTEK, r"8139|8168|8111|8125|8169|8411|ethernet|gigabit|10/100"),
    # --- Qualcomm/Atheros ---
    ("atheros_wifi", ATHEROS, r"wireless|wi-?fi|802\.11|ar9\d{3}|ar5\d{3}|ar1\d{3}|"
                              r"qca\d{3,4}|ath\d{3}|wcn\d{4}|ipq\d{4}"),
    # --- Broadcom ---
    ("broadcom_wifi", BROADCOM, r"bcm43\d+|wireless|wi-?fi|802\.11|\bwifi\b"),
    ("broadcom_eth", BROADCOM, r"netxtreme|ethernet|gigabit|bcm57\d+|bcm59\d+|"
                               r"bcm54\d+|bcm56\d+|switch asic|10g|\bnic\b|homepna"),
    # --- NVIDIA / AMD GPUs ---
    ("nvidia_gpu", NVIDIA, r"geforce|quadro|tesla|\brtx\b|\bgtx\b|\bgt\b|graphics|"
                           r"\bgpu\b|\bvga\b|display controller|video controller|"
                           r"\bga1\d\d|\btu\d\d\d|\bad1\d\d|\bgv1\d\d|\bgf1\d\d|"
                           r"nvs\b|cuda|nvidia t\d{3}"),
    ("amd_gpu", AMD, r"radeon|firepro|firegl|\brx\b|rx \d{3,4}|vega|navi|gcn|"
                     r"hd \d{4}|hd 8\d{3}|graphics|display adapter|\bgpu\b|"
                     r"polaris|raven|stoney|cezanne|renoir|instinct|mobility radeon|"
                     r"kaveri|mullins|carrizo|bristol ridge"),
    # --- storage-only vendors (Adaptec/HighPoint/Areca/3ware/Promise/ATTO
    #     make nothing but storage; LSI/Symbios except modems/FPGA) ---
    ("storage", r"adaptec|highpoint|areca|3ware|promise technology|atto technology",
     r"."),
    ("storage", r"lsi logic|symbios logic",
     r"^(?!.*(modem|ethernet|network|atm|fpsc|fpga|wan))"),
    # --- audio-only vendors (ESS/C-Media sound chips) ---
    ("audio_hda", r"c-media|ess technology", r"."),
    # --- other vendor-specific Ethernet (big driver families) ---
    ("chelsio_eth", CHELSIO, r"ethernet|unified wire|t\d{3}|\bt5\b|\bt6\b|nic|10g|40g|100g"),
    ("mellanox_eth", MELLANOX, r"connectx|ethernet|infiniband|10g|25g|40g|100g|"
                               r"mlx\d|mt2\d{4}|mt4\d{4}|bluefield|switchx|linkx|connect-ib"),
    ("marvell_eth", MARVELL, r"ethernet|gigabit|88e\d{4}|alaska|mv64\d+|mvebu|mrvl|10g|xaui"),
    # --- vendor-agnostic class rules ---
    ("virtio", ANY, r"virtio"),
    ("usb_host", ANY, r"xhci|ehci|uhci|ohci|usb host|usb3|usb 3|usb controller|"
                      r"universal host controller|open host controller|"
                      r"enhanced host controller|usb 2\.0|usb 1\.1|usb xhci|usb 3\.0|"
                      r"root hub|thunderbolt|usb4|\busb\b|usb\d"),
    ("storage", ANY, r"sata|ahci|\bnvme\b|scsi|raid|sas\d|\bsas\b|mass storage|"
                     r"storage controller|disk controller|ide interface|\bide\b|"
                     r"\bata\b|pata\b|nand\b|emmc|smart storage|\bi2o\b|"
                     r"fibre ?channel|\bhba\b|mega?raid|fasttrak|expresssas|"
                     r"sil \d{3}|sii \d{3}|88se\d{4}|u160|u320|fusion-mpt|"
                     r"card reader|flash media|memory card|smart card|sd/mmc|"
                     r"xd-picture|sd host|mmc controller|flash controller|"
                     r"iomemory|pblaze|ssd controller|nvme controller"),
    ("audio_hda", ANY, r"audio|sound|codec|\bhda\b|hd audio|azalia|ac97|"
                       r"high definition audio|\bi2s\b"),
    ("bridge", ANY, r"host bridge|pci bridge|pcie|pci express|root port|root complex|"
                    r"isa bridge|pci-to-pci|pci to pci|southbridge|northbridge|"
                    r"cardbus|pc card controller|pcmcia|\bbridge\b|lpc|dmi\d?|"
                    r"smbus|i2c|gpio|spi|memory controller hub|uninorth|"
                    r"switch upstream port|switch downstream port|\bcxl\b|"
                    r"486 pci chipset|pentium chipset|apollo pro"),
    # --- name-only GPU brands (board partners: Elsa GLoria, Gainward...) ---
    ("nvidia_gpu", ANY, r"geforce|quadro|gloria|tesla[ \d]|\brtx\b|\bgtx\b|\bnvs\b"),
    ("amd_gpu", ANY, r"radeon|firepro|firegl|mobility radeon|ati rage|ati mach"),
]
OTHER = "other"

# ─── Windows class → family (WDM fallback; vendor-agnostic classes only) ───
# Windows classes carry no vendor info, so they can only resolve the
# vendor-agnostic families. Everything else → 'other' (kept, honest).
WIN_CLASS_FAMILY = {
    "system": "bridge",          # chipset/system devices ≈ bridges
    "usb": "usb_host", "usbdevice": "usb_host",
    "audio": "audio_hda", "media": "audio_hda", "sound": "audio_hda",
    "hdc": "storage", "scsiadapter": "storage", "diskdrive": "storage",
}

# ─── Curated (vid,did) overrides (mirrors kernel table_lookup) ─────────────
OVERRIDE = {
    # Intel 440FX/PIIX chipset (QEMU default)
    (0x8086, 0x1237): "bridge",   # 440FX - 82441FX PMC [Natoma] (host bridge)
    (0x8086, 0x7000): "bridge",   # 82371SB PIIX3 ISA [Natoma/Triton II]
    (0x8086, 0x7010): "storage",  # 82371SB PIIX3 IDE [Natoma/Triton II]
    (0x8086, 0x7113): "bridge",   # 82371AB/EB/MB PIIX4 ACPI
    # QEMU virtio (name resolves too; kept for stability)
    (0x1AF4, 0x1000): "virtio",   # Virtio network device
    (0x1AF4, 0x1041): "virtio",   # Virtio 1.0 network
    (0x1AF4, 0x1001): "virtio",   # Virtio block
    (0x1AF4, 0x1050): "virtio",   # Virtio 1.0 GPU
    # QEMU Bochs VGA (absent from official pci.ids)
    (0x1234, 0x1111): "qemu_vga",
    # QEMU XHCI
    (0x1B36, 0x000D): "usb_host",
}

# ─── family → (fw_id, agent_id, caps_bits, next_action) ────────────────────
# Explicit port of kernel prediction_to_card / heuristic_card semantics at
# VENDOR-family granularity (crates/k_ai/src/hw_capability.rs).
def family_y(fam: str) -> dict:
    if fam in ("intel_eth", "realtek_eth", "broadcom_eth",
               "chelsio_eth", "mellanox_eth", "marvell_eth"):
        return {"fw_id": 0, "agent_id": 1,          # NetAgent
                "caps_bits": 1 << NET, "next_action": 2}   # bind_network
    if fam in ("intel_wifi", "realtek_wifi", "atheros_wifi", "broadcom_wifi"):
        fw = {"intel_wifi": 1, "realtek_wifi": 2, "atheros_wifi": 3,
              "broadcom_wifi": 4}[fam]  # iwlwifi/rtlwifi/ath9k/brcmfmac
        return {"fw_id": fw, "agent_id": 2,          # WifiAgent
                "caps_bits": (1 << WIFI) | (1 << NET) | (1 << NEEDS_FW) | (1 << SCAN),
                "next_action": 1}                     # load_firmware
    if fam == "nvidia_gpu":
        return {"fw_id": 5, "agent_id": 3,          # nvidia/gp108, DisplayAgent
                "caps_bits": (1 << DISPLAY) | (1 << COMPUTE) | (1 << NEEDS_FW),
                "next_action": 1}                     # load_firmware
    if fam == "amd_gpu":
        return {"fw_id": 7, "agent_id": 3,          # amdgpu, DisplayAgent
                "caps_bits": (1 << DISPLAY) | (1 << COMPUTE) | (1 << NEEDS_FW),
                "next_action": 1}
    if fam == "intel_gpu":
        return {"fw_id": 6, "agent_id": 3,          # i915, DisplayAgent
                "caps_bits": (1 << DISPLAY) | (1 << COMPUTE) | (1 << NEEDS_FW),
                "next_action": 1}
    if fam == "qemu_vga":
        return {"fw_id": 0, "agent_id": 3,          # DisplayAgent
                "caps_bits": 1 << DISPLAY, "next_action": 0}   # ready
    if fam == "audio_hda":
        return {"fw_id": 0, "agent_id": 6,          # HdaAudioAgent
                "caps_bits": 1 << AUDIO, "next_action": 6}     # bind_audio
    if fam == "usb_host":
        return {"fw_id": 0, "agent_id": 5,          # UsbDriverAgent
                "caps_bits": (1 << USB_HOST) | (1 << CAPTURE),
                "next_action": 5}                     # bind_usb_host
    if fam == "storage":
        return {"fw_id": 0, "agent_id": 7,          # DiskAgent
                "caps_bits": 1 << STORAGE, "next_action": 7}   # bind_storage
    # bridge / qemu_vga(other) / other: kernel default observe-only
    return {"fw_id": 0, "agent_id": 8,              # PlatformAgent
            "caps_bits": 0, "next_action": 8}        # observe_only


# Per-device agent/caps refinement for the ONE mixed family: virtio.
# Mirrors kernel table entries (1af4:1000/1041 net, 1050 gpu).
VIRTIO_REFINE = {
    (0x1AF4, 0x1000): (1, 1 << NET, 2),                       # net → NetAgent
    (0x1AF4, 0x1041): (1, 1 << NET, 2),                       # net (modern)
    (0x1AF4, 0x1001): (7, 1 << STORAGE, 7),                   # block → DiskAgent
    (0x1AF4, 0x1002): (0, 0, 8),                              # balloon
    (0x1AF4, 0x1003): (0, 0, 8),                              # console
    (0x1AF4, 0x1004): (0, 0, 8),                              # rng
    (0x1AF4, 0x1005): (7, 1 << STORAGE, 7),                   # scsi → DiskAgent
    (0x1AF4, 0x1009): (0, 0, 8),                              # 9p
    (0x1AF4, 0x1050): (3, (1 << DISPLAY) | (1 << COMPUTE), 0),  # gpu → DisplayAgent
    (0x1AF4, 0x1042): (3, (1 << DISPLAY) | (1 << COMPUTE), 0),  # gpu (modern)
}


def rx(*parts):
    return re.compile("|".join(parts), re.I)


def classify(vendor: str, name: str) -> str | None:
    """(vendor, name) → family via RULES (first match). None if no rule."""
    if not name:
        return None
    for fam, vrx, nrx in RULES:
        if (not vrx or re.search(vrx, vendor, re.I)) and re.search(nrx, name, re.I):
            return fam
    return None


def build_ground_truth():
    """(vid,did) → (family, source). Primary: official pci.ids/usb.ids names.
    Fallback: WDM class. Returns dict + per-source device counters."""
    pci = {}
    for e in json.load(open(TGT / "pci_ids.json", encoding="utf-8")):
        try:
            pci[(int(e["vid"], 16), int(e["did"], 16))] = (
                e["name"].strip(), e.get("vendor", "").strip())
        except (KeyError, ValueError):
            continue
    usb = {}
    for e in json.load(open(TGT / "usb_ids.json", encoding="utf-8")):
        try:
            usb[(int(e["vid"], 16), int(e["did"], 16))] = (
                e["name"].strip(), e.get("vendor", "").strip())
        except (KeyError, ValueError):
            continue
    wdm = {}
    for e in json.load(open(ROOT / "models" / "WDM" / "hwids.json", encoding="utf-8")):
        cls = str(e.get("class", "")).lower()
        if cls in WIN_CLASS_FAMILY:
            try:
                wdm[(int(e["vid"]), int(e["did"]))] = WIN_CLASS_FAMILY[cls]
            except (KeyError, ValueError):
                continue

    gt = {}
    src_cnt = Counter()
    for dev in sorted(set(pci) | set(usb) | set(wdm) | set(OVERRIDE)):
        if dev in OVERRIDE:
            gt[dev] = (OVERRIDE[dev], "override")
            src_cnt["override"] += 1
            continue
        if dev in pci:
            name, vendor = pci[dev]
            fam = classify(vendor, name)
            if fam:
                gt[dev] = (fam, "pci_ids")
                src_cnt["pci_ids"] += 1
                continue
        if dev in usb:
            name, vendor = usb[dev]
            fam = classify(vendor, name)
            if fam:
                gt[dev] = (fam, "usb_ids")
                src_cnt["usb_ids"] += 1
                continue
        if dev in wdm:
            gt[dev] = (wdm[dev], "wdm")
            src_cnt["wdm"] += 1
    # devices with a name but NO family signal → "other" (classified, honest)
    for dev in set(pci) | set(usb):
        if dev in gt:
            continue
        name = (pci.get(dev) or usb.get(dev))[0]
        if name:
            gt[dev] = ("other", "pci_ids" if dev in pci else "usb_ids")
            src_cnt["other_named"] += 1
    return gt, src_cnt


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    gt, src_cnt = build_ground_truth()
    print(f"ground truth devices classified : {len(gt)}")

    data = json.load(open(V4 / "dataset_class_v2.json", encoding="utf-8"))
    samples = data["samples"] if isinstance(data, dict) else data
    print(f"input samples (v2)              : {len(samples)}")

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
        agent, caps, nxt = y["agent_id"], y["caps_bits"], y["next_action"]
        if y_fam == "virtio" and dev in VIRTIO_REFINE:
            agent, caps, nxt = VIRTIO_REFINE[dev]
        ns = {
            "x": list(s["x"][:4]),
            "y": {"family": FAM2ID[y_fam], "fw_id": y["fw_id"], "agent_id": agent,
                  "caps_bits": caps, "next_action": nxt},
            "meta": {"vid": s["meta"]["vid"], "did": s["meta"]["did"],
                     "source": s["meta"].get("source", "?"),
                     "family": y_fam, "gt_source": gt_src,
                     "fw": FW[y["fw_id"]], "agent": AGENT[agent],
                     "caps": [CAPS[b] for b in range(10) if caps & (1 << b)],
                     "next": NEXT[nxt]},
        }
        kept.append(ns)

    print(f"unique devices in dataset       : {len(dev_all)}")
    print(f"unique devices WITH label       : {len(dev_lab)}  "
          f"({len(dev_lab) / max(len(dev_all), 1) * 100:.1f}%)")
    print(f"samples kept                    : {len(kept)}  dropped: {dropped}")
    print("ground-truth source (samples)   :", dict(src_sample))
    print("\nv3 family distribution (samples):")
    for i, n in enumerate(FAMILY):
        print(f"  {i:2d} {n:14s} {fam_cnt[n]:6d}  {fam_cnt[n] / max(len(kept), 1) * 100:5.1f}%")

    # ── coverage gates ────────────────────────────────────────────────────
    pci_named_devs = 0
    pci_named_specific = 0
    for dev in dev_lab:
        e = None
        for src in ("pci_ids",):
            if gt.get(dev, (None,))[1] == src:
                e = dev
                break
        if e is None:
            continue
        pci_named_devs += 1
        if gt[dev][0] != OTHER:
            pci_named_specific += 1
    cov = pci_named_specific / max(pci_named_devs, 1) * 100
    print(f"\nGATE coverage: {pci_named_specific}/{pci_named_devs} pci.ids-named "
          f"devices specific = {cov:.1f}% (want >=65%)")

    mx = fam_cnt.most_common(1)[0]
    print(f"GATE class distribution: max class '{mx[0]}' = "
          f"{mx[1] / max(len(kept), 1) * 100:.1f}% (want <=40%)")

    # ── canonical devices (name-based expectations from the spec) ─────────
    canon = {
        (0x8086, 0x100E): "intel_eth",   # Intel 82540EM Gigabit Ethernet
        (0x10EC, 0x8139): "realtek_eth", # RTL-8139
        (0x1AF4, 0x1000): "virtio",      # Virtio network device
        (0x8086, 0x2723): "intel_wifi",  # Wi-Fi 6 AX200
        (0x168C, 0x003E): "atheros_wifi",# QCA6174
        (0x1234, 0x1111): "qemu_vga",    # Bochs/QEMU VGA
        (0x8086, 0x1237): "bridge",      # 440FX host bridge
        (0x8086, 0x7000): "bridge",      # PIIX3 ISA
        (0x8086, 0x7010): "storage",     # PIIX3 IDE (documented: storage OK)
        (0x8086, 0x7113): "bridge",      # PIIX4 ACPI
        (0x1B36, 0x000D): "usb_host",    # QEMU XHCI
    }
    bad = []
    for (v, d), want in canon.items():
        got = gt.get((v, d), (None,))[0]
        mark = "OK " if got == want else "FAIL"
        print(f"  canonical {v:04x}:{d:04x} -> {got:14s} (want {want}) {mark}")
        if got != want:
            bad.append(f"{v:04x}:{d:04x}->{got}")
    if bad:
        print(f"  canonical MISMATCHES: {bad}")

    out = {"samples": kept}
    V4.mkdir(parents=True, exist_ok=True)
    v3_path = V4 / "dataset_class_v3.json"
    v3_path.write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")
    vocab = {"family": FAMILY, "fw": FW, "agent": AGENT, "next": NEXT, "caps": CAPS}
    vocab_path = V4 / "vocab_class_v3.json"
    vocab_path.write_text(json.dumps(vocab, ensure_ascii=False, indent=2), encoding="utf-8")

    h1 = hashlib.sha256(v3_path.read_bytes()).hexdigest()
    h2 = hashlib.sha256(vocab_path.read_bytes()).hexdigest()
    print(f"\nwrote {v3_path.name} ({len(kept)} samples) sha256={h1}")
    print(f"wrote {vocab_path.name} sha256={h2}")
    print(f"\nunique devices specific: {pci_named_specific} / {pci_named_devs} "
          f"= {cov:.1f}% (gate 65%)")


if __name__ == "__main__":
    main()
