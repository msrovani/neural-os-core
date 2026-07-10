#!/usr/bin/env python3
"""Consolida caches SDIO e treina modelo preditor de registradores."""
import sys, json, csv, time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))

TARGET = Path("target")

def main():
    cache_files = sorted(TARGET.glob("DP_*_inf.json"))
    print(f"Building dataset from {len(cache_files)} cache files...")

    all_hwids = {}
    classes = set()
    providers = set()

    for cf in cache_files:
        with open(cf) as f:
            entries = json.load(f)
        for entry in entries:
            for hwid in entry["hwids"]:
                if hwid["type"] != "PCI":
                    continue
                try:
                    vid = int(hwid["vid"], 16)
                    did = int(hwid["did"], 16)
                except ValueError:
                    continue
                key = (vid, did)
                if key not in all_hwids:
                    all_hwids[key] = {
                        "vid": vid, "did": did,
                        "class": entry.get("device_class", ""),
                        "provider": entry.get("provider", ""),
                    }
            if entry.get("device_class"): classes.add(entry["device_class"])
            if entry.get("provider"): providers.add(entry["provider"])

    print(f"PCI unicos: {len(all_hwids)}")
    print(f"Classes: {len(classes)}")
    print(f"Providers: {len(providers)}")

    with open(TARGET / "sdio_hwids_all.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["vid_hex","did_hex","vid_dec","did_dec","class","provider"])
        for key, info in sorted(all_hwids.items()):
            w.writerow([f"{info['vid']:04X}", f"{info['did']:04X}", info["vid"], info["did"], info["class"], info["provider"]])
    print(f"CSV salvo: target/sdio_hwids_all.csv")

    # Prepara amostras para treino
    from train_hw_register_predictor import build_register_dataset, train_model
    devices = list(all_hwids.values())
    fake_entries = [{"hwids": [{"type": "PCI", "vid": f"{d['vid']:04X}", "did": f"{d['did']:04X}"}]} for d in devices]
    samples = build_register_dataset(fake_entries)
    print(f"Amostras de treino: {len(samples)}")

    train_model(samples, epochs=100, batch_size=8192)
    print("Treino concluido!")

if __name__ == "__main__":
    main()
