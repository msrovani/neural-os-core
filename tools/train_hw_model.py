#!/usr/bin/env python3
import json, struct, math, random, argparse
import torch, torch.nn as nn, torch.nn.functional as F

torch.manual_seed(42); random.seed(42)

VOCAB_SIZE = 99; HIDDEN = 64; NUM_LAYERS = 4; NUM_HEADS = 4
HEAD_DIM = HIDDEN // NUM_HEADS; FFN_DIM = HIDDEN * 2; MAX_SEQ = 64
BOS = 0; EOS = 1; PAD = 2; CHAR_OFFSET = 3
MEDUSA_HEADS = 3

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
            if t in (BOS, PAD): continue
            if t == EOS: break
            if t < VOCAB_SIZE: chars.append(chr(t - CHAR_OFFSET + 32))
        return ''.join(chars)
    def __len__(self): return VOCAB_SIZE

class RMSNorm(nn.Module):
    def __init__(self, dim):
        super().__init__(); self.weight = nn.Parameter(torch.ones(dim))
    def forward(self, x):
        return x / torch.sqrt(x.pow(2).mean(-1, keepdim=True) + 1e-6) * self.weight

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
        return self.o((F.softmax(scores, dim=-1) @ v).transpose(1, 2).contiguous().view(B, T, C))

class TransformerBlock(nn.Module):
    def __init__(self):
        super().__init__()
        self.rms1 = RMSNorm(HIDDEN); self.attn = Attention()
        self.rms2 = RMSNorm(HIDDEN)
        self.gate = nn.Linear(HIDDEN, FFN_DIM, bias=False)
        self.up = nn.Linear(HIDDEN, FFN_DIM, bias=False)
        self.down = nn.Linear(FFN_DIM, HIDDEN, bias=False)
    def forward(self, x, mask):
        x = x + self.attn(self.rms1(x), mask)
        rms = self.rms2(x)
        return x + self.down(F.silu(self.gate(rms)) * self.up(rms))

class MedusaHead(nn.Module):
    """Predicts token at position t+k from hidden state at position t."""
    def __init__(self):
        super().__init__()
        self.proj = nn.Linear(HIDDEN, VOCAB_SIZE, bias=False)
    def forward(self, x):
        return self.proj(x)

class TransformerModel(nn.Module):
    def __init__(self):
        super().__init__()
        self.embed = nn.Embedding(VOCAB_SIZE, HIDDEN)
        self.layers = nn.ModuleList([TransformerBlock() for _ in range(NUM_LAYERS)])
        self.rms_final = RMSNorm(HIDDEN)
        self.unembed = nn.Linear(HIDDEN, VOCAB_SIZE, bias=False)
        self.medusa_heads = nn.ModuleList([MedusaHead() for _ in range(MEDUSA_HEADS)])

    def forward(self, tokens, mask=None):
        B, T = tokens.shape
        x = self.embed(tokens)
        if mask is None:
            mask = torch.triu(torch.full((1, 1, T, T), float('-inf'), device=tokens.device), diagonal=1)
        for layer in self.layers:
            x = layer(x, mask)
        hidden = self.rms_final(x)
        logits = self.unembed(hidden)
        return logits, hidden

    def generate(self, tokenizer, prompt, max_len=32, speculative=True):
        device = next(self.parameters()).device
        tokens = torch.tensor([tokenizer.encode(prompt)]).long().to(device)
        for _ in range(max_len):
            if tokens.size(1) >= MAX_SEQ: break
            logits, hidden = self.forward(tokens)

            if speculative and tokens.size(1) + MEDUSA_HEADS + 1 < MAX_SEQ:
                last_hidden = hidden[:, -1:, :]
                draft_tokens = []
                for head in self.medusa_heads:
                    hl = head(last_hidden)
                    draft_tokens.append(hl[0, -1].argmax().item())
                cand = torch.cat([tokens, torch.tensor([draft_tokens], device=device).long()], dim=1)
                cand_logits, _ = self.forward(cand)
                base_pos = tokens.size(1) - 1
                accept = 0
                for i, d in enumerate(draft_tokens):
                    pos = base_pos + 1 + i
                    if pos >= cand_logits.size(1): break
                    if cand_logits[0, pos].argmax().item() == d: accept += 1
                    else: break
                next_tok = cand_logits[0, base_pos].argmax().item()
                tokens = torch.cat([tokens, torch.tensor([[next_tok]], device=device).long()], dim=1)
                for d in draft_tokens[:accept]:
                    tokens = torch.cat([tokens, torch.tensor([[d]], device=device).long()], dim=1)
                if accept < MEDUSA_HEADS:
                    pos = base_pos + 1 + accept + 1
                    if pos < cand_logits.size(1):
                        tokens = torch.cat([tokens, torch.tensor([[cand_logits[0, pos].argmax().item()]], device=device).long()], dim=1)
            else:
                next_tok = logits[0, -1].argmax().item()
                tokens = torch.cat([tokens, torch.tensor([[next_tok]], device=device).long()], dim=1)
            if tokens[0, -1].item() == EOS: break
        return tokenizer.decode(tokens[0].tolist())

