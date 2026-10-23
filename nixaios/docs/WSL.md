# NixAIOS — Bancada WSL2

**WSL PASS ≠ ISO PASS ≠ metal PASS** (ADR-0113).

## Path 1 — WSL → Nix → ISO NixAIOS (não editar ISO da Downloads)

A ISO gráfica em `Downloads\` é imutável. Geramos uma **nova** ISO a partir do flake.

### A) Windows (Admin / UAC)

PowerShell **como Administrador**:

```powershell
cd C:\DEV\neural-os-core-latest\nixaios\scripts
.\install-wsl.ps1
# ou manual:
wsl --install -d Ubuntu-24.04
```

Reinicia se o Windows pedir. Abre **Ubuntu** uma vez (cria user/password).

### B) Dentro do Ubuntu (primeira vez)

```bash
bash /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/setup-nixaios-wsl.sh
# PowerShell: wsl --shutdown  → reabre Ubuntu
```

### C) Build da ISO

```bash
bash ~/src/neural-os-core-latest/nixaios/scripts/build-iso-wsl.sh
# equivalente:
cd ~/src/neural-os-core-latest/nixaios && nix build .#iso -L
```

Artefacto: `nixaios/result/iso/*.iso` (volumeID `NIXAIOS`, `nosd` embutido).

## Path 1b — Boot ISO no QEMU (dentro do WSL, sem reiniciar Windows)

```bash
# Uma vez (aceleração):
sudo usermod -aG kvm $USER && newgrp kvm

bash /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/run-iso-prep-and-boot.sh
# serial na consola; sair: Ctrl-A X
# log: /tmp/nixaios-qemu.log
```

ISO default: `~/Downloads/nixaios-minimal-*.iso`. QEMU PASS ≠ metal PASS.

## Path 1c — Metal USB (honesty)

```bash
bash /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/metal-iso-deep-preflight.sh
# DEEP_PREFLIGHT_OK = copytoram + configs; boot real = AWAITING_HW
# Windows handoff (não grava):
#   powershell -File nixaios\scripts\metal-usb-handoff.ps1
```

1. Rufus → **Imagem DD** (não ISO).
2. UEFI, Secure Boot OFF se falhar.
3. Emergency → **Copy ISO Files to RAM** (`copytoram`).
4. Aceite metal = login + `nosd` up; CUDA = prova à parte (`smoke-cuda-honesty.sh`).

## GGUF / llama.cpp (Onda 2)

```bash
# Preferir llama-completion (Nix llama-cpp≥0.6: llama-cli é wrapper de server)
export NOSD_LLAMA_CLI=$(nix-shell -p llama-cpp --run 'command -v llama-completion')
export NOSD_LLAMA_NGL=0
# .gguf em $DATA/models/
```

Sem CLI + `.gguf` → `cpu-echo` honesto.

### Smoke cognitive-loop (Onda 2) — OBSERVED WSL

```bash
bash /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/smoke-cognitive-loop.sh
# PASS = engine=llama-gguf + tokens reais + remember + recall
# modelo: ~/nosd-cognitive-data/models/stories15M-q4_0.gguf (~19MB)
```

### Smoke frugal-fleet (Onda 2)

```bash
bash /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/smoke-frugal-fleet.sh
# ou: cargo test -p nos-wasm --release -- fleet_ fuel_trap
# PASS = 32+128 identity + fuel trap; heap budget 256KiB×N (limite, não RSS)
```

### nix.propose (CapGate NIX_WRITE)

```bash
# sem bit NIX_WRITE → deny; com bit → escreve staging, hitl_required=true, nunca switch
nosctl --tcp 127.0.0.1:7700 nix-propose '{ ... }: { }'
# ficheiro: $DATA/staging-ai-managed.nix
```

## Setup (manual)

1. WSL2 Ubuntu 24.04 com systemd:
   ```ini
   # /etc/wsl.conf
   [boot]
   systemd=true
   ```
2. Preferir clone em filesystem Linux (`~/src/neural-os-core-latest`), **não** `/mnt/c` (I/O).
3. Rust toolchain estável + Nix Determinate installer para `flake.nix`.

## Smoke sem Nix

```bash
cd nixaios/packages/nosd
cargo test --workspace
cargo run -p nosd -- --foreground --socket /tmp/nosd.sock --data-dir /tmp/nosd-data
# outro terminal:
cargo run -p nosd --bin nosctl -- --socket /tmp/nosd.sock health
```

## Smoke com Nix

```bash
cd nixaios
nix develop
nix build .#nosd
```

## GPU / CUDA (honesty ADR-0113)

```bash
bash /mnt/c/DEV/neural-os-core-latest/nixaios/scripts/smoke-cuda-honesty.sh
# Host RTX: nvidia-smi OBSERVED; nixpkgs llama-cpp → RESULT: CUDA_ENGINE_ABSENT
# Opt-in build (pesado/unfree):
#   cd nixaios && nix build .#llama-cpp-cuda
#   export NOSD_LLAMA_CLI=$PWD/result/bin/llama-completion NOSD_LLAMA_NGL=99
# VERIFIED só se o smoke reportar CUDA_VERIFIED (offload real), não só nvidia-smi.
```

- `nosd` capacity: `posture=nvidia_smi` + `suggest_ngl=N` quando `nvidia-smi` responde; default `NOSD_LLAMA_NGL=0`.
- ROCm / Level Zero no WSL: UNKNOWN / fraco.
- Cage DRM kiosk: não equivalente a WSLg — validar em VM NixOS.

## Mesh lab

Usar hub L2 do monorepo (`tools/qemu_l2_hub.py`) ou dois processos `nosd` com `NOSD_MESH_PORT` distintos na mesma máquina WSL.
