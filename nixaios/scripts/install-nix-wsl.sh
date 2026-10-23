#!/usr/bin/env bash
# NixAIOS — install Determinate Nix inside Ubuntu WSL (needs your sudo password once)
set -euo pipefail

if command -v nix >/dev/null 2>&1; then
  echo "nix already: $(nix --version)"
  exit 0
fi

echo "==> Installing Determinate Nix (sudo password may be required)..."
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix \
  | sh -s -- install --no-confirm

# shellcheck disable=SC1091
if [[ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]]; then
  # shellcheck source=/dev/null
  . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
fi

mkdir -p "$HOME/.config/nix"
if ! grep -q 'experimental-features' "$HOME/.config/nix/nix.conf" 2>/dev/null; then
  cat >> "$HOME/.config/nix/nix.conf" <<'EOF'
experimental-features = nix-command flakes
EOF
fi

echo "OK: $(nix --version)"
echo "Next: bash ~/src/neural-os-core-latest/nixaios/scripts/build-iso-wsl.sh"
