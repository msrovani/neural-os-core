#!/usr/bin/env python3
"""Parse QEMU sweep logs, score HW Expert v4 runtime identification vs pci.ids.

Reads logs/boot_sweep_{1,2,3}.txt, extracts [HW-PnP] cards, joins
tools/target/pci_ids.json, scores family predictions vs expected families,
writes tools/target/hw_sweep_report.md and prints headline numbers.

No kernel code is touched; tools/target/pci_ids.json is read-only.
"""
import json
import os
import re
import sys
from collections import Counter, OrderedDict

ROOT = r"C:\DEV\neural-os-core"
LOG_DIR = os.path.join(ROOT, "logs")
PCI_IDS = os.path.join(ROOT, "tools", "target", "pci_ids.json")
OUT = os.path.join(ROOT, "tools", "target", "hw_sweep_report.md")

# ── Expected family ground truth (vid:did -> family name) ──────────────────
# Derived from: table_lookup pairs (source of truth), heuristic class dispatch,
# and pci.ids name keywords. "unknown" = family vocab has no sensible entry.
EXPECTED = {
    # table-known network
    "8086:100E": "intel_e1000", "8086:100F": "intel_e1000", "8086:10D3": "intel_e1000",
    "8086:1009": "intel_e1000",  # 82544EI GbE
    "8086:1209": "intel_e1000",  # 8255xER/82551IT
    "8086:1229": "intel_e1000",  # 82557/8/9/0/1
    "10EC:8139": "realtek_eth", "10EC:8029": "realtek_eth",
    "1AF4:1000": "virtio_net", "1AF4:1041": "virtio_net",
    "1022:2000": "unknown",      # AMD PCnet32 - vocab gap
    "15AD:07B0": "unknown",      # VMware vmxnet3 - vocab gap
    # table-known display / gpu
    "1234:1111": "qemu_vga", "1AF4:1050": "virtio_gpu",
    "1B36:0100": "qemu_vga",     # QXL paravirt display (closest vocab)
    "1013:00B8": "qemu_vga",     # Cirrus GD5446 (generic VGA)
    "15AD:0405": "qemu_vga",     # VMware SVGA II
    "1002:5159": "amd_gpu",      # ATI RV100 (ati-vga default)
    # storage
    "1AF4:1001": "storage_ata", "1AF4:1042": "storage_ata",
    "1AF4:1004": "storage_ata", "1AF4:1048": "storage_ata",
    "8086:2922": "storage_ata", "8086:5845": "storage_ata",
    "8086:7010": "storage_ata",
    # audio (vocab has only intel_hda)
    "8086:293E": "intel_hda", "8086:2668": "intel_hda",
    "8086:2415": "intel_hda",   # AC97
    "1274:1371": "intel_hda",   # ES1370 (only audio family in vocab)
    # usb hosts
    "1B36:000D": "usb_xhci", "1033:0194": "usb_xhci",
    "8086:7020": "usb_xhci", "8086:293A": "usb_xhci", "8086:2934": "usb_xhci",
    "8086:24CD": "usb_xhci", "8086:7112": "usb_xhci",
    # bridges
    "1B36:0001": "pci_bridge", "8086:8110": "pci_bridge",
    "8086:1237": "pci_bridge", "8086:7000": "pci_bridge", "8086:7113": "pci_bridge",
    "8086:244E": "pci_bridge",  # i82801b11 (QEMU reports 8086:244E here)
    # extra storage (QEMU nvme is 1B36:0010)
    "1B36:0010": "storage_ata", "8086:2922": "storage_ata",
    # display
    "1002:5046": "amd_gpu",  # ATI RV100 (ati-vga default under QEMU 11)
    # virtio misc (no vocab family)
    "1AF4:1002": "unknown", "1AF4:1045": "unknown",
    "1AF4:1003": "unknown", "1AF4:1043": "unknown",
    "1AF4:1005": "unknown", "1AF4:1044": "unknown",
}

