#!/usr/bin/env bash
set -euo pipefail
ISO="/nix/store/hh0g5yn5919c1dwgh0sdk9zz652r0bii-nixos-minimal-26.11.20261020.02c087f-x86_64-linux.iso/iso/nixos-minimal-26.11.20261020.02c087f-x86_64-linux.iso"
DEST="/mnt/c/Users/msrov/Downloads/nixaios-minimal-26.11.20261020-x86_64-linux.iso"
ls -lh "$ISO"
cp -v "$ISO" "$DEST"
ls -lh "$DEST"
