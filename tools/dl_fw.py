#!/usr/bin/env python3
"""Download NVIDIA GP108 firmware from Debian/Ubuntu mirrors."""
import requests, re, os, sys
from pathlib import Path

TARGET = Path(__file__).parent.parent / "target" / "firmware" / "nvidia" / "gp108"
TARGET.mkdir(parents=True, exist_ok=True)

# Try Ubuntu's firmware package (firmware-nvidia-gpu)
url = "https://archive.ubuntu.com/ubuntu/pool/multiverse/f/firmware-nvidia-gpu/"
try:
    r = requests.get(url, timeout=30)
    if r.status_code == 200:
        debs = re.findall(r'href="([^"]+\.deb)"', r.text)
        # Pick the latest gp108 firmware
        gp108_debs = [d for d in debs if 'gp108' in d]
        if gp108_debs:
            deb_url = url + gp108_debs[-1]
            print(f"[DL] {gp108_debs[-1]}")
            r2 = requests.get(deb_url, timeout=120)
            if r2.status_code == 200:
                deb_path = TARGET / gp108_debs[-1]
                deb_path.write_bytes(r2.content)
                # Extract .deb (it's an ar archive, then tar.gz)
                import subprocess, shutil
                ar_bin = shutil.which("ar")
                if ar_bin:
                    subprocess.run([ar_bin, "x", str(deb_path)], cwd=str(TARGET), capture_output=True)
                    # Extract data.tar.xz or data.tar.gz
                    for tarball in TARGET.glob("data.tar*"):
                        subprocess.run(["tar", "-xf", str(tarball), "-C", str(TARGET)], capture_output=True)
                    # Find firmware files
                    fw_files = list(TARGET.rglob("*.bin"))
                    for f in fw_files:
                        print(f"  [OK] {f.relative_to(TARGET)} ({f.stat().st_size//1024}KB)")
                else:
                    print("  'ar' not found, use 7z or manual extract")
            else:
                print(f"  HTTP {r2.status_code}")
        else:
            print("  No gp108 firmware found in pool")
    else:
        print(f"HTTP {r.status_code}")
except Exception as e:
    print(f"Error: {e}")
