#!/usr/bin/env python3
"""train_gpu_full.py — neural-os-core v1.0
Treino GPU com datasets grandes. GTX 1050 (4GB) a 100%.

Uso: CUDA_VISIBLE_DEVICES=0 python train_gpu_full.py --all
"""

import os, sys, struct, json, urllib.request, time, math, random, hashlib
from pathlib import Path
import numpy as np

TARGET = Path(__file__).parent / "target"
TARGET.mkdir(exist_ok=True)

import torch
import torch.nn as nn
import torch.nn.functional as F
import torch.optim as optim

DEVICE = torch.device("cuda" if torch.cuda.is_available() else "cpu")
print(f"[GPU] {torch.cuda.get_device_name(0) if torch.cuda.is_available() else 'cpu'}")
if torch.cuda.is_available():
    print(f"[VRAM] {torch.cuda.get_device_properties(0).total_memory/1e9:.1f}GB")

MAGIC = 0xBE11BE11

def write_bitnet(f, h, n_l, n_h, vcb, seq, ff, nkv, qd, tok):
    """.bitnet v4 header — layout DEVE bater com `read_u32`/`read_u16` de
    `crates/neural-kernel/src/cortex.rs::load_model()` e com
    `train_models_gpu.py::write_header()` (fonte da verdade).
    BUG (Sprint 107 Part B #8): versao anterior escrevia vocab_size e
    num_medusa como u16 dentro do loop de 9 campos — o kernel le ambos como
    u32, deslocando todo o parse (vocab virava lixo tipo 4194368) e o
    load_model() falhava (`[HWEXPERT] parse FAILED`). Fix: vocab_size e
    num_medusa agora sao u32 explicitos, iguais a write_header().
    """
    num_medusa = 0
    np_ = h*vcb + n_l*(4*h*h + 3*h*ff + 2*h + qd) + h*vcb
    f.write(struct.pack("<I", MAGIC))
    f.write(struct.pack("<H", 4))
    f.write(struct.pack("<I", np_))
    f.write(struct.pack("<H", h))
    f.write(struct.pack("<H", n_l))
    f.write(struct.pack("<H", n_h))
    f.write(struct.pack("<I", vcb))         # u32 (era u16 — bug)
    f.write(struct.pack("<H", seq))
    f.write(struct.pack("<H", ff))
    f.write(struct.pack("<H", nkv))
    f.write(struct.pack("<H", qd))
    f.write(struct.pack("<I", num_medusa))  # u32 (era u16 — bug)
    f.write(b"\x00\x00\x00\x00")
    f.write(b"\x01")
    f.write(struct.pack("<I", len(tok))); f.write(tok)
    f.write(b"\x07")

def qpack(arr):
    p = bytearray()
    for i in range(0, len(arr), 4):
        b = 0
        for j in range(4):
            if i+j < len(arr):
                v = float(arr[i+j])
                bits = 0b01 if v > 0.5 else (0b10 if v < -0.5 else 0b00)
                b |= bits << (j*2)
        p.append(b)
    return bytes(p)

def wt(f, t):
    t = t.detach().cpu().numpy().reshape(-1)
    f.write(struct.pack("<I", len(t)))
    f.write(struct.pack("<I", 0))
    f.write(qpack(t))

def wv(f, v):
    a = list(v.detach().cpu().numpy())
    f.write(struct.pack("<I", len(a)))
    for x in a: f.write(struct.pack("<f", float(x)))

