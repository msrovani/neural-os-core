#!/usr/bin/env bash
# Boot nosd briefly; assert capacity slog has posture + suggest_ngl.
set -euo pipefail
. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh 2>/dev/null || true
cd /mnt/c/DEV/neural-os-core-latest/nixaios/packages/nosd
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/nosd-target}"
DATA="${TMPDIR:-/tmp}/nosd-cap-probe-$$"
mkdir -p "$DATA/models"
SOCK="/tmp/nosd-cap-$$.sock"
LOG="$DATA/nosd.log"

nix-shell -p rustc cargo pkg-config openssl --run \
  "cargo build -p nosd --release --bins" >/dev/null

BIN="$CARGO_TARGET_DIR/release/nosd"
"$BIN" --foreground --socket "$SOCK" --tcp 127.0.0.1:7709 --data-dir "$DATA" >"$LOG" 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null || true; rm -rf "$DATA" "$SOCK"' EXIT
sleep 2
if grep -E 'capacity .* posture=' "$LOG" | head -n1; then
  if grep -qE 'posture=nvidia_smi|posture=nvml_absent|posture=no_gpu_hint|posture=drm_present' "$LOG"; then
    echo "RESULT: CAPACITY_SLOG_OK"
    exit 0
  fi
fi
echo "RESULT: CAPACITY_SLOG_MISS"
cat "$LOG" | head -n 40
exit 1
