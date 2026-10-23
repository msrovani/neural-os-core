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

Sem GGUF/`llama-completion` → `[cpu-echo]`. Loop cognitivo WSL:

```bash
bash nixaios/scripts/smoke-cognitive-loop.sh   # PASS = tokens + remember + recall
```

WASM: ADR-**0059** (não 0032). Frugal: 256 KiB = **4** páginas / fuel 100k.  
QEMU PASS ≠ metal PASS (ADR-0113). WSL: [docs/WSL.md](docs/WSL.md).  
CUDA: `smoke-cuda-honesty.sh` — host GPU OBSERVED ≠ `CUDA_ENGINE_ABSENT` (nixpkgs llama CPU); opt-in `nix build .#llama-cpp-cuda`.  
Metal: `metal-iso-deep-preflight.sh` / `metal-usb-handoff.ps1` → Rufus **DD** + copytoram; boot = AWAITING_HW.

## Nix

```bash
cd nixaios
nix develop
nix build .#nosd
# ISO / CUDA opt-in (WSL):
# nix build .#iso
# nix build .#llama-cpp-cuda   # pesado; só p/ NOSD_LLAMA_NGL>0 VERIFIED
```

## Layout

- `packages/nosd/` — workspace Rust (`nosd`, hub, factory, wasm Frugal, memory/nsgdb facade, mesh, BEI, hermes, jarbas, voice)
- `modules/` — systemd nosd, cage iGPU, ai-managed.nix
- `docs/` — MIGRATION_MATRIX, TECNOLOGIAS_NIXAIOS, WSL
