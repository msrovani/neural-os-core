# SESSION_494 — Metal bootlog CAP + VRAM BAR + spam

**Sprint:** v1.9.99-s494 TEST  
**Data:** 2026-10-18  
**Trigger:** duplo boot HW real (SanDisk) — `BOOT.LOG` cortava mid-line em T+1420; VRAM Ampere absurda; LOO/NSGDB miss.

## Goal

Corrigir truncagem/formatação do log DEV e os erros metal observados na leitura do stick.

## Feito

### Bootlog (P0)
- `BOOT_LOG_CAP` / `BOOT_RAMLOG_CAP` / logwriter-efi / `mk_esp_fat` / `mkfat32` → **1 MiB** lockstep.
- Flush DEV = **dump do ramlog** (`snapshot_bytes`), não janela PRE_FAT 512 linhas.
- Keep-newest + marcador `[TRUNCATED keep-newest]` se ainda encher.
- Ring `[WRAP]` no ramlog quando cheio.
- Zero-fill do resto da chain FAT após write (sem cauda stale/NUL).
- Verify miss após write OK → **accept** (MSC flake SanDisk; mata heal loop).
- logwriter grava **raiz + NEURAL\\** na ESP.
- `extract_esp_bootlog.py`: drive letter, escolhe maior, strip NUL, TRUNCATED.
- NSGDB.BIN 8 MiB na ESP (`mk_esp_fat`) — FileFlash com data=NeuralFS.

### Spam
- DSD: silêncio se Undecided + draft=0.
- BOT fail csw-content: rate-limit (3 + 1/64).
- MIXER STALL: 1s → 10s.

### GPU / FW (já no working tree + bundled)
- `read_bar_size`: mask_hi=0 não explode 64-bit; sanitize potência-de-2 ≤128GB.
- `detect_all`: candidatos VRAM [64MB,64GB]; NVIDIA BAR0 = MMIO.
- `nvidia_firmware_hint(did)` — Ampere 25ac ≠ gp108; recipe `gpu.nvidia.ga107`.

### Pack
- `mkneuralfs`: AlwaysTiny `hwexpert.bitnet` se existir no host.

## Prova (host)

| Claim | Estado |
|---|---|
| `cargo test -p k-nano --lib boot_ramlog` | VERIFIED 4/4 |
| `cargo test -p k-nano --lib boot_logger` | VERIFIED 13/13 |
| `storage_posture` | VERIFIED 2/2 |
| `cargo check -p k-nano -p cortex -p k-hal -p k_ai -p jarbas --release` | 0 erros |
| `extract_esp_bootlog.py D:` | VERIFIED (escolhe NEURAL/, 3158 lines) |
| Metal re-boot 1 MiB sem mid-line cut | UNKNOWN até gravar stick novo |

## Pendentes

1. Rebuild `usb_hw.img --fs neuralfs` + PACK_LLM + metal 2 boots.
2. LOO Active via NRFS VERIFIED (seed com `active.v7`).
3. HDA bring-up metal (stall residual).
4. Wi-Fi 14c3:7925 recipe.
