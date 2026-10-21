# Smoke nosd over TCP (Windows host)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$manifest = Join-Path $root "nixaios\packages\nosd\Cargo.toml"
$data = Join-Path $env:TEMP "nosd-smoke-data"
New-Item -ItemType Directory -Force -Path $data | Out-Null
$env:CARGO_TARGET_DIR = Join-Path $root "target\nixaios-check"

$proc = Start-Process -FilePath "cargo" -ArgumentList @(
  "run","--manifest-path",$manifest,"-p","nosd","--",
  "--foreground","--tcp","127.0.0.1:7700","--data-dir",$data
) -PassThru -NoNewWindow
Start-Sleep -Seconds 3
try {
  cargo run --manifest-path $manifest -p nosd --bin nosctl -- --tcp 127.0.0.1:7700 health
  cargo run --manifest-path $manifest -p nosd --bin nosctl -- --tcp 127.0.0.1:7700 chat "oi nixaios"
  cargo run --manifest-path $manifest -p nosd --bin nosctl -- --tcp 127.0.0.1:7700 recall "nixaios"
  Write-Host "SMOKE PASS"
} finally {
  Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
}
