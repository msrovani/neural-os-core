#!/usr/bin/env python3
"""QEMU UEFI launcher — pflash OVMF + chardev serial file.
Uses ovmf_code.fd (readonly) + ovmf_vars.fd like run-qemu-uefi.ps1.
Serial output captured to log file via -chardev file.
Auto-discovers models in target/ and loads via QEMU loader.
"""
import os, subprocess, sys, time, threading

ROOT = os.path.normpath(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
QEMU = r"C:\Program Files\qemu\qemu-system-x86_64.exe"

smp = 4
ram = "8G"
timeout = 300
instance = 0
visual = False
for i, arg in enumerate(sys.argv[1:], 1):
    if arg == "--smp" and i < len(sys.argv) - 1:
        smp = int(sys.argv[i + 1])
    elif arg == "--ram" and i < len(sys.argv) - 1:
        ram = sys.argv[i + 1]
    elif arg == "--timeout" and i < len(sys.argv) - 1:
        timeout = int(sys.argv[i + 1])
    elif arg == "--instance" and i < len(sys.argv) - 1:
        instance = int(sys.argv[i + 1])
    elif arg == "--visual":
        visual = True
    elif arg == "--no-timeout":
        timeout = 999999

ts = time.strftime("%Y%m%d_%H%M%S")
logdir = os.path.join(ROOT, "logs")
os.makedirs(logdir, exist_ok=True)
logfile = os.path.join(logdir, f"boot_{smp}c_inst{instance}_{ts}.txt")

# Files
ovmf_code = os.path.join(ROOT, "target", "ovmf_code.fd")
ovmf_vars = os.path.join(ROOT, "target", "ovmf_vars.fd")
uefi = os.path.join(ROOT, "target", "uefi.img")
disk = os.path.join(ROOT, "target", "disk_qemu.raw")

for name, path in [("OVMF CODE", ovmf_code), ("OVMF VARS", ovmf_vars), ("UEFI ESP", uefi), ("Disk", disk)]:
    if not os.path.exists(path):
        print(f"ERRO: {name} ausente: {path}")
        sys.exit(1)

# Find models — .bitnet, .BIN, .bin, .v6 (SESSION_293: include BitNet v6 format)
# DEDUP: skip files with same size (FALCON3.V6 == FALCON3B.v6 = 807MB duplicate)
ext_dirs = [d for d in [r"E:\modelos", r"D:\modelos"] if os.path.isdir(d)]
models = []
seen_sizes = set()
for d in [os.path.join(ROOT, "target"), os.path.join(ROOT, "target", "models")] + ext_dirs:
    if not os.path.isdir(d):
        continue
    for f in sorted(os.listdir(d)):
        if f.upper().endswith((".BITNET", ".BIN", ".V6", ".GGUF")):
            full = os.path.join(d, f)
            sz = os.path.getsize(full)
            if 10240 < sz <= 2 * 1024 * 1024 * 1024:  # 10KB..2GB
                if sz in seen_sizes:
                    print(f"  SKIP duplicate: {f} ({sz // (1024*1024)}MB, same size as existing)")
                    continue
                seen_sizes.add(sz)
                models.append((f, sz, full))

# LIMIT: cap total loading to 1GB physical (TCG slow with many large models).
# Core LLM + experts only — rest loaded from FAT32 or skipped.
total_model_bytes = 0
MAX_TOTAL = 2 * 1024 * 1024 * 1024  # 2GB (Falcon3 GGUF = 1.1GB)
capped = []
for m in models:
    if total_model_bytes + m[1] > MAX_TOTAL and capped:
        print(f"  CAP: skipping {m[0]} ({m[1] // (1024*1024)}MB) — total would exceed 1GB")
        continue
    capped.append(m)
    total_model_bytes += m[1]
models = capped
models.sort(key=lambda x: -x[1])

args = [
    QEMU, "-m", ram, "-smp", str(smp), "-accel", "tcg",
    "-drive", f"if=pflash,format=raw,file={ovmf_code},readonly=on",
    "-drive", f"if=pflash,format=raw,file={ovmf_vars}",
    "-drive", f"format=raw,file={uefi},if=ide,index=0",
    "-drive", f"format=raw,file={disk},if=ide,index=1",
]

addr = 0x100000000
gap = 0x100000
for name, sz, path in models:
    args += ["-device", f"loader,file={path},addr=0x{addr:X}"]
    print(f"  QEMU loader: {name} ({sz // (1024*1024)}MB) @0x{addr:X}")
    addr += ((sz + gap - 1) // gap) * gap + gap

if visual:
    args += [
        "-chardev", f"file,id=ser0,path={logfile}",
        "-serial", "chardev:ser0",
        "-netdev", "user,id=n0",
        "-device", "e1000,netdev=n0",
        "-vga", "std", "-display", "sdl",
    ]
else:
    args += [
        "-chardev", f"file,id=ser0,path={logfile}",
        "-serial", "chardev:ser0",
        "-netdev", "user,id=n0",
        "-device", "e1000,netdev=n0",
        "-vga", "none", "-display", "none", "-nographic",
    ]

print(f"QEMU {smp}C TCG {ram} (inst{instance})")
print(f"Models: {len(models)} loaded")
print(f"Log: {logfile}")
print(f"Timeout: {timeout}s")

proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)

def timeout_kill():
    try:
        proc.kill()
    except Exception:
        pass

timer = threading.Timer(timeout, timeout_kill)
timer.daemon = True
timer.start()

start = time.time()
jarbas_found = False
nsgdb_found = False
last_size = 0
try:
    while proc.poll() is None:
        time.sleep(2)
        elapsed = int(time.time() - start)
        if os.path.exists(logfile):
            sz = os.path.getsize(logfile)
            if sz > last_size:
                last_size = sz
            if elapsed % 30 == 0:
                print(f"  [{elapsed}s] {sz} bytes")
            # Check for key markers in the growing log
            if not jarbas_found and sz > 1000:
                try:
                    with open(logfile, "r", errors="replace") as f:
                        tail = f.read()[-4096:]
                    if "jarbas" in tail.lower() or "JARBAS" in tail:
                        jarbas_found = True
                        print(f"  *** JARBAS at {elapsed}s ***")
                    if "nsgdb" in tail.lower() or "recall" in tail.lower():
                        nsgdb_found = True
                        print(f"  *** NSGDB at {elapsed}s ***")
                except Exception:
                    pass
        if elapsed > timeout - 5:
            break
except KeyboardInterrupt:
    pass
finally:
    timer.cancel()
    proc.kill()
    proc.wait()

elapsed = int(time.time() - start)
final_size = os.path.getsize(logfile) if os.path.exists(logfile) else 0
print(f"\nDone: {elapsed}s, {final_size} bytes, log={logfile}")

# Print summary from log
if os.path.exists(logfile) and final_size > 0:
    try:
        with open(logfile, "r", errors="replace") as f:
            content = f.read()
        for line in content.split("\n"):
            ll = line.lower()
            if any(kw in ll for kw in ["jarbas", "nsgdb", "recall", "saudacao", "greeting", "theme", "[fail]", "panic", "LLM LOADED", "model", "active"]):
                print(f"  {line.strip()[:130]}")
    except Exception:
        pass
