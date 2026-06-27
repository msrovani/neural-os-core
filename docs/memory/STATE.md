# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.50.0 🏆
#   AGENT/SKILL-FIRST + BLOCOS 12+13 COMPLETOS
#   Tudo é agente ou skill. Drivers manuais sem dependências externas.
# ════════════════════════════════════════════════════════

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.** 16 agentes nativos. IDEA_BANK.md com 330+ itens catalogados.



## Agent Landscape (16 agentes nativos — v0.45.0)
| Código | Agente | Status | Tipo | Função |
|---|---|---|---|---|
| A-001 | SystemAgent | ✅ Agent | System (Oneshot) | Init, SYSTEM_READY, EchoSkill |
| A-002 | MonitorAgent | ✅ Agent | System (Oneshot) | Publica SYSTEM_READY |
| A-003 | HwBridgeAgent | ✅ Agent | Router (Continuous) | Scancode IRQ bridge |
| A-004 | NetAgent | ✅ Agent | Network (Continuous) | smoltcp poll + HTTP |
| A-005 | InputAgent | ✅ Agent | Console (Continuous) | Keyboard buffer |
| A-006 | CortexAgent | ✅ Agent | Inference (Continuous) | LLM generate_text() |
| A-007 | HermesAgent | ✅ Agent | Router (Continuous) | Intent routing + skills |
| A-008 | DisplayAgent | ✅ Agent | Console (Continuous) | VGA+serial+fb output |
| A-009 | NetDriverAgent | ✅ Agent | Driver (Oneshot) | RTL8139 init |
| A-010 | UsbDriverAgent | ✅ Agent | Driver (Oneshot) | xHCI port scan |
| A-011 | BootSelfHealAgent | ✅ Agent | System (Oneshot) | SelfHeal init |
| A-012 | BootTrustAgent | ✅ Agent | System (Oneshot) | TrustCache init |
| A-013 | PlatformAgent | ✅ Agent | System (Oneshot) | PCI+ACPI+APIC+SMP |
| A-014 | MemoryAgent | ✅ Agent | System (Oneshot) | MHI + Arch inference |
| A-015 | GpuDriverAgent | ✅ Agent | Driver (Oneshot) | VirtIO-GPU detection |
| A-016 | HwDetectAgent | ✅ Agent | System (Oneshot) | HwIdentifySkill |
| A-017 | **CronAgent** | ✅ Agent | System (Continuous) | Cron Scheduler (#232) |
| A-018 | **SecurityAgent** | ✅ Agent | System (Continuous) | Security Pipeline (#260) |

## Módulos do Kernel (v0.50.0, ~6500 LOC)
| Módulo | LOC | Função |
|---|---|---|
| `virtio_gpu.rs` | 420 | Driver VirtIO-GPU PCI caps + MMIO |
| `virtio_net.rs` | 344 | Driver VirtIO-net manual |
| `rtl8139.rs` | 246 | RTL8139 via I/O ports |
| `xhci.rs` | 118 | xHCI USB port scan |
| `cortex.rs` | 366 | Transformer 4 layers, BitNet |
| `apic.rs` | 388 | LAPIC/IOAPIC/x2APIC, IPI, page UC |
| `interrupts.rs` | 235 | IDT 0-31, PIC/APIC EOI |
| `pci.rs` | 220 | PCI scan, capabilities, bridges |
| `trust.rs` | 200 | TrustCache, PermissionMode, Ed25519 |
| `identity.rs` | 84 | Ed25519 verify, CapabilityToken enum |
| `security.rs` | 130 | 5 detectores + correlação |
| `cron.rs` | 90 | Cron Scheduler periódico |
| `chunker.rs` | 110 | CDC Rabin rolling hash |
| `delta.rs` | 130 | XOR Delta ArchiveTensor |
| `dma.rs` | 60 | DmaBuf alloc/free UC |
| `display/` | 400 | Framebuffer, NeuralConsole, font |
| `self_heal.rs` | 230 | Checkpoint, FailureClass, snapshot |

## Blocos Completos
| Bloco | Sprints | v | Foco |
|---|---|---|---|
| Chassi | 1-17 | 0.1–0.12 | VGA, heap, EventBus, SMP, APIC |
| Discovery | 18-22 | 0.13–0.17 | PCI, ACPI, MHI, Trust |
| Rede | 23-24 | 0.23–0.24 | RTL8139, smoltcp |
| Transformer | 26-27 | 0.26–0.27 | Attention BitNet |
| HW-Aware LLM | 28-30 | 0.28–0.30 | PCI+USB training |
| Capabilities | 31 | 0.31 | HW→skill mapping |
| Self-Healing | 32-37 | 0.32–0.37 | Failure taxonomy |
| Agent/Skill-First | 39-42 | 0.39–0.40 | Agent trait, 16 agents |
| Network Evo | 43-44 | 0.41–0.42 | DHCP, ARP, VirtIO-net |
| Display+Bugfix | 45 | 0.43–0.45 | Framebuffer, H3-H12 bugs |
| CDC+Delta+Locks | 46-47 | 0.46–0.47 | IrqSafeLock, DmaBuf, CDC, XOR |
| **Bloco 12 (Network+Platform)** | **48** | **0.48** | **x2APIC, Huge Pages, PCI bridges, Cron, MCP** |
| **Bloco 13 (Trust & Security)** | **49-50** | **0.49-0.50** | **Ed25519, Security Pipeline, Mask Secrets** |

## Bugs analisados e considerados inválidos
| ID | Motivo |
|---|---|
| H1 | `e1000.rs` removido no Sprint 24 |
| H2 | Código DHCP/ARP em `proto.rs` removido. DHCP via smoltcp socket |
| H6 | `allocate_below_1mb()` já chamado sob `GLOBAL_ALLOCATOR.lock()` |
| H7 | Ponte PCI já lê offset 0x19 desde v0.17.1 |

## Modelo
- 66.780 pares, GTX 1050, loss 1.156, 68 KB

## Skills em Runtime
- SKILL_STORAGE global, skills via `/add_skill <nome> <desc>` (LLM gera)
- `requires_network: bool` no frontmatter
- system prompt reconstruído a cada LLM_REQUEST

## Próximo (Bloco 14 — Hermes Cognitive + Self-Optimization)
- Runtime SDD (#178) — goal/context/plan/rollback antes de skill
- Algorithm loop 7 fases (#190) — THINK→PLAN→BUILD→EXECUTE→VERIFY→LEARN
- Council skill (#191) — 3 vozes Otimista/Cético/Pragmático
- Usage Pattern Analyzer (#157) — LLM detecta workflow do usuário
- Dynamic Resource Scaling (#160) — MHI auto-ajuste por uso real

## Pendente técnico
- VirtIO-GPU GET_DISPLAY_INFO — feature select corrigido, MMIO mapeado, queue OK (QEMU TCG lento)
- MCP TCP Server — smoltcp não tem socket server nativo; requer extensão
- WASM sandbox — `wasmi` + redesign do scheduler (~1500 LOC)
- acpi/raw-cpuid crates (#34-35) — integração com crate externa
