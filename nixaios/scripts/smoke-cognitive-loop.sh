#!/usr/bin/env bash
# Cognitive loop: GGUF → chat → remember → recall (ADR-0113).
set -euo pipefail
# shellcheck disable=SC1091
[[ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]] && \
  . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh

ROOT="${ROOT:-/mnt/c/DEV/neural-os-core-latest/nixaios/packages/nosd}"
DATA="${DATA:-$HOME/nosd-cognitive-data}"
PORT="${PORT:-7701}"
MODEL_URL="${MODEL_URL:-https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf}"
MODEL_NAME="${MODEL_NAME:-stories15M-q4_0.gguf}"
MARKER="nixaios-cognitive-$(date +%s)"
NIX_PKGS="rustc cargo pkg-config openssl llama-cpp"

mkdir -p "$DATA/models"
cd "$ROOT"

echo "==> resolve llama-completion (not server-wrapper llama-cli)"
if [[ -n "${NOSD_LLAMA_CLI:-}" && -x "${NOSD_LLAMA_CLI}" ]]; then
  LLAMA_BIN="$NOSD_LLAMA_CLI"
else
  LLAMA_BIN=$(nix-shell -p llama-cpp --run 'command -v llama-completion || command -v llama-cli || true')
fi
if [[ -z "$LLAMA_BIN" || ! -x "$LLAMA_BIN" ]]; then
  echo "FAIL: llama-completion not found" >&2
  exit 2
fi
# Reject tiny wrapper that only launches server without completion semantics if name is llama-cli
# and we can find completion next to it.
if [[ "$(basename "$LLAMA_BIN")" == "llama-cli" ]]; then
  sibling="$(dirname "$LLAMA_BIN")/llama-completion"
  if [[ -x "$sibling" ]]; then
    LLAMA_BIN="$sibling"
  fi
fi
echo "    LLAMA_BIN=$LLAMA_BIN"
export NOSD_LLAMA_CLI="$LLAMA_BIN"
export NOSD_LLAMA_NGL="${NOSD_LLAMA_NGL:-0}"
export NOSD_LLAMA_N_PREDICT="${NOSD_LLAMA_N_PREDICT:-32}"

MODEL_PATH="$DATA/models/$MODEL_NAME"
if [[ ! -f "$MODEL_PATH" || $(stat -c%s "$MODEL_PATH" 2>/dev/null || echo 0) -lt 1000000 ]]; then
  echo "==> download $MODEL_NAME → $MODEL_PATH"
  nix-shell -p curl --run "curl -L --fail --retry 3 -o '$MODEL_PATH' '$MODEL_URL'"
fi
ls -lh "$MODEL_PATH"
head -c 4 "$MODEL_PATH" | od -An -tx1 | grep -q '47 47 55 46' || {
  echo "FAIL: not GGUF magic" >&2
  exit 3
}

# Quick direct completion sanity
echo "==> direct completion probe"
PROBE=$(nix-shell -p llama-cpp --run "
  '$LLAMA_BIN' -m '$MODEL_PATH' -p 'Once upon a time' -n 16 2>/dev/null | tr -d '\r' | tail -c 200
" || true)
echo "    probe_tail=${PROBE:0:120}"
if [[ -z "${PROBE// /}" ]]; then
  echo "WARN: direct probe empty — will still try nosd path" >&2
fi

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/nixaios-cognitive-target}"
echo "==> build nosd"
nix-shell -p $NIX_PKGS --run "cargo build -p nosd --bins --release"

pkill -f "nosd.*--tcp.*$PORT" 2>/dev/null || true
sleep 1

echo "==> start nosd :$PORT"
nix-shell -p $NIX_PKGS --run "
  export NOSD_LLAMA_CLI='$NOSD_LLAMA_CLI'
  export NOSD_LLAMA_NGL='$NOSD_LLAMA_NGL'
  export NOSD_LLAMA_N_PREDICT='$NOSD_LLAMA_N_PREDICT'
  exec '$CARGO_TARGET_DIR/release/nosd' \
    --foreground --tcp '127.0.0.1:$PORT' --data-dir '$DATA'
" >"$DATA/nosd.log" 2>&1 &
NOSD_PID=$!
cleanup() { kill "$NOSD_PID" 2>/dev/null || true; }
trap cleanup EXIT

for i in $(seq 1 60); do
  if grep -qE 'engine=llama-gguf|listen tcp' "$DATA/nosd.log" 2>/dev/null; then
    break
  fi
  sleep 0.5
done
grep -E 'engine=|hub register|capacity ' "$DATA/nosd.log" || true
if ! grep -q 'engine=llama-gguf' "$DATA/nosd.log"; then
  echo "FAIL: engine is not llama-gguf" >&2
  tail -60 "$DATA/nosd.log" >&2
  exit 4
fi

NOSCTL="$CARGO_TARGET_DIR/release/nosctl"
echo "==> chat"
CHAT_OUT=$(nix-shell -p $NIX_PKGS --run "'$NOSCTL' --tcp '127.0.0.1:$PORT' chat 'diz a palavra secreta $MARKER por favor'")
echo "$CHAT_OUT"
echo "$CHAT_OUT" | grep -qi 'cpu-echo' && { echo "FAIL: cpu-echo" >&2; exit 5; }
# Exigir texto não vazio no result.text
TEXT=$(echo "$CHAT_OUT" | nix-shell -p jq --run 'jq -r .result.text' 2>/dev/null || true)
if [[ -z "${TEXT// /}" ]]; then
  # fallback parse sem jq
  TEXT=$(echo "$CHAT_OUT" | sed -n 's/.*"text": "\(.*\)".*/\1/p' | head -1)
fi
if [[ -z "${TEXT// /}" ]]; then
  echo "FAIL: empty completion text" >&2
  tail -40 "$DATA/nosd.log" >&2
  exit 7
fi
if echo "$TEXT" | grep -qiE 'common_perf|sampler chain|mirostat_lr'; then
  echo "FAIL: completion looks like llama logs, not tokens: $TEXT" >&2
  exit 8
fi
echo "    completion_len=${#TEXT}"
echo "    completion_preview=${TEXT:0:160}"

echo "==> recall"
RECALL_OUT=$(nix-shell -p $NIX_PKGS --run "'$NOSCTL' --tcp '127.0.0.1:$PORT' recall '$MARKER'")
echo "$RECALL_OUT"
echo "$RECALL_OUT" | grep -q "$MARKER" || {
  echo "FAIL: recall miss" >&2
  exit 6
}

echo "PASS cognitive-loop engine=llama-gguf tokens+remember+recall marker=$MARKER"
echo "log=$DATA/nosd.log data=$DATA"
