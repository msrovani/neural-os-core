# SESSION_146 — Onda 5: GPU / MHI DMA scaffolding + AWAITING_HW

**Data:** 2026-07-18  
**Pista:** GPU/#420/#423/#454–456 / #67  
**Check:** `cargo check --release` = 0 erros  
**PreFlight:** `python tools/preflight_wave.py --wave 5` EXIT=0

## Vereditos

| ID | Status | Nota |
|----|--------|------|
| #420 MHI DMA | ▶️ AWAITING_HW | Soft-migrate permanece; `[MHI-DMA] VERDICT=AWAITING_REAL_HW` em peer VRAM/NVMe |
| #67 AllocTier::Vram | ✅ wire parcial | `register_vram_allocator(vram_alloc)` apos `init_vram_tier` OK |
| #423 GDS | ▶️ AWAITING_HW | `direct_storage::probe_gds` → `[GDS-HW]` (sem P2P fake) |
| #454 NVIDIA | ▶️ AWAITING_HW | Degrau D2–D4 intacto; `[GPU-HW]` se canary ≠ Pass |
| #455 AMD | ▶️ AWAITING_HW | idem via backend canary |
| #456 Intel | ▶️ AWAITING_HW | idem |
| has_compute Ready | **nunca no QEMU** | só `VERDICT=PASS` apos canary vector_add real |

## Código

| Peça | Path |
|------|------|
| MHI-DMA + Vram hook | `crates/k_nano/src/mhi.rs` |
| GDS stub | `crates/k_hal/src/gpu/direct_storage.rs` |
| GPU-HW verdict | `crates/k_hal/src/gpu/backend.rs` `log_gpu_hw_verdict` |
| Vram→MHI | `neural-kernel/main.rs` pos `init_vram_tier` |

## Anti-fake

- VirtIO-GPU → CPU / Quarantine + AWAITING — **não** Ready.
- Soft-meta demote ≠ DMA bytes transferidos.
- Golden silício (#454–456) permanece gate HW real (`logs/boot_hw_*`).

## Próximo

Onda 6 (AirLLM ATA) ou bancada GPU real para promover canary → `has_compute`.
