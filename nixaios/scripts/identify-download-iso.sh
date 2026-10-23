#!/usr/bin/env bash
# Identify ISO in Downloads: stock NixOS vs NixAIOS flake.
set -euo pipefail
. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh 2>/dev/null || true

DL=/mnt/c/Users/msrov/Downloads
echo "==> ISOs in Downloads"
ls -lh "$DL"/*nixos*.iso "$DL"/*nixaios*.iso 2>/dev/null || true

ISO="${1:-}"
if [[ -z "$ISO" ]]; then
  # Prefer newest nixos/nixaios iso
  ISO=$(ls -t "$DL"/*nixos*.iso "$DL"/*nixaios*.iso 2>/dev/null | head -1 || true)
fi
[[ -n "$ISO" && -f "$ISO" ]] || { echo "no iso"; exit 1; }

echo
echo "==> probing: $ISO"
VOL=$(dd if="$ISO" bs=1 skip=$((16 * 2048 + 40)) count=32 2>/dev/null | tr -d ' \0' || true)
echo "volume_id=[${VOL}]"

WORK=/tmp/iso-id-$$
rm -rf "$WORK"
mkdir -p "$WORK"
nix-shell -p p7zip --run "7z e -y -o$WORK '$ISO' isolinux/isolinux.cfg" >/dev/null
LINE=$(grep -E 'APPEND.*init=' "$WORK/isolinux.cfg" | head -1 || true)
echo "init=${LINE:0:200}"

if echo "$LINE" | grep -q 'nixos-system-nixaios'; then
  echo "RESULT: NIXAIOS_FLAKE"
elif echo "$LINE" | grep -q 'nixos-system-nixos'; then
  echo "RESULT: STOCK_NIXOS (site/channel) — sem nosd embutido"
else
  echo "RESULT: UNKNOWN"
fi
grep -q copytoram "$WORK/isolinux.cfg" && echo "copytoram=yes" || echo "copytoram=no"
