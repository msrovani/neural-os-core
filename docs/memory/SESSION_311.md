# SESSION 311 — k_ai + Cortex Optimization: FASE 1-4 Complete

**Sprint:** v1.9.99-s317
**Date:** 2026-09-07
**Status:** ✅ COMPLETE

## Problema

Análise profunda de k_ai (38 módulos) + cortex (28 módulos) revelou:
- Muitos módulos implementados mas não-wired (ContextWindow, Economy, IntentPlanner, ReActLoop, etc.)
- Dead code accumulation (fine_tuning_pipeline, workflow_learner, self_heal_disk, etc.)
- KV-cache sem eviction (hard-clip em max_seq)
- MoE neural threshold alto demais (0.15)

## Plano Aprovado: 4 FASEs

### FASE 1 — Wire Semântico ✅

| Módulo | Singleton | Arquivo |
|--------|-----------|---------|
| ContextWindow | `GLOBAL_CONTEXT_WINDOW` | `k_ai/src/context_window.rs` |
| Economy | `GLOBAL_BUDGET` | `k_ai/src/economy.rs` |
| FeedbackAgent | HEALTH_ISSUE publisher | `k_ai/src/feedback_agent.rs` |
| DataCollector | throttle 5000→500 | `k_ai/src/self_learning.rs` |
| AgentRegistry | `apply_priority_hints()` | `agent-core/src/lib.rs` |

**CortexAgent** agora tracks user input + assistant responses via `add_global()`.
**BudgetManager** gateia inference quando budget esgota.

### FASE 2 — Completar 100% ✅

| Módulo | Singleton | Arquivo |
|--------|-----------|---------|
| IntentPlanner | `GLOBAL_PLANNER` | `k_ai/src/cognitive.rs` |
| ReActLoop | `react_run()` | `k_ai/src/cognitive.rs` + `hermes/src/agents.rs` |
| McpServer | `GLOBAL_MCP` | `k_ai/src/cognitive.rs` |
| NeuralCache | `GLOBAL_NCACHE` | `k_ai/src/cognitive.rs` |
| SuccessEngine | `GLOBAL_SUCCESS` | `k_ai/src/cognitive.rs` |
| CodebookVQ | `GLOBAL_CODEBOOK` (64×256) | `k_ai/src/cognitive.rs` |

**Pipeline HermesAgent:** IntentPlanner → ReActLoop → LLM_REQUEST → ContextWindow tracking

### FASE 3 — Dead Code Removal ✅

Removidos de `k_ai/src/lib.rs`:
- `fine_tuning_pipeline`
- `workflow_learner`
- `self_heal_disk`
- `agency_importer`

`native_agent_seed` stubbed para package_hub compat.

### FASE 4 — Cortex Extreme ✅

| Feature | Detalhe | Arquivo |
|---------|---------|---------|
| Trinity MoE | threshold 0.15→0.08 + keyword fallback warning | `cortex/src/trinity.rs` |
| KV-Cache H2O | `h2o_evict()` em `generate_speculative()` | `cortex/src/cortex.rs` |
| Speculative decoding | já em `generate_speculative()` via ngram | `cortex/src/cortex.rs` |

## H2O Eviction — Detalhes

Quando `tokens.len() >= max_seq` no loop de geração:
1. `evict_target = max_seq / 2` (metade para headroom de geração)
2. `h2o_heavy = evict_target / 3` (top 1/3 como heavy hitters)
3. `h2o_evict(cache, recent=8, heavy=h2o_heavy)` — mantém últimos 8 + top heavy
4. Sync tokens/recent_u16/tokens_u16 para novo cache length
5. Re-check: se ainda ≥ max_seq, break

## Arquivos Modificados (esta sessão)

| Arquivo | LOC± | Mudança |
|---------|------|---------|
| `crates/cortex/src/cortex.rs` | +20/-1 | H2O eviction in generate loop |
| `crates/hermes/src/agents.rs` | +5 | ReActLoop call before LLM |
| `crates/k_ai/src/cognitive.rs` | +17 | react_run(), GLOBAL_CODEBOOK |

## Build

```
cargo clean -p neural-kernel && cargo check --release — 0 errors
```
