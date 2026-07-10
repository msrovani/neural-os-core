#!/usr/bin/env python3
"""Continua extracao dos DriverPacks restantes, apagando .7z apos uso."""
import os, re, json, subprocess, tempfile, shutil, time
from pathlib import Path

SZ = r"C:\Program Files\7-Zip\7z.exe"
SDIO = Path(r"C:\Users\msrov\Downloads\SDIO\drivers")
TARGET = Path("target")
TARGET.mkdir(exist_ok=True)

def parse_inf(text):
    data = {"hwids": [], "device_class": "", "provider": ""}
    for m in re.finditer(r'PCI\\VEN_(\w{4})&DEV_(\w{4})', text, re.I):
        data["hwids"].append({"type":"PCI", "vid":m.group(1), "did":m.group(2)})
    for m in re.finditer(r'USB\\VID_(\w{4})&PID_(\w{4})', text, re.I):
        data["hwids"].append({"type":"USB", "vid":m.group(1), "did":m.group(2)})
    for m in re.finditer(r'Class\s*=\s*(\w+)', text, re.I):
        data["device_class"] = m.group(1)
    for m in re.finditer(r'Provider\s*=\s*%([^%]+)%', text, re.I):
        data["provider"] = m.group(1)
    return data if data["hwids"] else None

def process(pack):
    cache = TARGET / f"{pack.stem}_inf.json"
    if cache.exists():
        return
    tmp = tempfile.mkdtemp()
    results = []
    try:
        r = subprocess.run([SZ, "l", str(pack)], capture_output=True, text=True, timeout=60)
        infs = []
        for line in r.stdout.split("\n"):
            parts = line.split()
            if len(parts) > 4 and parts[-1].lower().endswith(".inf"):
                infs.append(parts[-1])
        for inf_path in infs[:800]:
            od = os.path.join(tmp, os.path.dirname(inf_path))
            os.makedirs(od, exist_ok=True)
            subprocess.run([SZ, "e", str(pack), f"-o{od}", inf_path, "-y"],
                         capture_output=True, timeout=30)
            extracted = os.path.join(od, os.path.basename(inf_path))
            if os.path.exists(extracted):
                try:
                    with open(extracted, "r", encoding="utf-8", errors="replace") as f:
                        p = parse_inf(f.read())
                    if p: results.append(p)
                except: pass
                try: os.remove(extracted)
                except: pass
        with open(cache, "w") as f: json.dump(results, f)
        nh = sum(len(e["hwids"]) for e in results)
        print(f"[OK] {pack.name}: {len(results)} .inf, {nh} HWIDs", flush=True)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
        try:
            sz = pack.stat().st_size
            pack.unlink()
            print(f"[DEL] {pack.name} ({sz//(1024*1024)}MB)", flush=True)
        except: pass

def main():
    cached = {p.stem for p in TARGET.glob("DP_*_inf.json")}
    packs = sorted(SDIO.glob("DP_*.7z"))
    pending = [p for p in packs if p.stem not in cached]
    print(f"Total: {len(packs)} packs | Cache: {len(cached)} | Pending: {len(pending)}", flush=True)

    t0 = time.time()
    for i, p in enumerate(pending):
        t1 = time.time()
        process(p)
        elapsed = time.time() - t1
        remaining = len(pending) - i - 1
        eta = remaining * elapsed / 60 if elapsed > 0 else 0
        print(f"  [{i+1}/{len(pending)}] {elapsed:.0f}s | ETA: {eta:.0f}min", flush=True)

    print(f"\nDone: {time.time()-t0:.0f}s", flush=True)

if __name__ == "__main__":
    main()
