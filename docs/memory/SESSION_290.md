# SESSION_290 — Falcon3 3B/7B auto + piso 8GB

**Data:** 2026-08-25  
**Sprint:** v1.9.99-s290  
**HITL:** aceite metal **não** neste passo (aguardar teste real).

## Decisão

- **Daily:** Falcon3-3B residente (`FALCON3B.BIN` / slot Active).
- **Pro:** Falcon3-7B residente (`PRO.v6`) só se RAM ≥ **16GB** (FullPack).
- **Piso de produto:** **8GB**. Abaixo: sobe degradado (ainda tenta 3B se couber); log warn; não é “não boota”.
- **Teto de política:** nenhum. Quanto mais RAM, maior `heap_budget_mb` (75% − 2GB reserva). Bitmap PMM **64GiB**.
- **Full-auto:** `cortex::model_fit::llm_boot_plan(TOTAL_RAM_MB)` no FAT hub + ATA/USB walk; skip GeneratorPro fora FullPack; PIO cap = `max_resident_mb`.

## Código

- `k_nano::memory`: `RAM_FLOOR_MB=8192`, `RAM_FULL_PACK_MB=16384`, `BITMAP_SIZE=2MiB`, PMM no **static** (`with_pmm`), `heap_budget_mb` sem cap 1536.
- NUMA: `Box<BitmapFrameAllocator>` (4MB não na stack).
- `neural-kernel` boot: `set_heap_budget_mb(heap_budget_mb(ram))`.
- Arena T+0: 12.5% RAM, 1GB se FullPack.

## Residual

- QEMU 6G < piso → Daily/degraded esperado; 16G+ metal = FullPack.
- AirLLM GGUF ≠ streaming Falcon v6.
