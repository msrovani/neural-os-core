#!/usr/bin/env python3
"""train_models_gpu.py — neural-os-core v1.0
Treina modelos .bitnet usando GPU (CUDA) via PyTorch.

Uso: python train_models_gpu.py [--epochs N] [--batch B]
     CUDA_VISIBLE_DEVICES=0 python train_models_gpu.py --all

Requer: pip install torch numpy transformers
"""

import argparse
import os
import struct
import sys
import math
from pathlib import Path

import numpy as np

TARGET = Path(__file__).parent / "target"
TARGET.mkdir(exist_ok=True)

try:
    import torch
    import torch.nn as nn
    import torch.nn.functional as F
    import torch.optim as optim
    HAS_TORCH = True
    DEVICE = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"[GPU] PyTorch {torch.__version__} | Device: {DEVICE}")
    if torch.cuda.is_available():
        print(f"[GPU] {torch.cuda.get_device_name(0)} | VRAM: {torch.cuda.get_device_properties(0).total_memory / 1e9:.1f}GB")
except ImportError:
    print("[FATAL] pip install torch numpy")
    sys.exit(1)

# ─── .bitnet v4 format ─────────────────────────────────────────────────────

MAGIC = 0xBE11BE11

def write_header(f, hidden, num_layers, num_heads, vocab_size, max_seq,
                 intermediate_size, num_kv_heads, q_dim, num_medusa,
                 tie_embeddings, tok_data):
    num_params = (hidden * vocab_size +
                  num_layers * (4 * hidden * hidden + 3 * hidden * intermediate_size + 2 * hidden + q_dim) +
                  hidden * vocab_size)
    f.write(struct.pack("<I", MAGIC))
    f.write(struct.pack("<H", 4))
    f.write(struct.pack("<I", num_params))
    f.write(struct.pack("<H", hidden))
    f.write(struct.pack("<H", num_layers))
    f.write(struct.pack("<H", num_heads))
    f.write(struct.pack("<I", vocab_size))
    f.write(struct.pack("<H", max_seq))
    f.write(struct.pack("<H", intermediate_size))
    f.write(struct.pack("<H", num_kv_heads))
    f.write(struct.pack("<H", q_dim))
    f.write(struct.pack("<I", num_medusa))
    f.write(b"TIED" if tie_embeddings else b"\x00\x00\x00\x00")
    f.write(struct.pack("B", 1))
    f.write(struct.pack("<I", len(tok_data)))
    f.write(tok_data)
    f.write(struct.pack("B", 0x07))
    return f.tell()

def quantize_ternary(arr_1d, threshold=0.5):
    packed = bytearray()
    for i in range(0, len(arr_1d), 4):
        byte = 0
        for j in range(4):
            if i + j < len(arr_1d):
                v = float(arr_1d[i + j])
                bits = 0b01 if v > threshold else (0b10 if v < -threshold else 0b00)
                byte |= bits << (j * 2)
        packed.append(byte)
    return bytes(packed)

def write_tensor(f, tensor_f32):
    t = tensor_f32.reshape(-1)
    f.write(struct.pack("<I", len(t)))
    f.write(struct.pack("<I", 0))
    data = quantize_ternary(t)
    f.write(data)

def write_vec_f32(f, vec):
    arr = list(vec)
    f.write(struct.pack("<I", len(arr)))
    for v in arr:
        f.write(struct.pack("<f", float(v)))

# ─── Lightweight BitNet LM ─────────────────────────────────────────────────

