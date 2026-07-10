#!/usr/bin/env python3
"""train_wakeword_mlp.py — neural-os-core v1.0
Treina MLP (16→8→1) para deteccao de wake word "jarvis".
Gera pesos exportaveis para o kernel (wakeword.rs).

Uso: python tools/train_wakeword_mlp.py [--epochs 2000]
"""

import os, sys, math, random, struct, json
from pathlib import Path

TARGET = Path(__file__).parent.parent / "target"
TARGET.mkdir(exist_ok=True)

import torch
import torch.nn as nn
import torch.nn.functional as F
import torch.optim as optim

DEVICE = torch.device("cuda" if torch.cuda.is_available() else "cpu")
print(f"[TRAIN] Device: {DEVICE}")

# ─── Geracao de dados sinteticos ───────────────────────────────────────

def generate_wakeword_pattern(word="jarvis", energy_len=16):
    """Gera padrao de energia para wake word de 2 silabas.
    'jar-vis': picos mais proximos (3-6 samples), acento na 1a silaba.
    'jar-bas': picos mais distantes (4-7 samples), acento mais forte na 1a."""
    pattern = [random.uniform(0.01, 0.05) for _ in range(energy_len)]
    first_amp = random.uniform(0.6, 1.0)
    first_pos = random.randint(1, 5)
    pattern[first_pos] = first_amp
    pattern[first_pos + 1] = first_amp * random.uniform(0.5, 0.8)

    gap = (4, 7) if "bas" in word else (3, 6)  # jarbas tem silabas mais separadas
    second_amp = random.uniform(0.5, 0.9)
    second_pos = first_pos + random.randint(*gap)
    if second_pos < energy_len - 1:
        pattern[second_pos] = second_amp
        pattern[second_pos + 1] = second_amp * random.uniform(0.5, 0.8)

    for i in range(energy_len):
        pattern[i] += random.uniform(-0.03, 0.03)
        pattern[i] = max(0.0, min(1.0, pattern[i]))
    return pattern

def generate_non_wakeword(energy_len=16):
    """Gera padrao sem wake word (ruido, fala continua, toque unico)."""
    pattern_type = random.choice(["noise", "continuous", "single_peak", "multi_peak", "silence"])

    if pattern_type == "noise":
        return [random.uniform(0.0, 0.15) for _ in range(energy_len)]
    elif pattern_type == "continuous":
        base = random.uniform(0.3, 0.7)
        return [base + random.uniform(-0.1, 0.1) for _ in range(energy_len)]
    elif pattern_type == "single_peak":
        pattern = [random.uniform(0.01, 0.05) for _ in range(energy_len)]
        pos = random.randint(0, energy_len - 1)
        pattern[pos] = random.uniform(0.7, 1.0)
        return pattern
    elif pattern_type == "multi_peak":
        pattern = [random.uniform(0.01, 0.05) for _ in range(energy_len)]
        for _ in range(random.randint(3, 6)):
            if random.random() < 0.3:
                pos = random.randint(0, energy_len - 1)
                pattern[pos] = random.uniform(0.4, 0.9)
        return pattern
    else:  # silence
        return [random.uniform(0.0, 0.02) for _ in range(energy_len)]

