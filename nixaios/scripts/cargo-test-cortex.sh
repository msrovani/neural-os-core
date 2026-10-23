#!/usr/bin/env bash
set -euo pipefail
. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh 2>/dev/null || true
cd /mnt/c/DEV/neural-os-core-latest/nixaios/packages/nosd
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/nosd-target}"
nix-shell -p rustc cargo pkg-config openssl --run \
  'cargo test -p nos-cortex --release --lib -- --test-threads=1 capacity posture_vram plan_n_gpu'
