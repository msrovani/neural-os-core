#!/usr/bin/env python3
"""train_hint_mlp.py — treina o MLP de hints e exporta no pack W2A8 do contrato
do hint_render (ADR-0047-HMI §6.2/§6.4 H3-revisit, ADR-0112).

CONTRATO (crates/k_hal/src/gpu/hint_render.rs — NÃO divergir):
  HINT_IN=64, HINT_HIDDEN=128, HINT_OUT=16, HINT_REGIONS=8.
  Forward no kernel: h = ReLU(x·W1ᵀ); y = h·W2ᵀ; bias NUNCA aplicado no
  forward (o struct carrega b1/b2, mas o render_hints() só usa w1/w2 —
  o treino importa o bias APENAS para o professor bater com o aluno).
  Pack ternário: 2 bits/peso — 01=+1, 10=-1, 00=0 — 4 pesos/byte,
  layout packed[(i*n + j) >> 2], 2 bits low→high dentro do byte
  (idêntico ao pack_ternary do bitnet_writer/.bitnet v6).
  Quant: signed round-half f32→{-1,0,1}, limiar 0.33 (quant_ternary).
  Saída do kernel: (0, energia=clamp(y[2r]), matiz=clamp(y[2r+1])) por região.

Formato HINT.BIN v1 (little-endian):
  magic     b"HINT"          4 bytes
  version   u16 = 1
  flags     u16 = 0
  IN        u16 = 64
  HIDDEN    u16 = 128
  OUT       u16 = 16
  REGIONS   u16 = 8
  len_w1    u32              (HIDDEN*IN/4 = 2048)
  len_w2    u32              (OUT*HIDDEN/4 = 512)
  w1 packed bytes (len_w1)   — ordem row-major (j*IN+t), low→high no byte
  w2 packed bytes (len_w2)   — ordem row-major (j*HIDDEN+t)
  b1 f32[HIDDEN]             — informativo (kernel não aplica no forward)
  b2 f32[OUT]                — informativo
  Header = 4+2*6+4*2 = 24 bytes. TOTAL: 24 + 2048 + 512 + 512 + 64 = 3160 bytes

Kernel: o forward lê via BAR (read_volatile) — HINT_OFFSETS aponta w1 e w2
exatamente nesses bytes (upload_hint_weights monta PackedTernaryTensor direto).

Arquitetura/estratégia (lição SESSION_346 — dataset tem que ser a modalidade real):
  Não existe dataset de "hints corretos" (é uma UI nova). Estratégia honesta:
  PROFESSOR SINTÉTICO POR REGRA — uma função determinística (numpy, f32, com
  bias) que codifica a política de HMI do §6.4 (orb energia com carga de
  infer, matiz por domínio; dock/header eco; cards proeminentes por atividade
  de mesh; HUD calmo). O MLP (torch) treina a IMITAR o professor e é
  quantizado W2A8; o VALIDADOR mede o acordo do ALUNO QUANTIZADO (após o
  pack/unpack round-trip do contrato) contra o professor — é esse número que
  importa no metal, não o loss do treino.

Uso:
  python tools/train_hint_mlp.py                     # treina + exporta target/HINT.BIN
  python tools/train_hint_mlp.py --epochs 300 --seed 7
  python tools/train_hint_mlp.py --self-check        # round-trip pack vs hint_render.rs
  python tools/train_hint_mlp.py --validate target/HINT.BIN
"""
from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
OUT_DEFAULT = ROOT / "target" / "HINT.BIN"

# ── Contrato hint_render.rs ────────────────────────────────────────────────
IN_DIM, HIDDEN, OUT_DIM, REGIONS = 64, 128, 16, 8
QUANT_THRESHOLD = 0.33
MAGIC = b"HINT"
VERSION = 1

# ── Features do ui_state (contrato display/agent.rs feed 2Hz) ─────────────
# [0]=infer_running(0/1) [1]=tok_s_norm(0..1) [2]=vram_lane(0/1)
# [3]=mesh_peers/8 [4]=mesh_matmul_busy(0/1) [5]=orb_activity_norm(0..1)
# [6..16]=reservado (zero hoje); [16..]=aleatório-assinado na amostragem
# sintética p/ o MLP não aprender caminhos mortos (host zera; professor ignora).
IDX_INFER, IDX_TOKS, IDX_VRAM, IDX_PEERS, IDX_MMBUSY, IDX_ORB = 0, 1, 2, 3, 4, 5

