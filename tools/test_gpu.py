"""Quick GPU test — trains RustCoder for 5 epochs with full visibility"""
import sys, os, time, json, random
from pathlib import Path
os.environ["CUDA_VISIBLE_DEVICES"] = "0"

sys.path.insert(0, str(Path(__file__).parent))
from train_gpu_full import *

print("[1/4] Generating dataset...")
data = gen_rust_dataset(20000)
ds = tokenize(data, vcb=256, sl=64)
print(f"  {len(ds)} samples")

print("[2/4] Creating model...")
model = BitNetLM(h=384, v=256, nl=12, nh=8, ff=768).to(DEVICE)
n = sum(p.numel() for p in model.parameters())
print(f"  {n:,} params")

print("[3/4] Running 5 epochs...")
t0 = time.time()
train("RustCoder-Test", model, ds, 256, ep=5, bs=64, tok=b"rustcoder_v1")
print(f"[4/4] Done in {time.time()-t0:.0f}s")

# Verify model
f = Path("target/rustcoder.bitnet")
if f.exists():
    import struct
    with open(f, "rb") as fh:
        magic = struct.unpack("<I", fh.read(4))[0]
        ver = struct.unpack("<H", fh.read(2))[0]
        np = struct.unpack("<I", fh.read(6))[0] if ver else 0
        print(f"Model: {f.stat().st_size//1024}KB, magic=0x{magic:X}, valid={magic==0xBE11BE11}")
else:
    print("Model file not found!")