class BitNetLM(nn.Module):
    def __init__(self, h=128, v=128, nl=6, nh=4, ff=256):
        super().__init__()
        self.h, self.v, self.nl, self.nh, self.ff = h, v, nl, nh, ff
        self.embed = nn.Embedding(v, h)
        self.q = nn.ModuleList([nn.Linear(h, h, 0) for _ in range(nl)])
        self.k = nn.ModuleList([nn.Linear(h, h, 0) for _ in range(nl)])
        self.v_ = nn.ModuleList([nn.Linear(h, h, 0) for _ in range(nl)])
        self.o = nn.ModuleList([nn.Linear(h, h, 0) for _ in range(nl)])
        self.g = nn.ModuleList([nn.Linear(h, ff, 0) for _ in range(nl)])
        self.u = nn.ModuleList([nn.Linear(h, ff, 0) for _ in range(nl)])
        self.d = nn.ModuleList([nn.Linear(ff, h, 0) for _ in range(nl)])
        ra = [nn.Parameter(torch.ones(h)) for _ in range(nl)]
        rf = [nn.Parameter(torch.ones(h)) for _ in range(nl)]
        self.rms_a = nn.ParameterList(ra); self.rms_f = nn.ParameterList(rf)
        self.unembed = nn.Linear(h, v, 0)
        self.rms_o = nn.Parameter(torch.ones(h))
    def forward(self, x):
        h = self.embed(x)
        for i in range(self.nl):
            r = h; h = h * self.rms_a[i]
            h = self.o[i](self.v_[i](h)) + r
            r = h; h = h * self.rms_f[i]
            h = self.d[i](self.g[i](h) * self.u[i](h)) + r
        return self.unembed(h * self.rms_o)
    def export(self, path, tok=b""):
        with open(path, "wb") as f:
            write_bitnet(f, self.h, self.nl, self.nh, self.v, 64, self.ff, self.nh, self.h//self.nh, tok)
            wt(f, self.embed.weight.T)
            for i in range(self.nl):
                wv(f, self.rms_a[i]); wv(f, self.rms_f[i])
                wv(f, torch.ones(self.h)); wv(f, torch.ones(self.ff))
                wt(f, self.q[i].weight.T); wt(f, self.k[i].weight.T)
                wt(f, self.v_[i].weight.T); wt(f, self.o[i].weight.T)
                wt(f, self.g[i].weight.T); wt(f, self.u[i].weight.T)
                wt(f, self.d[i].weight.T)
                wv(f, torch.tensor([10000.**(-2.*i/32) for i in range(16)]))
            wt(f, self.unembed.weight.T)
        print(f"  [OK] {path} ({os.path.getsize(path)//1024}KB)")

# ─── Datasets ──────────────────────────────────────────────────────────────

RUST_TEMPLATES = [
    "fn {n}({a}) -> {r} {{\n    {b}\n}}",
    "pub struct {n} {{\n    pub {f}: {t},\n}}",
    "impl {n} {{\n    pub fn new() -> Self {{ Self {{}} }}\n}}",
    "match {v} {{\n    {p} => {e},\n    _ => {f},\n}}",
    "for {v} in {i} {{\n    {b}\n}}",
    "let mut {v}: {t} = {x};",
    "loop {{\n    {b}\n    break;\n}}",
    "if let Some({v}) = {o} {{\n    {b}\n}}",
]

NAMES = "init run handle process map filter connect bind listen accept read write open close alloc free push pop send recv boot halt idle exec parse".split()
TYPES = ["u8","u32","i32","usize","bool","&str"]
BODIES = ["Ok(())","true","0","self","todo!()","x+1","ptr.read()","writeln!(serial)","continue","break"]

def gen_rust_dataset(n=10000):
    cache = TARGET / "rust_dataset.json"
    if cache.exists():
        with open(cache) as f: return json.load(f)
    print(f"[DATA] Gerando {n} exemplos Rust...")
    data = []
    for _ in range(n):
        t = random.choice(RUST_TEMPLATES)
        nn_ = random.choice(NAMES)
        a = ", ".join(f"{random.choice(NAMES)}: {random.choice(TYPES)}" for _ in range(random.randint(1,3)))
        r = random.choice(["()","bool","u32","Result<(),&str>"])
        b = random.choice(BODIES)
        f_ = random.choice(NAMES)
        t_ = random.choice(TYPES)
        v = random.choice(NAMES)
        ex = t.format(n=nn_, a=a, r=r, b=b, f=f_, t=t_, v=v, i="0..10", x="0", p="Some(x)", e="x", f_="false", o="opt")
        data.append(ex)
    random.shuffle(data)
    with open(cache, "w") as f: json.dump(data, f)
    print(f"  [OK] {len(data)} exemplos")
    return data

def tokenize(data, vcb=128, sl=64):
    ds = []
    for ex in data[:50000]:
        c = [ord(c) % vcb for c in ex[:sl]] + [0]*sl
        c = c[:sl]
        ds.append((c[:-1], c[1:]))
    return ds[:48000]

def gen_pci_dataset():
    cache = TARGET / "pci_dataset.json"
    if cache.exists():
        with open(cache) as f: return json.load(f)
    print("[DATA] Baixando pci.ids...")
    devs = {}
    try:
        req = urllib.request.urlopen("https://pci-ids.ucw.cz/v2.2/pci.ids", timeout=10)
        cv = None
        for line in req.read().decode("latin-1").split("\n"):
            if not line or line[0]=='#': continue
            if line[0]!='\t':
                p = line.strip().split(" ", 1)
                if len(p)>=2: cv=p[0]; devs[cv]={"n":p[1],"d":{}}
            elif cv and line[0]=='\t':
                p = line.strip().split(" ", 1)
                if len(p)>=2: devs[cv]["d"][p[0]]=p[1]
    except: pass

    ds = []
    for vid, vi in devs.items():
        for did, dn in vi["d"].items():
            ds.append({"v":vid,"d":did})
    # Augment
    for _ in range(len(ds)*10):
        d = random.choice(ds)
        v = f"{int(d['v'],16)+random.randint(-1,1)&0xFFFF:04X}"
        di = f"{int(d['d'],16)+random.randint(-1,1)&0xFFFF:04X}"
        ds.append({"v":v,"d":di})
    with open(cache, "w") as f: json.dump(ds, f)
    print(f"  [OK] {len(ds)} entradas")
    return ds

def tokenize_pci(data, vcb=64, sl=16):
    ds = []
    for e in data[:100000]:
        v = int(e["v"],16); d = int(e["d"],16)
        inp = [(v>>8)%vcb, v%vcb, (d>>8)%vcb, d%vcb]
        inp = (inp + [0]*sl)[:sl-1]
        tgt = [hash(e["v"]+e["d"]) % vcb] + [0]*(sl-2)
        ds.append((inp, tgt))
    return ds

# ─── Treino ────────────────────────────────────────────────────────────────

def train(name, model, ds, vcb, ep=50, bs=512, lr=3e-4, tok=b""):
    print(f"\n=== {name} ===", flush=True)
    n = sum(p.numel() for p in model.parameters())
    print(f"  Params: {n:,} | Data: {len(ds)} | Batch: {bs} | Epochs: {ep}", flush=True)

    t_in = torch.tensor([d[0] for d in ds], dtype=torch.long)
    t_out = torch.tensor([d[1] for d in ds], dtype=torch.long)
    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(t_in, t_out),
        batch_size=bs, shuffle=True, drop_last=True)

    opt = optim.AdamW(model.parameters(), lr=lr, weight_decay=1e-5)
    sched = optim.lr_scheduler.OneCycleLR(opt, lr, steps_per_epoch=len(loader), epochs=ep, last_epoch=-1)
    scaler = torch.amp.GradScaler("cuda") if DEVICE.type == "cuda" else None

    best = float("inf")
    iters = 0
    t0 = time.time()
    for ep_idx in range(ep):
        model.train()
        tl = 0.0; nb = 0
        for x, y in loader:
            x, y = x.to(DEVICE, non_blocking=True), y.to(DEVICE, non_blocking=True)
            opt.zero_grad()
            if scaler:
                with torch.amp.autocast(DEVICE.type):
                    l = F.cross_entropy(model(x).reshape(-1, vcb), y.reshape(-1))
                scaler.scale(l).backward()
                scaler.unscale_(opt)
                nn.utils.clip_grad_norm_(model.parameters(), 1.0)
                scaler.step(opt); scaler.update()
            else:
                l = F.cross_entropy(model(x).reshape(-1, vcb), y.reshape(-1))
                l.backward()
                nn.utils.clip_grad_norm_(model.parameters(), 1.0)
                opt.step()
            iters += 1
            sched.step()
            tl += l.item(); nb += 1

        avg = tl / nb
        if avg < best:
            best = avg
            if not math.isnan(avg):
                model.export(TARGET / f"{name.lower().replace(' ','_')}.bitnet", tok)

        if (ep_idx+1) % 10 == 0 or ep_idx == 0:
            sps = (ep_idx+1)*len(ds)/(time.time()-t0) if time.time()-t0 > 0 else 0
            print(f"  Ep {ep_idx+1:3d}/{ep} | l={avg:.4f} b={best:.4f} | {sps:.0f}s/s", flush=True)

    model.export(TARGET / f"{name.lower().replace(' ','_')}.bitnet", tok)
    print(f"  [OK] best={best:.4f} {time.time()-t0:.0f}s")

