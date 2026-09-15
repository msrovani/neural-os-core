# SESSION_350 — Heap AIOS (Observe→Plan→Act→Verify→Remember)

**Sprint:** v1.9.99-s350 · **Bloco:** cortex / k_nano allocator · **Data:** 2026-09-15

## Goal

Tratar pressão de heap / OOM como loop AIOS dinâmico (premissa ADR-0088), sem abandonar o piso fail-closed da SESSION_349.

## Feito

### Observe (`k_nano::allocator`)
- `heap_observe()` / `heap_headroom_bytes()` / `can_alloc_bytes`
- `note_alloc_refused` no grow refuse — **só atomics** (não aloca no caminho quente)
- Tópicos: `HEAP_PRESSURE`, `ALLOC_REFUSED`
- `publish_heap_pressure_if_due()` — EventBus **fora** do grow

### Plan + Act (`cortex::heap_aios` + InferQueue)
- `plan_for(base_tier, …)` lê headroom real (`window−used`)
- Degrade: Cheap/stride↑/ctx↓/force_slim; Escalate: HITL msg sem forward 4GB
- `apply_plan` → `difficulty_gate` + ctx cap
- `run_prefill_setup` aplica plano **antes** de máscara/KV
- `Tensor::new` recusa pró-ativo se headroom insuficiente

### Verify + Remember
- `verify_job` no `finish_job` (completed ∧ pressure&lt;2)
- Remember seam → hermes `install_heap_aios_remember` → `put_hanr("heap_aios")` + `remember_fact`

## Honesty
- `alloc_error_handler` ainda é `!` — AIOS evita chegar lá; não “recupera” de `vec!` falho
- Stamp `cortex_llm` no OOM antigo pode ser submit+AP; InferWorker continua BSP slice

## Aceite
- WHPX Window+FAT: chat sob pressão deve logar `HeapAIOS plan=degrade|escalate` sem `OOM/TALC` hlt
- SGDB: chave `heap_aios` após degrade

## Próximo
- HUD honesto (headroom vs “ample RAM”)
- InferWorker `tick_in_progress` próprio
- Ler `heap_aios` no boot para seed do tier
