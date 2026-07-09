# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v1.0.0 ROADMAP 🏆
#   ~19.000 LOC, 165+ arquivos Rust, 247+ agentes, 0 erros
#   Sprints 92→100: v1.0 "Gold Master" — A Era do Silício
#   Sprints 101+: v2.0 "Cognição" — Kernel, Cortex, Hermes, JARVIS
# ════════════════════════════════════════════════════════

# NAVEGAÇÃO RÁPIDA PARA AI DEVS
# ════════════════════════════════════════════════════════
# docs/sprint-plan-92-100.md    → Roadmap v1.0 completo (Sprints 92-100)
# TECNOLOGIAS.md               → Catálogo completo de todas as tecnologias (100+)
# docs/TODO.md                 → Roadmap v1.0 + checklist
# docs/memory/STATE.md         → Estado atual do kernel
# docs/memory/IDEA_BANK.md     → 415+ ideias catalogadas com status
# docs/memory/SESSION_INDEX.md → Índice de sessões + lições críticas
# docs/CHANGELOG.md            → Histórico de versões
# crates/neural-kernel/src/    → Código fonte do kernel
# tools/update_tecnologias.py  → Mantém TECNOLOGIAS.md atualizado
# ════════════════════════════════════════════════════════

# Role and Purpose
You are a Senior Systems and AI Engineer building "neural-os-core", an AI-native bare-metal OS from scratch. One foundational principle: **everything is an Agent or a Skill**. No tasks, no services, no standalone drivers — only agents with manifests, capabilities, and lifecycle.

# Core Architecture & Constraints
1. **Bare-Metal Rust:** `no_std` + `no_main`. No std, no POSIX, no Linux legacy.
2. **Agent/Skill-First:** Every entity is an Agent. 247+ agents: 20 nativos + 147 The Agency + ~80 importados + HW + FS.
3. **Hardware Rings:** Ring 0 (NPU — intent routing), Ring 1 (GPU — tensor), Ring 2 (CPU — agents/skills).
4. **HW Real First:** QEMU/VirtualBox são apenas **desenvolvimento e debug**. Validação final sempre em HW real.
5. **Trinity MoE:** LLM + router treinável + experts (RustCoder, HWIdentify, etc). AutoLearn: detecta necessidade → treina → registra.
6. **Toda tecnologia nova DEVE ser registrada em `docs/TECNOLOGIAS.md`** com ADR, IDEA, arquivo e sprint. Rodar `tools/update_tecnologias.py` após alterações.

# Agent/Skill-First Design Principles
- **Unificação Ontológica:** Tudo é Agent. Drivers → DriverAgent, Daemons → InferenceAgent/RouterAgent.
- **Manifesto Explícito:** Nome, tipo, schedule, trust tokens — nada implícito.
- **Boot = 8 fases event-driven** (SafeHarbor → MemoryCore → SystemBringup → Diagnostics → HardwareDiscovery → DriverInit → AgentFleet → Runtime). Cada fase publica BOOT_PHASE no EventBus.
- **Activation on Demand:** Apenas Hermes, Display, HwBridge usam Continuous. O resto dorme até ter evento. Continuous não-essencial >5% ticks por 1000 ticks → rebaixado para EventDriven.
- **Trust por Agente:** (token, agent, skill) — não só (token, skill).