def generate_dataset(n_pos=4000, n_neg=8000):
    """Gera dataset balanceado: 2000 jarvis + 2000 jarbas + 8000 negativos."""
    data = []
    for _ in range(n_pos // 2):
        data.append((generate_wakeword_pattern("jarvis"), 1.0))
        data.append((generate_wakeword_pattern("jarbas"), 1.0))
    for _ in range(n_neg):
        data.append((generate_non_wakeword(), 0.0))
    random.shuffle(data)
    return data

# ─── MLP Model ──────────────────────────────────────────────────────────

class WakeWordMLP(nn.Module):
    """MLP 16→8→1, mesma arquitetura do kernel."""
    def __init__(self):
        super().__init__()
        self.fc1 = nn.Linear(16, 8)
        self.fc2 = nn.Linear(8, 1)

    def forward(self, x):
        h = torch.relu(self.fc1(x))
        return torch.sigmoid(self.fc2(h))

    def export_weights(self):
        """Exporta pesos como arrays i8 para o kernel (ternarizado)."""
        w1 = self.fc1.weight.data.cpu().numpy()  # (8, 16)
        b1 = self.fc1.bias.data.cpu().numpy()     # (8,)
        w2 = self.fc2.weight.data.cpu().numpy()  # (1, 8)
        b2 = self.fc2.bias.data.cpu().numpy()     # (1,)

        # Ternariza: threshold 0.1
        def ternary(arr):
            return [[1 if v > 0.1 else (-1 if v < -0.1 else 0) for v in row] for row in arr]

        result = {
            "w1": ternary(w1.tolist()),
            "b1": [round(v, 4) for v in b1.flatten()],
            "w2": ternary(w2.tolist())[0],
            "b2": [round(v, 4) for v in b2.flatten()],
        }
        return result

    def export_kernel_code(self, path):
        """Gera codigo Rust para wakeword.rs com os pesos treinados."""
        w = self.export_weights()

        code = f"""// Pesos treinados do MLP wakeword — gerado por train_wakeword_mlp.py
// Nao editar manualmente.

impl WakeWordML {{
    pub fn new() -> Self {{
        let w1: [[i8; 16]; 8] = {json.dumps(w['w1']).replace('[', '{').replace(']', '}')};
        let b1: [f32; 8] = {json.dumps(w['b1'])};
        let w2: [i8; 8] = {json.dumps(w['w2']).replace('[', '{').replace(']', '}')};
        let b2: [f32; 1] = {json.dumps(w['b2'])};
        WakeWordML {{ w1, b1, w2, b2 }}
    }}

    pub fn predict(&self, energy: &[f32; 16]) -> f32 {{
        // Hidden layer: 8 neurons, ReLU
        let mut h = [0.0f32; 8];
        for i in 0..8 {{
            let mut s = self.b1[i];
            for j in 0..16 {{
                s += match self.w1[i][j] {{
                    1 => energy[j],
                    -1 => -energy[j],
                    _ => 0.0,
                }};
            }}
            h[i] = if s > 0.0 {{ s }} else {{ 0.0 }}; // ReLU
        }}
        // Output: sigmoid
        let mut out = self.b2[0];
        for i in 0..8 {{
            out += match self.w2[i] {{
                1 => h[i],
                -1 => -h[i],
                _ => 0.0,
            }};
        }}
        1.0 / (1.0 + (-out).exp()) // sigmoid
    }}
}}
"""
        with open(path, "w") as f:
            f.write(code)
        print(f"  [OK] Codigo Rust exportado: {path}")

def train(epochs=2000, batch=64):
    print(f"[TRAIN] Gerando dataset...")
    dataset = generate_dataset(2000, 8000)
    X = torch.tensor([[v for v in pattern] for pattern, _ in dataset], dtype=torch.float32)
    y = torch.tensor([[label] for _, label in dataset], dtype=torch.float32)
    print(f"  Positivos: {sum(1 for _, l in dataset if l > 0.5)} | Negativos: {sum(1 for _, l in dataset if l < 0.5)}")
    print(f"  Total: {len(dataset)}")

    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(X, y), batch_size=batch, shuffle=True)

    model = WakeWordMLP().to(DEVICE)
    opt = optim.Adam(model.parameters(), lr=0.01)
    best_loss = float("inf")

    for ep in range(epochs):
        model.train()
        total_loss = 0.0
        for xb, yb in loader:
            xb, yb = xb.to(DEVICE), yb.to(DEVICE)
            opt.zero_grad()
            loss = F.binary_cross_entropy(model(xb), yb)
            loss.backward()
            opt.step()
            total_loss += loss.item()

        avg_loss = total_loss / len(loader)
        if avg_loss < best_loss:
            best_loss = avg_loss
            torch.save(model.state_dict(), TARGET / "wakeword_mlp.pt")

        if (ep + 1) % 200 == 0 or ep == 0:
            # Validacao
            model.eval()
            with torch.no_grad():
                pred = model(X.to(DEVICE))
                acc = ((pred > 0.5) == (y.to(DEVICE) > 0.5)).float().mean().item()
            print(f"  Ep {ep+1:4d}/{epochs} | loss={avg_loss:.6f} | acc={acc*100:.1f}% | best={best_loss:.6f}")

    # Export
    model.eval()
    with torch.no_grad():
        pred = model(X.to(DEVICE))
        acc = ((pred > 0.5) == (y.to(DEVICE) > 0.5)).float().mean().item()
        tp = ((pred > 0.5) & (y.to(DEVICE) > 0.5)).float().sum().item()
        fp = ((pred > 0.5) & (y.to(DEVICE) < 0.5)).float().sum().item()
        fn = ((pred < 0.5) & (y.to(DEVICE) > 0.5)).float().sum().item()
        print(f"\nResultado final:")
        print(f"  Acurácia: {acc*100:.1f}%")
        print(f"  TP: {int(tp)} FP: {int(fp)} FN: {int(fn)}")

    model.export_kernel_code(TARGET / "wakeword_weights.rs")
    print(f"  Modelo salvo: {TARGET / 'wakeword_mlp.pt'}")
    return model

if __name__ == "__main__":
    train(epochs=2000, batch=64)