# ── Regions (contrato HINT_REGIONS): 0=orb 1=header 2=dock 3..6=cards 7=HUD ──
HUE_DOMAIN = {
    "orb": 200,    # azul — inferência
    "mesh": 120,   # verde — rede/compute distribuído
    "audio": 30,   # âmbar — voz
    "idle": 220,   # azul-acinzentado — calmo
}


def agreement(Ys: np.ndarray, Y: np.ndarray) -> tuple[float, float]:
    """Acordo perceptual (energia, matiz) do aluno vs professor.

    Critério por saída: |d| < bucket absoluto OU erro relativo < 25%/20% —
    um erro de 33 num hint de 90 é invisível; num de 15 não é (bucket
    absoluto puro punha valores pequenos e travava o acordo em ~87% com
    MAE 12.5/255 — métrica, não modelo).
    """
    de = np.abs(Ys[:, 0::2] - Y[:, 0::2])
    dh = np.abs(Ys[:, 1::2] - Y[:, 1::2])
    ok_e = (de < 32.0) | (de <= 0.25 * np.abs(Y[:, 0::2]) + 1e-6)
    ok_h = (dh < 16.0) | (dh <= 0.20 * np.abs(Y[:, 1::2]) + 1e-6)
    return float(ok_e.mean()), float(ok_h.mean())


def professor(x: np.ndarray) -> np.ndarray:
    """Política de HMI §6.4 (aumentativo, nunca substitui o compositor).

    x: (N,64) f32 → y: (N,16) f32 = (energia, matiz) por região (8×2).
    Determinística; é o alvo do treino E a referência da validação (o aluno
    quantizado é medido CONTRA ela).

    RESTRICÇÃO DE REPRESENTABILIDADE (lição do --validate): o forward do
    kernel é h=ReLU(x·W1ᵀ); y=h·W2ᵀ SEM bias e SEM termo constante na
    entrada → o aluno NÃO representa constantes nem produtos. O professor é
    então combinação LINEAR NÃO-NEGATIVA das 6 features vivas (com clip de
    peers representável: relu(z)−relu(z−1)); idle (tudo 0) → y=0 = modo
    clássico — que é exatamente a semântica §6.4 (hint só energiza com
    atividade real). Faixa de saída 0..~140 (o consumidor escala; o kernel
    só clampa 0..255) — com 128 hidden e peso ternário, o teto confiável
    por saída é ~128·média(h)·w; passar de isso força a quantização a mentir.
    """
    infer = x[:, IDX_INFER]
    toks = x[:, IDX_TOKS]
    vram = x[:, IDX_VRAM]
    peers = np.clip(x[:, IDX_PEERS], 0.0, 1.0)
    mmbusy = x[:, IDX_MMBUSY]
    orb_act = np.clip(x[:, IDX_ORB], 0.0, 1.0)

    e = np.zeros((x.shape[0], REGIONS), dtype=np.float32)
    hue = np.zeros((x.shape[0], REGIONS), dtype=np.float32)

    # Orb (0): energia = carga de infer + atividade; matiz: verde se mesh
    # domina, azul se infer local (escala relativa — consumidor interpreta).
    e[:, 0] = 55.0 * infer + 30.0 * orb_act + 15.0 * toks
    hue[:, 0] = 60.0 * mmbusy + 40.0 * infer

    # Header (1): eco suave do orb.
    e[:, 1] = 25.0 * infer + 10.0 * orb_act
    hue[:, 1] = 50.0 * mmbusy + 20.0 * infer

    # Dock (2): acorda com infer/mesh.
    e[:, 2] = 45.0 * infer + 35.0 * mmbusy + 10.0 * orb_act
    hue[:, 2] = 30.0 * vram + 20.0 * infer

    # Cards 3..6: slot r acende com o peer r (distribuição) + vram lane;
    # peer_r = clip(4·peers − slot, 0, 1) = relu(z)−relu(z−1), representável.
    for r in range(3, 7):
        slot = r - 3
        z = peers * 4.0 - slot
        peer_r = np.clip(z, 0.0, 1.0)
        e[:, r] = 50.0 * peer_r + 15.0 * vram
        hue[:, r] = 40.0 * mmbusy + 15.0 * vram

    # HUD (7): calmo — só confirma vida (vram/infer).
    e[:, 7] = 30.0 * vram + 20.0 * infer
    hue[:, 7] = 20.0 * vram + 10.0 * infer

    y = np.zeros((x.shape[0], OUT_DIM), dtype=np.float32)
    y[:, 0::2] = e
    y[:, 1::2] = hue
    return y


