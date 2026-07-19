# SESSION_161 — ath10k A3 BMI → fw_ready (Note 1050)

**Data:** 2026-07-18  
**HW alvo:** Note 1050 QCA6174 `168C:003E`

---

## Goal

A3: wake → `FW_IND_INITIALIZED` (ROM) → CE0/CE1 BMI → LZ download `FW_IMAGE` → `BMI_DONE` → poll `fw_ready`.  
**PASS** só com `FW_IND_INITIALIZED` **após** BMI_DONE (não claim falso).

## Código

| Módulo | Papel |
|--------|-------|
| `ath10k_fw.rs` | FAT + parse IE (`FW_IMAGE` 681821 B, OTP 24429 B) |
| `ath10k_ce_bmi.rs` | CE0/CE1 + BMI exchange / LZ / write_mem |
| `wifi_ath10k.rs` | Orquestração A3 + `VERDICT=PASS\|PARTIAL\|FAIL` |

Constantes QCA6174 (Linux): `patch_load_addr=0x1234`, `fw_indicator=0x3a028`, CE0=`0x34400`.

## Serial esperado (Note)

```
[ATH10K] step=wake status=OK
[ATH10K] step=target_init status=OK
[ATH10K] fw_fat fw=FOUND …
[ATH10K] fw_ie image=681821 otp=24429
[ATH10K] step=ce status=OK
[ATH10K] step=bmi_target version=…
[ATH10K] step=bmi_lz …
[ATH10K] step=bmi_done status=OK
[ATH10K] step=fw_ready status=OK   → VERDICT=PASS fw_ready=1
```

Ou `VERDICT=PARTIAL reason=…` se BMI/CE falhar (honesto).

## Pré-requisito Note

1. `python tools/build_image.py` (embebe `AT10K_*.BIN`)
2. Boot bare-metal no Note com serial

## Residuais

- Board addr `0x004007d4` heurístico (hi_board_data)
- CE ring single-slot simplificado (vs Linux multi)
- Scan/assoc (A4) não iniciado
- QEMU sem radio — A3 só valida no Note

## Aceite compile

`cargo check -p k-hal` / hermes 0 erros. Aceite runtime = log Note.
