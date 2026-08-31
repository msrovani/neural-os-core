# Neural OS Hermes — AI Summary

**O que é:** Sistema operacional bare-metal (`no_std` Rust) onde **tudo é Agente ou Skill**. ~50 agentes nativos, Trinity MoE no kernel (VOCAB=256, routing telemetry), BitNet ternário para HW e inferência.

**Versão release:** **v1.9.99-s297 TESTE / NÃO ESTÁVEL** (2026-08-31) — virtio_blk + NSGDB persistente; TTS streaming; compositor hot path; Trinity improvements.
**Estado:** ~28.000 LOC, 180+ arquivos, `cargo nk` = 0 erros, 168 testes host.

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
- **ADR-0059** Runtime App Factory ✅ **Caminho A** (wasmi): módulo WASM real roda no bare-metal (`add(2,3)=5`); seletor por IA A(wasmi)/B(Cranelift JIT)/C(Rust-subset) + CapGate/HW-gate/HITL; B/C compilam (feature) mas exec nativa **gated** por ring de isolamento (ADR-0041). Motor do self-improve/heal/update. Supersede ADR-0031(WASM)/0032; aposenta VM `Op`
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

**Pista ativa:** s297 virtio_blk + NSGDB; s296 HW splash freeze; s295 HW pendrive; s294 compositor hot path + TTS streaming; s293 Trinity vocab256 + routing telemetry; s292 instalador pendrive→HD. Gate v2.0.0 review pendente.

**Para agentes de IA:**
1. `AGENTS.md` — regras operacionais
2. `docs/memory/STATE.md` — estado atual
3. `TECNOLOGIAS.md` — catálogo de PI
4. `TODO.md` — backlog

**Stack:** Rust nightly · `x86_64-unknown-none` · bootloader 0.11.15 · smoltcp 0.13 · QEMU/WHPX dev · HW real validação final.

> "We don't need an OS that runs AI. We need an OS that IS AI."
