#!/usr/bin/env python3
"""Treino HW Expert na GPU com 44K dispositivos PCI+USB+SDIO.
Uso: CUDA_VISIBLE_DEVICES=0 python train_hw_gpu.py --epochs 300
"""
import os, sys, json, csv, time, torch, torch.nn as nn
from pathlib import Path
os.environ["CUDA_VISIBLE_DEVICES"] = os.environ.get("CUDA_VISIBLE_DEVICES", "0")
sys.path.insert(0, str(Path(__file__).parent))

TARGET = Path("target")
DEVICE = torch.device("cuda" if torch.cuda.is_available() else "cpu")
print(f"[GPU] Device: {DEVICE}")
if torch.cuda.is_available():
    print(f"[GPU] {torch.cuda.get_device_name(0)} | VRAM: {torch.cuda.get_device_properties(0).total_memory/1e9:.1f}GB")

# Register map families (same as generic_wifi.rs)
FAMILIAS = ["AtherosWiFi","BroadcomWiFi","GenericPCI","IntelEthernet",
            "IntelWiFi","RealtekEthernet","RealtekWiFi","VirtIO"]
FAM2IDX = {n:i for i,n in enumerate(FAMILIAS)}

def familia_para(vid, did):
    if vid==0x8086:
        if (0x08B1<=did<=0x2726) or did in(0x3165,0x3166,0x06F0,0x02F0,0x2526,0x2527,0x2723,0x2725,0x2726,0x24F3,0x24F4,0x24F5,0x24F6,0x24FD) or (did>>8)==0x08: return "IntelWiFi"
        if did in(0x100E,0x105E,0x10D3,0x10D5,0x10DE,0x10EA,0x10F5,0x10FB,0x10C9,0x1526,0x1527,0x154D,0x156F,0x1570,0x1533,0x1538,0x1539,0x1502,0x1503): return "IntelEthernet"
        return "GenericPCI"
    if vid==0x10EC: return "RealtekWiFi" if did in(0x8176,0x8179,0x8812) else ("RealtekEthernet" if did in(0x8139,0x8168,0x8169) else "GenericPCI")
    if vid==0x0BDA: return "RealtekWiFi"
    if vid==0x168C: return "AtherosWiFi"
    if vid==0x14E4: return "BroadcomWiFi"
    if vid in(0x1AF4,0x1234): return "VirtIO"
    if vid in(0x17CB,0x13D7): return "AtherosWiFi"
    return "GenericPCI"