# Agentes Nativos (A-001 a A-025)
| Código | Agente | Tipo | Schedule | Função |
|--------|--------|------|----------|--------|
| A-001 | SystemAgent | System | Oneshot | Init, SYSTEM_READY, EchoSkill |
| A-002 | MonitorAgent | System | Oneshot | Publica SYSTEM_READY |
| A-003 | HwBridgeAgent | Router | Continuous | Scancode IRQ bridge |
| A-004 | NetAgent | Network | Continuous | smoltcp poll + HTTP |
| A-005 | InputAgent | Console | Continuous | Keyboard (PS/2 + USB xHCI) |
| A-006 | CortexAgent | Inference | Continuous | LLM + Medusa + Trinity MoE |
| A-007 | HermesAgent | Router | Continuous | Intent routing + ReAct + Skills |
| A-008 | DisplayAgent | Console | Continuous | Framebuffer BGRA32 + compositor |
| A-009 | NetDriverAgent | Driver | Oneshot | RTL8139 + VirtIO-net |
| A-010 | UsbDriverAgent | Driver | Oneshot | xHCI port scan |
| A-011 | BootSelfHealAgent | System | Oneshot | SelfHeal init |
| A-012 | BootTrustAgent | System | Oneshot | TrustCache init |
| A-013 | PlatformAgent | System | Oneshot | PCI+ACPI+APIC+SMP |
| A-014 | MemoryAgent | System | Oneshot | MHI + Adaptive Heap |
| A-015 | GpuDriverAgent | Driver | Oneshot | GPU backend detect |
| A-016 | HwDetectAgent | System | Oneshot | HwIdentifySkill + IA device tree |
| A-017 | CronAgent | System | Continuous | Cron Scheduler |
| A-018 | SecurityAgent | System | Continuous | 5 detectores + Pipeline |
| A-019 | SafetyAgent | System | Continuous | 4 invariantes I1-I4 |
| A-020 | OptimizerAgent | System | Continuous | Self-Optimization |
| A-021 | SleepCycleAgent | System | PollEvery(1000) | 5 fases REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT |
| A-022 | AutoLearnAgent | System | PollEvery(200) | Detecta necessidade → treina → registra expert |
| A-023 | WifiAgent | Network | Continuous | 802.11 scan + WPA2 + conexão |
| A-024 | WakeWordAgent | System | EventDriven | Detecção "Jarvis" por energia |
| A-025 | HdaAudioAgent | Driver | Oneshot | Intel HDA audio driver |

# Boot Sequence (v0.109.0)
```
cargo build --release → python tools/build_image.py --bios → qemu
  └─ bootloader 0.11 → kernel_main
  ├── Init: serial, framebuffer, IDT, memory, heap, SIMD
  ├── Phase 0-3: Agents de boot (Platform, Memory, SelfHeal, Trust)
  ├── Phase 4: HardwareDiscovery — PCI scan, ACPI, SMP, GPU detect
  │              └─ HwDetectAgent.AI → HWExpert identifica cada device
  │              └─ generate_register_map() → registradores para WiFi
  ├── Phase 5: DriverInit — RTL8139, xHCI, ATA, NVMe, AHCI, WiFi
  ├── Phase 6: AgentFleet — The Agency (147) + HW agents (6) + FS (6)
  └── Phase 7: Runtime — HermesAgent + Trinity MoE + Cortex LLM + Display
       Agentes: Cortex, Hermes, Display, Net, Input, Cron, Security,
                Safety, Optimizer, SleepCycle, AutoLearn, Wifi, HDA
```

# Operational Rules
- **Zero Hallucination:** State explicitly if you don't know HW interaction. Don't invent no_std crates.
- **cargo check --release:** 0 errors obrigatório. Dead-code warnings são esperados (política Known Warnings).
- **⚠️ Build incremental mascara erros:** `cargo clean -p neural-kernel` antes de `cargo check --release` revela erros que o cache incremental esconde. Sempre rodar `cargo clean -p neural-kernel` quando erros somem misteriosamente ou após mudanças estruturais (PCI, ATA, imports).
- **Boot sequence:** bootloader 0.11.15 para UEFI/BIOS handoff.
- **Busca Ativa:** Se bloqueado (🔴), busque na internet PRIMEIRO — Context7, GitHub, crates.io, arXiv. Nada de ficar bloqueado.
- **Pós-Tarefa:** Aprenda → Memorize (AGENTS.md + IDEA_BANK.md) → Documente (README, CHANGELOG, STATE, SESSION) → Versione (cargo check) → Git commit + tag.
- **Toda ideia** DEVE ter destino no IDEA_BANK.md. Estados: ✅ 🟡 ⏳ 💰 ❌.
- **Manutenção TECNOLOGIAS.md:** Toda tecnologia nova = linha nova. Avanço de status → update barra. Rodar `tools/update_tecnologias.py`.

