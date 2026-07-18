# Neural OS Hermes — AI Summary

**O que é:** Sistema operacional bare-metal (`no_std` Rust) onde **tudo é Agente ou Skill**. 247+ agentes, Trinity MoE no kernel, BitNet ternário para HW e inferência.

**Versão release:** **v1.8.6 TESTE / NÃO ESTÁVEL** (2026-07-18) — base v1.8.0; ADR-0041 H4+/H5+/AS PoC.
**Estado:** ~26.000 LOC, 180+ arquivos, `cargo nk` = 0 erros.

**Base v1.8.0:**
- ADR-0042 N1–N5 ✅ — cadeia funcional K³CHJ
- Wire crates N2.5–N5.7 ✅ — `k_nano`, `k_ai`, `cortex`, `hermes`, `jarbas` (+ **`k_hal`** em v1.8.6) linkados no bin
- Produto **K³CHJ** (ADR-0042 §0); histórico K²CHJ = sem k-hal na marca
- Sprint 107 Voice ✅ — clima e2e, Piper neural-lite, EventBus skinny

**Consolidação v1.8.6 (teste):**
- **ADR-0041:** H4+ QUEUE_NOTIFY · residual MMIO→`k_hal` · H5+ Cap · AS shallow (SESSION_140)
- **HalOffer** API R3 + Cap grant; VirtIO = transporte BE
- Sprint 108 Self-Evolve ✅ · Sprint Sound ✅ parcial · NeuralFS/AirLLM/ADR-0047 MVPs
- ADRs GPU 0048–0050 ⏳
- **Não** é declaração de `v2.0.0`

**Arquitetura K³CHJ:**

| Crate | Função |
|-------|--------|
| `k_nano` | Ring 0 — HAL base, drivers, PCI |
| `k_hal` | Ring 1 — DeviceCap, HalOffer, MMIO BE, VirtIO transporte |
| `k_ai` | SelfHeal, Trust, inventário |
| `cortex` | BitNet, Trinity MoE, tensores |
| `hermes` | WASM, rede, skills, intent, HalOffer client |
| `jarbas` | Display FE, persona (GPU BE em k_hal) |
| `neural-kernel` | Bin de boot (integração + residuals) |

**Pista ativa:** pós-Residuals 0–7 ✅ (SESSION_151) — LAN L3.5–L5 OK; WiFi/TLS/#418 abertos; gate v2.0.0 review.

**Para agentes de IA:**
1. `AGENTS.md` — regras operacionais
2. `docs/memory/STATE.md` — estado atual
3. `TECNOLOGIAS.md` — catálogo de PI
4. `TODO.md` — backlog

**Stack:** Rust nightly · `x86_64-unknown-none` · bootloader 0.11.15 · smoltcp 0.13 · QEMU/WHPX dev · HW real validação final.

> "We don't need an OS that runs AI. We need an OS that IS AI."
