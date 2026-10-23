#!/usr/bin/env bash
# Mede frota wasmi Frugal (ADR-0118) — host OBSERVED ≠ metal.
set -euo pipefail
[[ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]] && \
  . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh

ROOT="${ROOT:-/mnt/c/DEV/neural-os-core-latest/nixaios/packages/nosd}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/nixaios-frugal-target}"
cd "$ROOT"

NIX_PKGS="rustc cargo pkg-config openssl"
echo "==> nos-wasm fleet + fuel_trap"
OUT=$(nix-shell -p $NIX_PKGS --run '
  cargo test -p nos-wasm --release -- --nocapture fleet_ fuel_trap 2>&1
')
echo "$OUT"

echo "$OUT" | grep -q 'test fleet_32_identity ... ok' || { echo "FAIL fleet_32"; exit 1; }
echo "$OUT" | grep -q 'test fleet_128_identity ... ok' || { echo "FAIL fleet_128"; exit 1; }
echo "$OUT" | grep -q 'test fuel_trap_on_burn ... ok' || { echo "FAIL fuel_trap"; exit 1; }

# Extrai tempos se o harness imprimir Debug (não obrigatório)
echo "PASS frugal-fleet: 32+128 identity ok; fuel trap ok"
echo "heap_budget: 32×256KiB=8MiB; 128×256KiB=32MiB (limites por instância, não RSS medido)"
echo "NOTE: fuel dinâmico / page-out = HYPOTHESIS (não nesta fatia)"