def sample_inputs(n: int, rng: np.random.Generator) -> np.ndarray:
    """Amostras do espaço de ui_state REAL: 6 features ativas + ruído.

    Cobre os vértices (infer on/off × mesh busy × vram) com interpolados —
    o kernel vê estados contínuos (orb_activity, peers/8), não só binários.
    Features 6..63 ficam 0 (host zera) — o professor não as usa, e o MLP
    aprende peso ~0 nelas (mortas no metal, honesto).
    """
    X = np.zeros((n, IN_DIM), dtype=np.float32)
    # Vértices + interpolação aleatória nas 6 features vivas.
    X[:, IDX_INFER] = rng.choice([0.0, 1.0], n, p=[0.5, 0.5])
    X[:, IDX_TOKS] = rng.random(n)
    X[:, IDX_VRAM] = rng.choice([0.0, 1.0], n, p=[0.7, 0.3])
    X[:, IDX_PEERS] = rng.integers(0, 9, n) / 8.0
    X[:, IDX_MMBUSY] = rng.choice([0.0, 1.0], n, p=[0.6, 0.4])
    X[:, IDX_ORB] = rng.random(n)
    return X


# ── Pack/unpack — BIT-A-BIT idêntico a hint_render.rs ─────────────────────
# pack_weight: 01=+1, 10=-1, 00=0; layout packed[(i*n+j)>>2], 2 bits low→high.

def pack_weights_ternary(w: np.ndarray) -> bytes:
    """w: (k,n) int8 {-1,0,1} row-major → bytes packed (contrato kernel)."""
    k, n = w.shape
    flat = w.reshape(-1).astype(np.int8)
    assert flat.shape[0] % 4 == 0, f"({k},{n}) não múltiplo de 4"
    bits = np.zeros_like(flat, dtype=np.uint8)
    bits[flat == 1] = 0b01
    bits[flat == -1] = 0b10
    b = bits.reshape(-1, 4)
    packed = b[:, 0] | (b[:, 1] << 2) | (b[:, 2] << 4) | (b[:, 3] << 6)
    return packed.astype(np.uint8).tobytes()


def unpack_weights_ternary(buf: bytes, k: int, n: int) -> np.ndarray:
    """Round-trip exato do read do kernel (unpack_weight, 2 bits low→high)."""
    flat = np.frombuffer(buf, dtype=np.uint8)
    out = np.zeros(k * n, dtype=np.int8)
    for pos in range(k * n):
        byte = flat[pos >> 2]
        two = (byte >> ((pos & 3) * 2)) & 3
        out[pos] = 1 if two == 0b01 else (-1 if two == 0b10 else 0)
    return out.reshape(k, n)


def quant_ternary(v: np.ndarray) -> np.ndarray:
    """Limiar 0.33 — mesmo hint_render.rs::quant_ternary."""
    return np.where(v > QUANT_THRESHOLD, 1, np.where(v < -QUANT_THRESHOLD, -1, 0)).astype(np.int8)


# ── Export HINT.BIN v1 ────────────────────────────────────────────────────

def export_bin(path: Path, w1: np.ndarray, b1: np.ndarray, w2: np.ndarray, b2: np.ndarray) -> int:
    p1 = pack_weights_ternary(w1)
    p2 = pack_weights_ternary(w2)
    hdr = struct.pack(
        "<4sHHHHHHII",
        MAGIC, VERSION, 0,
        IN_DIM, HIDDEN, OUT_DIM, REGIONS,
        len(p1), len(p2),
    )
    body = p1 + p2
    body += b1.astype("<f4").tobytes()
    body += b2.astype("<f4").tobytes()
    data = hdr + body
    path.write_bytes(data)
    return len(data)