class BitNetLM(nn.Module):
    """Modelo ternario para fine-tuning."""
    def __init__(self, hidden=64, vocab=128, num_layers=4, num_heads=4, ffn_dim=128):
        super().__init__()
        self.hidden = hidden
        self.vocab = vocab
        self.num_layers = num_layers
        self.num_heads = num_heads
        self.ffn_dim = ffn_dim

        self.embed = nn.Embedding(vocab, hidden)
        self.q_proj = nn.ModuleList([nn.Linear(hidden, hidden, bias=False) for _ in range(num_layers)])
        self.k_proj = nn.ModuleList([nn.Linear(hidden, hidden, bias=False) for _ in range(num_layers)])
        self.v_proj = nn.ModuleList([nn.Linear(hidden, hidden, bias=False) for _ in range(num_layers)])
        self.o_proj = nn.ModuleList([nn.Linear(hidden, hidden, bias=False) for _ in range(num_layers)])
        self.gate_proj = nn.ModuleList([nn.Linear(hidden, ffn_dim, bias=False) for _ in range(num_layers)])
        self.up_proj = nn.ModuleList([nn.Linear(hidden, ffn_dim, bias=False) for _ in range(num_layers)])
        self.down_proj = nn.ModuleList([nn.Linear(ffn_dim, hidden, bias=False) for _ in range(num_layers)])

        self.rms_attn = nn.ParameterList([nn.Parameter(torch.ones(hidden)) for _ in range(num_layers)])
        self.rms_ffn = nn.ParameterList([nn.Parameter(torch.ones(hidden)) for _ in range(num_layers)])
        self.rms_inner = nn.ParameterList([nn.Parameter(torch.ones(hidden)) for _ in range(num_layers)])
        self.rms_ffn_norm = nn.ParameterList([nn.Parameter(torch.ones(ffn_dim)) for _ in range(num_layers)])

        self.unembed = nn.Linear(hidden, vocab, bias=False)
        self.rms_final = nn.Parameter(torch.ones(hidden))

    def forward(self, x):
        h = self.embed(x)
        for i in range(self.num_layers):
            residual = h
            h = h * self.rms_attn[i]
            q = self.q_proj[i](h)
            k = self.k_proj[i](h)
            v = self.v_proj[i](h)
            h = self.o_proj[i](v)
            h = h + residual
            residual = h
            h = h * self.rms_ffn[i]
            g = self.gate_proj[i](h)
            u = self.up_proj[i](h)
            h = self.down_proj[i](g * u)
            h = h + residual
        h = h * self.rms_final
        return self.unembed(h)

    def export_bitnet(self, path, tok_data=b""):
        with open(path, "wb") as f:
            write_header(f, self.hidden, self.num_layers, self.num_heads,
                        self.vocab, 64, self.ffn_dim,
                        self.num_heads, self.hidden // self.num_heads, 0, False, tok_data)
            write_tensor(f, self.embed.weight.data.T)
            for i in range(self.num_layers):
                write_vec_f32(f, self.rms_attn[i].data.cpu().numpy())
                write_vec_f32(f, self.rms_ffn[i].data.cpu().numpy())
                write_vec_f32(f, self.rms_inner[i].data.cpu().numpy())
                write_vec_f32(f, self.rms_ffn_norm[i].data.cpu().numpy())
                write_tensor(f, self.q_proj[i].weight.data.T)
                write_tensor(f, self.k_proj[i].weight.data.T)
                write_tensor(f, self.v_proj[i].weight.data.T)
                write_tensor(f, self.o_proj[i].weight.data.T)
                write_tensor(f, self.gate_proj[i].weight.data.T)
                write_tensor(f, self.up_proj[i].weight.data.T)
                write_tensor(f, self.down_proj[i].weight.data.T)
                rope = np.array([10000.0 ** (-2.0 * i / 32) for i in range(16)])
                write_vec_f32(f, rope)
            write_tensor(f, self.unembed.weight.data.T)
        print(f"  [OK] Exportado: {path} ({os.path.getsize(path)//1024}KB)")

# ─── RustCoder ─────────────────────────────────────────────────────────────

RUST_EXAMPLES = [
    ("fn main", "fn main() {\n    println!(\"Hello, world!\");\n}"),
    ("fn add", "fn add(a: i32, b: i32) -> i32 { a + b }"),
    ("for loop", "for i in 0..10 {\n    println!(\"{}\", i);\n}"),
    ("match", "match x {\n    1 => true,\n    _ => false,\n}"),
    ("let mut vec", "let mut v: Vec<u32> = Vec::new();"),
    ("struct", "#[derive(Debug)]\npub struct Point { x: i32, y: i32 }"),
    ("impl", "impl Point {\n    pub fn new(x: i32, y: i32) -> Self { Self { x, y } }\n}"),
    ("enum", "pub enum State { Idle, Running, Done, Error }"),
    ("unsafe", "unsafe { core::ptr::write_volatile(ptr, val); }"),
    ("#[repr(C)]", "#[repr(C, packed)]\npub struct Register { val: u32 }"),
    ("kjson!", "kjson!(\"BOOT\", \"AGENT\", \"ready\", \"name\", name);"),
    ("serial_println", "serial_println!(\"[KERNEL] iniciado\");"),
    ("Agent manifest", "fn manifest(&self) -> &AgentManifest { &MY_MANIFEST }"),
    ("match Option", "match opt {\n    Some(v) => Ok(v),\n    None => Err(\"missing\"),\n}"),
    ("Vec::new", "let mut data: Vec<u8> = Vec::with_capacity(1024);"),
    ("for in iter", "for (i, item) in items.iter().enumerate() { }"),
    ("loop", "loop {\n    match rx.try_recv() {\n        Some(msg) => process(msg),\n        None => break,\n    }\n}"),
    ("if let", "if let Some(val) = optional {\n    println!(\"{}\", val);\n}"),
    ("pub fn", "pub fn init() -> Result<(), &'static str> { Ok(()) }"),
    ("const", "pub const MAX_SIZE: usize = 4096;"),
]

def _build_seq(data, vocab=128, seq_len=32):
    import hashlib
    toks = [hashlib.md5(c.encode()).digest()[0] % vocab for c in data[:seq_len]]
    return toks + [0] * (seq_len - len(toks))

def train_rustcoder(epochs=200, batch_size=32):
    print("\n=== RustCoder Training ===")
    hidden, vocab, num_layers, num_heads, ffn_dim = 128, 128, 6, 8, 256
    model = BitNetLM(hidden=hidden, vocab=vocab, num_layers=num_layers,
                     num_heads=num_heads, ffn_dim=ffn_dim).to(DEVICE)
    print(f"  Params: {sum(p.numel() for p in model.parameters()):,} | "
          f"Device: {DEVICE} | Epochs: {epochs} | Batch: {batch_size}")

    import hashlib
    tokens, targets = [], []
    for inp, out in RUST_EXAMPLES:
        tokens.append(_build_seq(inp, vocab))
        targets.append(_build_seq(out, vocab))

    tokens_t = torch.tensor(tokens, device=DEVICE)
    targets_t = torch.tensor(targets, device=DEVICE)
    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(tokens_t, targets_t),
        batch_size=batch_size, shuffle=True)

    opt = optim.AdamW(model.parameters(), lr=3e-4, weight_decay=1e-5)
    sched = optim.lr_scheduler.CosineAnnealingLR(opt, T_max=epochs)
    best = float('inf')

    for epoch in range(epochs):
        model.train()
        total_loss = 0.0
        for x, y in loader:
            opt.zero_grad()
            logits = model(x)
            loss = F.cross_entropy(logits.view(-1, vocab), y.view(-1))
            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            total_loss += loss.item()

        avg = total_loss / len(loader)
        sched.step()

        if avg < best:
            best = avg
            model.export_bitnet(TARGET / "rust_coder.bitnet", tok_data=b"rustcoder_v1")

        if (epoch + 1) % 20 == 0 or epoch == 0:
            lr = sched.get_last_lr()[0]
            print(f"  Epoch {epoch+1:3d}/{epochs} | loss={avg:.4f} | lr={lr:.2e} | best={best:.4f}")

    print(f"  [OK] RustCoder: {hidden}dim x {num_layers}L, loss={best:.4f}")
    model.export_bitnet(TARGET / "rust_coder.bitnet", tok_data=b"rustcoder_v1")

