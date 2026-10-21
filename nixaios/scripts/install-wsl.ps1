# NixAIOS — install WSL2 + Ubuntu 24.04 (requires Admin / UAC)
# Run from elevated PowerShell OR accept the UAC prompt.
# After success: reboot, then run setup-nixaios-wsl.sh inside Ubuntu.
$ErrorActionPreference = "Stop"

Write-Host "==> Installing WSL2 + Ubuntu-24.04 (may prompt UAC)..."
$p = Start-Process -FilePath "wsl.exe" `
  -ArgumentList "--install", "-d", "Ubuntu-24.04", "--no-launch" `
  -Verb RunAs -Wait -PassThru
Write-Host "wsl --install exit=$($p.ExitCode)"

Write-Host "==> Listing distros:"
& wsl.exe -l -v

Write-Host @"

NEXT:
  1) Reboot if Windows asked.
  2) Open Ubuntu once (create user/password).
  3) Inside Ubuntu:
       bash /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/setup-nixaios-wsl.sh
  4) Build ISO (after Nix install + new shell):
       bash /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/build-iso-wsl.sh

"@
