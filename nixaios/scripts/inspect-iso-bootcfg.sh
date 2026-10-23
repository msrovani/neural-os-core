#!/usr/bin/env bash
# Inspect LABEL/APPEND from NixAIOS/NixOS ISO (diagnose emergency mode).
set -euo pipefail
# shellcheck disable=SC1091
[[ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]] && \
  . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh

ISO="${ISO:-/mnt/c/Users/msrov/Downloads/nixaios-minimal-26.11.20261020-x86_64-linux.iso}"
WORK="${WORK:-/tmp/nixaios-label-check}"

rm -rf "$WORK"
mkdir -p "$WORK"

echo "==> ISO=$ISO"
ls -lh "$ISO"

nix-shell -p p7zip --run "7z x -y -o$WORK '$ISO' isolinux EFI boot/grub loader 2>&1" | tail -n 30

echo "==== files ===="
find "$WORK" -type f | head -n 60

echo "==== volume / APPEND / linux lines ===="
grep -RInE 'LABEL=|APPEND|^\s*linux |volumeID|NIXAIOS|copytoram|boot.shell_on_fail' "$WORK" 2>/dev/null | head -n 80 || true

echo "==== isolinux.cfg (if any) ===="
find "$WORK" -name 'isolinux.cfg' -print -exec head -n 60 {} \;

echo "==== grub.cfg snippets ===="
find "$WORK" -name 'grub.cfg' -print | while read -r f; do
  echo "--- $f"
  grep -nE 'linux |LABEL=|menuentry' "$f" | head -n 40 || true
done