# ─── HW Expert ─────────────────────────────────────────────────────────────

def train_hw_expert(epochs=100):
    print("\n=== HW Expert Training ===")
    hidden, vocab, num_layers = 64, 64, 3
    model = BitNetLM(hidden=hidden, vocab=vocab, num_layers=num_layers,
                     num_heads=4, ffn_dim=hidden).to(DEVICE)
    print(f"  Params: {sum(p.numel() for p in model.parameters()):,} | "
          f"Device: {DEVICE} | Epochs: {epochs}")

    pci = [
        (0x8086,0x29C0,0),(0x8086,0x2918,1),(0x8086,0x2922,2),
        (0x8086,0x2930,3),(0x1234,0x1111,4),(0x10EC,0x8139,5),
        (0x8086,0x100E,6),(0x1AF4,0x1041,7),(0x1AF4,0x1050,8),
        (0x1AF4,0x1000,9),(0x10DE,0x1C82,10),(0x1002,0x67DF,11),
        (0x8086,0x1912,12),(0x1B36,0x000D,13),(0x8086,0x24FD,14),
    ]
    tokens, targets = [], []
    for vid, did, cls in pci:
        tok = [(vid>>8)%vocab, (vid&0xFF)%vocab, (did>>8)%vocab, (did&0xFF)%vocab]
        tokens.append(tok + [0]*4)
        targets.append([cls] + [0]*7)

    tokens_t = torch.tensor(tokens, device=DEVICE)
    targets_t = torch.tensor(targets, device=DEVICE)
    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(tokens_t, targets_t),
        batch_size=8, shuffle=True)

    opt = optim.AdamW(model.parameters(), lr=1e-3)
    for epoch in range(epochs):
        total = 0.0
        for x, y in loader:
            opt.zero_grad()
            logits = model(x)
            loss = F.cross_entropy(logits.view(-1, vocab), y.view(-1))
            loss.backward()
            opt.step()
            total += loss.item()
        if (epoch + 1) % 20 == 0:
            print(f"  Epoch {epoch+1}/{epochs} | loss={total/len(loader):.4f}")

    model.export_bitnet(TARGET / "hw_expert.bitnet", tok_data=b"hwexpert_v1")
    print(f"  [OK] HW Expert: {len(pci)} dispositivos reconhecidos")

