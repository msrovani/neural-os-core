# SESSION_160 — WiFi pivô ath10k QCA6174 (Note 1050)

**Data:** 2026-07-18  
**HW alvo:** Note 1050 — Qualcomm QCA61x4A ≈ **QCA6174** PCI **`168C:003E`**

---

## Goal

A0+A1+A2: DID correto, firmware/FAT, scaffold BMI/CE. **Sem** claim Ready/fw_ready (A3 = Note HW).

## Inventário firmware (repo)

`firmware/ath10k/QCA6174/hw3.0/` (linux-firmware GitLab):

| Arquivo | Bytes | FAT 8.3 |
|---------|------:|---------|
| `firmware-6.bin` | 706 360 | `AT10K_F6.BIN` |
| `board-2.bin` | 740 076 | `AT10K_B2.BIN` |
| `board.bin` | 8 124 | `AT10K_BD.BIN` |
| **Total** | **1 454 560** | |

## Código

| Peça | Onde |
|------|------|
| DID `003E`/`0041` → `Ath10kQca6174` | `k_hal/net/generic_wifi.rs` |
| Resolve + FAT | `k_hal/net/ath10k_fw.rs` |
| BMI/CE scaffold | `k_hal/net/wifi_ath10k.rs` |
| Short names | `tools/mkfat32.py` |
| Download specs | `tools/download_firmware.py` (`ath10k_specs`) |

iwlwifi prep (SESSION_159) = **secundário**.

## Aceite A0–A2

- `cargo check -p k-hal` / hermes 0 erros
- Blobs no repo + short FAT mapeados
- QEMU sem radio: `ATH10K fw_resolve=SKIP` (via `detect_wifi`)
- Zero Ready / Connected / fw_ready

## Próximo (A3 Note) — feito em SESSION_161 (código); runtime Note AWAITING

1. Rebuild `disk_*.raw` com `AT10K_*.BIN`
2. Boot Note → serial: `pci=168c:003e` + `fw_fat … FOUND`
3. Implementar BMI download → `fw_ready` **medido**
