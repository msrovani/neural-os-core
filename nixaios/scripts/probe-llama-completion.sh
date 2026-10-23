#!/usr/bin/env bash
set -euo pipefail
. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
DATA="${HOME}/nosd-cognitive-data"
mkdir -p "$DATA/models"
M="$DATA/models/stories15M-q4_0.gguf"
URL="https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf"
if [[ ! -f "$M" || $(stat -c%s "$M") -lt 1000000 ]]; then
  nix-shell -p curl --run "curl -L --fail --retry 3 -o '$M' '$URL'"
fi
BIN=$(nix-shell -p llama-cpp --run 'command -v llama-completion')
echo "BIN=$BIN M=$M size=$(stat -c%s "$M")"
"$BIN" -m "$M" -p "Once upon a time" -n 24 2>"$DATA/probe.err" | tee "$DATA/probe.out"
echo EXIT:$?
echo "--- stderr tail ---"
tail -n 30 "$DATA/probe.err" || true
echo "out_bytes=$(wc -c <"$DATA/probe.out")"