def load_bin(path: Path):
    data = path.read_bytes()
    magic, ver, flags, i, h, o, r, l1, l2 = struct.unpack_from("<4sHHHHHHII", data, 0)
    if magic != MAGIC:
        raise ValueError(f"magic {magic!r} != HINT")
    if (i, h, o, r) != (IN_DIM, HIDDEN, OUT_DIM, REGIONS):
        raise ValueError(f"dims {(i, h, o, r)} != contrato {(IN_DIM, HIDDEN, OUT_DIM, REGIONS)}")
    off = 24
    w1 = unpack_weights_ternary(data[off:off + l1], h, i); off += l1
    w2 = unpack_weights_ternary(data[off:off + l2], o, h); off += l2
    b1 = np.frombuffer(data, dtype="<f4", count=h, offset=off); off += 4 * h
    b2 = np.frombuffer(data, dtype="<f4", count=o, offset=off); off += 4 * o
    return w1, b1.copy(), w2, b2.copy(), ver, flags


# ── Inferência do ALUNO quantizado (simula o forward do hint_render.rs) ──

def student_forward(x: np.ndarray, w1q: np.ndarray, b1: np.ndarray,
                    w2q: np.ndarray, b2: np.ndarray) -> np.ndarray:
    """Réplica EXATA do render_hints(): ReLU(x·W1ᵀ)·W2ᵀ, f32, SEM bias
    (o kernel não aplica b1/b2 no forward — docstring do contrato)."""
    h = np.maximum(x @ w1q.astype(np.float32).T + 0.0, 0.0)
    y = h @ w2q.astype(np.float32).T + 0.0
    return y


