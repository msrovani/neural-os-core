#!/usr/bin/env bash
# Honesty gate CUDA (ADR-0113): GPU host ≠ engine CUDA.
# Exit 0 always with RESULT=… (não falha o CI por EngineAbsent).
set -euo pipefail
. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh 2>/dev/null || true

echo "==> host GPU"
if ! command -v nvidia-smi >/dev/null; then
  echo "RESULT: NO_NVIDIA_SMI — CUDA path UNKNOWN"
  exit 0
fi
nvidia-smi --query-gpu=name,memory.total,memory.free --format=csv
FREE=$(nvidia-smi --query-gpu=memory.free --format=csv,noheader,nounits | head -n1 | tr -d ' ')
echo "vram_free_mb_hint=${FREE:-0}"

BIN="${NOSD_LLAMA_CLI:-}"
if [[ -z "$BIN" ]]; then
  BIN=$(nix-shell -p llama-cpp --run 'command -v llama-completion' 2>/dev/null || true)
fi
MODEL="${MODEL:-$HOME/nosd-cognitive-data/models/stories15M-q4_0.gguf}"
echo "BIN=${BIN:-none} MODEL=$MODEL"

if [[ -z "$BIN" || ! -f "$MODEL" ]]; then
  echo "RESULT: SKIP_PROBE — need NOSD_LLAMA_CLI + tiny gguf"
  exit 0
fi

echo "==> ngl=99 forward (detect CUDA in system_info)"
set +e
OUT=$("$BIN" -m "$MODEL" -p "hi" -n 8 -ngl 99 2>&1)
EC=$?
set -e

if echo "$OUT" | grep -qiE 'CUDA = 1|CUDA=1|ggml_cuda|offloaded[[:space:]]+[1-9]'; then
  echo "RESULT: CUDA_VERIFIED — GPU offload observed (export NOSD_LLAMA_NGL for nosd)"
  exit 0
fi

if echo "$OUT" | grep -qiE 'no usable GPU|compiled without GPU'; then
  echo "RESULT: CUDA_ENGINE_ABSENT — host GPU OK; llama build CPU-only"
  exit 0
fi

if echo "$OUT" | grep -qi 'system_info' && echo "$OUT" | grep -qi '| CPU :' \
  && ! echo "$OUT" | grep -qiE 'CUDA = 1|CUDA=1'; then
  echo "RESULT: CUDA_ENGINE_ABSENT — nixpkgs llama-cpp default is CPU (build .#llama-cpp-cuda)"
  exit 0
fi

echo "RESULT: CUDA_UNKNOWN exit=$EC — inspect llama log manually"
echo "$OUT" | tail -n 20
exit 0
