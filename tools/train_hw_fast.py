#!/usr/bin/env python3
"""train_hw_fast.py — Treino rapido HW Expert na GPU com dados sinteticos de PCI."""
import os, sys, struct, time
os.environ["CUDA_VISIBLE_DEVICES"] = os.environ.get("CUDA_VISIBLE_DEVICES", "0")
from pathlib import Path
TARGET = Path(__file__).parent.parent / "target"
TARGET.mkdir(exist_ok=True)

import torch, torch.nn as nn, torch.nn.functional as F, torch.optim as optim
DEVICE = torch.device("cuda" if torch.cuda.is_available() else "cpu")
print(f"[GPU] {DEVICE}")
if torch.cuda.is_available():
    print(f"[VRAM] {torch.cuda.get_device_properties(0).total_memory/1e9:.1f}GB")

FAMILIAS = ["AtherosWiFi","BroadcomWiFi","GenericPCI","IntelEthernet",
            "IntelWiFi","RealtekEthernet","RealtekWiFi","VirtIO"]
FAM2IDX = {n:i for i,n in enumerate(FAMILIAS)}

def familia(vid, did):
    if vid==0x8086:
        if (0x08B1<=did<=0x2726) or did in(0x3165,0x3166,0x06F0,0x02F0,0x2526,0x2527,0x2723,0x2725,0x2726,0x24F3,0x24F4,0x24F5,0x24F6,0x24FD) or (did>>8)==0x08: return "IntelWiFi"
        if did in(0x100E,0x105E,0x10D3,0x10D5,0x10DE,0x10EA,0x10F5,0x10FB,0x10C9,0x1526,0x1527,0x154D,0x156F,0x1570,0x1533,0x1538,0x1539,0x1502,0x1503): return "IntelEthernet"
        return "GenericPCI"
    if vid==0x10EC: return "RealtekWiFi" if did in(0x8176,0x8179,0x8812) else ("RealtekEthernet" if did in(0x8139,0x8168,0x8169) else "GenericPCI")
    if vid==0x0BDA: return "RealtekWiFi"
    if vid==0x168C: return "AtherosWiFi"
    if vid==0x14E4: return "BroadcomWiFi"
    if vid in(0x1AF4,0x1234): return "VirtIO"
    return "GenericPCI"

