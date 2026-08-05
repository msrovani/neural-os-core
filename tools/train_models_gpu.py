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

from bitnet_writer import (
    write_header_v6, write_embed, write_rms, write_ternary, compute_feat,
    MODEL_LLM, ACT_SILU, EMBED_TERNARY,
)

TARGET = Path(__file__).parent / "target"
TARGET.mkdir(exist_ok=True)

os.environ.setdefault("CUDA_VISIBLE_DEVICES", "0")

try:
    import torch
    import torch.nn as nn
    import torch.nn.functional as F
    import torch.optim as optim
    HAS_TORCH = True
    _cuda_ok = False
    if torch.cuda.is_available():
        try:
            torch.zeros(4, device="cuda").sum().item()
            _cuda_ok = True
        except Exception as e:
            print(f"[WARN] CUDA sem kernels sm_61: {e}")
            print("[HINT] pip install torch==2.13.0+cu126 --index-url https://download.pytorch.org/whl/cu126")
    DEVICE = torch.device("cuda" if _cuda_ok else "cpu")
    print(f"[GPU] PyTorch {torch.__version__} cuda={getattr(torch.version,'cuda',None)} | Device: {DEVICE}")
    if DEVICE.type == "cuda":
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
    """Quantiza float -> packing 2-bit ternario. Aceita list/np/torch (sempre via numpy)."""
    if hasattr(arr_1d, "detach"):
        arr = arr_1d.detach().float().cpu().numpy()
    else:
        arr = np.asarray(arr_1d, dtype=np.float32)
    flat = np.ascontiguousarray(arr).reshape(-1)
    n = flat.size
    bits = np.zeros(n, dtype=np.uint8)
    bits[flat > threshold] = 0b01
    bits[flat < -threshold] = 0b10
    pad = (-n) % 4
    if pad:
        bits = np.concatenate([bits, np.zeros(pad, dtype=np.uint8)])
    b = bits.reshape(-1, 4)
    packed = b[:, 0] | (b[:, 1] << 2) | (b[:, 2] << 4) | (b[:, 3] << 6)
    return packed.tobytes()

def quantize_ternary_i8(arr, threshold=0.5):
    """Quantiza float -> int8 {-1,0,1} (mesma matematica de quantize_ternary)."""
    if hasattr(arr, "detach"):
        a = arr.detach().float().cpu().numpy()
    else:
        a = np.asarray(arr, dtype=np.float32)
    flat = np.ascontiguousarray(a).reshape(-1)
    q = np.zeros(flat.size, dtype=np.int8)
    q[flat > threshold] = 1
    q[flat < -threshold] = -1
    return q.reshape(a.shape)

