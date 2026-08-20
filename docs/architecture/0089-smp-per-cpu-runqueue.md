# ADR-0089: Per-CPU Run-Queues para Agents — Distribuição SMP Cooperativa

**Status:** Proposed
**Date:** 2026-08-20
**Deciders:** Marcelo Scapin Rovani
**Relates to:** ADR-0055 (FeatureGate), ADR-0057 (Compute Dispatch), ADR-0065 (TSS/IST multi-AP)

## Contexto

O scheduler do Neural OS agenda todos os ~50 agents cooperativamente no BSP
(`AgentRegistry::run`). APs (3 cores em `-smp 4`) executam apenas jobs de
compute via `ap_work::ap_idle_loop` (SPSC mailbox) e work-stealing de matmul.
Não há distribuição de agents entre cores, não há IPI de reschedule funcional
para acordar APs com trabalho de agents, e não há telemetria por-core.

Comparado ao Redox OS (EEVDF per-CPU run-queues, work-stealing de contextos,
IPI de wakeup, custo de troca ~250ns), o Neural está ~2 gerações atrás em
utilização de multicore para agents.

## Decisão

Criar `k_nano::smp::runqueue` com:

### 1. Per-CPU Run-Ques para Agents
- Slot-based MPMC (padrão `ap_work.rs`): `SyncCell<[RqSlot; 128]>` + HEAD/TAIL
  atômicos + CAS. Sem heap, sem const-fn restriction, compatível com `static`.
- Cada core tem sua run-queue. BSP distribui, APs consomem.
- **Elegibilidade:** agents com `affinity_ring >= 1` (R1/R2) migram; ring 0
  (BSP/critical) nunca migra.
- **Coherence:** agents com `coherence_partner` ficam no mesmo core.

### 2. Reschedule-IPI Funcional
- `send_reschedule_ipi_to(lapic_id)`: envia IPI vetor 0x80 para AP específico.
- `wake_core_if_needed(core_id)`: verifica run-queue + AP_POLLABLE antes de IPI.
- Integrado ao `dispatch_to_core` existente no `spsc.rs`.

### 3. Work-Stealing de Agents
- `steal_agent(core_id)`: round-robin entre cores, rouba no máximo 1 task por
  cycle, só se vítima tem >1 task (evita starvation).
- Complementa o work-stealing existente de matmul (`work_stealing.rs`).

### 4. Telemetria por-CPU
- `CpuStats` (cache-line padded): running, blocked, stolen, enqueued.
- `cpu_stats(core_id)` expõe para MonitorAgent/HUD.

### 5. Load Balancing
- `resolve_target_core()`: BSP/critical→core 0; coherence→mesmo core;
  senão→core com menor fila.
- `distribute_batch()`: BSP distribui agents elegíveis antes do tick local.

## Feature Gate

Tudo atrás de `smp-runqueue` (default OFF). Quando OFF:
- O scheduler BSP-only roda inalterado.
- APs continuam em idle (hlt/mwait) sem agent work.
- Zero regressão em QEMU/TCG.

## Segurança

- **Deadlock-free:** APs só executam agents quando `ap_pollable() == true`.
  Default OFF previne deadlock em QEMU sem IDT/IPI pleno.
- **Starvation-free:** work-stealing rouba no máximo 1 task por cycle;
  rate-limiting existente do scheduler (consecutive_pending > 50) aplica.
- **Coherence:** `coherence_partner` preserva localidade de agents que
  comunicam entre si (reduz cache misses cross-core).

## Alternativas Consideradas

1. **Preemptivo com timer IPI:** Requer IDT compartilhada + reschedule-IPI
   por timer. Complexidade alta, risco de deadlock. Adiado para ADR-0065 Fase 4.
2. **MpmcQueue heap-allocada (sync::mpmc):** Requer heap, não é const-fn,
   não compatível com `static` arrays. Rejeitado.
3. **MpmcQueue caseira (mpmc.rs):** Exige `T: Copy + Default`, usa `Vec`.
   Funcional mas não adequado para static.

## Testes

12 testes host:
- `enqueue_dequeue_basic`, `run_queue_fifo_order`, `run_queue_fill_and_reject`
- `steal_from_busy_core`, `steal_respects_min_one`
- `total_pending_counts_all_queues`
- `resolve_target_core_ring0_is_bsp`, `resolve_target_core_coherence_partner`
- `cpu_stats_enqueued_increments`
- `distribute_batch_skips_ring0`
- `agent_task_score_ordering`, `agent_task_default_is_invalid`

## Validação QEMU

```powershell
# Build com feature gate
cargo build --release --features smp-runqueue -p neural-kernel

# Boot QEMU
.\run-qemu-whpx.ps1 -Window

# No boot log, procurar:
# [SMP] runqueue: 3 agents distribuidos para APs
# [SMP] AP 1: 2 agents executados (steal: 1)
```

## 6. Core Role Mapping (ADR-0057 CorePair + ADR-0089)

Mapeamento de papéis por core (configurável em boot):

| Core | Papel | Ring | Agents |
|------|-------|------|--------|
| 0 (BSP) | System | R0 | HwBridge, Input, Display, Cron, Security, Safety |
| 1 (AP1) | Compute | R1 | CortexAgent (LLM decode), matmul workers |
| 2 (AP2) | Worker | R2 | HermesAgent (orchestration), WASM sandbox |
| 3 (AP3) | Memory | R2 | NetAgent, WifiAgent, SGDB, SelfHeal |

Funções:
-  enum: System/Compute/Memory/Worker/Idle
-  / 
- : configura baseado em CorePools (r0→System, r1→Compute, r2→Worker)
- : distribui por papel + menor carga

## 7. Relatório de Uso de Cores

Ver  para o mapeamento completo.

Gargalos identificados:
1. **CortexAgent monopoliza BSP** (~60-95% do tempo em inference)
2. **WASM sandbox compete com agents críticos** no mesmo core
3. **SGDB sync compete com inference** por cache L2/L3
4. **APs subutilizados** (~10% em média, 90% em idle)

## Pendente (Futuro)

- **Objetivo 4 (Telemetria no HUD):** `MonitorAgent` lê `cpu_stats()` e
  expõe no HUD do Jarbas. Requer integração com `jarbas/display/gauges.rs`.
  Deixado como TODO — não bloqueia a feature.
- **Preemptivo (ADR-0065 Fase 4):** Timer IPI para preempção de agents.
  Requer IDT compartilhada + reschedule-IPI por timer. Complexidade alta.
- **Work-stealing de agents com virtual-time:** Similar ao EEVDF do Redox.
  Requer tracking de vruntime por agent. Adiado.

## Referências

- Redox OS: `kernel/scheduler/eevdf.rs` — per-CPU run-queues, work-stealing
- Linux CFS: `kernel/sched/core.c` — per-CPU rq, load balancing
- ADR-0055: FeatureGate pattern (default OFF)
- ADR-0057: Compute Dispatch SMP (AP workers)
- ADR-0065: TSS/IST multi-AP (IDT compartilhada)
