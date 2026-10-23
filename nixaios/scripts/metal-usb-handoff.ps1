# Metal USB handoff (Windows) — does NOT flash the stick.
# ADR-0113: software preflight ≠ metal PASS.
$ErrorActionPreference = "Stop"
$Iso = $env:NIXAIOS_ISO
if (-not $Iso) {
  $Iso = Join-Path $env:USERPROFILE "Downloads\nixaios-minimal-26.11.20261020-x86_64-linux.iso"
}
Write-Host "==> ISO=$Iso"
if (-not (Test-Path $Iso)) { throw "ISO missing: $Iso" }
Get-Item $Iso | Format-List FullName, Length, LastWriteTime

Write-Host @"

==> Human steps (Rufus DD)
1. Open Rufus → select this ISO → Boot selection: DD Image (not ISO)
2. Target USB ≥4GB → START
3. Boot notebook UEFI; Secure Boot OFF if needed
4. Emergency mode → menu option with copytoram / Copy ISO to RAM
5. Accept: login + ``systemctl status nosd`` (or journalctl -u nosd)
6. Capture serial/journal → SESSION — only then metal = VERIFIED

Preflight (WSL):
  bash /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/metal-iso-deep-preflight.sh

"@
Write-Host "HANDOFF_READY (boot = AWAITING_HW)"
