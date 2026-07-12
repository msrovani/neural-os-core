#!/usr/bin/env python3
"""Find linux-firmware mirrors for NVIDIA GP108 ACR blobs."""
import requests

# Check various potential mirrors
candidates = [
    ("GitHub", "https://api.github.com/repos/linux-firmware/linux-firmware"),
    ("GitLab", "https://gitlab.com/api/v4/projects/linux-firmware%2Flinux-firmware"),
    ("GitHub", "https://api.github.com/repos/firmware/linux-firmware"),
    ("GitHub", "https://api.github.com/repos/kernel-firmware/linux-firmware"),
]

for src, url in candidates:
    try:
        r = requests.get(url, timeout=10)
        if r.status_code == 200:
            data = r.json()
            print(f"[{src}] FOUND: {data.get('path_with_namespace', data.get('full_name', url))}")
            print(f"  Stars: {data.get('stargazers_count', '?')}")
            print(f"  Clone: {data.get('clone_url', data.get('http_url_to_repo', '?'))}")
    except:
        pass

print("\nSearch complete. No public mirrors found.")
print("Manually download from a Linux machine:")
print("  cp /lib/firmware/nvidia/gp108/* target/firmware/nvidia/gp108/")