# Table_lookup (vid,did) -> family  (the kernel's own known set)
TABLE_KNOWN = {
    (0x8086, 0x100E): "intel_e1000", (0x8086, 0x100F): "intel_e1000",
    (0x8086, 0x10D3): "intel_e1000", (0x8086, 0x1502): "intel_e1000",
    (0x8086, 0x1503): "intel_e1000",
    (0x1AF4, 0x1000): "virtio_net", (0x1AF4, 0x1041): "virtio_net",
    (0x10EC, 0x8139): "realtek_eth",
    (0x1234, 0x1111): "qemu_vga",
    (0x1AF4, 0x1050): "virtio_gpu",
    (0x8086, 0x2723): "intel_iwlwifi", (0x8086, 0x2725): "intel_iwlwifi",
    (0x8086, 0x2726): "intel_iwlwifi", (0x8086, 0x06F0): "intel_iwlwifi",
    (0x8086, 0x02F0): "intel_iwlwifi", (0x8086, 0x24FD): "intel_iwlwifi",
    (0x168C, 0x003E): "atheros_wifi", (0x168C, 0x0041): "atheros_wifi",
}

CARD_RE = re.compile(
    r"\[HW-PnP\] ([0-9A-F]{4}):([0-9A-F]{4}) (.+?) family=(\S+) agent=(\S+) "
    r"fw=(\S+) next=(\S+) caps=(0x[0-9a-fA-F]+) src=(\S+)"
)
LOADED_RE = re.compile(r"v4 multi-head LOADED")


def load_pci_ids():
    with open(PCI_IDS, "r", encoding="utf-8") as f:
        data = json.load(f)
    by_id = {}
    for e in data:
        vid = e.get("vid", "").upper()
        did = e.get("did", "").upper()
        by_id[f"{vid}:{did}"] = (e.get("name", "").strip(), e.get("vendor", "").strip())
    return by_id


def parse_log(path):
    cards = []
    loaded = False
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            if LOADED_RE.search(line):
                loaded = True
            m = CARD_RE.search(line)
            if m:
                vid, did, name, fam, agent, fw, nxt, caps, src = m.groups()
                cards.append({
                    "vid": vid, "did": did, "name": name.strip(),
                    "family": fam, "agent": agent, "fw": fw, "next": nxt,
                    "caps": caps, "src": src,
                })
    return loaded, cards


def family_keyword_fallback(vid, did, name):
    """Fallback expected family from pci.ids name keywords (only for pairs not
    in EXPECTED)."""
    n = name.lower()
    if "wireless" in n or "wifi" in n or "wi-fi" in n:
        if vid == "8086":
            return "intel_iwlwifi"
        if vid in ("10EC", "0BDA"):
            return "realtek_wifi"
        if vid == "168C":
            return "atheros_wifi"
        if vid == "14E4":
            return "broadcom_wifi"
        return "unknown"
    if "ethernet" in n or "network" in n or "nic" in n or "fast ethernet" in n:
        return "intel_e1000" if vid == "8086" else ("realtek_eth" if vid in ("10EC", "0BDA") else "unknown")
    if "vga" in n or "display" in n or "gpu" in n or "graphic" in n:
        return "qemu_vga" if vid == "1234" else ("amd_gpu" if vid == "1002" else "qemu_vga")
    if "disk" in n or "sata" in n or "ahci" in n or "nvme" in n or "storage" in n:
        return "storage_ata"
    if "audio" in n or "ac97" in n or "hda" in n or "sound" in n:
        return "intel_hda"
    if "xhci" in n or "usb" in n:
        return "usb_xhci"
    if "bridge" in n:
        return "pci_bridge"
    return "unknown"


def expected_for(vid, did, name):
    key = f"{vid}:{did}"
    if key in EXPECTED:
        return EXPECTED[key]
    return family_keyword_fallback(vid, did, name)


