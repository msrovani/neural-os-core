# SESSION_133 — Fechar residuals NeuralFS implementáveis

**Data:** 2026-07-17  
**Objetivo:** Fechar (além de testes) trava USB, GPT NeuralFS, unified→exFAT e conformidade mínima exFAT.

## Entregue

### 1) Trava de formatação USB (CRÍTICO)
- Mount de NeuralFS existente em USB: **sempre** permitido
- Format (in-place / cauda / virgin): **opt-in**
  - `debug_assertions` → liberado (ciclo QEMU/dev)
  - release: requer `NEURALFS_USB_FORMAT=1` em `CONFIG.TXT` (exFAT no stick) **ou** `neural_fs::neural_fs_agent::allow_usb_format(true)`
- Default em `CONFIG.TXT` gerado: `NEURALFS_USB_FORMAT=0`

### 2) GPT dedicada NeuralFS
- GUID estável `GPT_TYPE_NEURALFS` = UUID `4E455552-414C-4653-2D4E-465320000001`
- `fat32::parse_gpt_partitions` mapeia GUID → type `0x7F`
- Disco vazio (sem FAT/ESP/NeuralFS): `gpt_format_single` + format NeuralFS em LBA 2048
- Mount via MBR `0x7F` **ou** GPT GUID

### 3) USB unificado → dados exFAT
- `build_usb_unified.py` default: ESP FAT + dados **exFAT** (MBR type `0x07`)
- `--fat32` legado
- `patch_exfat_vbr` atualiza PartitionOffset/VolumeLength + boot checksum

### 4) exFAT conformidade mínima (`mkexfat.py`)
- Boot region 11 setores + checksum Microsoft + backup @+12
- Bitmap dimensionado ao `cluster_count`
- Up-case identity 128 KiB
- Root: entradas `0x83` (label), `0x81` (bitmap), `0x82` (upcase)
- NameLength UTF-16 correto nos File entries

## Check
- `cargo check --release -p neural-kernel` → 0 erros
- `cargo check --release -p k-nano` → 0 erros (agent USB fica no bin; k_nano espelho gpt)

## Ainda aberto (só com evidência)
- Power-loss fault injection e2e
- Stress B-tree com milhares de keys (comprovar level≥2)
- Interop Windows/Linux mount da imagem exFAT (validação externa)

## Relacao
- Fecha residuals SESSION_132 itens 1–4 (implementáveis)
- NeuralFS.md lifecycle permanece `fazendo` até power-loss/stress
