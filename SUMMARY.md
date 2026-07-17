# Neural OS Hermes — AI Summary

**O que é:** Sistema operacional bare-metal (`no_std` Rust) onde **tudo é Agente ou Skill**. 247+ agentes, Trinity MoE no kernel, BitNet ternário para HW e inferência.

**Versão release:** **v1.8.5 TESTE / NÃO ESTÁVEL** (2026-07-16) — base v1.8.0.
**Estado:** ~26.000 LOC, 180+ arquivos, `cargo nk` = 0 erros.

**Base v1.8.0:**
- ADR-0042 N1–N5 ✅ — cadeia funcional K²CHJ
- Wire crates N2.5→N5.7 ✅ — `k_nano`, `k_ai`, `cortex`, `hermes`, `jarbas` linkados no bin
- Sprint 107 Voice ✅ — clima e2e, Piper neural-lite, EventBus skinny

**Consolidação v1.8.5 (teste):**
- Sprint 108 Self-Evolve ✅ · Sprint Sound ✅ parcial honesto
- NeuralFS/ADR-0040 MVP ✅ · AirLLM/ADR-0046 MVP ✅ · família ADR-0047 ✅ MVP/PoC
- ADRs GPU 0048–0050 ⏳ propostas
- **Não** é declaração de `v2.0.0` (review + `por_fazer` + OK humano pendentes)

**Arquitetura K²CHJ:**

| Crate | Função |
|-------|--------|
| `k_nano` | Ring 0 — HAL, drivers, PCI |
| `k_ai` | SelfHeal, Trust, inventário |
| `cortex` | BitNet, Trinity MoE, tensores |
| `hermes` | WASM, rede, skills, intent |
| `jarbas` | Display, GPU, persona |
| `neural-kernel` | Bin de boot (integração + residuals) |

**Pista ativa:** estabilização v1.8.5 + Sprint Net — validar residuals (soft-float/VITS, UAC iso, NeuralFS disco, AirLLM DMA/RX) em HW real.

**Para agentes de IA:**
1. `AGENTS.md` — regras operacionais
2. `docs/memory/STATE.md` — estado atual
3. `TECNOLOGIAS.md` — catálogo de PI
4. `TODO.md` — backlog

**Stack:** Rust nightly · `x86_64-unknown-none` · bootloader 0.11.15 · smoltcp 0.13 · QEMU/WHPX dev · HW real validação final.

> "We don't need an OS that runs AI. We need an OS that IS AI."