def write_tensor(f, tensor_f32):
    if hasattr(tensor_f32, "detach"):
        t = tensor_f32.detach().float().cpu().numpy().reshape(-1)
    else:
        t = np.asarray(tensor_f32, dtype=np.float32).reshape(-1)
    f.write(struct.pack("<I", int(t.size)))
    f.write(struct.pack("<I", 0))
    f.write(quantize_ternary(t))

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
            # ADR-0085 §10.3: silu(g) * u — ativa act_type=0 no export (era g * u).
            # ⚠️ TinyStories/RustCoder precisam de RETREINO apos esta mudanca.
            h = self.down_proj[i](F.silu(g) * u)
            h = h + residual
        h = h * self.rms_final
        return self.unembed(h)

    def export_bitnet(self, path, tok_data=b""):
        hidden, layers, heads, vocab, ffn = self.hidden, self.num_layers, self.num_heads, self.vocab, self.ffn_dim
        q_dim = hidden  # v6: dim total da projecao Q (Linear(hidden, hidden))
        kv_heads = heads
        k_dim = kv_heads * (q_dim // heads)  # == hidden (k/v projetam para hidden)
        num_params = (hidden * vocab +                  # embed
                      ffn * layers +                    # rms_ffn_norm (D2: ffn_dim)
                      layers * (hidden * q_dim + hidden * k_dim * 2 + q_dim * hidden +
                                hidden * ffn * 2 + ffn * q_dim) +
                      hidden * vocab)                   # unembed (not tied)
        feat = compute_feat(True, True, True)  # rms_inner + rms_ffn_norm + theta
        with open(path, "wb") as f:
            write_header_v6(
                f, model_type=MODEL_LLM, num_params=num_params,
                hidden=hidden, layers=layers, heads=heads, vocab=vocab,
                max_seq=64, intermediate=ffn, kv_heads=kv_heads, q_dim=q_dim,
                medusa=0, tie=False, tok_data=tok_data,
                act_type=ACT_SILU, embed_type=EMBED_TERNARY, feat=feat,
            )
            write_embed(f, quantize_ternary_i8(self.embed.weight.data.T), EMBED_TERNARY, 1.0)
            for i in range(layers):
                write_rms(f, self.rms_attn[i].data.cpu().numpy())
                write_rms(f, self.rms_ffn[i].data.cpu().numpy())
                write_rms(f, self.rms_inner[i].data.cpu().numpy())       # feat bit0
                write_rms(f, self.rms_ffn_norm[i].data.cpu().numpy())    # feat bit1 (ffn_dim)
                write_ternary(f, quantize_ternary_i8(self.q_proj[i].weight.data.T), 1.0)
                write_ternary(f, quantize_ternary_i8(self.k_proj[i].weight.data.T), 1.0)
                write_ternary(f, quantize_ternary_i8(self.v_proj[i].weight.data.T), 1.0)
                write_ternary(f, quantize_ternary_i8(self.o_proj[i].weight.data.T), 1.0)
                write_ternary(f, quantize_ternary_i8(self.gate_proj[i].weight.data.T), 1.0)
                write_ternary(f, quantize_ternary_i8(self.up_proj[i].weight.data.T), 1.0)
                write_ternary(f, quantize_ternary_i8(self.down_proj[i].weight.data.T), 1.0)
            write_rms(f, self.rms_final.data.cpu().numpy())
            write_ternary(f, quantize_ternary_i8(self.unembed.weight.data.T), 1.0)
            f.write(struct.pack("<f", 10000.0))  # theta (feat bit2) — rope base unica no fim
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

def _build_seq_bytes(data, vocab=128, seq_len=256):
    """Byte-level tokenization: ord(c) % vocab preserves character identity."""
    toks = [ord(c) % vocab for c in data[:seq_len]]
    return toks + [0] * (seq_len - len(toks))


def _load_rusttraining_pairs():
    """Load Microsoft RustTraining dataset. Falls back to RUST_EXAMPLES if not found."""
    pairs_path = TARGET / "rusttraining_pairs.json"
    if not pairs_path.exists():
        print("  [WARN] rusttraining_pairs.json not found, using 20 built-in examples")
        return RUST_EXAMPLES
    import json
    with open(pairs_path, "r", encoding="utf-8") as f:
        data = json.load(f)
    pairs = [(item["context"], item["code"]) for item in data]
    print(f"  Loaded {len(pairs)} training pairs from {pairs_path}")
    return pairs


def train_rustcoder(epochs=200, batch_size=32):
    print("\n=== RustCoder Training ===")
    hidden, vocab, num_layers, num_heads, ffn_dim = 128, 128, 6, 8, 256
    model = BitNetLM(hidden=hidden, vocab=vocab, num_layers=num_layers,
                     num_heads=num_heads, ffn_dim=ffn_dim).to(DEVICE)
    print(f"  Params: {sum(p.numel() for p in model.parameters()):,} | "
          f"Device: {DEVICE} | Epochs: {epochs} | Batch: {batch_size}")

    pairs = _load_rusttraining_pairs()
    tokens, targets = [], []
    for inp, out in pairs:
        tokens.append(_build_seq_bytes(inp, vocab))
        targets.append(_build_seq_bytes(out, vocab))

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
            model.export_bitnet(TARGET / "rust_coder.bitnet", tok_data=b"rustcoder_v2")

        if (epoch + 1) % 20 == 0 or epoch == 0:
            lr = sched.get_last_lr()[0]
            print(f"  Epoch {epoch+1:3d}/{epochs} | loss={avg:.4f} | lr={lr:.2e} | best={best:.4f}")

    print(f"  [OK] RustCoder: {hidden}dim x {num_layers}L, loss={best:.4f}")
    model.export_bitnet(TARGET / "rust_coder.bitnet", tok_data=b"rustcoder_v2")


def train_rustcoder_microsoft(epochs=200, batch_size=32):
    """Train RustCoder specifically on Microsoft RustTraining dataset (1291 pairs).
    
    Uses byte-level tokenization (ord-based, no hash collisions) and seq_len=256
    to handle full Rust code examples. Automatically scales down model for CPU.
    """
    is_cpu = str(DEVICE) == "cpu"
    if is_cpu:
        hidden, num_layers, num_heads, ffn_dim = 64, 4, 4, 128
        print("\n=== RustCoder Microsoft Training (CPU mode: 64dim x 4L) ===")
    else:
        hidden, num_layers, num_heads, ffn_dim = 128, 6, 8, 256
        print("\n=== RustCoder Microsoft Training (GPU mode: 128dim x 6L) ===")
    vocab, seq_len = 128, 256
    model = BitNetLM(hidden=hidden, vocab=vocab, num_layers=num_layers,
                     num_heads=num_heads, ffn_dim=ffn_dim).to(DEVICE)
    total_params = sum(p.numel() for p in model.parameters())
    print(f"  Params: {total_params:,} | Device: {DEVICE} | Epochs: {epochs}")
    print(f"  Batch: {batch_size} | SeqLen: {seq_len} | Vocab: {vocab}")

    pairs = _load_rusttraining_pairs()
    if len(pairs) <= 20:
        print("  [FALLBACK] Only built-in examples, using standard training")
        return train_rustcoder(epochs, batch_size)

    tokens, targets = [], []
    skipped = 0
    for inp, out in pairs:
        toks = _build_seq_bytes(inp, vocab, seq_len)
        tgt = _build_seq_bytes(out, vocab, seq_len)
        # Skip pairs where target is all padding (empty code)
        if all(t == 0 for t in tgt):
            skipped += 1
            continue
        tokens.append(toks)
        targets.append(tgt)

    print(f"  Pairs: {len(pairs)} loaded, {len(tokens)} usable, {skipped} skipped (empty target)")

    tokens_t = torch.tensor(tokens, device=DEVICE)
    targets_t = torch.tensor(targets, device=DEVICE)
    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(tokens_t, targets_t),
        batch_size=batch_size, shuffle=True, drop_last=True)

    opt = optim.AdamW(model.parameters(), lr=3e-4, weight_decay=1e-5)
    sched = optim.lr_scheduler.CosineAnnealingLR(opt, T_max=epochs)
    best = float('inf')

    for epoch in range(epochs):
        model.train()
        total_loss = 0.0
        batches = 0
        for x, y in loader:
            opt.zero_grad()
            logits = model(x)
            loss = F.cross_entropy(logits.view(-1, vocab), y.view(-1))
            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            total_loss += loss.item()
            batches += 1

        avg = total_loss / max(batches, 1)
        sched.step()

        if avg < best:
            best = avg
            model.export_bitnet(TARGET / "rust_coder.bitnet", tok_data=b"rustcoder_microsoft_v1")

        if (epoch + 1) % 10 == 0 or epoch == 0:
            lr = sched.get_last_lr()[0]
            print(f"  Epoch {epoch+1:3d}/{epochs} | loss={avg:.4f} | lr={lr:.2e} | best={best:.4f}")

    print(f"  [OK] RustCoder Microsoft: {hidden}dim x {num_layers}L, loss={best:.4f}")
    model.export_bitnet(TARGET / "rust_coder.bitnet", tok_data=b"rustcoder_microsoft_v1")

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
    parser.add_argument("--microsoft", action="store_true")
    parser.add_argument("--hw", action="store_true")
    parser.add_argument("--bge", action="store_true")
    parser.add_argument("--all", action="store_true")
    args = parser.parse_args()

    if not any([args.all, args.rustcoder, args.microsoft, args.hw, args.bge]):
        args.all = True

    print("=" * 60)
    print(f"  neural-os-core v1.0 - GPU Training Pipeline")
    print(f"  Device: {DEVICE}  |  Epochs: {args.epochs}  |  Batch: {args.batch}")
    print("=" * 60)

    if args.all or args.bge:
        convert_bge()
    if args.all or args.rustcoder:
        train_rustcoder(epochs=args.epochs, batch_size=args.batch)
    if args.microsoft:
        train_rustcoder_microsoft(epochs=args.epochs, batch_size=args.batch)
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