def quantize_ternary(weight, threshold=0.1):
    w = weight.detach().cpu().numpy()
    ternary = [(1 if v > threshold else (-1 if v < -threshold else 0)) for v in w.flatten()]
    return ternary

def pack_ternary(weights):
    packed = bytearray()
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
    buf = bytearray()
    num_params = sum(p.numel() for p in model.parameters())
    buf.extend(struct.pack('<I', 0xBE11BE11))
    buf.extend(struct.pack('<H', 2))
    buf.extend(struct.pack('<I', num_params))
    buf.extend(struct.pack('<H', HIDDEN))
    buf.extend(struct.pack('<H', NUM_LAYERS))
    buf.extend(struct.pack('<H', NUM_HEADS))
    buf.extend(struct.pack('<H', VOCAB_SIZE))
    buf.extend(struct.pack('<H', MAX_SEQ))
    buf.extend(struct.pack('<B', MEDUSA_HEADS))
    buf.extend(b'\x00' * 3)

    embed = model.embed.weight.detach().cpu().numpy()
    for val in embed.flatten(): buf.extend(struct.pack('<f', float(val)))

    for layer in model.layers:
        buf.extend(struct.pack('<f', float(layer.rms1.weight[0].item())))
        for name in ['q', 'k', 'v', 'o']:
            ternary = quantize_ternary(getattr(layer.attn, name).weight, threshold)
            buf.extend(pack_ternary(ternary))
        buf.extend(struct.pack('<f', float(layer.rms2.weight[0].item())))
        for name in ['gate', 'up', 'down']:
            ternary = quantize_ternary(getattr(layer, name).weight, threshold)
            buf.extend(pack_ternary(ternary))

    ternary = quantize_ternary(model.unembed.weight, threshold)
    buf.extend(pack_ternary(ternary))

    for head in model.medusa_heads:
        ternary = quantize_ternary(head.proj.weight, threshold)
        buf.extend(pack_ternary(ternary))

    with open(path, 'wb') as f: f.write(bytes(buf))
    print(f"[EXPORT] {path}: {len(buf)} bytes ({num_params} params)")

def load_dataset(path, max_examples=50000, augment=True):
    tokenizer = Tokenizer()
    examples = []
    with open(path, 'r', encoding='utf-8') as f:
        for line in f:
            if len(examples) >= max_examples: break
            item = json.loads(line)
            inp = item['input'].lower().strip()
            out = item['output'].lower().strip()
            if inp and out:
                examples.append(tokenizer.encode(f"<{inp}>{out}"))

    if augment:
        added = 0
        base = examples[:10000]
        for tokens in base:
            text = tokenizer.decode(tokens)
            if '<' not in text: continue
            parts = text.split('>', 1)
            if len(parts) != 2: continue
            query, answer = parts[0][1:], parts[1]
            for prefix in ['o que e ', 'fale sobre ', 'identifique ', 'explique ']:
                aug = f"<{prefix}{query}>{answer}"
                aug_tok = tokenizer.encode(aug)
                if len(aug_tok) >= 3 and len(aug_tok) <= MAX_SEQ:
                    examples.append(aug_tok)
                    added += 1
        print(f"[AUGMENT] {added} exemplos sinteticos gerados")

    return examples, tokenizer

def collate_batch(batch, max_len=None):
    if max_len is None:
        max_len = max(len(t) for t in batch)
    max_len = min(max_len, MAX_SEQ)
    x_batch, y_batch = [], []
    for tokens in batch:
        tokens = tokens[:max_len]
        pad_len = max_len - len(tokens)
        x = tokens[:-1] + [PAD] * max(0, pad_len)
        y = tokens[1:] + [-100] * max(0, pad_len)
        x_batch.append(x[:max_len])
        y_batch.append(y[:max_len])
    return torch.tensor(x_batch).long(), torch.tensor(y_batch).long()

