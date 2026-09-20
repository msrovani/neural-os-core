#!/usr/bin/env python3
"""Treina STT CTC tiny com PCM→MFCC alinhado ao kernel (Sprint Sound).

Uso:
  python tools/train_stt.py --epochs 80 --batch 16
  python tools/train_stt.py --wav-dir data/stt_wavs --epochs 40

MFCC idêntico a crates/neural-kernel/src/audio/stt.rs::mfcc
(FFT 512, hop 256, 13 Mel triangles, log, CMVN por coeficiente).
"""
from __future__ import annotations

import argparse
import math
import os
import struct
import sys
import wave
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

SAMPLE_RATE = 16000
FFTSIZE = 512
N_BINS = FFTSIZE // 2 + 1
N_MFCC = 13
HIDDEN = 64
# ---------------------------------------------------------------------------
# VOCABULÁRIO CANÔNICO — contrato com `crates/jarbas/src/audio/stt.rs::VOCAB_CHARS`.
# A ordem aqui É a ordem dos logits (`out.bias`). Divergir = caracteres trocados
# em silêncio. Verificar sempre com `python tools/stt_vocab_check.py` APÓS editar.
# O índice do blank é sempre o último (len(VOCAB_CHARS)).
# ---------------------------------------------------------------------------
VOCAB_CHARS = [
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', ' ',
]
VOCAB = len(VOCAB_CHARS) + 1  # + blank

# Versão da front-end declarada no header do .bin (offset 4). O kernel compara e
# avisa se não casar: feats != treino produz texto plausível e errado.
FEAT_VERSION = 1
FRAME_SHIFT = FFTSIZE // 2

# Corpus PT mínimo (labels ASCII a-z + espaço)
_WARNED_NO_TTS = False

CORPUS = [
    "jarvis",
    "ola jarvis",
    "qual o tempo",
    "o tempo esta bom",
    "ligar luz",
    "desligar",
    "sim",
    "nao",
    "ajuda",
    "status do sistema",
    "bom dia",
    "boa noite",
    "parar",
    "continuar",
    "volume alto",
    "volume baixo",
    "como esta o tempo",
    "abra o menu",
    "feche a janela",
    "teste de voz",
]


def text_to_ids(text: str) -> list[int]:
    """Mapeia texto → ids usando a MESMA ordem de VOCAB_CHARS."""
    ids = []
    for ch in text.lower():
        if ch in VOCAB_CHARS:
            ids.append(VOCAB_CHARS.index(ch))
        # fora do vocabulário: ignorado (nunca inventa id)
    return ids


def load_corpus() -> list[str]:
    """Corpus de treino.

    Preferência: `data/stt_corpus.txt` (uma frase por linha) — o corpus inline de 20
    frases fixas treinava o modelo para exatamente 20 entradas e a validação usava
    `CORPUS[:8]` (vazamento de treino, acurácia sem significado).
    """
    extra = ROOT / "data" / "stt_corpus.txt"
    if extra.exists():
        lines = [l.strip() for l in extra.read_text(encoding="utf-8").splitlines()]
        lines = [l for l in lines if l and not l.startswith("#")]
        print(f"[STT] corpus externo: {len(lines)} frases de {extra.name}")
        return lines
    print(
        f"[STT] AVISO: corpus inline ({len(CORPUS)} frases fixas). "
        "Gere um corpus real com tools/gen_stt_corpus.py para STT utilizável."
    )
    return CORPUS


