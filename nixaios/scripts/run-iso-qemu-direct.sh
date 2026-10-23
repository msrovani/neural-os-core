#!/usr/bin/env bash
# Boot NixAIOS ISO via extracted kernel+initrd + console=ttyS0 (serial).
set -euo pipefail
# shellcheck disable=SC1091
[[ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]] && \
  . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh

ISO="${ISO:-/mnt/c/Users/msrov/Downloads/nixaios-minimal-26.11.20261020-x86_64-linux.iso}"
WORK="${WORK:-/tmp/nixaios-iso-boot}"
MEM="${MEM:-3G}"
SMP="${SMP:-2}"
LOG="${LOG:-/tmp/nixaios-qemu.log}"

mkdir -p "$WORK"

need_extract=0
[[ -f "$WORK/bzImage" && -f "$WORK/initrd" && -f "$WORK/cmdline" ]] || need_extract=1

if [[ "$need_extract" -eq 1 ]]; then
  echo "==> extracting boot bits from ISO..."
  rm -rf "$WORK/extract"
  mkdir -p "$WORK/extract"
  nix-shell -p p7zip --run "7z x -y -o$WORK/extract '$ISO' 'boot' 'isolinux' 'EFI' 'loader'"
  IMG=$(find "$WORK/extract" -type f -name 'bzImage' | head -1)
  IRD=$(find "$WORK/extract" -type f \( -name 'initrd' -o -name 'initrd.gz' \) | head -1)
  CFG=$(find "$WORK/extract" -type f \( -name 'isolinux.cfg' -o -name 'grub.cfg' \) | head -1)
  if [[ -z "$IMG" || -z "$IRD" ]]; then
    echo "extract failed" >&2
    find "$WORK/extract" -type f | head -50 >&2
    exit 1
  fi
  cp -f "$IMG" "$WORK/bzImage"
  cp -f "$IRD" "$WORK/initrd"
  CMDLINE=""
  if [[ -n "${CFG:-}" ]]; then
    echo "==> cfg=$CFG"
    # Prefer first linux/APPEND with init=
    if grep -q 'APPEND' "$CFG"; then
      CMDLINE=$(grep -E 'APPEND' "$CFG" | head -1 | sed -E 's/.*APPEND[[:space:]]+//')
    elif grep -q 'linux ' "$CFG"; then
      # grub: linux $kernel $params
      CMDLINE=$(grep -E '^\s*linux\s' "$CFG" | head -1 | sed -E 's/^\s*linux\s+\S+\s+//')
    fi
  fi
  if [[ -z "$CMDLINE" ]]; then
    echo "WARN: no cmdline from cfg — using fallback label root" >&2
    CMDLINE="root=LABEL=NIXAIOS boot.shell_on_fail"
  fi
  # Force serial for -nographic
  case "$CMDLINE" in
    *console=ttyS0*) ;;
    *) CMDLINE="$CMDLINE console=ttyS0,115200n8" ;;
  esac
  printf '%s\n' "$CMDLINE" > "$WORK/cmdline"
  echo "    kernel ok; cmdline saved"
fi

CMDLINE=$(cat "$WORK/cmdline")

if [[ -e /dev/kvm && -r /dev/kvm && -w /dev/kvm ]]; then
  ACCEL=(-enable-kvm -cpu host)
  echo "==> KVM ON"
else
  ACCEL=(-cpu max)
  echo "==> TCG (slow) — once: sudo usermod -aG kvm \$USER && newgrp kvm"
fi

echo "==> MEM=$MEM SMP=$SMP"
echo "==> CMDLINE=$CMDLINE"
echo "==> LOG=$LOG  (quit: Ctrl-A X)"
exec qemu-system-x86_64 \
  "${ACCEL[@]}" \
  -m "$MEM" \
  -smp "$SMP" \
  -kernel "$WORK/bzImage" \
  -initrd "$WORK/initrd" \
  -append "$CMDLINE" \
  -cdrom "$ISO" \
  -boot order=d \
  -netdev user,id=net0 \
  -device virtio-net-pci,netdev=net0 \
  -nographic \
  -serial mon:stdio \
  2>&1 | tee "$LOG"
