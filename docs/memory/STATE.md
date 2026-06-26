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
| Skill Loader | 39 | 0.39 | skills.md + runtime SKILL_STORAGE + /add_skill via LLM |

## Agent Landscape (16 agents planejados, 8 como tasks)
| Código | Agente | Status | Tipo |
|---|---|---|---|
| A-001 | SystemAgent | 🟡 task | System |
| A-002 | MonitorAgent | 🟡 task | System |
| A-003 | HwBridgeAgent | 🟡 task | Router |
| A-004 | NetAgent | 🟡 task | Network |
| A-005 | InputAgent | 🟡 task | Console |
| A-006 | CortexAgent | 🟡 task | Inference |
| A-007 | HermesAgent | 🟡 task | Router |
| A-008 | ConsoleAgent | 🟡 task | Console |
| A-009 | NetDriverAgent | 📝 módulo | Driver |
| A-010 | UsbDriverAgent | 📝 módulo | Driver |
| A-011 | SelfHealAgent | ✅ struct | System |
| A-012 | MemoryAgent | ✅ struct | System |
| A-013 | PlatformAgent | ✅ módulo | System |
| A-014 | SMPAgent | ✅ módulo | System |
| A-015 | TrustAgent | ✅ struct | System |
| A-016 | SkillManagerAgent | 🟡 struct | Skill |

## 8 Tasks Atuais (serão Agent instances)
1. system → 2. monitor → 3. hw_bridge → 4. network_agent →
5. input → 6. cortex_llm → 7. intent_router → 8. hermes_console

## Modelo
- 66.780 pares, GTX 1050, loss 1.156, 68 KB

## Skills em Runtime
- SKILL_STORAGE global (Mutex<SkillLoader>), seed do disco, modificável via /show_skills, /add_skill, /rm_skill, /reload_skills
- /add_skill <nome> <desc> → LLM gera skill automaticamente
- system prompt reconstruído a cada LLM_REQUEST a partir das skills atuais

## Próximo Sprint (40 — Agent-First Refactoring)
Encapsular 8 tasks em Agent trait. AgentRegistry + AgentScheduler substituem NeuralExecutor. Manter compatibilidade (migração aditiva).
