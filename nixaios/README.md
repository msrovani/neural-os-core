# NixAIOS

Produto **paralelo** ao neural-os-core bare-metal (não é fork). ADR-0118.

## Quick start (Windows / WSL)

```bash
cd nixaios/packages/nosd
cargo test --workspace
# Windows TCP lab:
cargo run -p nosd -- --foreground --tcp 127.0.0.1:7700 --data-dir ./data
cargo run -p nosd --bin nosctl -- --tcp 127.0.0.1:7700 health
cargo run -p nosd --bin nosctl -- --tcp 127.0.0.1:7700 chat "oi"
```

WSL: ver [docs/WSL.md](docs/WSL.md).

## Nix

```bash
cd nixaios
nix develop
nix build .#nosd
# ISO (Linux host / WSL com Nix):
# nix build .#nixosConfigurations.nixaios-iso.config.system.build.isoImage
```

## Layout

- `packages/nosd/` — workspace Rust (`nosd`, hub, factory, wasm Frugal, memory/nsgdb facade, mesh, BEI, hermes, jarbas, voice)
- `modules/` — systemd nosd, cage iGPU, ai-managed.nix
- `docs/` — MIGRATION_MATRIX, TECNOLOGIAS_NIXAIOS, WSL
