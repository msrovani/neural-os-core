#!/usr/bin/env bash
# NixAIOS — build bootable ISO via flake (run inside WSL Ubuntu with Nix)
# Output: nixaios/result/iso/*.iso (symlink result/)
set -euo pipefail

REPO="${REPO:-$HOME/src/neural-os-core-latest}"
NIXAIOS="$REPO/nixaios"

if [[ ! -d "$NIXAIOS" ]]; then
  # Fallback: Windows mount (slower I/O)
  NIXAIOS="/mnt/c/DEV/neural-os-core-latest/nixaios"
fi

cd "$NIXAIOS"
echo "==> Building NixAIOS ISO from $NIXAIOS"
echo "    (first run downloads nixpkgs — can take 30–90+ min)"

# Prefer convenience attr; fall back to full config path
if nix build .#iso -L --print-out-paths 2>/tmp/nixaios-iso-build.log; then
  OUT=$(readlink -f result)
else
  echo "    .#iso failed, trying nixosConfigurations path..."
  nix build .#nixosConfigurations.nixaios-iso.config.system.build.isoImage -L
  OUT=$(readlink -f result)
fi

echo ""
echo "ISO build OK → $OUT"
find "$OUT" -name '*.iso' -printf '  %p (%s bytes)\n' 2>/dev/null || find "$OUT" -name '*.iso'
echo ""
echo "Copy to Windows Downloads (optional):"
echo "  cp -v \$OUT/*.iso /mnt/c/Users/\$USER/Downloads/  # adjust USER"
echo "Honesty: WSL build PASS ≠ metal boot PASS (ADR-0113)."
