#!/usr/bin/env bash
# NixAIOS — first-boot setup inside Ubuntu WSL2
# Copies repo to Linux FS, enables systemd, installs Determinate Nix.
set -euo pipefail

WIN_REPO="${WIN_REPO:-/mnt/c/DEV/neural-os-core-latest}"
LINUX_REPO="${LINUX_REPO:-$HOME/src/neural-os-core-latest}"

echo "==> Enabling systemd in /etc/wsl.conf (needs sudo)"
if [[ ! -f /etc/wsl.conf ]] || ! grep -q 'systemd=true' /etc/wsl.conf 2>/dev/null; then
  sudo tee /etc/wsl.conf >/dev/null <<'EOF'
[boot]
systemd=true
EOF
  echo "    wrote /etc/wsl.conf — run 'wsl --shutdown' from PowerShell, then reopen Ubuntu"
fi

echo "==> Syncing repo to Linux FS: $LINUX_REPO"
mkdir -p "$(dirname "$LINUX_REPO")"
if [[ ! -d "$LINUX_REPO/.git" ]]; then
  # Prefer rsync; fallback to cp
  if command -v rsync >/dev/null 2>&1; then
    rsync -a --exclude target --exclude target1 --exclude .git/objects/pack \
      "$WIN_REPO/" "$LINUX_REPO/"
  else
    mkdir -p "$LINUX_REPO"
    cp -a "$WIN_REPO/." "$LINUX_REPO/"
  fi
  # Keep .git if present on Windows side
  if [[ -d "$WIN_REPO/.git" && ! -d "$LINUX_REPO/.git" ]]; then
    cp -a "$WIN_REPO/.git" "$LINUX_REPO/"
  fi
else
  echo "    already present — skipping full copy (pull/rsync manually if needed)"
fi

if ! command -v nix >/dev/null 2>&1; then
  echo "==> Installing Determinate Nix (single-user / multi-user as installer chooses)"
  curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install --no-confirm
  # shellcheck disable=SC1091
  if [[ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]]; then
    # shellcheck source=/dev/null
    . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
  fi
else
  echo "==> nix already installed: $(nix --version)"
fi

echo "==> Enabling flakes"
mkdir -p "$HOME/.config/nix"
if ! grep -q 'experimental-features' "$HOME/.config/nix/nix.conf" 2>/dev/null; then
  cat >> "$HOME/.config/nix/nix.conf" <<'EOF'
experimental-features = nix-command flakes
EOF
fi

echo ""
echo "DONE setup. Next:"
echo "  1) From PowerShell:  wsl --shutdown"
echo "  2) Reopen Ubuntu"
echo "  3) bash $LINUX_REPO/nixaios/scripts/build-iso-wsl.sh"
echo "     (or: cd $LINUX_REPO/nixaios && nix build .#iso -L)"
