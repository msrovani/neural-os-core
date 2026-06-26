# ═══════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.27.0
#   Sprint 26-27: Transformer Engine + Cortex LLM Daemon
# ═══════════════════════════════════════════════

# Project State — neural-os-core

## Sprint 26 — Transformer Engine (v0.26.0)
**Completo:** Attention, 4-layer BitNet, tokenizer, generate(), modelo 68 KB

### Concluído
- ✅ Attention Q/K/V/O com causal mask + softmax row-wise
- ✅ 4 camadas TransformerBlock (RMSNorm → Attn → FFN SiLU → residual)
- ✅ Tokenizer char-level (99 tokens, ASCII 32-126)
- ✅ generate_next() + generate_text() loop autoregressivo
- ✅ PackedTernaryTensor para todos os pesos (2-bit, ~272K params)
- ✅ Python gen_micro_model.py para gerar .bitnet
- ✅ Model loader .bitnet (magic 0xBE11BE11, header validado)

## Sprint 27 — Cortex LLM Daemon (v0.27.0)
**Completo:** 8ª task no executor, transformer carregado, LLM_REQUEST/LLM_RESPONSE

### Concluído
- ✅ cortex_llm_daemon async task (subscribe → generate → publish)
- ✅ LLM_REQUEST / LLM_RESPONSE EventBus topics
- ✅ 8 tasks cooperativas rodando (9600+ ticks estável)
- ✅ Transformer carregado no boot sem travamentos
- ✅ 0 erros cargo check, 48 warnings esperados

### Pendências
- Integrar intent_router → publica LLM_REQUEST para Chat
- Treinar modelo real (host-side Python)
- Non-blocking generate (1 token/tick)

## 8 Tasks no Executor
| # | Task | Persiste? | O que faz |
|---|---|---|---|
| 1 | system_daemon | ❌ morre | Publica SYSTEM_READY |
| 2 | hardware_monitor | ✅ | Context tensor a cada 100 ticks |
| 3 | hw_bridge | ✅ | Scancode → EventBus |
| 4 | network_agent | ✅ | smoltcp poll → HTTP |
| 5 | input_daemon | ✅ | ASCII buffer → ENTER → USER_INTENT |
| 6 | cortex_llm | ✅ | LLM_REQUEST → generate → LLM_RESPONSE |
| 7 | intent_router | ✅ | Cortex.think() → skills |
| 8 | hermes_console | ✅ | HERMES_RESPONSE → VGA |

## Sprints Completos
| Sprint | v | Foco |
|--------|---|------|
| 1-25 | 0.1–0.25 | MVP toolchain → VGA → IDT → heap → SIMD → tensor → NN → IPC → SMP → APIC → RTL8139 → smoltcp → Cortex |
| 26 | 0.26.0 | Transformer Engine (Attention, 4 layers, tokenizer, generate) |
| 27 | 0.27.0 | Cortex LLM Daemon (8 tasks, LLM_REQUEST/LLM_RESPONSE) |
