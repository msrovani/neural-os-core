#!/usr/bin/env python3
"""Baixa firmware NVIDIA GP108 do Ubuntu package mirror."""
import requests, re, os, sys, shutil, subprocess
from pathlib import Path

TARGET = Path(__file__).parent.parent / "target" / "firmware" / "nvidia" / "gp108"
TARGET.mkdir(parents=True, exist_ok=True)

# Try to get firmware from Debian/Ubuntu firmware package
sources = [
    # Ubuntu noble (24.04) firmware-nvidia-gpu
    ("https://archive.ubuntu.com/ubuntu/pool/multiverse/f/firmware-nvidia-gpu/", "noble"),
    # Debian bookworm
    ("https://http.us.debian.org/debian/pool/non-free-firmware/f/firmware-nvidia-gpu/", "bookworm"),
]

for base, dist in sources:
    try:
        r = requests.get(base, timeout=30, headers={'User-Agent': 'Mozilla/5.0'})
        if r.status_code != 200:
            print(f"[{dist}] HTTP {r.status_code}")
            continue
        debs = re.findall(r'href="([^"]*firmware-nvidia-gpu[^"]*\.deb)"', r.text)
        print(f"[{dist}] Found {len(debs)} DEB packages")
        # Pick the latest
        if debs:
            deb_name = debs[-1]
            deb_url = base + deb_name
            print(f"[DL] {deb_name} ({deb_url})")
            r2 = requests.get(deb_url, timeout=300, headers={'User-Agent': 'Mozilla/5.0'})
            if r2.status_code == 200:
                deb_path = TARGET / deb_name
                deb_path.write_bytes(r2.content)
                print(f"  Saved {len(r2.content)//1024//1024}MB")
                # Extract .deb (ar archive + tar.xz)
                if shutil.which("7z"):
                    subprocess.run(["7z", "x", str(deb_path), f"-o{TARGET}", "-y"], capture_output=True)
                    # Find firmware files
                    fw = list(TARGET.rglob("*.bin"))
                    if fw:
                        print(f"  Firmware blobs found: {len(fw)}")
                        for f in fw:
                            rel = f.relative_to(TARGET.parent.parent)
                            print(f"    {rel} ({f.stat().st_size//1024}KB)")
                        sys.exit(0)
                # Try ar + tar directly
                if shutil.which("ar"):
                    import tempfile
                    tmp = tempfile.mkdtemp()
                    subprocess.run(["ar", "x", str(deb_path)], cwd=tmp, capture_output=True)
                    for tarball in Path(tmp).glob("data.tar.*"):
                        subprocess.run(["tar", "-xf", str(tarball), "-C", str(TARGET)], capture_output=True)
                    fw = list(TARGET.rglob("*.bin"))
                    if fw:
                        print(f"  Firmware blobs found: {len(fw)}")
                        for f in fw:
                            rel = f.relative_to(TARGET.parent.parent)
                            print(f"    {rel} ({f.stat().st_size//1024}KB)")
                        sys.exit(0)
            else:
                print(f"  HTTP {r2.status_code}")
    except Exception as e:
        print(f"[{dist}] Error: {e}")

# Fallback: direct kernel.org raw download
print("\nTrying kernel.org raw download...")
files = ['fecs_bl.bin', 'fecs_data.bin', 'fecs_inst.bin', 'fecs_sig.bin',
         'gpccs_bl.bin', 'gpccs_data.bin', 'gpccs_inst.bin', 'gpccs_sig.bin']
base = "https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/plain/nvidia/gp108"
for f in files:
    url = f"{base}/{f}"
    dest = TARGET / f
    if dest.exists() and dest.stat().st_size > 0:
        print(f"  [SKIP] {f}")
        continue
    try:
        r = requests.get(url, timeout=60, headers={'User-Agent': 'Git/2.0', 'Accept': '*/*'})
        if r.status_code == 200:
            dest.write_bytes(r.content)
            print(f"  [OK] {f} ({len(r.content)//1024}KB)")
        else:
            print(f"  [--] {f} HTTP {r.status_code}")
    except Exception as e:
        print(f"  [--] {f}: {e}")

print("\nDone. Firmware em:", TARGET)
