# SESSION 312 — hermes + cortex Complete Optimization (FASE 1-4)

**Sprint:** v1.9.99-s318
**Date:** 2026-09-07
**Status:** ✅ COMPLETE

## Problema

Análise profunda de hermes (97+ módulos) + cortex (37 módulos) revelou:
- KvCache criada nova a cada chamada de generate (desperdício ~40%)
- Emotion/Soul/BeiInit implementados mas não-wired
- 17 dead modules (~1500 LOC mortos)
- MCP com tools hardcoded
- Session search sem busca semântica

## Plano: 4 FASEs, 17 tasks

### FASE 1: Optimize Functional (7 tasks) ✅

| Task | Change | File |
|------|--------|------|
| Persistent KvCache | `GLOBAL_KV_CACHE` singleton, reuse between generate calls | `cortex/src/cortex.rs` |
| MoE Neural Routing | threshold 0.08→0.05, warmup 100 ticks | `cortex/src/trinity.rs` |
| Emotion→Affect | `EmotionAnalyzer::analyze()` → `AffectRegulator::incorporate()` | `hermes/src/agents.rs` |
| Soul→LLM | SoulEngine personality in system prompt | `hermes/src/agents.rs` |
| BeiInit→Hermes | LoopPhase modulation (Think=200, Execute=50, Learn=150) | `hermes/src/executive.rs` |
| PonderNet Gate | 3 inference calls per tick max | `hermes/src/agents.rs` |
| LoopPhase Event | `publish_phase()` publishes `LOOP_PHASE` on EventBus | `hermes/src/executive.rs` |

### FASE 2: Complete Semi-Functional (6 tasks) ✅

| Task | Change | File |
|------|--------|------|
| MCP Dynamic Tools | `tools/list` discovers skills from SKILL_REGISTRY | `hermes/src/mcp.rs` |
| SkillGen Persist | Auto-skill persists via PackageHub after 3 uses | `hermes/src/skill_gen.rs` |
| Marketplace CONFIG.TXT | Allowlist loads `MARKET_HOST=` from CONFIG.TXT | `hermes/src/marketplace.rs` |
| VectorClock Dedup | Mesh knowledge skip older peer docs | `hermes/src/mesh_knowledge.rs` |
| HNSW Session Search | Semantic search with 64D hash projection | `hermes/src/cognitive_bridge.rs` |
| LoopPhase Event | `publish_phase()` on EventBus | `hermes/src/executive.rs` |

### FASE 3: Dead Code Removal (17 modules) ✅

Removed from `hermes/src/lib.rs`:
- `actor_registry`, `app_store`, `elf_loader`, `email_agent`
- `expert_skills`, `gguf_wasm`, `intent_bus`, `ipc_bus`
- `link_watcher`, `native_agents`, `optimizer`, `quarantine`
- `rss_agent`, `search_agent`, `wasi_host`, `wifi_agent`, `wpa2_hs`

### FASE 4: Connect Ecosystem (3 tasks) ✅

| Task | Change | File |
|------|--------|------|
| Cortex→Hermes Feedback | `LLM_FEEDBACK` event after LLM response | `hermes/src/agents.rs` |
| HNSW Memory | `memory_search_hnsw()` for semantic recall | `hermes/src/cognitive_bridge.rs` |
| Federated Health | Mesh health check before broadcast | `cortex/src/federated.rs` |

## Arquivos Modificados (esta sessão)

| Arquivo | LOC± | Mudança |
|---------|------|---------|
| `cortex/src/cortex.rs` | +38/-1 | GLOBAL_KV_CACHE, reuse between generate calls |
| `cortex/src/trinity.rs` | +6/-4 | MoE threshold 0.05, warmup |
| `cortex/src/federated.rs` | +14/-0 | Mesh health check before broadcast |
| `hermes/src/agents.rs` | +23/-2 | Emotion→Affect, Soul→LLM, PonderNet, LLM feedback |
| `hermes/src/executive.rs` | +24/-0 | current_phase(), publish_phase(), LoopPhase event |
| `hermes/src/mcp.rs` | +33/-5 | Dynamic tools discovery from SKILL_REGISTRY |
| `hermes/src/skill_gen.rs` | +16/-2 | PackageHub persistence for auto-skills |
| `hermes/src/marketplace.rs` | +32/-8 | CONFIG.TXT allowlist loading |
| `hermes/src/mesh_knowledge.rs` | +12/-0 | VectorClock dedup |
| `hermes/src/cognitive_bridge.rs` | +67/-18 | HNSW session search + memory_search_hnsw |
| `hermes/src/lib.rs` | -17 | 17 dead modules removed |

## Build

```
cargo clean -p neural-kernel && cargo check --release — 0 errors
```

## Commits

| Commit | FASE | Description |
|--------|------|-------------|
| `1242d951` | FASE 1 | Persistent KvCache, MoE 0.05, Emotion→Affect, Soul→LLM, BeiInit, PonderNet |
| `9d769652` | FASE 2a | MCP dynamic tools, SkillGen→PackageHub, HNSW session search |
| `c6a3f62a` | FASE 3 | Removed 17 dead modules (~1500 LOC) |
| `71688ed8` | FASE 2b+4 | CONFIG.TXT, VectorClock, LoopPhase event, LLM feedback, HNSW memory, Federated health |
