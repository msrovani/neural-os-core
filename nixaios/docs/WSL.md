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

## GPU

- NVIDIA CUDA no WSL: só marcar VERIFIED se `nvidia-smi` listar a GPU e um forward GGUF correr.
- ROCm / Level Zero no WSL: UNKNOWN / fraco.
- Cage DRM kiosk: não equivalente a WSLg — validar em VM NixOS.

## Mesh lab

Usar hub L2 do monorepo (`tools/qemu_l2_hub.py`) ou dois processos `nosd` com `NOSD_MESH_PORT` distintos na mesma máquina WSL.
