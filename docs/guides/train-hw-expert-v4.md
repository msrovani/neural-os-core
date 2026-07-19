# Guia — Treinar HW Expert v4

## Goal

Classificador tipado HWID → `HwCapabilityCard` (não free-text).

## Commands

```powershell
python tools/train_hw_expert_v4.py --dry-run
python tools/train_hw_expert_v4.py --epochs 50 --hidden 128
python tools/sim_load_model_hwexpert.py   # se aplicável ao artefato
```

Expandir dataset com SDIO + pci.ids + labels comunitários (ver guia SDIO).

## Vocab

Alinhar `FAMILY`/`FW` com ath10k / `qca6174` (seed legado pode ainda listar ath9k).

## Export

`.bitnet` → FAT `HWEXPRT.BIN` / QEMU loader. Publicar candidato HF só após review.

## Honesty

Card roteia (`NEEDS_FW`, `load_firmware`) — **nunca** marca Ready de rádio/GPU.
