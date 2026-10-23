#!/usr/bin/env bash
# Boot NixAIOS ISO in QEMU inside WSL2 (nested KVM when /dev/kvm exists).
# Honesty: QEMU PASS ≠ metal PASS (ADR-0113).
set -euo pipefail

ISO="${ISO:-/mnt/c/Users/msrov/Downloads/nixaios-minimal-26.11.20261020-x86_64-linux.iso}"
MEM="${MEM:-4G}"
SMP="${SMP:-2}"
# serial console (headless) by default; set DISPLAY_MODE=gtk for WSLg window
DISPLAY_MODE="${DISPLAY_MODE:-serial}"
LOG="${LOG:-/tmp/nixaios-qemu.log}"

if [[ ! -f "$ISO" ]]; then
  echo "ISO missing: $ISO" >&2
  exit 1
fi

# Prefer nix-provided qemu (no apt/sudo)
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
  # shellcheck disable=SC1091
  [[ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]] && \
    . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
  echo "==> installing qemu via nix profile (one-time)..."
  nix profile install nixpkgs#qemu_kvm 2>/dev/null || \
    nix profile install nixpkgs#qemu
fi

ACCEL=()
# WSL: /dev/kvm is root:kvm — user must be in group kvm (or one-shot chmod)
if [[ -e /dev/kvm ]] && [[ ! -r /dev/kvm || ! -w /dev/kvm ]]; then
  if sudo -n chmod 666 /dev/kvm 2>/dev/null; then
    echo "==> opened /dev/kvm (chmod via passwordless sudo)"
  else
    echo "==> WARN: not in group kvm — run once: sudo usermod -aG kvm \$USER && newgrp kvm"
  fi
fi
if [[ -e /dev/kvm ]] && [[ -r /dev/kvm ]] && [[ -w /dev/kvm ]]; then
  ACCEL=(-enable-kvm -cpu host)
  echo "==> KVM accel ON"
else
  ACCEL=(-cpu qemu64)
  echo "==> KVM unavailable — TCG (slow)"
fi

DISP=()
case "$DISPLAY_MODE" in
  serial|nographic)
    DISP=(-nographic -serial mon:stdio)
    ;;
  gtk)
    DISP=(-display gtk -serial stdio)
    ;;
  vnc)
    DISP=(-display vnc=:1 -serial stdio)
    echo "==> VNC on :5901 (localhost)"
    ;;
  *)
    echo "DISPLAY_MODE=$DISPLAY_MODE unknown (serial|gtk|vnc)" >&2
    exit 1
    ;;
esac

echo "==> ISO=$ISO MEM=$MEM SMP=$SMP"
echo "==> log=$LOG (also serial on this tty)"
echo "==> quit: Ctrl-A X (nographic) or poweroff in guest"
set -x
exec qemu-system-x86_64 \
  "${ACCEL[@]}" \
  -m "$MEM" \
  -smp "$SMP" \
  -cdrom "$ISO" \
  -boot d \
  -netdev user,id=net0 \
  -device virtio-net-pci,netdev=net0 \
  "${DISP[@]}" \
  2>&1 | tee "$LOG"