class Muon(torch.optim.Optimizer):
    """Sign-Descent with momentum, Newton-Schulz for 2D params."""
    def __init__(self, params, lr=1e-3, weight_decay=0.0, betas=(0.95, 0.95), eps=1e-8, ns_iter=3):
        defaults = dict(lr=lr, weight_decay=weight_decay, betas=betas, eps=eps, ns_iter=ns_iter)
        super().__init__(params, defaults)

    @torch.no_grad()
    def step(self):
        a, b, c = 3.4445, -4.7750, 2.0315
        for group in self.param_groups:
            lr, wd = group['lr'], group['weight_decay']
            b1, b2, eps = *group['betas'], group['eps']
            ns_n = group['ns_iter']
            for p in group['params']:
                if p.grad is None: continue
                g = p.grad
                if wd: g = g + wd * p
                state = self.state[p]
                if not state:
                    state.update(step=0, exp_avg=torch.zeros_like(g), exp_avg_sq=torch.zeros_like(g))
                state['step'] += 1
                state['exp_avg'].lerp_(g, 1 - b1)
                state['exp_avg_sq'].lerp_(g.square(), 1 - b2)
                m = state['exp_avg'] / (1 - b1 ** state['step'])
                v = state['exp_avg_sq'] / (1 - b2 ** state['step'])
                mu = m / (v.sqrt() + eps)
                if p.dim() >= 2 and mu.numel() > 1 and not torch.isnan(mu).any():
                    m_rows, m_cols = mu.shape
                    if m_rows <= m_cols:
                        a_mat = mu @ mu.T
                        for _ in range(ns_n):
                            a2 = a_mat @ a_mat; a3 = a2 @ a_mat
                            a_mat = a * a_mat + b * a2 + c * a3
                        if not torch.isnan(a_mat).any():
                            mu = a_mat @ mu
                    else:
                        a_mat = mu.T @ mu
                        for _ in range(ns_n):
                            a2 = a_mat @ a_mat; a3 = a2 @ a_mat
                            a_mat = a * a_mat + b * a2 + c * a3
                        if not torch.isnan(a_mat).any():
                            mu = mu @ a_mat
                mu = mu.clamp(-1.0, 1.0)
                p.add_(mu, alpha=-lr)

def train():
    parser = argparse.ArgumentParser(description='Train HW-aware transformer with Muon + Medusa')
    parser.add_argument('--dataset', default='hw_knowledge.jsonl')
    parser.add_argument('--output', default='micro.bitnet')
    parser.add_argument('--epochs', type=int, default=8)
    parser.add_argument('--lr', type=float, default=1e-3)
    parser.add_argument('--threshold', type=float, default=0.05)
    parser.add_argument('--batch-size', type=int, default=32)
    parser.add_argument('--max-examples', type=int, default=50000)
    parser.add_argument('--muon', action='store_true', default=False)
    parser.add_argument('--augment', action='store_true', default=True)
    parser.add_argument('--medusa-loss', type=float, default=0.3, help='Medusa head loss weight')
    args = parser.parse_args()

    print("[TRAIN] Carregando dataset...")
    examples, tokenizer = load_dataset(args.dataset, args.max_examples, augment=args.augment)
    print(f"[TRAIN] {len(examples)} exemplos totais")

    import os; os.environ['CUDA_VISIBLE_DEVICES'] = '0'
    device = torch.device('cuda' if torch.cuda.is_available() else 'cpu')
    model = TransformerModel().to(device)
    print(f"[TRAIN] Device: {device}")

    if args.muon:
        optimizer = Muon(model.parameters(), lr=args.lr)
        print("[TRAIN] Otimizador: Muon")
    else:
        optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr)
        print("[TRAIN] Otimizador: AdamW")

    for epoch in range(args.epochs):
        random.shuffle(examples)
        total_loss = 0; batches = 0
        for i in range(0, len(examples), args.batch_size):
            x, y = collate_batch(examples[i:i + args.batch_size])
            x, y = x.to(device), y.to(device)
            logits, hidden = model(x)
            loss = F.cross_entropy(logits.view(-1, VOCAB_SIZE), y.view(-1), ignore_index=-100)

            if args.medusa_loss > 0:
                B, T, _ = hidden.shape
                medusa_loss = 0.0
                for k, head in enumerate(model.medusa_heads):
                    offset = k + 1
                    if offset >= T: continue
                    head_logits = head(hidden[:, :-offset, :])
                    head_target = x[:, offset:]
                    medusa_loss += F.cross_entropy(
                        head_logits.reshape(-1, VOCAB_SIZE), head_target.reshape(-1), ignore_index=-100)
                loss = loss + args.medusa_loss * medusa_loss / MEDUSA_HEADS

            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 10.0)
            optimizer.step()
            optimizer.zero_grad(set_to_none=True)
            total_loss += loss.item(); batches += 1

        avg_loss = total_loss / batches
        print(f"[TRAIN] Epoch {epoch+1}/{args.epochs}: loss={avg_loss:.4f}")

        if (epoch + 1) % 5 == 0:
            for test in ["8086 1237", "o que e 10EC 8139", "class 0300", "mostre hardware"]:
                out = model.generate(tokenizer, test, speculative=True)
                print(f"  '{test}' -> '{out}'")

    print("[TRAIN] Exportando modelo com Medusa heads...")
    export_bitnet(model, tokenizer, args.output, threshold=args.threshold)
    print("[DONE] Modelo treinado e exportado!")

if __name__ == '__main__':
    train()
