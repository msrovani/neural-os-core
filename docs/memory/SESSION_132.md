# SESSION_132 — NeuralFS residuals + USB teste + exFAT boot dados

**Data:** 2026-07-16  
**Objetivo:** Fechar residuals NeuralFS (multi-nivel B-tree); montar/formatar NeuralFS em pendrive USB no ciclo dev/debug; volume de dados de boot em exFAT.

## Entregue

### B-tree multi-nivel
- `cow_leaf_for_key` caminha path completo (nao so root→folha)
- `insert_separator` + `split_internal_into` — parent cheio promove e cresce altura
- Smoke `smoke_multilevel` (200 keys) alem de `smoke_split`
- Remove `Err("parent full")` como teto duro de 2 niveis

### USB-MSC pendrive de teste
- Bootstrap ordem: **ATA → USB → RAM**
- MBR `0x7F`: mount / format in-place / cauda livre / stick virgem (cria MBR)
- `BlockDevice::total_sectors` em ATA + USB (`max_lba+1`)
- `fat32::read_mbr_dev` generico

### exFAT boot (volume de dados)
- `tools/mkexfat.py` + `build_image.py` default exFAT (`--fat32` legado)
- `read_file_from_dev` prefere exFAT (VBR `EXFAT   `), fallback FAT32
- Fix `parse_vbr`: VolumeLength @ offset **72** (spec Microsoft; antes lia @56)

## Residuais honestos
- Particao GPT dedicada NeuralFS (ainda MBR `0x7F`)
- Power-loss e2e (kill mid-write + replay) em HW
- `build_usb_unified.py` ainda FAT32 na particao de dados (ESP UEFI permanece FAT)

## Check
- `cargo check --release -p neural-kernel` → 0 erros
- `cargo check --release -p k-nano` → 0 erros (btree/volume/tests/exfat sync)

## Relacao
- Reabre residual SESSION_125 so no eixo **teste pendrive** (nao writes alien NTFS/EXT)
- NeuralFS.md lifecycle → `fazendo`
