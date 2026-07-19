# Guia — Firmware download e FAT

## Goal

Sincronizar blobs redistribuíveis do linux-firmware → `firmware/` lab → short names no disco FAT, com `blob_hash` para DeviceRecipe.

## Pipeline

```text
gitlab.com/kernel-firmware/linux-firmware
  → python tools/download_firmware.py [--pull] [--list]
firmware/          (git; SEM GSP)
target/firmware/   (GSP pinado local)
  → FW_FAT_CHIPS=... + python tools/build_image.py
FAT: AT10K_*.BIN, FW_FECS_*, …
```

## Commands

```powershell
python tools/download_firmware.py --list
python tools/download_firmware.py --pull
$env:FW_FAT_CHIPS="gp108,ath10k"
python tools/build_image.py
```

## Incluir vs não

| Incluir em `firmware/` | Não versionar |
|------------------------|---------------|
| ath10k QCA6174 hw3.0 | NVIDIA GSP |
| NVIDIA gp108 GR/ACR | Catálogo inteiro |
| AMD/Intel lab subsets | `.sys` / DriverPacks |

## Short names (exemplos)

| Path lab | FAT | role |
|----------|-----|------|
| ath10k/.../firmware-6.bin | `AT10K_F6.BIN` | FW_IMAGE |
| ath10k/.../board-2.bin | `AT10K_B2.BIN` | BOARD |
| nvidia/gp108/fecs_* | `FW_FECS_*` | ACR/GR |

## blob_hash

Obrigatório na recipe L2 ativa. Calcular após download e colar no YAML. Mismatch → Deny.

## PR checklist

- [ ] WHENCE-ok
- [ ] Sem GSP no git
- [ ] Short name ↔ path documentado
- [ ] `blob_hash` anexado