class BitNetTF(nn.Module):
    def __init__(self, h=32, v=64, nl=4, nh=4, ff=64):
        super().__init__()
        self.h, self.v, self.nl, self.ff = h, v, nl, ff
        self.embed = nn.Embedding(v, h)
        self.q = nn.ModuleList([nn.Linear(h,h,0) for _ in range(nl)])
        self.k = nn.ModuleList([nn.Linear(h,h,0) for _ in range(nl)])
        self.v_ = nn.ModuleList([nn.Linear(h,h,0) for _ in range(nl)])
        self.o = nn.ModuleList([nn.Linear(h,h,0) for _ in range(nl)])
        self.g = nn.ModuleList([nn.Linear(h,ff,0) for _ in range(nl)])
        self.u = nn.ModuleList([nn.Linear(h,ff,0) for _ in range(nl)])
        self.d = nn.ModuleList([nn.Linear(ff,h,0) for _ in range(nl)])
        self.r1 = nn.ParameterList([nn.Parameter(torch.ones(h)) for _ in range(nl)])
        self.r2 = nn.ParameterList([nn.Parameter(torch.ones(h)) for _ in range(nl)])
        self.unembed = nn.Linear(h, v, 0)
        self.rf = nn.Parameter(torch.ones(h))
    def forward(self, x):
        h = self.embed(x)
        for i in range(self.nl):
            r = h; h = h * self.r1[i]
            h = self.o[i](self.v_[i](h)) + r; r = h
            h = h * self.r2[i]
            h = self.d[i](self.g[i](h) * self.u[i](h)) + r
        return self.unembed(h * self.rf)
    def export(self, path):
        kv = self.h // 4
        with open(path, "wb") as f:
            np_ = self.h*self.v + self.nl*(4*self.h*self.h + 3*self.h*self.ff + 2*self.h) + self.h*self.v
            f.write(struct.pack("<I", 0xBE11BE11))
            f.write(struct.pack("<H", 4))
            f.write(struct.pack("<I", np_))
            for v in (self.h, self.nl, 4, self.v, 64, self.ff, kv, self.h//4, 0):
                f.write(struct.pack("<H", v))
            f.write(b"\x00\x00\x00\x00\x01")
            tok = b"hwexpert_v4"
            f.write(struct.pack("<I", len(tok))); f.write(tok)
            f.write(b"\x07")
            def qp(arr):
                p = bytearray()
                for i in range(0, len(arr), 4):
                    b = 0
                    for j in range(4):
                        if i+j < len(arr):
                            bits = 0b01 if float(arr[i+j])>0.5 else (0b10 if float(arr[i+j])<-0.5 else 0b00)
                            b |= bits << (j*2)
                    p.append(b)
                return bytes(p)
            def wt(t):
                t2 = t.detach().cpu().to(torch.float32).numpy().reshape(-1)
                f.write(struct.pack("<I", len(t2))); f.write(struct.pack("<I", 0))
                f.write(qp(t2))
            def wv(t):
                a = t.detach().cpu().to(torch.float32).numpy().reshape(-1)
                f.write(struct.pack("<I", len(a)))
                for x in a: f.write(struct.pack("<f", float(x)))
            wt(self.embed.weight.T)
            for i in range(self.nl):
                wv(self.r1[i]); wv(self.r2[i]); wv(torch.ones(self.h)); wv(torch.ones(self.ff))
                wt(self.q[i].weight.T); wt(self.k[i].weight.T)
                wt(self.v_[i].weight.T); wt(self.o[i].weight.T)
                wt(self.g[i].weight.T); wt(self.u[i].weight.T); wt(self.d[i].weight.T)
                wv(torch.tensor([10000.**(-2.*j/32) for j in range(16)]))
            wv(self.rf); wt(self.unembed.weight.T)

def gerar_dados(n=50000):
    import random, csv
    random.seed(42)
    devices = []
    vendors = [0x8086, 0x10EC, 0x168C, 0x14E4, 0x1AF4, 0x1234, 0x0BDA, 0x10DE, 0x1002, 0x1022]
    for _ in range(n):
        vid = random.choice(vendors) + random.randint(0, 0x00FF)
        did = random.randint(0x0000, 0xFFFF)
        devices.append({"vid": vid, "did": did, "family": familia(vid, did)})
    # Salva CSV
    with open(TARGET / "hw_all_unified.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["vid","did","family"])
        for d in devices: w.writerow([d["vid"], d["did"], d["family"]])
    return devices

def treinar(devices, epochs=200, batch=256):
    print(f"\nAmostras: {len(devices)} | Epochs: {epochs} | Batch: {batch}")
    for fn in FAMILIAS:
        cnt = sum(1 for d in devices if d["family"] == fn)
        print(f"  {fn:20s} {cnt:4d}")

    vocab = 64
    inp = torch.tensor([[(d["vid"]>>8)%vocab, d["vid"]%vocab, (d["did"]>>8)%vocab, d["did"]%vocab] for d in devices], dtype=torch.long)
    tgt = torch.tensor([FAM2IDX.get(d["family"], 0) for d in devices], dtype=torch.long)

    model = BitNetTF().to(DEVICE)
    n = sum(p.numel() for p in model.parameters())
    print(f"Params: {n:,}")

    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(inp, tgt), batch_size=batch, shuffle=True, drop_last=True)

    opt = optim.AdamW(model.parameters(), lr=3e-3, weight_decay=1e-5)
    sched = optim.lr_scheduler.OneCycleLR(opt, 3e-3, steps_per_epoch=max(len(loader),1), epochs=epochs, last_epoch=-1)

    best = float("inf")
    t0 = time.time()
    for ep in range(epochs):
        model.train()
        tl = 0.0; nb = 0
        for x, y in loader:
            x, y = x.to(DEVICE), y.to(DEVICE)
            opt.zero_grad()
            loss = F.cross_entropy(model(x).mean(dim=1), y)
            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step(); sched.step()
            tl += loss.item(); nb += 1
        avg = tl / max(nb, 1)
        if avg < best:
            best = avg
            model.export(TARGET / "hw_expert_tf.bitnet")
        if (ep+1) % 50 == 0 or ep == 0:
            print(f"Ep {ep+1:4d}/{epochs} | loss={avg:.5f} | best={best:.5f}")

    # Validacao
    model.eval()
    with torch.no_grad():
        logits = model(inp.to(DEVICE))
        pred = logits.mean(dim=1).argmax(1)
        acc = (pred == tgt.to(DEVICE)).float().mean().item()
        print(f"\nAcuracia: {acc*100:.1f}%")
        print(f"Tempo: {time.time()-t0:.0f}s")
    model.export(TARGET / "hw_expert_tf.bitnet")

    # Verifica export
    with open(TARGET / "hw_expert_tf.bitnet", "rb") as f:
        magic = struct.unpack("<I", f.read(4))[0]
        print(f"Modelo OK: magic=0x{magic:X}")
    return model

if __name__ == "__main__":
    print("="*50)
    print("  HW EXPERT GPU TRAINING")
    print("="*50)
    devices = gerar_dados(50000)
    treinar(devices, epochs=300, batch=256)
