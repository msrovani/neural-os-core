# SESSION 026 — Sprint 23: Hermes Governance & Agent Memory

**Data:** 2026-06-25
**Versão:** v0.19.0 → v0.20.0

## Objetivo

Concluir Sprint 23, implementando os 4 itens imediatos do Ecosystem Analysis (Tier 4 - Agent Frameworks):
- #228 Tool Policy Registry
- #229 Usage Tracker
- #230 Auto-Compact Hermes Buffer
- #231 Event-Sourced Conversation

## Dificuldades e Decisões

### Tool Policy Registry
- Decisão: `ToolPolicy` com `enabled` + `auto_approve` segue exatamente o padrão Cline (ToolPolicy { enabled, autoApprove })
- Wildcard `"*"` como fallback permite default global sem configurar skill por skill
- `list_skills()` devolve tuplas `(String, ToolPolicy)` para futura exposição via Hermes

### Usage Tracker
- `to_metrics_tensor()` retorna `[f32; 4]` compatível com o `hardware_context_tensor()` do roteador MLP
- Atomic counter leve (`USAGE_COUNTER`) para eventos sem lock no hot path

### Auto-Compact Hermes Buffer
- Threshold de 3 ciclos escolhido empiricamente (não há testes de usabilidade)
- Compact gera sumário textual: contagem de ciclos + último input/response
- Buffer é limpo após compact para evitar crescimento infinito

### Event-Sourced Conversation
- Max 256 eventos no VecDeque (bound de memória: ~256 * ~200 bytes = ~50 KB)
- `EventKind::ContextCompacted` registrado quando auto-compact acontece
- `summarize()` usado pelo comando `/conv`

### Network Stack (e1000)
- Código existente de e1000, net e proto compila sem erros
- DMA PageFault conhecido pode persistir em runtime QEMU
- Sprint 23 original (ADR-0016) previa VirtIO-net + smoltcp, mas o código existente usa e1000
- Decisão: postergar depuração do e1000 para Sprint 24

## Pontos de Atenção

1. **e1000 DMA PageFault** — bug conhecido, `send()` acessa TX buffer físico. Não reproduzido (sem QEMU nesta sessão).
2. **Known Warnings Policy** — todos os 36 warnings são esperados (dead code, fields never read). Nenhum novo warning introduzido.
3. **Nova API conversation::EventKind** — `SkillExecuted` e `SystemEvent` não são usados ainda, mas estão no enum para uso futuro.
4. **UsageTracker::start_tick** — campo não lido, será usado quando métricas forem expostas periodicamente.

## Resultados

- `cargo check --release`: 0 erros, 0 failures
- ~300 LOC novas
- v0.20.0 — "Hermes Governance"
- CHANGELOG.md, STATE.md, AGENTS.md atualizados