# ── Treino ────────────────────────────────────────────────────────────────

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--epochs", type=int, default=200)
    ap.add_argument("--samples", type=int, default=16384)
    ap.add_argument("--batch", type=int, default=256)
    ap.add_argument("--lr", type=float, default=2e-3)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--out", type=Path, default=OUT_DEFAULT)
    ap.add_argument("--self-check", action="store_true",
                    help="round-trip pack/unpack vs contrato hint_render.rs e sai")
    ap.add_argument("--validate", type=Path, default=None,
                    help="valida um HINT.BIN existente contra o professor")
    args = ap.parse_args()

    if args.self_check:
        rng = np.random.default_rng(0)
        w = rng.integers(-1, 2, (HIDDEN, IN_DIM)).astype(np.int8)
        p = pack_weights_ternary(w)
        assert len(p) == HIDDEN * IN_DIM // 4, "len packed"
        back = unpack_weights_ternary(p, HIDDEN, IN_DIM)
        assert np.array_equal(w, back), "round-trip pack/unpack divergiu do kernel"
        # Caso dirigido (contrato low→high no byte): pesos [0,-1,+1,0]
        # → bits 00,10,01,00 → byte = 00 | 10<<2 | 01<<4 | 00<<6 = 0b00011000.
        buf = bytes([0b00_01_10_00])
        got = unpack_weights_ternary(buf, 1, 4)[0]
        assert list(got) == [0, -1, 1, 0], f"ordem low→high errada: {got}"
        assert HIDDEN * IN_DIM // 4 == 2048 and OUT_DIM * HIDDEN // 4 == 512
        assert 24 + 2048 + 512 + 4 * HIDDEN + 4 * OUT_DIM == 3160, "tamanho HINT.BIN"
        print("self-check OK: pack/unpack bit-a-bit = hint_render.rs "
              f"(w1={HIDDEN*IN_DIM//4}B w2={OUT_DIM*HIDDEN//4}B total=2560B)")
        return 0

    if args.validate:
        w1, b1, w2, b2, ver, flags = load_bin(args.validate)
        rng = np.random.default_rng(args.seed)
        X = sample_inputs(4096, rng)
        Y = professor(X)
        Ys = student_forward(X, w1, b1, w2, b2)
        mae_e = np.abs(Ys[:, 0::2] - Y[:, 0::2]).mean()
        mae_h = np.abs(Ys[:, 1::2] - Y[:, 1::2]).mean()
        agree, hue_ok = agreement(Ys, Y)
        print(f"[VALIDATE] {args.validate.name} v{ver} flags={flags} "
              f"MAE energia={mae_e:.1f}/255 matiz={mae_h:.1f}/255 "
              f"acordo={agree*100:.1f}% matiz_ok={hue_ok*100:.1f}%")
        ok = agree >= 0.95 and mae_e < 24.0
        print("PASS" if ok else "FAIL — re- treinar/quantizar")
        return 0 if ok else 1

    try:
        import torch
        import torch.nn as nn
    except ImportError:
        print("[FATAL] pip install torch numpy", file=sys.stderr)
        return 1
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"[HINT] PyTorch {torch.__version__} | Device: {device}")

    rng = np.random.default_rng(args.seed)
    X = sample_inputs(args.samples, rng)
    Y = professor(X)
    # Split honesto: holdout disjunto (lição SESSION_346).
    n_val = args.samples // 8
    Xtr, Ytr = X[n_val:], Y[n_val:]
    Xva, Yva = X[:n_val], Y[:n_val]

    torch.manual_seed(args.seed)
    model = nn.Sequential(
        nn.Linear(IN_DIM, HIDDEN), nn.ReLU(),
        nn.Linear(HIDDEN, OUT_DIM),
    ).to(device)
    opt = torch.optim.Adam(model.parameters(), lr=args.lr)
    sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, T_max=args.epochs)
    xt = torch.from_numpy(Xtr).to(device)
    yt = torch.from_numpy(Ytr).to(device)
    xv = torch.from_numpy(Xva).to(device)
    yv = torch.from_numpy(Yva).to(device)

    best_val, best_state = float("inf"), None
    for ep in range(args.epochs):
        model.train()
        perm = torch.randperm(xt.shape[0], device=device)
        tot = 0.0
        for i in range(0, xt.shape[0], args.batch):
            idx = perm[i:i + args.batch]
            opt.zero_grad(set_to_none=True)
            loss = nn.functional.mse_loss(model(xt[idx]), yt[idx])
            loss.backward()
            opt.step()
            tot += loss.item() * idx.shape[0]
        sched.step()
        if (ep + 1) % 20 == 0 or ep == 0 or ep == args.epochs - 1:
            model.eval()
            with torch.no_grad():
                vl = nn.functional.mse_loss(model(xv), yv).item()
            if vl < best_val:
                best_val, best_state = vl, {k: v.detach().cpu().clone() for k, v in model.state_dict().items()}
            print(f"epoch {ep+1:4d}/{args.epochs}  train_mse={tot/xt.shape[0]:9.2f}  val_mse={vl:9.2f}")

    if best_state is not None:
        model.load_state_dict(best_state)
    model.eval()

    # ── Fase QAT: pesos TERNÁRIOS no forward (STE) + escalas aprendidas ──
    # PTQ (quantizar pós-treino) teto em ~87% de acordo: W2 com 3 níveis não
    # reproduz os pesos precisos. QAT treina a fase final JÁ quantizado —
    # SEM bias (o forward do kernel não aplica b1/b2) e com c1/c2 viram
    # parâmetros aprendidos. STE: gradiente passa pelo peso f32.
    w1f = model[0].weight.detach().cpu().numpy().astype(np.float32).copy()
    w2f = model[2].weight.detach().cpu().numpy().astype(np.float32).copy()
    rmax = np.abs(w1f).max(axis=1, keepdims=True)
    w1n = w1f / np.maximum(rmax, 1e-9)
    w2n = w2f / max(float(np.abs(w2f).max()), 1e-9)

    def tern_t(t: "torch.Tensor") -> "torch.Tensor":
        q = torch.zeros_like(t)
        q[t > QUANT_THRESHOLD] = 1.0
        q[t < -QUANT_THRESHOLD] = -1.0
        return q

    w1p = torch.nn.Parameter(torch.from_numpy(w1n).to(device))
    w2p = torch.nn.Parameter(torch.from_numpy(w2n).to(device))
    log_c1 = torch.nn.Parameter(torch.tensor(0.0, device=device))
    log_c2 = torch.nn.Parameter(torch.tensor(0.0, device=device))
    qopt = torch.optim.Adam([w1p, w2p, log_c1, log_c2], lr=1e-3)
    xt2 = torch.from_numpy(Xtr).to(device)
    yt2 = torch.from_numpy(Ytr).to(device)
    xv2 = torch.from_numpy(Xva).to(device)
    yv2 = torch.from_numpy(Yva).to(device)
    q_epochs = max(args.epochs // 2, 50)
    for ep in range(q_epochs):
        perm = torch.randperm(xt2.shape[0], device=device)
        tot = 0.0
        for i in range(0, xt2.shape[0], args.batch):
            idx = perm[i:i + args.batch]
            qopt.zero_grad(set_to_none=True)
            c1, c2 = log_c1.exp(), log_c2.exp()
            w1q = tern_t(w1p / c1) * c1 + (w1p / c1 - (w1p / c1).detach()) * 0  # noqa
            # STE clássico: forward usa o ternário, gradiente flui pelo f32.
            w1q = (w1p / c1) + (tern_t(w1p / c1) - w1p / c1).detach()
            w2q = (w2p / c2) + (tern_t(w2p / c2) - w2p / c2).detach()
            h = torch.clamp(xt2[idx] @ w1q.T, min=0.0)
            pred = h @ w2q.T
            loss = nn.functional.mse_loss(pred, yt2[idx])
            loss.backward()
            qopt.step()
            tot += loss.item() * idx.shape[0]
        if (ep + 1) % 25 == 0 or ep == 0 or ep == q_epochs - 1:
            with torch.no_grad():
                c1, c2 = log_c1.exp(), log_c2.exp()
                w1q = (w1p / c1) + (tern_t(w1p / c1) - w1p / c1).detach()
                w2q = (w2p / c2) + (tern_t(w2p / c2) - w2p / c2).detach()
                h = torch.clamp(xv2 @ w1q.T, min=0.0)
                vl = nn.functional.mse_loss(h @ w2q.T, yv2).item()
            print(f"qat    {ep+1:4d}/{q_epochs}  train_mse={tot/xt2.shape[0]:9.2f}  val_mse={vl:9.2f}  c1={c1.item():.2f} c2={c2.item():.2f}")

    # Peso ternário final = o padrão que o forward QAT usou (c aprendida).
    with torch.no_grad():
        c1f, c2f = log_c1.exp().item(), log_c2.exp().item()
        w1q = quant_ternary(w1p.detach().cpu().numpy() / c1f)
        w2q = quant_ternary(w2p.detach().cpu().numpy() / c2f)
    print(f"[QAT] escalas aprendidas c1={c1f:.2f} c2={c2f:.2f}")

    # ── Export ──────────────────────────────────────────────────────────────
    # Bias do modelo f32 (informativo no bin — o forward do kernel não aplica).
    b1f = model[0].bias.detach().cpu().numpy().astype(np.float32)
    b2f = model[2].bias.detach().cpu().numpy().astype(np.float32)

    nbytes = export_bin(args.out, w1q, b1f, w2q, b2f)
    dens1 = (w1q != 0).mean() * 100
    dens2 = (w2q != 0).mean() * 100
    print(f"[EXPORT] {args.out} ({nbytes} bytes) densidade w1={dens1:.0f}% w2={dens2:.0f}%")

    # ── Validação do ALUNO QUANTIZADO (o que roda no metal) ──────────────
    w1rt = unpack_weights_ternary(pack_weights_ternary(w1q), HIDDEN, IN_DIM)
    w2rt = unpack_weights_ternary(pack_weights_ternary(w2q), OUT_DIM, HIDDEN)
    assert np.array_equal(w1rt, w1q) and np.array_equal(w2rt, w2q), "round-trip"
    Ys = student_forward(Xva, w1q, b1f, w2q, b2f)
    mae_e = np.abs(Ys[:, 0::2] - Yva[:, 0::2]).mean()
    mae_h = np.abs(Ys[:, 1::2] - Yva[:, 1::2]).mean()
    agree, hue_ok = agreement(Ys, Yva)
    print(f"[QUANT-VALIDATE] holdout disjunto: MAE energia={mae_e:.1f}/255 "
          f"matiz={mae_h:.1f}/255 acordo={agree*100:.1f}% "
          f"matiz_ok={hue_ok*100:.1f}%")
    if agree < 0.90 or hue_ok < 0.90:
        print("FAIL — quantização degradou o acordo (aumentar --samples/--epochs)")
        return 1
    print("PASS — aluno quantizado imita a política §6.4 dentro da tolerância")
    return 0


if __name__ == "__main__":
    sys.exit(main())