def main():
    import argparse
    p = argparse.ArgumentParser()
    p.add_argument("--ep", type=int, default=50)
    p.add_argument("--bs", type=int, default=512)
    p.add_argument("--rust", action="store_true")
    p.add_argument("--hw", action="store_true")
    p.add_argument("--all", action="store_true")
    a = p.parse_args()
    if not any([a.all, a.rust, a.hw]): a.all = True

    print("="*65)
    print(f"  neural-os-core v1.0 - GPU Training (GTX 1050)")
    print(f"  Ep={a.ep}  Batch={a.bs}  Device={DEVICE}")
    print("="*65)

    if a.all or a.rust:
        data = gen_rust_dataset(20000)
        ds = tokenize(data, vcb=256, sl=64)
        model = BitNetLM(h=384, v=256, nl=12, nh=8, ff=768).to(DEVICE)
        train("RustCoder", model, ds, 256, a.ep, a.bs, tok=b"rustcoder_v1")

    if a.all or a.hw:
        data = gen_pci_dataset()
        ds = tokenize_pci(data, vcb=64, sl=16)
        model = BitNetLM(h=64, v=64, nl=4, nh=4, ff=128).to(DEVICE)
        train("HWExpert", model, ds, 64, a.ep//2, a.bs, tok=b"hwexpert_v1")

    print("\n"+ "="*65)
    print("  MODELOS:")
    total = 0
    for f in sorted(TARGET.glob("*.bitnet")):
        mb = f.stat().st_size/1024
        total += mb
        print(f"  {f.name:30s} {mb:7.1f}KB")
    print(f"  {'TOTAL':30s} {total:7.1f}KB")

if __name__ == "__main__":
    main()
