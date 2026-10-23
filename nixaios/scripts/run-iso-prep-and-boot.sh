#!/usr/bin/env bash
set -euo pipefail
sed -i 's/\r$//' /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/run-iso-qemu-direct.sh
cp /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/run-iso-qemu-direct.sh /tmp/run-iso-qemu-direct.sh
# stop previous guest if any (exact binary path only)
if pgrep -x qemu-system-x86_64 >/dev/null 2>&1; then
  pkill -x qemu-system-x86_64 || true
  sleep 1
fi
rm -f /tmp/nixaios-iso-boot/cmdline
exec bash /tmp/run-iso-qemu-direct.sh