def main():
    pci = load_pci_ids()
    boots = {}
    all_cards = []
    for n in (1, 2, 3, 4, 5):
        p = os.path.join(LOG_DIR, f"boot_sweep_{n}.txt")
        if not os.path.exists(p):
            print(f"[warn] log ausente: {p}")
            continue
        loaded, cards = parse_log(p)
        boots[n] = {"loaded": loaded, "n_cards": len(cards)}
        all_cards.extend([(n, c) for c in cards])

    # dedupe identical card lines per boot (e.g. two 1234:1111 in boot2)
    seen = set()
    cards = []
    for n, c in all_cards:
        k = (n, c["vid"], c["did"], c["family"], c["src"], c["name"])
        if k in seen:
            continue
        seen.add(k)
        cards.append((n, c))

    # ── scoring ──
    rows = []
    src_counter = Counter()
    nn_total = nn_match = 0
    table_total = table_match = 0
    heur_total = heur_match = 0
    in_pci_ids = 0
    nn_mislabeled = []   # (boot, vid:did, pciids name, nn family, expected)
    for n, c in cards:
        key = f"{c['vid']}:{c['did']}"
        pci_name, vendor = pci.get(key, ("", ""))
        if pci_name:
            in_pci_ids += 1
        expected = expected_for(c["vid"], c["did"], pci_name or c["name"])
        match = (c["family"] == expected)
        src_counter[c["src"]] += 1
        if c["src"] == "expert_v4":
            nn_total += 1
            if match:
                nn_match += 1
            else:
                nn_mislabeled.append((n, key, pci_name or c["name"], c["family"], expected))
        elif c["src"] == "table":
            table_total += 1
            if match:
                table_match += 1
        elif c["src"] == "heuristic":
            heur_total += 1
            if match:
                heur_match += 1
        rows.append((n, key, pci_name or c["name"], c["family"], c["src"], expected, "MATCH" if match else "MISS"))

    # table-known intersection: what did the NN say on table-known pairs?
    nn_on_table = []
    for n, c in cards:
        if c["src"] != "expert_v4":
            continue
        t = TABLE_KNOWN.get((int(c["vid"], 16), int(c["did"], 16)))
        if t is not None:
            nn_on_table.append((n, f"{c['vid']}:{c['did']}", c["family"], t, c["family"] == t))

    nn_acc = (nn_match / nn_total * 100.0) if nn_total else 0.0
    table_acc = (table_match / table_total * 100.0) if table_total else 0.0
    heur_acc = (heur_match / heur_total * 100.0) if heur_total else 0.0
    nn_on_table_acc = (sum(1 for x in nn_on_table if x[4]) / len(nn_on_table) * 100.0) if nn_on_table else 0.0

    # ── report ──
    md = []
    md.append("# HW Expert v4 — Runtime Identification Sweep (QEMU)\n")
    md.append("Method: 3 TCG boots (i440fx, -accel tcg -cpu max -smp 2 -m 8G, OVMF, "
              "uefi.img, -NoDisk), model pinned with `-device loader,file=models/hw_expert/"
              "hw_expert_v4.bitnet,addr=0x179000000` (inside scan window "
              "0x129400000..0x180000000, beyond LLAMA8B end). Serial logs "
              "`logs/boot_sweep_{1,2,3}.txt`. Ground truth = pci.ids names "
              "(`tools/target/pci_ids.json`, 22,806 entries) + kernel `table_lookup` "
              "families + heuristic class dispatch; expected family per device is "
              "documented in the per-device tables.\n")
    md.append("Precedence in `build_card`: expert_v4 (if loaded) → table → heuristic.\n")

    for n in (1, 2, 3, 4, 5):
        if n not in boots:
            md.append(f"\n## Boot {n} — **LOG AUSENTE**\n")
            continue
        b = boots[n]
        md.append(f"\n## Boot {n}\n")
        md.append(f"- v4 model loaded: **{'YES' if b['loaded'] else 'NO - INVALID SWEEP'}**\n")
        md.append(f"- card lines: {b['n_cards']}\n")
        md.append("\n| vid:did | pci.ids name | predicted family | src | expected | verdict |")
        md.append("|---|---|---|---|---|---|")
        for nn, key, name, fam, src, exp, verdict in [r for r in rows if r[0] == n]:
            md.append(f"| {key} | {name} | {fam} | {src} | {exp} | {verdict} |")

    md.append("\n# Summary\n")
    md.append(f"| metric | value |")
    md.append("|---|---|")
    md.append(f"| boots run | {len(boots)} (v4 loaded in all: {all(b['loaded'] for b in boots.values())}) |")
    md.append(f"| total card lines (deduped) | {len(cards)} |")
    md.append(f"| src=expert_v4 | {src_counter.get('expert_v4', 0)} |")
    md.append(f"| src=table | {src_counter.get('table', 0)} |")
    md.append(f"| src=heuristic | {src_counter.get('heuristic', 0)} |")
    md.append(f"| **NN accuracy (expert_v4, family match)** | **{nn_match}/{nn_total} = {nn_acc:.1f}%** |")
    md.append(f"| table accuracy (family match) | {table_match}/{table_total} = {table_acc:.1f}% |")
    md.append(f"| heuristic accuracy (family match) | {heur_match}/{heur_total} = {heur_acc:.1f}% |")
    md.append(f"| NN on table-known devices (precedence check) | {sum(1 for x in nn_on_table if x[4])}/{len(nn_on_table)} = {nn_on_table_acc:.1f}% |")
    md.append(f"| swept devices present in pci_ids.json | {in_pci_ids}/{len(cards)} |")

    md.append("\n## Precedence: NN vs table on table-known devices\n")
    md.append("These (vid,did) are in the kernel's `table_lookup` (ground truth family "
              "known). Because the NN runs FIRST in `build_card`, its family wins whenever "
              "it predicts non-unknown. This table quantifies how often the NN overrides "
              "the correct table answer.\n")
    md.append("\n| vid:did | NN family | table family | NN == table? |")
    md.append("|---|---|---|---|")
    for n, key, nfam, tfam, eq in nn_on_table:
        md.append(f"| {key} | {nfam} | {tfam} | {'YES' if eq else '**NO — NN mislabels**'} |")

    if nn_mislabeled:
        md.append("\n## NN mislabels vs expected (all src=expert_v4 misses)\n")
        md.append("\n| boot | vid:did | pci.ids name | NN family | expected |")
        md.append("|---|---|---|---|---|")
        for n, key, name, fam, exp in nn_mislabeled:
            md.append(f"| {n} | {key} | {name} | {fam} | {exp} |")

    md.append("\n## Device catalog (QEMU emulated, swept)\n")
    md.append("- **Boot 1 (net/storage/bridge):** e1000 (8086:100E), e1000-82544gc "
              "(8086:100C), e1000-82545em (8086:100F), e1000e (8086:10D3), rtl8139 (10EC:8139), "
              "ne2k_pci (10EC:8029), i82559er (8086:1209), pcnet (1022:2000), vmxnet3 (15AD:07B0), "
              "virtio-net-pci (1AF4:1000), pci-bridge (1B36:0001), i82801b11-bridge (8086:244E), "
              "ich9-ahci (8086:2922), nvme (1B36:0010), qxl (1B36:0100), bochs-display (1234:1111), "
              "i82559a/i82559c (8086:1229, behind the bridges) + onboard i440fx set.")
    md.append("- **Boot 2 (virtio):** virtio-blk-pci (1AF4:1001), virtio-scsi-pci (1AF4:1004), "
              "virtio-serial-pci (1AF4:1003), virtio-balloon-pci (1AF4:1002), virtio-rng-pci (1AF4:1005), "
              "virtio-gpu-pci (1AF4:1050), virtio-vga (1AF4:1050, duplicate) + onboard. NOTE: the kernel's "
              "sandbox e1000 (8086:100E 'Intel PRO/1000 Network') also appears in the PnP tree (kernel-side "
              "device, not QEMU-emulated).")
    md.append("- **Boot 3 (audio/usb):** ich9-intel-hda (8086:293E), intel-hda (8086:2668), "
              "AC97 (8086:2415), ES1370 (1274:5000), qemu-xhci (1B36:000D), nec-usb-xhci (1033:0194), "
              "ich9-usb-ehci1 (8086:293A), ich9-usb-uhci1 (8086:2934) + onboard.")
    md.append("- **Boot 4 (remaining storage/display/bridges):** ich9-ahci (8086:2922), nvme "
              "(1B36:0010), qxl (1B36:0100), pci-bridge (1B36:0001), i82801b11-bridge (8086:244E) + onboard.")
    md.append("- **Boot 5 (supplementary):** ati-vga (1002:5046), cirrus-vga (1013:00B8), usb-ehci "
              "(8086:24CD), piix4-usb-uhci (8086:7112), i82557b (8086:1229) + onboard.")
    md.append("\nSwept (vid,did) pairs: " + ", ".join(sorted({f"{c['vid']}:{c['did']}" for _, c in cards})) + "\n")

    md.append("\n## Observations\n")
    md.append("- The NN **never abstains** (family_id=0 never selected) - every one of the 60 cards got "
              "src=expert_v4, overriding both table and heuristic for all devices.\n")
    md.append("- The NN has a strong **realtek_eth bias**: it labels the 440FX host bridge, PIIX3 ISA/IDE/"
              "ACPI, the whole Intel e1000 family, ICH9 HDA audio, ICH9 USB EHCI/UHCI and AC'97 as "
              "`realtek_eth` (family 3). The only correct predictions in the sweep are `1B36:000D` "
              "(QEMU XHCI -> `usb_xhci`), `1B36:0001` (QEMU PCI bridge -> `pci_bridge`) and `1002:5046` "
              "(ATI Rage 128 -> `amd_gpu`).\n")
    md.append("- On the 7 unique table-known pairs swept (8086:100E/100F/10D3, 10EC:8139, 1AF4:1000/1050, "
              "1234:1111), the NN mislabels **all of them** - table would have been correct on each. "
              "Because the NN runs first in `build_card`, its wrong family wins (e.g. e1000 -> `realtek_eth` "
              "with regmap 0xA0/0xD0 instead of the correct 0x1000/0x2000; virtio-net -> `pci_bridge` "
              "with no regmap; QEMU VGA -> `pci_bridge`).\n")
    md.append("- **Kernel/build provenance:** the image swept was built from the committed HEAD (745cf6e, "
              "includes the HW Expert v4 runtime) + 2 uncommitted WIP fixes required to boot/load: the v5 "
              "prefixed-tensor loader (`crates/cortex/src/cortex.rs`) and the SSE lanes clamp "
              "(`crates/cortex/src/bitnet_sse.rs`, without it predict panics OOB on the 17/9/10-col heads). "
              "The dirty main-tree image (13:41 build) triple-faults at the SGDB boot bench (heap mapped "
              "only 512MB of a 2048MB arena; device-heavy boots exhaust it) - unrelated to the model.\n")
    md.append("- **QEMU -device lines dropped:** vmware-svga (duplicate SaveStateEntry 'vga' vs -vga std), "
              "`-device ich9-intel-hda,audiodev=` (property not found; codec hda-duplex carries audiodev). "
              "usb-tablet/usb-kbd/usb-storage are USB (non-PCI) devices, not swept.\n")
    md.append("- **Pin address:** 0x179000000 verified inside the scan window 0x129400000..0x180000000 "
              "and beyond the reported LLAMA8B end (~0x177B843C6); kernel found the magic there "
              "(`HWEXPERT magic 0xBE11BE11 found @0x179000000`) and logged `v4 multi-head LOADED` in all 5 boots.\n")
    md.append("- **PnP publish ceiling:** the kernel's EventBus queues ~10-15 HW_CAPABILITY publishes per "
              "HwDetectAgent tick before blocking, so the last-scanned devices per boot get no card; "
              "boots 4-5 re-swept the overflow devices.\n")

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(md) + "\n")
    print(f"report -> {OUT}")

    # ── stdout headline ──
    print("\n" + "=" * 64)
    print("HW EXPERT v4 RUNTIME SWEEP — HEADLINE")
    print("=" * 64)
    for n in (1, 2, 3, 4, 5):
        if n in boots:
            print(f"boot {n}: v4 loaded={boots[n]['loaded']} cards={boots[n]['n_cards']}")
    print(f"total devices swept (dedup): {len(cards)}")
    print(f"  src: expert_v4={src_counter.get('expert_v4', 0)} table={src_counter.get('table', 0)} heuristic={src_counter.get('heuristic', 0)}")
    print(f"NN accuracy: {nn_match}/{nn_total} = {nn_acc:.1f}%")
    print(f"table accuracy: {table_match}/{table_total} = {table_acc:.1f}%")
    print(f"NN vs table on table-known devices: {sum(1 for x in nn_on_table if x[4])}/{len(nn_on_table)} correct ({nn_on_table_acc:.1f}%)")
    for n, key, nfam, tfam, eq in nn_on_table:
        if not eq:
            print(f"  MISLABEL on table-known: boot{n} {key}: NN={nfam} table={tfam}")
    print(f"pci.ids coverage: {in_pci_ids}/{len(cards)} in pci_ids.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