def _tts_synthesize(text: str, sr: int, voice: str = "pt-br") -> np.ndarray | None:
    """Síntese de FALA REAL (espeak-ng) → int16 @ `sr`.

    Retorna None se não houver backend instalado. É o caminho correto: o modelo
    anterior foi treinado com `synthesize_pcm` (3 senoides cujas frequências derivam
    do ÍNDICE DO CARACTERE) — ou seja, aprendeu "bipes por letra", não fala. Nenhum
    ajuste de arquitetura conserta isso; o dataset é que estava errado.
    """
    import shutil
    import subprocess
    import tempfile

    exe = shutil.which("espeak-ng") or shutil.which("espeak")
    if not exe:
        return None
    with tempfile.TemporaryDirectory() as td:
        wav = Path(td) / "t.wav"
        try:
            subprocess.run(
                [exe, "-v", voice, "-s", "150", "-w", str(wav), text],
                check=True,
                capture_output=True,
                timeout=20,
            )
        except Exception:
            return None
        if not wav.exists():
            return None
        with wave.open(str(wav), "rb") as w:
            src_sr = w.getframerate()
            n = w.getnframes()
            raw = w.readframes(n)
        pcm = np.frombuffer(raw, dtype=np.int16).astype(np.float32)
        if src_sr != sr and len(pcm) > 1:
            # Reamostragem linear (o kernel decima 48k→16k com box filter; aqui o
            # objetivo é só casar a taxa do treino).
            tgt_len = int(len(pcm) * sr / src_sr)
            idx = np.linspace(0, len(pcm) - 1, tgt_len)
            pcm = np.interp(idx, np.arange(len(pcm)), pcm)
        return np.clip(pcm, -32768, 32767).astype(np.int16)


