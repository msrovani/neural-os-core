# SESSION 137 — Cognitive Bridge (HANR UX, stack superior)

**Data:** 2026-07-17  
**Foco:** Uso igual ao HANR (`/skills` L0, `/skill` L1, memória, slash HITL) com stack K²CHJ superior (BGE, Trinity, CapGate, IterationBudget, PERSONA≠SOUL).

## Entregue

| Peça | Onde | Comportamento |
|------|------|----------------|
| `cognitive_bridge` | `hermes/cognitive_bridge.rs` | budget + session search + memory nudge + CapGate L0 + prompt Cortex |
| SOUL vs PERSONA | `memory_store.rs` + `jarbas/jarvis.rs` | SOUL=orquestração Hermes; PERSONA=tom/voz Jarbas (`PERSONA.md`) |
| Prompt Cortex | `skill_loader::build_system_prompt_for` | BGE-RAG + Trinity route + L0 gated + budget |
| IterationBudget | Chat Hermes (crate + residual) | Continue / Grace / Exhausted; `/budget [N]` |
| Session search | `/search <q>` | ring 48 + SESSION.log + fallback BGE |
| Memory nudge | SleepCycle REFLECT | `MEMORY_NUDGE` → overlay Jarbas; `/remember` |
| Cap gating | `skill_visible` | `capabilities`/`requires` no manifesto; L0 omite sem cap |

## Comandos novos

```
/persona [texto]   PERSONA Jarbas
/search <q>        session + BGE
/budget [N]        IterationBudget
/cog               status cognitivo
```

## Verificação

```
cargo check --release -p hermes     → 0 erros
cargo check --release -p jarbas     → 0 erros
cargo check --release -p neural-kernel → 0 erros
```

## Direcionamento (SESSION_137+)

Path Chat: **Cortex.think (structured-first) → Trinity MoE (single-pass + R3) → skill expert → LLM** com Trust `llm_generate`.

| HANR | Neural (superior) |
|------|-------------------|
| LLM + tool-loop cego | `route_user_intent` on-device |
| Sem CapGate no path | Trust(token,agent,skill) antes do LLM |
| Sem MoE nativo | Trinity expert → skill nativa preferida |
| FTS5 only | BGE-RAG + emotion lite no prompt |
| Classificação repetida | Expert e trace R3 calculados uma vez no bin |
| Expert→system_status genérico | Skills reais `disk_diag` / `security` no registry |
| Trust LLM permissivo (`session_ready`) | Boot TTL curto; token não-boot → Escalate HITL |

Comandos: `/cog` mostra última rota. Serial: `[ROUTE] …`.


## Próximo

- Boot QEMU: `/cog` + SleepCycle REFLECT nudge no serial/overlay.
- Opcional: ADR-0053 addendum “Cognitive Bridge”.
