#!/usr/bin/env python3
"""Check firmware paths on kernel.org and GitHub mirrors."""
import requests, re

# Check GitHub for linux-firmware mirror
github_urls = [
    "https://api.github.com/repos/torvalds/linux/contents/drivers/gpu/drm/nouveau/nvkm/subdev/acr",
]

# Check kernel.org firmware paths  
kernel_urls = [
    "https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/nvidia",
    "https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/nvidia/gp108",
    "https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/nvidia/gp108/acr",
]

for url in kernel_urls:
    try:
        r = requests.get(url, timeout=15, headers={'User-Agent': 'Mozilla/5.0'})
        print(f"[{r.status_code}] {url}")
        if r.status_code == 200:
            items = re.findall(r'<a[^>]*href="[^"]*/([^/"]+)"[^>]*>', r.text)
            for item in items[:20]:
                print(f"  - {item}")
    except Exception as e:
        print(f"[ERR] {url}: {e}")
