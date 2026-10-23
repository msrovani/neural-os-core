#!/usr/bin/env bash
# Pre-flight metal USB (ADR-0113: QEMU ≠ metal). Não grava o pendrive.
set -euo pipefail
ISO="${ISO:-/mnt/c/Users/msrov/Downloads/nixaios-minimal-26.11.20261020-x86_64-linux.iso}"
WORK="${WORK:-/tmp/nixaios-metal-check}"

echo "==> ISO=$ISO"
[[ -f "$ISO" ]] || { echo "ISO missing"; exit 1; }
ls -lh "$ISO"

. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh 2>/dev/null || true
rm -rf "$WORK"
mkdir -p "$WORK"
nix-shell -p p7zip --run "7z e -y -o$WORK '$ISO' isolinux/isolinux.cfg" >/dev/null

CFG="$WORK/isolinux.cfg"
[[ -f "$CFG" ]] || { echo "FAIL: no isolinux.cfg"; exit 2; }

echo "==> boot entries"
grep -E 'LABEL |MENU LABEL|copytoram|root=|init=' "$CFG" | head -n 40

echo "==> checks"
grep -q 'copytoram' "$CFG" && echo "OK: copytoram menu present" || echo "WARN: no copytoram"
grep -q 'nixos-system-nixaios' "$CFG" && echo "OK: init= nixaios system" || echo "WARN: unexpected init"
grep -q 'root=fstab' "$CFG" && echo "OK: root=fstab" || echo "WARN: root= not fstab"

cat <<'EOF'

==> Metal procedure (human)
1. Rufus → Imagem DD (não ISO) → gravar pendrive ≥4GB
2. UEFI boot, Secure Boot OFF se falhar
3. Se emergency mode → Options → Copy ISO Files to RAM
4. Na shell: journalctl -xb ; blkid ; ls /dev/disk/by-label
5. Aceite: login + systemctl status nosd (ou journalctl -u nosd)
6. Registar log em SESSION / nsgdb — metal PASS só com essa evidência

EOF
echo "PREFLIGHT_OK (gravação/boot = AWAITING_HW)"
