# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.39.0 🏆
#   AGENT/SKILL-FIRST ARCHITECTURE
#   Tudo é agente ou skill. Nada de tasks, serviços, drivers avulsos.
# ════════════════════════════════════════════════════════

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.** 8 tasks async serão migradas para Agent instances (Sprint 40+). IDEA_BANK.md Section 1.28 define 20 itens (A-001 a A-020) para a migração.

## Blocos Completos
| Bloco | Sprints | v | Foco |
|---|---|---|---|
| Chassi | 1-17 | 0.1–0.12 | VGA, heap, EventBus, IPC, SMP, APIC |
| Discovery | 18-22 | 0.13–0.17 | PCI, ACPI, MHI, Trust, LAPIC |
| Rede | 23-24 | 0.23–0.24 | RTL8139, smoltcp, NIC |
| Transformer | 26-27 | 0.26–0.27 | Attention, 4 layers, generate |
| HW-Aware LLM | 28-30 | 0.28–0.30 | PCI+USB+SMBIOS+xHCI+model |
| Capabilities | 31 | 0.31 | O que cada HW faz + skills + MHI |
| Self-Healing | 32-37 | 0.32–0.37 | Panic→LLM→recovery, taxonomy, respawn, checkpoint |
| **Agent/Skill-First** | **39-42** | **0.39-0.40** | **Agent trait, AgentRegistry, skill_loader, runtime skills** |

## Agent Landscape (16 agents — SystemAgent nativo, 7 LegacyTaskAgent wrapper, 8 structs)
| Código | Agente | Status | Tipo |
|---|---|---|---|
| A-001 | **SystemAgent** | ✅ agent | System — Oneshot, nativo |
| A-002 | MonitorAgent | 🟡 wrapper | System — LegacyTaskAgent |
| A-003 | HwBridgeAgent | 🟡 wrapper | Router — LegacyTaskAgent |
| A-004 | NetAgent | 🟡 wrapper | Network — LegacyTaskAgent |
| A-005 | InputAgent | 🟡 wrapper | Console — LegacyTaskAgent |
| A-006 | CortexAgent | 🟡 wrapper | Inference — LegacyTaskAgent |
| A-007 | HermesAgent | 🟡 wrapper | Router — LegacyTaskAgent |
| A-008 | ConsoleAgent | 🟡 wrapper | Console — LegacyTaskAgent |
| A-009 | NetDriverAgent | 📝 módulo | Driver |
| A-010 | UsbDriverAgent | 📝 módulo | Driver |
| A-011 | SelfHealAgent | ✅ struct | System |
| A-012 | MemoryAgent | ✅ struct | System |
| A-013 | PlatformAgent | ✅ módulo | System |
| A-014 | SMPAgent | ✅ módulo | System |
| A-015 | TrustAgent | ✅ struct | System |
| A-016 | SkillManagerAgent | 🟡 struct | Skill |

## Agentes e Tasks Atuais
- **SystemAgent** (nativo ✅) — SYSTEM_READY, EchoSkill
- **7 LegacyTaskAgent** (wrappers 🟡) — monitor, hw_bridge, network_agent, input, cortex_llm, intent_router, hermes_console
- **8 structs** (✅) — SelfHealAgent, MemoryAgent, PlatformAgent, SMPAgent, TrustAgent, SkillManagerAgent, NetDriverAgent, UsbDriverAgent

## Modelo
- 66.780 pares, GTX 1050, loss 1.156, 68 KB

## Skills em Runtime
- SKILL_STORAGE global, skills via `/add_skill <nome> <desc>` (LLM gera automaticamente)
- system prompt reconstruído a cada LLM_REQUEST

## Próximo (Sprint 41-42, mesmo bloco)
Migrar 7 LegacyTaskAgent para Agentes nativos + DriverAgents. EventDriven schedule.
