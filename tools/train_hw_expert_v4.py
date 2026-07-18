#!/usr/bin/env python3
"""HW Expert v4 — classificador HWID → HwCapabilityCard (mesmo schema do kernel).

NÃO treina free-text. Entrada = VID/DID empacotados (como v3).
Saída = heads tipados alinhados a crates/k_ai/src/hw_capability.rs:
  family_id, fw_id, agent_id, caps_bits, next_action

Uso:
  python tools/train_hw_expert_v4.py --dry-run
  python tools/train_hw_expert_v4.py --epochs 50 --hidden 128

O dataset seed vem das mesmas tabelas/heurísticas do kernel (plug-and-play).
Expandir depois com SDIO + pci.ids + firmware metadata.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "tools" / "target"

# Espelha k_ai::hw_capability::HwFamily (u8)
FAMILY = {
    "unknown": 0,
    "intel_e1000": 1,
    "virtio_net": 2,
    "realtek_eth": 3,
    "intel_iwlwifi": 4,
    "realtek_wifi": 5,
    "atheros_wifi": 6,
    "broadcom_wifi": 7,
    "nvidia_gpu": 8,
    "intel_i915": 9,
    "amd_gpu": 10,
    "qemu_vga": 11,
    "virtio_gpu": 12,
    "usb_xhci": 13,
    "intel_hda": 14,
    "storage_ata": 15,
    "pci_bridge": 16,
}

# fw_id vocab pequeno (estável)
FW = {
    "-": 0,
    "intel/iwlwifi": 1,
    "rtlwifi": 2,
    "ath9k": 3,
    "brcmfmac": 4,
    "nvidia/gp108": 5,
    "i915": 6,
    "amdgpu": 7,
}

AGENT = {
    "HwBridgeAgent": 0,
    "NetAgent": 1,
    "WifiAgent": 2,
    "DisplayAgent": 3,
    "GpuBackend": 4,
    "UsbDriverAgent": 5,
    "HdaAudioAgent": 6,
    "DiskAgent": 7,
    "PlatformAgent": 8,
}

NEXT = {
    "ready": 0,
    "load_firmware": 1,
    "bind_network": 2,
    "bind_wifi_scan": 3,
    "bind_gpu_compute": 4,
    "bind_usb_host": 5,
    "bind_audio": 6,
    "bind_storage": 7,
    "observe_only": 8,
}

CAPS = {
    "NET": 1 << 0,
    "WIFI": 1 << 1,
    "DISPLAY": 1 << 2,
    "COMPUTE": 1 << 3,
    "AUDIO": 1 << 4,
    "USB_HOST": 1 << 5,
    "STORAGE": 1 << 6,
    "NEEDS_FW": 1 << 7,
    "SCAN": 1 << 8,
    "CAPTURE": 1 << 9,
}


def pack_vid_did(vid: int, did: int, vocab: int = 64) -> list[int]:
    """Mesmo packing do v3 — entrada do classificador."""
    return [(vid >> 8) % vocab, vid % vocab, (did >> 8) % vocab, did % vocab]


def seed_table_rows() -> list[dict]:
    """Seed = tabela direta do kernel (build_card table_lookup)."""
    rows = [
        dict(vid=0x8086, did=0x100E, family="intel_e1000", fw="-", agent="NetAgent",
             caps=["NET"], next="bind_network"),
        dict(vid=0x8086, did=0x100F, family="intel_e1000", fw="-", agent="NetAgent",
             caps=["NET"], next="bind_network"),
        dict(vid=0x1AF4, did=0x1041, family="virtio_net", fw="-", agent="NetAgent",
             caps=["NET"], next="bind_network"),
        dict(vid=0x10EC, did=0x8139, family="realtek_eth", fw="-", agent="NetAgent",
             caps=["NET"], next="bind_network"),
        dict(vid=0x1234, did=0x1111, family="qemu_vga", fw="-", agent="DisplayAgent",
             caps=["DISPLAY"], next="ready"),
        dict(vid=0x1AF4, did=0x1050, family="virtio_gpu", fw="-", agent="DisplayAgent",
             caps=["DISPLAY", "COMPUTE"], next="ready"),
        dict(vid=0x8086, did=0x2723, family="intel_iwlwifi", fw="intel/iwlwifi", agent="WifiAgent",
             caps=["WIFI", "NET", "NEEDS_FW", "SCAN"], next="load_firmware"),
        dict(vid=0x8086, did=0x2725, family="intel_iwlwifi", fw="intel/iwlwifi", agent="WifiAgent",
             caps=["WIFI", "NET", "NEEDS_FW", "SCAN"], next="load_firmware"),
        dict(vid=0x10DE, did=0x1C82, family="nvidia_gpu", fw="nvidia/gp108", agent="GpuBackend",
             caps=["DISPLAY", "COMPUTE", "NEEDS_FW"], next="load_firmware"),
        dict(vid=0x8086, did=0x1912, family="intel_i915", fw="i915", agent="DisplayAgent",
             caps=["DISPLAY", "COMPUTE", "NEEDS_FW"], next="load_firmware"),
        dict(vid=0x8086, did=0xA170, family="intel_hda", fw="-", agent="HdaAudioAgent",
             caps=["AUDIO"], next="bind_audio"),
        dict(vid=0x8086, did=0xA2AF, family="usb_xhci", fw="-", agent="UsbDriverAgent",
             caps=["USB_HOST", "CAPTURE"], next="bind_usb_host"),
    ]
    return rows


def row_to_sample(row: dict, vocab: int = 64) -> dict:
    caps_bits = 0
    for c in row["caps"]:
        caps_bits |= CAPS[c]
    return {
        "x": pack_vid_did(row["vid"], row["did"], vocab),
        "y": {
            "family": FAMILY[row["family"]],
            "fw_id": FW[row["fw"]],
            "agent_id": AGENT[row["agent"]],
            "caps_bits": caps_bits,
            "next_action": NEXT[row["next"]],
        },
        "meta": row,
    }


def build_dataset(vocab: int = 64) -> list[dict]:
    samples = [row_to_sample(r, vocab) for r in seed_table_rows()]
    # Amplia com heurística vendor genérica (wifi vendors)
    for vid, fam, fw in [
        (0x168C, "atheros_wifi", "ath9k"),
        (0x14E4, "broadcom_wifi", "brcmfmac"),
        (0x0BDA, "realtek_wifi", "rtlwifi"),
    ]:
        samples.append(row_to_sample(dict(
            vid=vid, did=0x0032, family=fam, fw=fw, agent="WifiAgent",
            caps=["WIFI", "NET", "NEEDS_FW", "SCAN"], next="load_firmware",
        ), vocab))
    return samples


def main():
    ap = argparse.ArgumentParser(description="HW Expert v4 — HWID→HwCapabilityCard")
    ap.add_argument("--epochs", type=int, default=50)
    ap.add_argument("--hidden", type=int, default=128)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--export-json", action="store_true",
                    help="Exporta dataset seed JSON (sem treinar)")
    args = ap.parse_args()

    print("=" * 60)
    print("  HW Expert v4 - HWID -> HwCapabilityCard (AIOS PnP)")
    print("=" * 60)
    print("  Schema: family | fw_id | agent_id | caps_bits | next_action")
    print("  Kernel: crates/k_ai/src/hw_capability.rs")
    print("  NAO free-text / NAO hash%64 como rotulo final")

    ds = build_dataset()
    print(f"  Seed samples: {len(ds)}")

    out = TARGET / "hw_expert_v4_seed.json"
    TARGET.mkdir(parents=True, exist_ok=True)
    if args.export_json or args.dry_run:
        with open(out, "w", encoding="utf-8") as f:
            json.dump({"schema": list(FAMILY.keys()), "samples": ds}, f, indent=2)
        print(f"  Wrote {out}")

    if args.dry_run:
        print("  [DRY-RUN] OK — treino completo: plugar BitNetLM multi-head neste script")
        print("  Próximo: heads separados (family CE, fw CE, agent CE, next CE, caps BCE)")
        return

    print("  [TODO] Treino multi-head ainda não ligado ao BitNetLM export.")
    print("  Use --dry-run / --export-json até o export .bitnet v4 estar pronto.")
    print(f"  (epochs={args.epochs} hidden={args.hidden} reservados)")


if __name__ == "__main__":
    main()
