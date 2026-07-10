#!/usr/bin/env python3
"""Verifica se os HWIDs extraidos dos .bin sao validos."""
import json
from collections import Counter

with open("target/sdio_bin_hwids.json") as f:
    data = json.load(f)

print(f"Total: {len(data)} entries")

# Check for known NVIDIA GPUs
nvidia = [d for d in data if d["vid"] == "10DE"]
print(f"\nNVIDIA (10DE): {len(nvidia)} devices")
for d in nvidia[:25]:
    print(f"  DID={d['did']}")

# Check for known AMD GPUs
amd = [d for d in data if d["vid"] == "1002"]
print(f"\nAMD (1002): {len(amd)} devices")
for d in amd[:25]:
    print(f"  DID={d['did']}")

# Check for known Intel
intel = [d for d in data if d["vid"] == "8086"]
print(f"\nIntel (8086): {len(intel)} devices")
for d in intel[:25]:
    print(f"  DID={d['did']}")

# Check if we have GTX 1050 (10DE:1C82)
gtx1050 = [d for d in data if d["vid"] == "10DE" and d["did"] == "1C82"]
print(f"\nGTX 1050 found: {len(gtx1050) > 0}")

# Check for RTL8139 (10EC:8139)
rtl8139 = [d for d in data if d["vid"] == "10EC" and d["did"] == "8139"]
print(f"RTL8139 found: {len(rtl8139) > 0}")

# Top vendors
vendors = Counter(d["vid"] for d in data)
print("\nTop vendors:")
for v, c in vendors.most_common(20):
    print(f"  {v}: {c}")

# Check for duplicates with our existing SDIO data
with open("target/sdio_hwids_all.csv") as f:
    existing = set()
    import csv
    for row in csv.DictReader(f):
        existing.add((row["vid_hex"], row["did_hex"]))

bin_set = set((d["vid"], d["did"]) for d in data)
overlap = len(existing & bin_set)
new = len(bin_set - existing)
print(f"\nOverlap with existing SDIO: {overlap}")
print(f"New from .bin: {new}")
