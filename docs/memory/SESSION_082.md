# SESSION_082 — Sprint 84: GPU Foundations (BAR mapping + Job Ring + VRAM Buddy + Secure Boot)

**Data:** 2026-07-06 | **Sprint:** 84 — Bloco 21c | **v0.84.0-design**

## Objective
Implementar 4 itens do Bloco 21c (GPU Foundations):
1. #326 GPU BAR0/BAR1 mapping UC ✅
2. #327 GPU doorbell + SPSC job ring ✅
3. #328 VRAM buddy allocator ✅
4. #352 Secure Boot GPU (ACR/PSP/GuC) ✅

## Created Files
- **`gpu/ring.rs`** (+135 LOC) — SPSC job ring genérico: `GpuJobRing`, `GpuJob`, doorbells para Intel/NVIDIA/AMD/VirtIO, push/ring_doorbell/poll_head/submit_and_wait.
- **`gpu/firmware.rs`** (+95 LOC) — Secure boot GPU: `SecureBootResult`, `FirmwareBlob`, `nvidia_acr_load()`, `amd_psp_load()`, `intel_guc_load()`, `secure_boot_gpu()`.

## Modified Files
- **`gpu/vram.rs`** — Upgrade para buddy allocator power-of-2 (4KB-4GB). Substitui first-fit BTreeMap. `VramBuddy` struct com splitting/merging. `vram_alloc()`/`vram_free()`/`vram_status()` mantidos.
- **`gpu/backend.rs`** — `init_backend()` expandido: BAR UC mapping → validate → job ring → secure boot → vendor init.
- **`gpu/mod.rs`** — `pub mod ring` + `pub mod firmware`.
- **`memory_agent.rs`** — `VRAM_STATE` → `VRAM_BUDLY`.

## Test Results
| Platform | Result | Key Logs |
|----------|--------|----------|
| QEMU -smp 2 WHPX | ✅ OK | BAR0 UC, SECURE-BOOT (no firmware), CPU fallback |
| VirtualBox 1 CPU | ✅ OK | Hermes Chat, GPU-BACKEND, no panic |

## Total LOC (Sprint 84)
| Item | LOC | Status |
|------|-----|--------|
| #326 GPU BAR0/BAR1 mapping UC | ~300 | ✅ |
| #327 GPU doorbell + SPSC job ring | ~135 | ✅ |
| #328 VRAM buddy allocator | ~150 | ✅ |
| #352 Secure Boot GPU | ~95 | ✅ |
| **Total** | **~680** | ✅ |

## Próximos Passos
- Sprint 85 (Bloco 21d): GPU Decode — Agent.xpu split, matmul ternário, KV cache DMA, XQueue
- Ou B-01 (RX fix) em HW real
