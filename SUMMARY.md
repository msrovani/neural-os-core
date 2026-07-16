# Neural OS Hermes — AI Summary

**O que é:** Sistema operacional bare-metal (`no_std` Rust) onde **tudo é Agente ou Skill**. 247+ agentes, Trinity MoE no kernel, BitNet ternário para HW e inferência.

**Versão release:** **v1.8.0** (2026-07-16)
**Estado:** ~26.000 LOC, 180+ arquivos, `cargo nk` = 0 erros.

**Marco v1.8.0:**
- ADR-0042 N1–N5 ✅ — cadeia funcional K²CHJ
- Wire crates N2.5→N5.7 ✅ — `k_nano`, `k_ai`, `cortex`, `hermes`, `jarbas` linkados no bin
- Sprint 107 Voice ✅ — clima e2e, Piper neural-lite, EventBus skinny
- **Não** é declaração de `v2.0.0` (review + Sprint Sound pendente)

**Arquitetura K²CHJ:**

| Crate | Função |
|-------|--------|
| `k_nano` | Ring 0 — HAL, drivers, PCI |
| `k_ai` | SelfHeal, Trust, inventário |
| `cortex` | BitNet, Trinity MoE, tensores |
| `hermes` | WASM, rede, skills, intent |
| `jarbas` | Display, GPU, persona |
| `neural-kernel` | Bin de boot (integração + residuals) |

**Pista ativa:** Sprint Sound — STT real, Mic→Wake runtime, Piper VITS, soft-float latency.

**Para agentes de IA:**
1. `AGENTS.md` — regras operacionais
2. `docs/memory/STATE.md` — estado atual
3. `TECNOLOGIAS.md` — catálogo de PI
4. `TODO.md` — backlog

**Stack:** Rust nightly · `x86_64-unknown-none` · bootloader 0.11.15 · smoltcp 0.13 · QEMU/WHPX dev · HW real validação final.

> "We don't need an OS that runs AI. We need an OS that IS AI."
