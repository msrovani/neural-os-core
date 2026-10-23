#!/usr/bin/env bash
set -euo pipefail
. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
STORE=$(nix-shell -p llama-cpp --run 'dirname "$(command -v llama-cli)"')
echo "STORE_BIN=$STORE"
ls -la "$STORE"
echo "===="
# Prefer completion-style tools over server wrappers
for b in llama-completion llama-cli-completion main llama-bench llama-cli; do
  if [[ -x "$STORE/$b" ]]; then
    echo "found $b"
    "$STORE/$b" -h 2>&1 | head -n 15 || true
    echo "----"
  fi
done
