#!/usr/bin/env bash
# Prove: Downloads ISO = flake build (nixaios) ≠ channels.nixos.org stock.
set -euo pipefail
. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh 2>/dev/null || true

ISO="${ISO:-/mnt/c/Users/msrov/Downloads/nixaios-minimal-26.11.20261020-x86_64-linux.iso}"
STORE_CANDIDATE="/nix/store/hh0g5yn5919c1dwgh0sdk9zz652r0bii-nixos-minimal-26.11.20261020.02c087f-x86_64-linux.iso/iso/nixos-minimal-26.11.20261020.02c087f-x86_64-linux.iso"
# Official channel hash (fetched 2026-10-22) — stock minimal unstable
OFFICIAL_SHA256="${OFFICIAL_SHA256:-69d519600f2e4e6f60f3a3d5d4c1e2afda6779afbeea30e0638d90fdca07f56c}"

echo "==> local ISO"
ls -lh "$ISO"
LOCAL_SHA=$(sha256sum "$ISO" | awk '{print $1}')
echo "sha256=$LOCAL_SHA"

# ISO9660 Primary Volume Descriptor: volume id at byte 40 of sector 16
VOL=$(dd if="$ISO" bs=1 skip=$((16 * 2048 + 40)) count=32 2>/dev/null | tr -d ' \0' || true)
echo "volume_id=[${VOL}]"

WORK=/tmp/iso-origin-check
rm -rf "$WORK"
mkdir -p "$WORK"
nix-shell -p p7zip --run "7z e -y -o$WORK '$ISO' isolinux/isolinux.cfg" >/dev/null
INIT_LINE=$(grep -E 'APPEND.*init=' "$WORK/isolinux.cfg" | head -1 || true)
echo "init_line=${INIT_LINE:0:160}..."

if echo "$INIT_LINE" | grep -q 'nixos-system-nixaios'; then
  echo "ORIGIN: FLAKE_NIXAIOS (init= nixos-system-nixaios) — NOT stock website ISO"
elif echo "$INIT_LINE" | grep -q 'nixos-system-nixos'; then
  echo "ORIGIN: STOCK_NIXOS_INSTALLER (init= nixos-system-nixos)"
else
  echo "ORIGIN: UNKNOWN"
fi

if [[ "$VOL" == "NIXAIOS" ]]; then
  echo "volumeID=NIXAIOS → customized by nixaios/images/iso.nix (stock uses NIXOS_* label)"
elif [[ "$VOL" == NIXOS* ]]; then
  echo "volumeID looks stock NixOS: $VOL"
fi

if [[ "$LOCAL_SHA" == "$OFFICIAL_SHA256" ]]; then
  echo "HASH: MATCHES channels.nixos.org latest-nixos-minimal (stock download)"
else
  echo "HASH: DIFFERS from channels.nixos.org stock ($OFFICIAL_SHA256)"
  echo "      → not a corrupt download from the site; it is a different image (our build)"
fi

if [[ -f "$STORE_CANDIDATE" ]]; then
  STORE_SHA=$(sha256sum "$STORE_CANDIDATE" | awk '{print $1}')
  echo "nix_store_sha=$STORE_SHA"
  if [[ "$STORE_SHA" == "$LOCAL_SHA" ]]; then
    echo "COPY: Downloads ISO == nix store build (bit-identical)"
  else
    echo "COPY: Downloads ISO != current nix store candidate (rebuild or different out)"
  fi
else
  echo "nix store candidate GC'd — copy-iso-to-downloads.sh pointed at that path historically"
fi

echo
echo "==> emergency mode note"
echo "Stock NixOS USB also hits emergency when root label/device not found (Rufus ISO mode,"
echo "bad USB, missing copytoram). See r/NixOS + nixpkgs iso-image.nix — not unique to NixAIOS."
echo "cmdline uses root=fstab (systemd initrd) + volumeID NIXAIOS; prefer Rufus DD + copytoram."
