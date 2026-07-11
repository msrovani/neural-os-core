#!/usr/bin/env python3
"""Treina modelo STT CTC tiny (MFCC → 2×LSTM → CTC) e exporta .bin.
Uso: python tools/train_stt.py [--epochs 100]
"""
import argparse, os, struct, sys, math
from pathlib import Path

import numpy as np

ROOT = Path(__file__).parent.parent
TARGET = ROOT / "target"

try:
    import torch
    import torch.nn as nn
    import torch.nn.functional as F
    import torch.optim as optim
    DEVICE = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"[STT] PyTorch {torch.__version__} | Device: {DEVICE}")
except ImportError:
    print("[FATAL] pip install torch numpy")
    sys.exit(1)

N_MFCC = 13
HIDDEN = 64
VOCAB = 28  # a-z + space(26) + blank(27)

class TinyLSTM(nn.Module):
    def __init__(self):
        super().__init__()
        self.lstm0 = nn.LSTMCell(N_MFCC, HIDDEN)
        self.lstm1 = nn.LSTMCell(HIDDEN, HIDDEN)
        self.out = nn.Linear(HIDDEN, VOCAB)

    def forward(self, x):
        # x: [batch, frames, N_MFCC]
        B, T, _ = x.shape
        h0 = torch.zeros(B, HIDDEN, device=x.device)
        c0 = torch.zeros(B, HIDDEN, device=x.device)
        h1 = torch.zeros(B, HIDDEN, device=x.device)
        c1 = torch.zeros(B, HIDDEN, device=x.device)
        logits = []
        for t in range(T):
            h0, c0 = self.lstm0(x[:, t], (h0, c0))
            h1, c1 = self.lstm1(h0, (h1, c1))
            logits.append(self.out(h1))
        return torch.stack(logits, dim=1)  # [B, T, V]

def generate_synthetic_batch(batch_size=16, max_frames=50):
    """Gera dados sintéticos: ruído + tom para treino."""
    x = torch.randn(batch_size, max_frames, N_MFCC) * 0.1
    # Add some structured signal (tones)
    for b in range(batch_size):
        for t in range(max_frames):
            x[b, t, 0] += 0.5 * math.sin(t * 0.2)
    # Random target sequences (simple words)
    words = [b"hello", b"world", b"test", b"jarvis", b"stop", b"start", b"yes", b"no", b"up", b"down"]
    targets = []
    for b in range(batch_size):
        w = words[np.random.randint(len(words))]
        targets.append(torch.tensor([c - 97 if c >= 97 and c <= 122 else 26 for c in w], dtype=torch.long))
    return x.to(DEVICE), targets

def train_model(epochs=100, batch_size=16):
    model = TinyLSTM().to(DEVICE)
    opt = optim.Adam(model.parameters(), lr=1e-3)
    print(f"[STT] Params: {sum(p.numel() for p in model.parameters()):,}")

    for epoch in range(epochs):
        opt.zero_grad()
        x, targets = generate_synthetic_batch(batch_size)
        logits = model(x)  # [B, T, V]
        # CTC loss needs log_softmax + target lengths
        B, T, V = logits.shape
        log_probs = F.log_softmax(logits, dim=2).permute(1, 0, 2)  # [T, B, V]
        target_lengths = torch.tensor([len(t) for t in targets], device=DEVICE)
        input_lengths = torch.full((B,), T, dtype=torch.long, device=DEVICE)
        targets_padded = torch.nn.utils.rnn.pad_sequence(targets, batch_first=True).to(DEVICE)
        loss = F.ctc_loss(log_probs, targets_padded, input_lengths, target_lengths, blank=VOCAB-1)
        loss.backward()
        opt.step()

        if (epoch + 1) % 20 == 0 or epoch == 0:
            print(f"  Epoch {epoch+1}/{epochs} | loss={loss.item():.4f}")

    # Export .bin
    output = TARGET / "STT.BIN"
    MAGIC = 0xBE11BE11
    with open(output, "wb") as f:
        # Collect named parameters
        params = []
        for name, p in model.named_parameters():
            arr = p.detach().cpu().numpy().reshape(-1).astype(np.float32)
            params.append((name, arr))

        # Header
        f.write(struct.pack("<I", MAGIC))
        f.write(struct.pack("<I", 4))
        f.write(struct.pack("<I", len(params)))
        f.write(struct.pack("<I", 0))

        # Index
        data_off = 0
        for name, arr in params:
            bname = name.encode().ljust(32, b'\x00')[:32]
            cnt = len(arr)
            f.write(bname)
            f.write(struct.pack("<I", 16 + len(params) * 40 + data_off * 4))
            f.write(struct.pack("<I", cnt))
            data_off += cnt

        # Data
        for name, arr in params:
            f.write(arr.tobytes())

    sz = output.stat().st_size
    print(f"\n[OK] {output}: {sz:,} bytes ({sz/1024:.1f}KB)")
    print("  Params:", {n: list(p.shape) for n, p in model.named_parameters()})

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--epochs", type=int, default=100)
    parser.add_argument("--batch", type=int, default=16)
    args = parser.parse_args()
    train_model(args.epochs, args.batch)
