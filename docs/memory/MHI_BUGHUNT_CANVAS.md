# MHI Bughunt — Canvas (lane fixer)

Data: 2026-09-21 · Escopo: vram, memory_agent, disk cache, PMM, fat32, msched,
global_arena, stubs mhi_scheduler. Fora do lane: `allocator.rs`, `mhi.rs`
(outros fixers donos — só comentários/follow-ups apontando para eles).

## Tabela HIGH / MED / LOW

| Sev | Arquivo:linha | Sintoma | Correção | Status |
|-----|---------------|---------|----------|--------|
| HIGH | `crates/k_hal/src/gpu/vram.rs:122` | `total_allocated += req` (pedido, não bloco real pós-split) — contabilidade menor que o alocado | `+= 1u64 << o` | ✅ feito |
| HIGH | `crates/k_hal/src/gpu/vram.rs:133` | `free` subtraía `req` (assimétrico com o alloc) | `saturating_sub(1u64 << o)` | ✅ feito |
| HIGH | `crates/k_hal/src/gpu/vram.rs:150` | `idx=(o-MIN_ORDER)` OOB se merge leva `o` a 33 (NUM_ORDERS=21) → panic no free | `min((o-MIN_ORDER), NUM_ORDERS-1)` + `break` se `o>MAX_ORDER` mantido | ✅ feito |
| MED | `crates/neural-kernel/src/memory_agent.rs:215` | `resize_heap_to_mb` mirava pool errada (bump vs TALC) | Linha deletada; auto-grow (`grow_bump_auto`) é o dono do sizing; `heap_target_mb` vira telemetria | ✅ feito |
| LOW | `crates/k_nano/src/disk_agent/cache.rs:55,62,87` | Write-back morto: `mark_dirty`/`tick`/`write_coalesce_ms` sem callers (CachedDisk é write-through, nunca dirty); evict logava "DATA LOSS RISK" inalcançável | Deletados `mark_dirty`, `tick`, campo `write_coalesce_ms`, campos `dirty`/`last_write`; `evict_one` só LFU; `stats()` mantém assinatura (dirty=0) | ✅ feito |
| LOW | `crates/k_nano/src/memory.rs:122` | `next_free_bit = 256` mágico | `const LOWMEM_RESERVE_FRAMES: usize = 256` (IVT/BDA/EBDA + trampoline SMP) | ✅ feito |
| LOW | `crates/k_nano/src/memory.rs:heap_budget_mb/heap_piso_mb` | Dois pisos literais divergentes (128/256 vs 256/512) | `HEAP_FLOOR_SMALL_MB`/`HEAP_FLOOR_MB` compartilhados; valores preservados, invariante `piso >= floor` documentada | ✅ feito |
| LOW | `crates/k_nano/src/fat32.rs:312-328` | `register(PhysAddr::new(lba*512))` — chave de disco impressa no summary MHI `@{:x}` como se fosse RAM | Comentário no site (chave=lba, não RAM); prefixo `lba:` no `summary` é follow-up do lane dono de `mhi.rs` | ✅ parcial (follow-up mhi.rs) |
| LOW | `crates/k_hal/src/gpu/vram.rs:260` + `msched.rs:26` | `msched_predict→unwrap_or(0)`: 0 ambíguo como vítima Belady; `unwrap_or(MAX)` no msched parecia fallback | `msched_predict` retorna `Option<u64>` (zero callers → sem break); `MAX` documentado como intencional (nunca-vista = vítima ideal) | ✅ feito |
| LOW | `crates/cortex/src/global_arena.rs:44` | `record_access(virt_base)` morto (VirtMapped nunca promove; mhi_tick não faz memcpy de VA) | Chamada deletada + comentário anti-regressão | ✅ feito |
| LOW | `crates/k_ai/src/fs/mhi_scheduler.rs` + `crates/hermes/src/fs/mhi_scheduler.rs` | 2 stubs idênticos (`mhi_scheduler_tick` vazio, zero callers) | Viraram facade de 1 linha: `pub use k_nano::mhi::mhi_tick;` | ✅ feito |

## Simplificações (deletado, não abstraído)

- Write-back da ARC cache: deletado, não "gated por feature" (bare-metal sem
  bateria nunca terá dirty — lição F16 SESSION_252).
- `resize_heap_to_mb` no MemoryAgent: deletado, não trocado por outro resize
  (um dono de sizing: auto-grow).
- `record_access` da arena: deletado, não gated (telemetria que ninguém consome
  é ruído, não dado).
- Stubs MHI: facade `pub use`, não trait/adapter novo (1 linha > 7 linhas mortas).
- Pisos de heap: consts compartilhadas, não config nem enum novo.

## Updates externos — NÃO subir (decisão librarian)

- **talc 4.4.3 → 5.x: NÃO.** Breaking (API do allocator muda); registrado como
  comentário-política em `crates/k_nano/src/memory.rs` (`heap_budget_mb`).
- **x86_64 0.14 → 0.15: NÃO.** Breaking (requer nightly posterior ao pin);
  mesmo registro.
- **Memtype guard anti-aliasing: follow-up**, fora deste lane.

## Premissas de trabalho (resumo)

P0: cérebro MHI único = `k_nano::mhi::mhi_tick` (sem política duplicada).
P1: bare-metal sem bateria → write-through, nunca dirty.
P2: um dono por sizing (heap = auto-grow).
P3: contabilidade buddy em blocos reais (`1<<o`), não em pedido.
P4: nenhum índice sem clamp (buddy OOB = panic em free).
P5: chave Block (lba) ≠ endereço RAM — nunca memcpy de LBA como PA.
P6: VirtMapped nunca promove (mhi_tick não copia VA).
P7: Belady: nunca-vista = vítima ideal (MAX intencional); predição ausente =
`None`, nunca 0 mágico.
P8: lowmem <1MB reservada (PMM nunca entrega frames 0..255 ao geral).
P9: piso único documentado (SMALL <1.5GB, FULL demais).
P10: sem std, sem `unwrap` novo, sem API breaking amplo.
P11: lanes alheios (`allocator.rs`, `mhi.rs`) = só follow-ups, nunca edit.

Mandatórias M1–M10: M1 medir RAM, não hardcodar · M2 honest noop sem device ·
M3 log com sev visível (`ok`/`warn`, não `trace` p/ diagnóstico) · M4 menor diff
vence · M5 facade > duplicata · M6 deletar > gatear código morto ·
M7 const nomeada > literal mágico · M8 simetria alloc/free ·
M9 `Option` > sentinela mágica · M10 validar com `cargo check` dos crates tocados.

## Validação

`cargo check` dos crates tocados (k_nano, k_hal, cortex, k_ai, hermes,
neural-kernel) — ver relatório do fixer.
