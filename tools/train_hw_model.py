#!/usr/bin/env python3
"""
Train hardware-aware transformer model for Neural OS Cortex.
Matches the kernel's architecture exactly for .bitnet export.

Usage:
    python prepare_hw_dataset.py --download
    python train_hw_model.py --dataset hw_knowledge.jsonl --output micro.bitnet
"""

import json
import struct
import math
import random
import argparse
import torch
import torch.nn as nn
import torch.nn.functional as F

torch.manual_seed(42)
random.seed(42)

VOCAB_SIZE = 99
HIDDEN = 64
NUM_LAYERS = 4
NUM_HEADS = 4
HEAD_DIM = HIDDEN // NUM_HEADS
FFN_DIM = HIDDEN * 2
MAX_SEQ = 64
BOS = 0
EOS = 1
PAD = 2
CHAR_OFFSET = 3

# Character-level tokenizer (same as kernel)
class Tokenizer:
    def encode(self, text):
        tokens = [BOS]
        for c in text.lower():
            b = ord(c)
            if 32 <= b <= 126:
                tokens.append(b - 32 + CHAR_OFFSET)
        tokens.append(EOS)
        return tokens[:MAX_SEQ]

    def decode(self, tokens):
        chars = []
        for t in tokens:
            if t == BOS or t == PAD:
                continue
            if t == EOS:
                break
            if t < VOCAB_SIZE:
                chars.append(chr(t - CHAR_OFFSET + 32))
        return ''.join(chars)

    def __len__(self):
        return VOCAB_SIZE

# TransformerBlock (same architecture as kernel)
class RMSNorm(nn.Module):
    def __init__(self, dim):
        super().__init__()
        self.weight = nn.Parameter(torch.ones(dim))

    def forward(self, x):
        rms = torch.sqrt((x.pow(2).mean(-1, keepdim=True)) + 1e-6)
        return x / rms * self.weight

class Attention(nn.Module):
    def __init__(self):
        super().__init__()
        self.q = nn.Linear(HIDDEN, HIDDEN, bias=False)
        self.k = nn.Linear(HIDDEN, HIDDEN, bias=False)
        self.v = nn.Linear(HIDDEN, HIDDEN, bias=False)
        self.o = nn.Linear(HIDDEN, HIDDEN, bias=False)

    def forward(self, x, mask):
        B, T, C = x.shape
        q = self.q(x).view(B, T, NUM_HEADS, HEAD_DIM).transpose(1, 2)
        k = self.k(x).view(B, T, NUM_HEADS, HEAD_DIM).transpose(1, 2)
        v = self.v(x).view(B, T, NUM_HEADS, HEAD_DIM).transpose(1, 2)
        scores = (q @ k.transpose(-2, -1)) / math.sqrt(HEAD_DIM)
        scores = scores + mask[:, :, :T, :T]
        attn = F.softmax(scores, dim=-1)
        out = (attn @ v).transpose(1, 2).contiguous().view(B, T, C)
        return self.o(out)

class TransformerBlock(nn.Module):
    def __init__(self):
        super().__init__()
        self.rms1 = RMSNorm(HIDDEN)
        self.attn = Attention()
        self.rms2 = RMSNorm(HIDDEN)
        self.gate = nn.Linear(HIDDEN, FFN_DIM, bias=False)
        self.up = nn.Linear(HIDDEN, FFN_DIM, bias=False)
        self.down = nn.Linear(FFN_DIM, HIDDEN, bias=False)

    def forward(self, x, mask):
        x = x + self.attn(self.rms1(x), mask)
        rms = self.rms2(x)
        return x + self.down(F.silu(self.gate(rms)) * self.up(rms))

class TransformerModel(nn.Module):
    def __init__(self):
        super().__init__()
        self.embed = nn.Embedding(VOCAB_SIZE, HIDDEN)
        self.layers = nn.ModuleList([TransformerBlock() for _ in range(NUM_LAYERS)])
        self.rms_final = RMSNorm(HIDDEN)
        self.unembed = nn.Linear(HIDDEN, VOCAB_SIZE, bias=False)

    def forward(self, tokens, mask=None):
        B, T = tokens.shape
        x = self.embed(tokens)
        if mask is None:
            mask = torch.triu(torch.full((1, 1, T, T), float('-inf'), device=tokens.device), diagonal=1)
        for layer in self.layers:
            x = layer(x, mask)
        logits = self.unembed(self.rms_final(x))
        return logits

    def generate(self, tokenizer, prompt, max_len=32):
        device = next(self.parameters()).device
        tokens = torch.tensor([tokenizer.encode(prompt)]).long().to(device)
        for _ in range(max_len):
            if tokens.size(1) >= MAX_SEQ:
                break
            logits = self.forward(tokens)
            next_token = logits[0, -1].argmax().item()
            tokens = torch.cat([tokens, torch.tensor([[next_token]], device=device)], dim=1)
            if next_token == EOS:
                break
        return tokenizer.decode(tokens[0].tolist())

# Quantize Linear weights to ternary {-1, 0, +1}
def quantize_ternary(weight: torch.Tensor, threshold=0.1):
    w = weight.detach().cpu().numpy()
    ternary = []
    for v in w.flatten():
        if v > threshold:
            ternary.append(1)
        elif v < -threshold:
            ternary.append(-1)
        else:
            ternary.append(0)
    return ternary

def pack_ternary(weights):
    packed = []
    for i in range(0, len(weights), 4):
        byte = 0
        for j in range(4):
            if i + j < len(weights):
                v = weights[i + j]
                bits = 0b00 if v == 0 else (0b01 if v == 1 else 0b10)
                byte |= bits << (j * 2)
        packed.append(byte)
    return bytes(packed)