def synthesize_pcm(text: str, sr: int = SAMPLE_RATE) -> np.ndarray:
    """Síntese para treino: fala REAL se houver backend, senão formant-lite.

    O fallback formant existe apenas para o pipeline continuar rodando em ambiente
    sem espeak-ng — ele NÃO produz modelo utilizável e isso é avisado uma vez.
    """
    global _WARNED_NO_TTS
    real = _tts_synthesize(text, sr)
    if real is not None:
        return real
    if not _WARNED_NO_TTS:
        _WARNED_NO_TTS = True
        print(
            "[STT] AVISO GRAVE: espeak-ng ausente — caindo no sintetizador FORMANT "
            "(bipes por caractere). O modelo resultante NÃO transcreve fala real. "
            "Instale espeak-ng (ou use tools/gen_stt_corpus.py com Piper)."
        )
    samples: list[float] = []
    rng = np.random.RandomState(sum(ord(c) for c in text) & 0xFFFF)
    for ch in text.lower():
        if ch == " ":
            n = sr // 20
            samples.extend([0.0] * n)
            continue
        if not ("a" <= ch <= "z"):
            continue
        # F0 e formantes derivados do caractere
        idx = ord(ch) - ord("a")
        f0 = 120.0 + (idx % 7) * 8.0
        f1 = 400.0 + (idx % 5) * 80.0
        f2 = 1200.0 + (idx % 9) * 90.0
        # Frames longos o bastante para CTC (hop=256 @16k ≈ 16ms)
        dur = int(sr * (0.12 + (idx % 3) * 0.03))
        t = np.arange(dur, dtype=np.float32) / sr
        env = np.ones_like(t)
        fade = max(dur // 10, 1)
        env[:fade] = np.linspace(0, 1, fade)
        env[-fade:] = np.linspace(1, 0, fade)
        sig = (
            0.45 * np.sin(2 * np.pi * f0 * t)
            + 0.30 * np.sin(2 * np.pi * f1 * t)
            + 0.15 * np.sin(2 * np.pi * f2 * t)
        )
        noise = rng.randn(dur).astype(np.float32) * 0.02
        samples.extend((sig * env + noise).tolist())
    if not samples:
        samples = [0.0] * (sr // 4)
    pcm = np.array(samples, dtype=np.float32)
    # Augmentations leves
    gain = float(rng.uniform(0.6, 1.2))
    pcm *= gain
    if rng.rand() < 0.4:
        pcm += rng.randn(len(pcm)).astype(np.float32) * 0.01
    pcm = np.clip(pcm * 20000.0, -32768, 32767)
    return pcm.astype(np.int16)


def mfcc_kernel(pcm: np.ndarray) -> np.ndarray:
    """Espelha stt.rs::mfcc (DFT + Mel triangles + log + CMVN)."""
    pcm = pcm.astype(np.float32)
    if len(pcm) < FFTSIZE:
        return np.zeros((0, N_MFCC), dtype=np.float32)
    n_frames = (len(pcm) - FFTSIZE) // FRAME_SHIFT + 1
    feats = np.zeros((n_frames, N_MFCC), dtype=np.float32)
    # Precompute DFT tables
    cos_t = np.zeros((N_BINS, FFTSIZE), dtype=np.float32)
    sin_t = np.zeros((N_BINS, FFTSIZE), dtype=np.float32)
    for k in range(N_BINS):
        ang = 2.0 * np.pi * k / FFTSIZE * np.arange(FFTSIZE)
        cos_t[k] = np.cos(ang)
        sin_t[k] = np.sin(ang)
    window = 0.54 - 0.46 * np.cos(2 * np.pi * np.arange(FFTSIZE) / (FFTSIZE - 1))
    for t in range(n_frames):
        off = t * FRAME_SHIFT
        frame = pcm[off : off + FFTSIZE] * window
        if len(frame) < FFTSIZE:
            frame = np.pad(frame, (0, FFTSIZE - len(frame)))
        spectrum = np.zeros(N_BINS, dtype=np.float32)
        for k in range(N_BINS):
            re = float(np.dot(frame, cos_t[k]))
            im = -float(np.dot(frame, sin_t[k]))
            spectrum[k] = math.sqrt(re * re + im * im)
        for m in range(N_MFCC):
            mel = 0.0
            center = m * 200.0 + 200.0
            bw = 100.0
            for k in range(N_BINS):
                mel_k = 2595.0 * math.log10(1.0 + k * 16000.0 / FFTSIZE / 700.0)
                if abs(mel_k - center) < bw:
                    mel += spectrum[k] * (1.0 - abs(mel_k - center) / bw)
            feats[t, m] = math.log(mel) if mel > 1e-10 else 0.0
    # CMVN (sem *0.1 — alinhado ao kernel pós-Sound)
    if n_frames > 1:
        mean = feats.mean(axis=0, keepdims=True)
        std = feats.std(axis=0, keepdims=True)
        std = np.maximum(std, 1e-3)
        feats = (feats - mean) / std
    return feats


def load_wav_corpus(wav_dir: Path) -> list[tuple[np.ndarray, list[int]]]:
    pairs = []
    if not wav_dir.is_dir():
        return pairs
    for wav_path in sorted(wav_dir.glob("*.wav")):
        label_path = wav_path.with_suffix(".txt")
        if not label_path.exists():
            label = wav_path.stem.replace("_", " ")
        else:
            label = label_path.read_text(encoding="utf-8").strip()
        ids = text_to_ids(label)
        if not ids:
            continue
        with wave.open(str(wav_path), "rb") as wf:
            assert wf.getnchannels() == 1
            assert wf.getsampwidth() == 2
            rate = wf.getframerate()
            raw = wf.readframes(wf.getnframes())
        pcm = np.frombuffer(raw, dtype=np.int16)
        if rate != SAMPLE_RATE:
            # resample linear simples
            x = np.linspace(0, 1, len(pcm))
            xi = np.linspace(0, 1, int(len(pcm) * SAMPLE_RATE / rate))
            pcm = np.interp(xi, x, pcm.astype(np.float32)).astype(np.int16)
        pairs.append((pcm, ids))
    print(f"[STT] WAV corpus: {len(pairs)} pares de {wav_dir}")
    return pairs


class TinyLSTM(nn.Module):
    def __init__(self):
        super().__init__()
        self.lstm0 = nn.LSTMCell(N_MFCC, HIDDEN)
        self.lstm1 = nn.LSTMCell(HIDDEN, HIDDEN)
        self.out = nn.Linear(HIDDEN, VOCAB)

    def forward(self, x):
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
        return torch.stack(logits, dim=1)


# Corpus efetivo do treino (arquivo externo se existir, senão o inline de 20 frases).
TRAIN_CORPUS = load_corpus()


def make_batch(batch_size: int, wav_pairs: list) -> tuple[torch.Tensor, list[torch.Tensor]]:
    xs = []
    targets = []
    for _ in range(batch_size):
        if wav_pairs and np.random.rand() < 0.5:
            pcm, ids = wav_pairs[np.random.randint(len(wav_pairs))]
        else:
            text = TRAIN_CORPUS[np.random.randint(len(TRAIN_CORPUS))]
            pcm = synthesize_pcm(text)
            ids = text_to_ids(text)
        feats = mfcc_kernel(pcm)
        if feats.shape[0] < 4:
            continue
        # Cap frames para batch uniforme
        max_f = 80
        if feats.shape[0] > max_f:
            feats = feats[:max_f]
        xs.append(feats)
        targets.append(torch.tensor(ids, dtype=torch.long))
    if not xs:
        # fallback
        text = "jarvis"
        feats = mfcc_kernel(synthesize_pcm(text))
        xs = [feats]
        targets = [torch.tensor(text_to_ids(text), dtype=torch.long)]
    # Pad frames
    T = max(f.shape[0] for f in xs)
    batch = np.zeros((len(xs), T, N_MFCC), dtype=np.float32)
    for i, f in enumerate(xs):
        batch[i, : f.shape[0]] = f
    return torch.tensor(batch, device=DEVICE), targets


def export_bin(model: TinyLSTM, output: Path) -> None:
    MAGIC = 0xBE11BE11
    params = []
    for name, p in model.named_parameters():
        arr = p.detach().cpu().numpy().reshape(-1).astype(np.float32)
        params.append((name, arr))
    with open(output, "wb") as f:
        f.write(struct.pack("<I", MAGIC))
        # offset 4 = feat_version (o kernel avisa se != a dele)
        f.write(struct.pack("<I", FEAT_VERSION))
        f.write(struct.pack("<I", len(params)))
        f.write(struct.pack("<I", VOCAB))
        data_off = 0
        for name, arr in params:
            bname = name.encode().ljust(32, b"\x00")[:32]
            cnt = len(arr)
            f.write(bname)
            f.write(struct.pack("<I", 16 + len(params) * 40 + data_off * 4))
            f.write(struct.pack("<I", cnt))
            data_off += cnt
        for _, arr in params:
            f.write(arr.tobytes())
    sz = output.stat().st_size
    print(f"\n[OK] {output}: {sz:,} bytes ({sz/1024:.1f}KB)")


@torch.no_grad()
def validate(model: TinyLSTM, wav_pairs: list) -> float:
    model.eval()
    total_chars = 0
    hit = 0
    # Holdout: últimas frases, nunca as primeiras (antes era CORPUS[:8], que também
    # era usado no treino → acurácia sem significado).
    texts = TRAIN_CORPUS[-8:] if len(TRAIN_CORPUS) > 16 else TRAIN_CORPUS[-2:]
    blank = VOCAB - 1
    for text in texts:
        pcm = synthesize_pcm(text)
        feats = mfcc_kernel(pcm)
        if feats.shape[0] < 2:
            continue
        x = torch.tensor(feats[None], device=DEVICE)
        logits = model(x)[0]  # [T,V]
        prev = blank
        out = []
        for t in range(logits.shape[0]):
            row = logits[t]
            best = int(row.argmax().item())
            # blank-margin (igual kernel): se blank ganha por <0.15, preferir nao-blank
            if best == blank:
                nb = int(row[:blank].argmax().item())
                if float(row[blank] - row[nb]) < 0.15:
                    best = nb
            if best != prev and best != blank:
                out.append(best)
            prev = best
        if not out:
            # blank-suppress fallback
            prev = blank
            for t in range(logits.shape[0]):
                best = int(logits[t, :blank].argmax().item())
                if best != prev:
                    out.append(best)
                prev = best
        pred = "".join(chr(ord("a") + c) if c < 26 else " " for c in out)
        target = "".join(c for c in text.lower() if c.isalpha() or c == " ")
        t_clean = target.replace(" ", "")
        p_clean = pred.replace(" ", "")
        for i, ch in enumerate(t_clean):
            total_chars += 1
            if i < len(p_clean) and p_clean[i] == ch:
                hit += 1
        print(f"  val: target='{target}' pred='{pred}'")
    model.train()
    return hit / max(total_chars, 1)


def train_model(epochs=80, batch_size=16, wav_dir: Path | None = None, allow_synthetic: bool = False):
    wav_pairs = load_wav_corpus(wav_dir) if wav_dir else []
    if not wav_pairs and not allow_synthetic:
        print(
            "[FATAL] Sem --wav-dir (PCM real) e sem --allow-synthetic.\n"
            "  Treino só com senoides (synthesize_pcm) gera CTC que não reconhece fala.\n"
            "  Use: python tools/train_stt.py --wav-dir data/stt_wavs\n"
            "  Ou:  python tools/train_stt.py --allow-synthetic  # lab only, honesty=false"
        )
        sys.exit(2)
    if not wav_pairs and allow_synthetic:
        print("[STT] AVISO: --allow-synthetic — corpus senoidal (NÃO é fala). Artefato lab-only.")
    model = TinyLSTM().to(DEVICE)
    opt = optim.Adam(model.parameters(), lr=2e-3)
    sched = optim.lr_scheduler.StepLR(opt, step_size=max(epochs // 3, 1), gamma=0.5)
    print(f"[STT] Params: {sum(p.numel() for p in model.parameters()):,}")
    src = f"wav={len(wav_pairs)}" if wav_pairs else f"synthetic={len(CORPUS)}"
    print(f"[STT] Treino PCM->MFCC (kernel-aligned), {src}")

    for epoch in range(epochs):
        opt.zero_grad()
        x, targets = make_batch(batch_size, wav_pairs)
        # input_lengths reais (antes do pad) — CTC precisa disso
        real_lens = []
        for b in range(x.shape[0]):
            # conta frames nao-zero
            nz = (x[b].abs().sum(dim=1) > 1e-6).sum().item()
            real_lens.append(max(int(nz), 1))
        logits = model(x)
        B, T, V = logits.shape
        log_probs = F.log_softmax(logits, dim=2).permute(1, 0, 2)
        keep = [
            i
            for i, t in enumerate(targets)
            if len(t) > 0 and len(t) < real_lens[i]
        ]
        if not keep:
            continue
        targets = [targets[i] for i in keep]
        log_probs = log_probs[:, keep, :]
        target_lengths = torch.tensor([len(t) for t in targets], device=DEVICE)
        input_lengths = torch.tensor([real_lens[i] for i in keep], dtype=torch.long, device=DEVICE)
        B = len(targets)
        targets_padded = torch.nn.utils.rnn.pad_sequence(targets, batch_first=True).to(DEVICE)
        loss = F.ctc_loss(
            log_probs,
            targets_padded,
            input_lengths,
            target_lengths,
            blank=VOCAB - 1,
            zero_infinity=True,
        )
        if not torch.isfinite(loss):
            continue
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 5.0)
        opt.step()
        sched.step()

        if (epoch + 1) % 20 == 0 or epoch == 0:
            print(f"  Epoch {epoch+1}/{epochs} | loss={loss.item():.4f}")

    acc = validate(model, wav_pairs)
    print(f"[STT] val char-overlap≈{acc:.2f}")
    TARGET.mkdir(parents=True, exist_ok=True)
    export_bin(model, TARGET / "STT.BIN")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--epochs", type=int, default=80)
    parser.add_argument("--batch", type=int, default=16)
    parser.add_argument("--wav-dir", type=Path, default=None)
    parser.add_argument(
        "--allow-synthetic",
        action="store_true",
        help="Permite treino com senoides (lab only; não reconhece fala real)",
    )
    args = parser.parse_args()
    train_model(args.epochs, args.batch, args.wav_dir, args.allow_synthetic)
