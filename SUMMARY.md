# Neural OS Hermes — AI Summary

**O que é:** Sistema operacional bare-metal (`no_std` Rust) onde **tudo é Agente ou Skill**. 247+ agentes, Trinity MoE no kernel, BitNet ternário para HW e inferência.

**Versão release:** **v1.9.0 TESTE / NÃO ESTÁVEL** (2026-07-18) — Pós-LAN + Residuals 0–7; base v1.8.6.
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
- **ADR-0057** Compute Dispatch SMP+GPU+NPU: WS-A wake multi-AP (`-smp 4`→APs=3, CorePools r0=1 r1=2 r2=1) + `cortex::compute` dispatcher + WS-G #412 structured decode ✅; GPU/NPU hooks + on-demand AP-worker (IDT/IPI) = Layer S/HW
- **ADR-0058** Generative Card Desktop (UI/Jarbas) ✅ **S1–S4**: embedded-graphics (`DrawTarget`) + `UiDeclaration`/`UiRenderer` (cards por LLM #412 ou skill WASM); orb responsivo + barra de relógios/HUD preservados; WM stacking; supersede parcial ADR-0047-HMI (H3 ❌); S5+A/V residual
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

**Pista ativa:** Pós-LAN B-01 ✅ (SESSION_152) — NetFs PASS; TLS BLOCKED; WiFi AWAITING; gate v2.0.0 review.

**Para agentes de IA:**
1. `AGENTS.md` — regras operacionais
2. `docs/memory/STATE.md` — estado atual
3. `TECNOLOGIAS.md` — catálogo de PI
4. `TODO.md` — backlog

**Stack:** Rust nightly · `x86_64-unknown-none` · bootloader 0.11.15 · smoltcp 0.13 · QEMU/WHPX dev · HW real validação final.

> "We don't need an OS that runs AI. We need an OS that IS AI."
