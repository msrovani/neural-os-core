#!/usr/bin/env python3
"""Probe rapido: torch + CUDA + sm_61."""
import os
import sys

os.environ.setdefault("CUDA_VISIBLE_DEVICES", "0")
print("python", sys.version)
print("CUDA_VISIBLE_DEVICES", os.environ.get("CUDA_VISIBLE_DEVICES"))
try:
    import torch
except Exception as e:
    print("torch import FAIL", e)
    sys.exit(1)
print("torch", torch.__version__)
print("torch.version.cuda", getattr(torch.version, "cuda", None))
print("cuda.is_available", torch.cuda.is_available())
if torch.cuda.is_available():
    p = torch.cuda.get_device_properties(0)
    print("gpu", torch.cuda.get_device_name(0))
    print("vram_gb", round(p.total_memory / 1e9, 2))
    print("cc", f"{p.major}.{p.minor}")
    x = torch.randn(256, 256, device="cuda")
    y = x @ x
    print("matmul_ok", float(y.sum().item()))
else:
    print("HINT: GTX1050/sm_61 precisa torch+cu118 ou cu126 (nao cu130)")
