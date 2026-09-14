#!/usr/bin/env python3
"""gen_stt_corpus.py — corpus PT-BR REAL para o STT CTC tiny.

Por que existe: `tools/train_stt.py` treinava com `synthesize_pcm`, que gera 3 senoides
cujas frequências derivam do ÍNDICE DO CARACTERE. O modelo aprendeu "bipes por letra",
não fala — nenhum ajuste de arquitetura no kernel conserta isso. O dataset é que estava
errado.

O que faz:
  1. gera `data/stt_corpus.txt` — comandos do domínio + variações (uma frase/linha);
  2. sintetiza cada frase em WAV com espeak-ng (multi-voz/velocidade para dar
     variabilidade real ao treino) em `data/stt_wavs/`;
  3. escreve `data/stt_wavs/index.tsv` (caminho \t texto) para o trainer consumir.

Uso:
    python tools/gen_stt_corpus.py                 # corpus + WAVs (espeak-ng)
    python tools/gen_stt_corpus.py --corpus-only   # só o .txt (sem backend de TTS)
    python tools/gen_stt_corpus.py --voices pt-br,pt --speeds 130,150,180

Requisito de TTS: espeak-ng no PATH (`apt install espeak-ng` / `choco install espeak`).
Sem ele o script gera só o corpus e diz isso explicitamente — não finge treinar.
"""
from __future__ import annotations

import argparse
import random
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
OUT_CORPUS = ROOT / "data" / "stt_corpus.txt"
OUT_WAVS = ROOT / "data" / "stt_wavs"

# Comandos do domínio (o que o usuário realmente diz ao JARBAS) + conversa curta.
BASE = [
    "jarvis",
    "ola jarvis",
    "bom dia jarvis",
    "boa noite jarvis",
    "qual o tempo",
    "como esta o tempo",
    "vai chover hoje",
    "o tempo esta bom",
    "temperatura agora",
    "ligar a luz",
    "apagar a luz",
    "desligar",
    "parar",
    "continuar",
    "sim",
    "nao",
    "obrigado",
    "ajuda",
    "status do sistema",
    "temperatura da cpu",
    "uso de memoria",
    "quantos agentes estao ativos",
    "abra o menu",
    "feche a janela",
    "abrir terminal",
    "toque uma musica",
    "aumentar o volume",
    "diminuir o volume",
    "volume alto",
    "volume baixo",
    "silenciar o microfone",
    "tirar um print da tela",
    "pesquisar na internet",
    "me mande um email",
    "que horas sao",
]

# Variações de preenchimento (o modelo precisa ver o comando em posição diferente).
PREFIXES = ["", "ei ", "por favor ", "jarvis ", "jarbas ", "oi "]
SUFFIXES = ["", " por favor", " agora", " pra mim"]


def build_corpus() -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for b in BASE:
        for p in PREFIXES:
            for s in SUFFIXES:
                line = f"{p}{b}{s}".strip()
                if line and line not in seen:
                    seen.add(line)
                    out.append(line)
    random.Random(7).shuffle(out)
    return out


def write_corpus(lines: list[str]) -> None:
    OUT_CORPUS.parent.mkdir(parents=True, exist_ok=True)
    header = (
        "# Corpus STT PT-BR — gerado por tools/gen_stt_corpus.py\n"
        "# Uma frase por linha; '#' = comentário. Consumido por train_stt.py::load_corpus\n"
    )
    OUT_CORPUS.write_text(header + "\n".join(lines) + "\n", encoding="utf-8")
    print(f"[corpus] {len(lines)} frases -> {OUT_CORPUS.relative_to(ROOT)}")


def synth_wavs(lines: list[str], voices: list[str], speeds: list[int]) -> int:
    exe = shutil.which("espeak-ng") or shutil.which("espeak")
    if not exe:
        print(
            "[wavs] espeak-ng AUSENTE — corpus gerado, WAVs não.\n"
            "       Instale espeak-ng e rode de novo: sem fala real o STT continua\n"
            "       inutilizável (ver o aviso em train_stt.py::synthesize_pcm)."
        )
        return 0
    OUT_WAVS.mkdir(parents=True, exist_ok=True)
    index = []
    rng = random.Random(11)
    n = 0
    for i, text in enumerate(lines):
        voice = rng.choice(voices)
        speed = rng.choice(speeds)
        wav = OUT_WAVS / f"u{i:05d}.wav"
        try:
            subprocess.run(
                [exe, "-v", voice, "-s", str(speed), "-w", str(wav), text],
                check=True,
                capture_output=True,
                timeout=20,
            )
        except Exception as e:  # noqa: BLE001
            print(f"[wavs] falha em '{text}': {e}")
            continue
        index.append(f"{wav.name}\t{text}")
        n += 1
    (OUT_WAVS / "index.tsv").write_text("\n".join(index) + "\n", encoding="utf-8")
    print(f"[wavs] {n} WAVs em {OUT_WAVS.relative_to(ROOT)} (index.tsv escrito)")
    return n


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus-only", action="store_true")
    ap.add_argument("--voices", default="pt-br,pt")
    ap.add_argument("--speeds", default="130,150,180")
    args = ap.parse_args()

    lines = build_corpus()
    write_corpus(lines)
    if args.corpus_only:
        return 0
    n = synth_wavs(
        lines,
        [v.strip() for v in args.voices.split(",") if v.strip()],
        [int(s) for s in args.speeds.split(",") if s.strip()],
    )
    if n == 0:
        print("[fim] corpus sem áudio — treino NÃO produzirá STT utilizável.")
        return 1
    print("[fim] rode: python tools/train_stt.py --wav-dir data/stt_wavs --epochs 80")
    return 0


if __name__ == "__main__":
    sys.exit(main())
