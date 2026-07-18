# SESSION_148 — Onda 6 AirLLM + Onda 7 crônicos (fecho plano residuals)

**Data:** 2026-07-18  
**Plano:** Residuals + IDEA (ondas 0–7)  
**Check:** `cargo check --release` = 0 erros  
**PreFlight:** waves 0–7 + cache `docs/memory/.preflight_cache/`

## Ondas 0–5 (já entregues — reconfirmadas)

| Onda | SESSION | Status |
|------|---------|--------|
| 0 PreFlight + docs | 142–143 | ✅ `preflight_wave.py` + Gaps/crônicos→7 |
| 1 NeuralFS | 142 | ✅ smokes + `[NRFS-HW]` |
| 3 Write FS | 144 | ✅ `#417` EXFAT_WRITE; `#418` BLOCKED lan |
| 4 Sound/USB | 145 | ✅ USB Trust; `#84` UAC-HW AWAITING |
| 5 GPU/MHI | 146 | ✅ `[GPU-HW]`/`[MHI-DMA]`/`[GDS-HW]` |
| R soft-float | 147 | ✅ pesquisa defer |

## Onda 6 — AirLLM

| Subitem | Fecho |
|---------|--------|
| ATA `hot_swap_from_ata` + soft prefetch | ✅ MVP (já) |
| DMA peer / stream-to-disk / K-quants Q4_K / e2e 9B | ▶️ `[AIRLLM-DMA] VERDICT=AWAITING_REAL_HW` |
| Q4_0/Q8_0 dequant | ✅ parcial (não K-quants llama.cpp) |
| Net `/model-fetch` L3.5 | **BLOCKED** `depends_on: lan` (Onda 7) |

Código: `gguf_streaming::log_airllm_residuals`, `stream_to_disk_deferred`.

## Onda 7 — Crônicos

Ordem: LAN → fila `depends_on: lan` → WiFi.

| Área | Fecho |
|------|--------|
| LAN RX | `[NET-HW]` PASS se RX>0; senão `AWAITING_REAL_HW` (L3.5 + runtime) |
| #418 / TLS / fetch / HTTP update | PreFlight **force_blocked** até LAN |
| WiFi | `[WIFI-HW] VERDICT=AWAITING_REAL_HW` (detect fail ou scan demo ≠ RF) |

**Não** inventamos RX>0 no QEMU. Aceite Onda 7 scaffold = tags+logs+PreFlight; fix RX pleno = bancada/continuidade.

## Gate v2.0.0

Residuals 0–6 com AWAITING_HW honesto. Onda 7 pode permanecer aberta se maintainer defer “crônicos fora do gate” (plano §Gate).