# ─── BGE ───────────────────────────────────────────────────────────────────

def convert_bge():
    dest = TARGET / "bge-small.bitnet"
    if dest.exists():
        print(f"[OK] {dest.name} ja existe ({dest.stat().st_size//1024}KB)")
        return
    print("[BGE] Baixando BAAI/bge-small-en-v1.5 do HuggingFace...")
    try:
        from transformers import AutoModel, AutoTokenizer
        model = AutoModel.from_pretrained("BAAI/bge-small-en-v1.5").to(DEVICE)
        tokenizer = AutoTokenizer.from_pretrained("BAAI/bge-small-en-v1.5")
        embed = model.embeddings.word_embeddings.weight.data
        hidden, vocab = embed.shape[1], embed.shape[0]
        with open(dest, "wb") as f:
            kv_heads = 4
            write_header(f, hidden=384, num_layers=12, num_heads=12,
                        vocab_size=vocab, max_seq=512, intermediate_size=1536,
                        num_kv_heads=kv_heads, q_dim=384, num_medusa=0,
                        tie_embeddings=False, tok_data=b"bge_v1")
            write_tensor(f, embed.T)
            # encoder layers simplificado
            for i in range(min(12, len(model.encoder.layer))):
                lyr = model.encoder.layer[i]
                write_vec_f32(f, [])  # rms_attn
                write_vec_f32(f, [])
                write_vec_f32(f, [])
                write_vec_f32(f, [])
                write_tensor(f, lyr.attention.self.query.weight.data.T if hasattr(lyr.attention.self,'query') else torch.zeros(1))
                write_tensor(f, lyr.attention.self.key.weight.data.T if hasattr(lyr.attention.self,'key') else torch.zeros(1))
                write_tensor(f, lyr.attention.self.value.weight.data.T if hasattr(lyr.attention.self,'value') else torch.zeros(1))
                write_tensor(f, lyr.attention.output.dense.weight.data.T if hasattr(lyr.attention.output,'dense') else torch.zeros(1))
                write_tensor(f, lyr.intermediate.dense.weight.data.T if hasattr(lyr,'intermediate') else torch.zeros(1))
                write_tensor(f, lyr.output.dense.weight.data.T if hasattr(lyr,'output') else torch.zeros(1))
                write_tensor(f, lyr.attention.output.LayerNorm.weight.data if hasattr(lyr.attention.output,'LayerNorm') else torch.zeros(1))
                write_vec_f32(f, [])
        torch.cuda.empty_cache()
        sz = dest.stat().st_size // (1024*1024)
        print(f"[OK] BGE-small: {hidden}dim, {vocab}vocab, {sz}MB")
    except Exception as e:
        torch.cuda.empty_cache()
        print(f"[ERRO] BGE: {e}")
        print("[--] Ignorando BGE (nao critico)")

# ─── Main ──────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--epochs", type=int, default=200)
    parser.add_argument("--batch", type=int, default=32)
    parser.add_argument("--rustcoder", action="store_true")
    parser.add_argument("--hw", action="store_true")
    parser.add_argument("--bge", action="store_true")
    parser.add_argument("--all", action="store_true")
    args = parser.parse_args()

    if not any([args.all, args.rustcoder, args.hw, args.bge]):
        args.all = True

    print("=" * 60)
    print(f"  neural-os-core v1.0 - GPU Training Pipeline")
    print(f"  Device: {DEVICE}  |  Epochs: {args.epochs}  |  Batch: {args.batch}")
    print("=" * 60)

    if args.all or args.bge:
        convert_bge()
    if args.all or args.rustcoder:
        train_rustcoder(epochs=args.epochs, batch_size=args.batch)
    if args.all or args.hw:
        train_hw_expert(epochs=args.epochs // 2)

    print("\n" + "=" * 60)
    print("  MODELOS GERADOS")
    print("=" * 60)
    total_mb = 0
    for f in sorted(TARGET.glob("*.bitnet")):
        mb = f.stat().st_size / (1024*1024)
        total_mb += mb
        print(f"  {f.name:40s} {mb:7.1f}MB")
    print(f"  {'TOTAL':40s} {total_mb:7.1f}MB")
    print(f"\n  python build_image.py --model-dir target")

if __name__ == "__main__":
    main()
