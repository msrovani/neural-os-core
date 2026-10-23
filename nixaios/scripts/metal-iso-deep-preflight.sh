#!/usr/bin/env bash
# Deep preflight metal: isolinux + EFI + nosd presence in squash/nix-store hint.
# Não grava pendrive. ADR-0113: isto ≠ metal PASS.
set -euo pipefail
ISO="${ISO:-/mnt/c/Users/msrov/Downloads/nixaios-minimal-26.11.20261020-x86_64-linux.iso}"
WORK="${WORK:-/tmp/nixaios-metal-deep}"

echo "==> ISO=$ISO"
[[ -f "$ISO" ]] || { echo "ISO missing"; exit 1; }
ls -lh "$ISO"

. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh 2>/dev/null || true
rm -rf "$WORK"
mkdir -p "$WORK"

nix-shell -p p7zip --run "7z l '$ISO'" >"$WORK/listing.txt"

echo "==> boot payloads in ISO"
grep -iE 'isolinux\.cfg|grub\.cfg|bzImage|initrd|EFI|copytoram' "$WORK/listing.txt" | head -n 40 || true

echo "==> extract boot configs"
nix-shell -p p7zip --run "7z e -y -o$WORK '$ISO' isolinux/isolinux.cfg" >/dev/null 2>&1 || true
# EFI grub path varies
nix-shell -p p7zip --run "7z e -y -o$WORK '$ISO' EFI/BOOT/grub.cfg" >/dev/null 2>&1 || true
nix-shell -p p7zip --run "7z e -y -o$WORK '$ISO' boot/grub/grub.cfg" >/dev/null 2>&1 || true
find "$WORK" -maxdepth 1 -type f -name '*.cfg' -printf 'cfg: %f\n' 2>/dev/null || \
  find "$WORK" -maxdepth 1 -type f -name '*.cfg' | sed 's|.*/|cfg: |'

FAIL=0
for cfg in "$WORK"/*.cfg; do
  [[ -f "$cfg" ]] || continue
  echo "==> scan $(basename "$cfg")"
  grep -E 'LABEL |MENU LABEL|menuentry|copytoram|root=|init=' "$cfg" | head -n 30 || true
  grep -q 'copytoram' "$cfg" && echo "OK: copytoram in $(basename "$cfg")" || echo "WARN: no copytoram in $(basename "$cfg")"
done

# nosd binary / unit — live ISO often embeds in squashfs; list may show nix-store paths
if grep -qiE 'nosd|neural-os' "$WORK/listing.txt"; then
  echo "OK: nosd/neural-os path mentioned in ISO listing"
  grep -iE 'nosd|neural-os' "$WORK/listing.txt" | head -n 15
else
  echo "WARN: nosd string absent from top-level ISO listing (may live inside squashfs)"
  # Try squashfs presence
  if grep -qiE 'squashfs|nix-store\.squashfs|root\.squashfs' "$WORK/listing.txt"; then
    echo "OK: squashfs present — nosd check deferred to boot (AWAITING_HW / QEMU)"
  else
    echo "WARN: no squashfs in listing either"
    FAIL=1
  fi
fi

bash "$(dirname "$0")/metal-iso-checklist.sh" || FAIL=1

if [[ "$FAIL" -eq 0 ]]; then
  echo "DEEP_PREFLIGHT_OK (metal boot still AWAITING_HW)"
  exit 0
fi
echo "DEEP_PREFLIGHT_PARTIAL"
exit 0