# Modelo Transformer pequeno
class HwExpertTF(nn.Module):
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

    def export_bitnet(self, path):
        import struct
        MAGIC = 0xBE11BE11
        n_kv = self.h // 4
        with open(path, "wb") as f:
            np_ = self.h*self.v + self.nl*(4*self.h*self.h + 3*self.h*self.ff + 2*self.h) + self.h*self.v
            f.write(struct.pack("<I", MAGIC))
            f.write(struct.pack("<H", 4))
            f.write(struct.pack("<I", np_))
            for v in (self.h, self.nl, 4, self.v, 64, self.ff, n_kv, self.h//4, 0):
                f.write(struct.pack("<H", v))
            f.write(b"\x00\x00\x00\x00\x01")
            tok = b"hwexpert_v3"
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
                f.write(struct.pack("<I", len(t2)))
                f.write(struct.pack("<I", 0))
                f.write(qp(t2))

            def wv(t):
                a = t.detach().cpu().to(torch.float32).numpy().reshape(-1)
                f.write(struct.pack("<I", len(a)))
                for x in a: f.write(struct.pack("<f", float(x)))

            wt(self.embed.weight.T)
            for i in range(self.nl):
                wv(self.r1[i]); wv(self.r2[i])
                wv(torch.ones(self.h)); wv(torch.ones(self.ff))
                wt(self.q[i].weight.T); wt(self.k[i].weight.T)
                wt(self.v_[i].weight.T); wt(self.o[i].weight.T)
                wt(self.g[i].weight.T); wt(self.u[i].weight.T)
                wt(self.d[i].weight.T)
                wv(torch.tensor([10000.**(-2.*j/32) for j in range(16)]))
            wv(self.rf); wt(self.unembed.weight.T)
        print(f"  [OK] {path} ({path.stat().st_size//1024}KB)")

def carregar_dados():
    """Carrega dataset unificado (PCI + USB + SDIO)."""
    csv_path = TARGET / "hw_all_unified.csv"
    if not csv_path.exists():
        from download_hw_databases import download_pci, download_usb, merge_datasets
        pci = download_pci()
        usb = download_usb()
        from extract_all_sdio import build_dataset
        sdio = build_dataset()
        unified = merge_datasets(pci, usb, sdio)
    else:
        unified = []
        with open(csv_path, newline="") as f:
            for row in csv.DictReader(f):
                unified.append({"vid": int(row["vid"]), "did": int(row["did"])})

    print(f"Dados: {len(unified)} dispositivos")
    return unified

def treinar(unified, epochs=300, batch=128):
    print(f"\n=== Treino HW Expert (GPU) ===")
    vocab = 64

    # Converte para tensores
    inp = torch.tensor([[(d["vid"]>>8)%vocab, d["vid"]%vocab, (d["did"]>>8)%vocab, d["did"]%vocab] for d in unified], dtype=torch.long)
    tgt = torch.tensor([FAM2IDX.get(familia_para(d["vid"], d["did"]), 0) for d in unified], dtype=torch.long)
    print(f"  Amostras: {len(inp)} | Familias: {len(FAMILIAS)}")

    for fn in FAMILIAS:
        cnt = sum(1 for d in unified if familia_para(d["vid"], d["did"]) == fn)
        print(f"    {fn:20s} {cnt:5d}")

    model = HwExpertTF().to(DEVICE)
    n = sum(p.numel() for p in model.parameters())
    print(f"  Parametros: {n:,}")

    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(inp, tgt),
        batch_size=batch, shuffle=True, drop_last=True)

    opt = torch.optim.AdamW(model.parameters(), lr=3e-3, weight_decay=1e-5)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, 3e-3, steps_per_epoch=max(len(loader),1), epochs=epochs, last_epoch=-1)

    best = float("inf")
    t0 = time.time()
    for ep in range(epochs):
        model.train()
        tl = 0.0; nb = 0
        for x, y in loader:
            x, y = x.to(DEVICE), y.to(DEVICE)
            opt.zero_grad()
            loss = torch.nn.functional.cross_entropy(model(x).mean(dim=1), y)
            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step(); sched.step()
            tl += loss.item(); nb += 1
        avg = tl / max(nb, 1)
        if avg < best:
            best = avg
            model.export_bitnet(TARGET / "hw_expert_tf.bitnet")
        if (ep+1) % 50 == 0 or ep == 0:
            print(f"  Ep {ep+1:4d}/{epochs} | loss={avg:.5f} | best={best:.5f}")

    # Avaliacao
    model.eval()
    with torch.no_grad():
        logits = model(inp.to(DEVICE))
        pred = logits.mean(dim=1).argmax(1)
        correct = (pred == tgt.to(DEVICE)).sum().item()
        acc = correct / len(unified)
        print(f"\n  Acuracia: {acc*100:.1f}% ({correct}/{len(unified)})")
        print(f"  Tempo: {time.time()-t0:.0f}s")

    model.export_bitnet(TARGET / "hw_expert_tf.bitnet")
    return model, acc

if __name__ == "__main__":
    import argparse
    p = argparse.ArgumentParser()
    p.add_argument("--epochs", type=int, default=300)
    p.add_argument("--batch", type=int, default=128)
    args = p.parse_args()

    print("="*60)
    print("  HW EXPERT - GPU TRAINING (44,526 devices)")
    print("="*60)

    unified = carregar_dados()
    model, acc = treinar(unified, args.epochs, args.batch)

    print(f"\nModelo final: target/hw_expert_tf.bitnet")
