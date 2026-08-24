# SESSION_287 — Budget Cap Heap + QEMU 4-Core Display

## Data: 2026-08-23

## Contexto
Sessao de boot QEMU matrix test apos conclusao dos fixes P0-P4 do k_ai/cortex.
Objetivo: eliminar OOM do heap auto-grow e validar boot com 4 cores + display.

## Fix: Budget Cap no Heap Auto-Grow
- **Arquivo:** `crates/k_nano/src/allocator.rs`
  - `HEAP_BUDGET_MB: AtomicUsize` (default 1536)
  - `set_heap_budget_mb(mb)` — chamado de main.rs
  - `grow_bump_auto()` — retorna false quando `current_limit >= budget_bytes`
- **Arquivo:** `crates/neural-kernel/src/main.rs`
  - `k_nano::allocator::set_heap_budget_mb(heap_initial_mb)` antes do log

### Resultado
| Metrica | Antes | Depois |
|---------|-------|--------|
| Heap auto-grow | 512->2048->2304->2453MB | 512->768MB (cap ok) |
| OOM crash | SIM | NAO (eliminado) |
| Boot phases | 6/8 | 7/8 (chegou ao LLM load) |
| HW Expert v6 | Nao carregou | 590KB loaded |

## Fix: virtio_gpu stale reference
- `hermes/src/agents.rs:2967` — `k_nano::virtio_gpu::init_driver_virtio_gpu()` removido
- Substituido por log de status (GPU detect ja rodou em k_hal Phase 5)

## Fix: Falcon3 converter
- `tools/convert_falcon3_bitnet.py` — lm_head fallback + per-projection weight_scale

## QEMU Boot Results

### TCG (6G, 4 cores, display ON, 5min)
- 902 linhas, 7/8 phases
- TTS: "JARBAS online and ready — 7168MB RAM, 4 CPU cores"
- HW Expert v6 loaded (590KB)
- Display: FB 1280x800, P4 JARBAS FB OK
- ATA: TCG skip intencional (boot_observe.rs:60)
- LLM: resize_heap 768->1795MB em progresso (ATA PIO lento)

### WHPX (4G, 2 cores)
- ATA detectado, FAT32 montado
- FAT walk hang (PIO lento de 3GB)

### 8G (4 cores)
- Limine carrega kernel mas nao inicia (sem serial output)
- Possivel: HHDM offset com 8G excede layout

## Commits
| Hash | O que |
|------|-------|
| `7e2c52e` | fix(converter): Falcon3 lm_head fallback + weight_scale |
| `645e2ee` | docs: SESSION_286 |

## Lições registradas em AGENTS.md
1. grow_bump_auto sem budget cap = OOM
2. QEMU 8G falha com Limine
3. ATA TCG skip intencional
4. virtio_gpu stale reference