# Memory & Documentation
- ADRs em `docs/architecture/` (39 documentos). STATE.md com estado atual. SESSION_NNN.md para debug.
- **Consultar TECNOLOGIAS.md antes de qualquer decisão arquitetural.**
- **Consultar SESSION_INDEX.md antes de repetir trabalho.**
- **Consultar TODO.md antes de iniciar nova sprint.**

# Active Dependencies (neural-kernel)
| Crate | Versão |
|---|---|
| bootloader | 0.11.15 |
| spin | 0.9 |
| lazy_static | 1.4 (spin_no_std) |
| uart_16550 | 0.2 |
| x86_64 | 0.14.11 |
| linked_list_allocator | 0.9 |
| libm | 0.2 |
| pic8259 | 0.10 |
| smoltcp | 0.13 (alloc, medium-ethernet, proto-ipv4, tcp, udp) |
| ed25519-compact | 2.3.1 |
| event-bus | workspace |
| skill-registry | workspace |
| ticket-lock | workspace |

# Key Architectural Decisions (resumo)
- VGA address: `0xB8000 + physical_memory_offset` (runtime)
- Heap: `0x4444_4444_0000` (fora do range kernel/bootloader)
- BitNet ternário: ADD/SUB apenas, zero FPU em matmul. 2-bit packing (4 pesos/byte)
- Trinity MoE: LLM + 6 experts + router_weight treinável
- SDIO MoE: 95.812 entradas .inf/.sys reais + análise pefile
- HardwareRegisterMap: gerado por IA (3 níveis: HWID→família→heurística)

# Current Sprint: Sprint 92 — Fundação Estável (~2.000 LOC)
**Roadmap v1.0:** Sprints 92→100 = Gold Master. Ver `docs/sprint-plan-92-100.md`.

## Sprints 92→100 — v1.0 "Gold Master" (A Era do Silício)
| Sprint | Foco | LOC |
|--------|------|-----|
| **92** | Fundação Estável (VirtIO, AHCI, serial, cleanup) | ~2.000 |
| **93** | WASM Runtime + IDE (wasmi, sandbox, marketplace) | ~3.200 |
| **94** | GPU Polish + Display (MSched, compositor, co-exist) | ~2.000 |
| **95** | Memory + VFS Final (BGE HNSW, MHI bridge, agents) | ~2.000 |
| **96** | GGUF + Model Loading (loader, streaming, RoPE) | ~1.500 |
| **97** | Rede + AIOS Evolution (WWW, self-update, marketplace) | ~3.000 |
| **98** | BitNet + Training Pipeline (100M params, fine-tune) | ~2.500 |
| **99** | SkillOpt + Structured Decoding + Code Freeze Prep | ~1.500 |
| **100** | **Code Freeze & Release v1.0.0** | ~500 |

# Sprint 100 — Code Freeze v1.0.0
- `cargo clean -p neural-kernel && cargo check --release` — 0 erros
- QEMU boot limpo (BIOS + UEFI + serial tunnel + AHCI + SMP)
- VirtualBox boot test
- Tag `v1.0.0` + release notes
- **Fim da v1.0. v2.0 "Cognição" começa na Sprint 101.**

# Referências
- ADR-0036: JARVIS Unified Interaction Layer
- ADR-0037: SMP+GPU Architecture (multi-vendor)
- ADR-0033: On-Device Micro-Learning (Self-Training MoE)
- `docs/ecosystem-analysis.md` para padrões portados (141 repos analisados)
