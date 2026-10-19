# SESSION_497 — Metal dual-boot: HW-GATE honesty + modelo miss

**Data:** 2026-10-18  
**Evidência:** `logs/BOOT_esp_now.txt` (ESP BOOT.LOG metal); relato maintainer (orb lento, sem modelo).

## Confirmado em HW real (`qemu=false`)

| Claim | Evidência | Taxonomia |
|-------|-----------|-----------|
| Limine + MemoryCore smoke | `BOOT_011_SMOKE` CLOSED; CapToken BootSmokeOk | VERIFIED |
| USB-MSC root live | `MSC installed via k_hal::usb`; storage plan usb-msc; BOT retry | VERIFIED (root) |
| NeuralFS data mount | `data_fs=neuralfs` LBA=262144 `nrfs_mounted=true` | VERIFIED (mount) |
| UI / orb | `Desktop limpo — orb + HUD` T+66; 2560×1600 | VERIFIED |
| SMP | `smp_online=16` `ram_mb=18680` BOOT SCORE | VERIFIED |
| Runtime estável | ticks → T+1400+ sem SILENCE/panic no trecho | OBSERVED |

## Negado / ainda AWAITING (não fechar gate)

| Gate / frente | Evidência metal | Status honesto |
|---------------|-----------------|----------------|
| ATH10K_A5_SCAN / ASSOC | `WIFI-HW SKIP no_WifiAssociated` | AWAITING_REAL_HW |
| GPU_CANARY / GPU_GSP | `GPU-HW bar0_validate_failed`; gsp scaffold SKIP | AWAITING_REAL_HW |
| I225_E2E | sem prova igc | AWAITING_REAL_HW |
| USB_HUB (TT/route) | `hub=ADDR SKIP no_child_ccs` | **PARTIAL** se MSC live; CLOSED só com hub_child |
| NET-HW | `rx_count_zero_at_runtime` | AWAITING |
| UAC-HW | `ep0_get_descriptor_incomplete` | AWAITING |
| LLM / hub Active | NeuralFS miss → FAT fallback; `ModelHub active=0`; `llm=ABSENT` | NEGADO neste boot (seed/path) |

## Causa “sem modelo” (este log)

1. Volume **NeuralFS** montou mas LOO `active` / AlwaysTiny **miss** (`path/validate/pio`).
2. Fallback FAT DEGRADED sem partição 0x0B/0C no mesmo stick → também miss.
3. Código pré-fix: `falcon3_boot_names()` só `.V6` enquanto pack canónico é `.V7`; hub LOO só ATA (não USB-MSC).

Fixes de código (mesma sessão / follow-up): `.V7` first em `falcon3_boot_names`; LOO USB-MSC em `try_hub_slot_fat`; HW-GATE `PARTIAL` para USB_HUB quando MSC live.

## Código HW-GATE (s497)

- `gate_status("USB_HUB")` → CLOSED (hub) | PARTIAL (MSC live) | AWAITING.
- Textos `need=` citam evidência metal s497 (WiFi/GPU/i225).
- Emit sev: CLOSED/PARTIAL/AWAITING visíveis (`ok`/`warn`, não Trace).
- `emit_all_refresh` pós-fleet continua atualizando PARTIAL após MSC.

## Pendente

- Regravar `usb_hw.img` (FAT32 12G + kernel com `.V7`/USB LOO) e re-extrair BOOT.LOG.
- NeuralFS seed com blobs reais **ou** FAT dados com FALCON3.V7 legível no metal.
- Fechar ATH10K/GPU/i225 só com VERDICT medido na mesma máquina.