def export_bitnet(model, tokenizer, path, threshold=0.1):
    """Export trained model to .bitnet format matching kernel's load_model()."""
    buf = bytearray()

    # Header
    hidden = HIDDEN
    num_layers = NUM_LAYERS
    num_heads = NUM_HEADS
    vocab = VOCAB_SIZE
    max_seq = MAX_SEQ
    num_params = sum(p.numel() for p in model.parameters())

    buf.extend(struct.pack('<I', 0xBE11BE11))
    buf.extend(struct.pack('<H', 1))
    buf.extend(struct.pack('<I', num_params))
    buf.extend(struct.pack('<H', hidden))
    buf.extend(struct.pack('<H', num_layers))
    buf.extend(struct.pack('<H', num_heads))
    buf.extend(struct.pack('<H', vocab))
    buf.extend(struct.pack('<H', max_seq))
    buf.extend(b'\x00' * 4)

    # Embedding table: f32 [vocab, hidden]
    embed = model.embed.weight.detach().cpu().numpy()
    for val in embed.flatten():
        buf.extend(struct.pack('<f', float(val)))

    # Layers
    for layer in model.layers:
        # rms_attn weight
        buf.extend(struct.pack('<f', float(layer.rms1.weight[0].item())))

        # Q, K, V, O — ternarized
        for name in ['q', 'k', 'v', 'o']:
            w = getattr(layer.attn, name).weight
            ternary = quantize_ternary(w, threshold)
            buf.extend(pack_ternary(ternary))

        # rms_ffn weight
        buf.extend(struct.pack('<f', float(layer.rms2.weight[0].item())))

        # Gate, Up, Down — ternarized
        for name in ['gate', 'up', 'down']:
            w = getattr(layer, name).weight
            ternary = quantize_ternary(w, threshold)
            buf.extend(pack_ternary(ternary))

    # Unembed — ternarized
    w = model.unembed.weight
    ternary = quantize_ternary(w, threshold)
    buf.extend(pack_ternary(ternary))

    with open(path, 'wb') as f:
        f.write(bytes(buf))
    print(f"[EXPORT] Modelo salvo em {path}: {len(buf)} bytes (~{num_params} params)")

def load_dataset(path, max_examples=5000):
    """Load JSONL dataset and prepare training examples."""
    tokenizer = Tokenizer()
    examples = []
    with open(path, 'r', encoding='utf-8') as f:
        for line in f:
            if len(examples) >= max_examples:
                break
            item = json.loads(line)
            inp = item['input'].lower().strip()
            out = item['output'].lower().strip()
            if inp and out:
                full_text = f"<{inp}>{out}"
                tokens = tokenizer.encode(full_text)
                if len(tokens) >= 3:
                    examples.append(tokens)
    return examples, tokenizer

def collate_batch(batch):
    max_len = max(len(t) for t in batch)
    x_batch, y_batch = [], []
    for tokens in batch:
        pad_len = max_len - len(tokens)
        x = tokens[:-1] + [PAD] * max(0, pad_len)
        y = tokens[1:] + [-100] * max(0, pad_len)
        x_batch.append(x[:max_len])
        y_batch.append(y[:max_len])
    return torch.tensor(x_batch).long(), torch.tensor(y_batch).long()

def train():
    parser = argparse.ArgumentParser(description='Train HW-aware transformer')
    parser.add_argument('--dataset', default='hw_knowledge.jsonl')
    parser.add_argument('--output', default='micro.bitnet')
    parser.add_argument('--epochs', type=int, default=8)
    parser.add_argument('--lr', type=float, default=1e-3)
    parser.add_argument('--threshold', type=float, default=0.05)
    parser.add_argument('--batch-size', type=int, default=32)
    parser.add_argument('--max-examples', type=int, default=5000)
    args = parser.parse_args()

    print("[TRAIN] Carregando dataset...")
    examples, tokenizer = load_dataset(args.dataset, args.max_examples)
    print(f"[TRAIN] {len(examples)} exemplos carregados")

    import os
    os.environ['CUDA_VISIBLE_DEVICES'] = '0'
    device = torch.device('cuda' if torch.cuda.is_available() else 'cpu')
    model = TransformerModel().to(device)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr)

    for epoch in range(args.epochs):
        random.shuffle(examples)
        total_loss = 0
        batches = 0
        for i in range(0, len(examples), args.batch_size):
            batch = examples[i:i + args.batch_size]
            x, y = collate_batch(batch)
            x, y = x.to(device), y.to(device)

            optimizer.zero_grad()
            logits = model(x)
            loss = F.cross_entropy(logits.view(-1, VOCAB_SIZE), y.view(-1), ignore_index=-100)
            loss.backward()
            optimizer.step()
            total_loss += loss.item()
            batches += 1

        avg_loss = total_loss / batches
        print(f"[TRAIN] Epoch {epoch+1}/{args.epochs}: loss={avg_loss:.4f}")

        if (epoch + 1) % 5 == 0:
            # Test generation
            for test in ["8086 1237", "o que e 10EC 8139", "class 0300", "mostre hardware"]:
                out = model.generate(tokenizer, test)
                print(f"  '{test}' -> '{out}'")

    print("[TRAIN] Exportando modelo...")
    export_bitnet(model, tokenizer, args.output, threshold=args.threshold)
    print("[DONE] Modelo treinado e exportado!")

if __name__ == '__main__':
    train()
