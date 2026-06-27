# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.45.0 🏆
#   AGENT/SKILL-FIRST + VIRTIO-GPU + BUGFIX ESTRUTURAL
#   Tudo é agente ou skill. Drivers manuais sem dependências externas.
# ════════════════════════════════════════════════════════

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.** 16 agentes nativos. IDEA_BANK.md com 330+ itens catalogados.

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
| Agent/Skill-First | 39-42 | 0.39-0.40 | Agent trait, AgentRegistry, 15 agentes nativos |
| Network Evolution | 43-44 | 0.41-0.42 | DHCP, ARP, VirtIO-net manual, NetPhy unificada |
| **Display + Bugfix** | **45** | **0.43-0.45** | **Framebuffer, DisplayAgent, VirtIO-GPU PCI caps, bugfix estrutural** |

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
| A-013 | PlatformAgent | ✅ Agent | System (Oneshot) | PCI+ACPI+APIC+SMP init |
| A-014 | MemoryAgent | ✅ Agent | System (Oneshot) | MHI + Arch inference |
| A-015 | GpuDriverAgent | ✅ Agent | Driver (Oneshot) | VirtIO-GPU detection |
| A-016 | HwDetectAgent | ✅ Agent | System (Oneshot) | HwIdentifySkill |

## Módulos de Hardware (bare-metal, 0 dependências externas)
| Módulo | LOC | Função |
|---|---|---|
| `virtio_gpu.rs` | 425 | Driver VirtIO-GPU PCI caps + MMIO + control queue |
| `virtio_net.rs` | 344 | Driver VirtIO-net PCI legacy I/O |
| `rtl8139.rs` | 246 | RTL8139 via I/O ports |
| `xhci.rs` | 118 | xHCI USB port scan |
| `display/fb.rs` | 130 | Framebuffer BGRA32 + DrawTarget |
| `display/console.rs` | 130 | NeuralConsole multi-região |
| `display/font.rs` | 95 | Fonte VGA 8x16 bitmap |
| `display/agent.rs` | 43 | DisplayAgent |
| `pci.rs` | 198 | PCI scan + capabilities parser |

## Bugfix Register (v0.45.0 — 5 bugs estruturais corrigidos)
| ID | Arquivo | Bug | Fix |
|---|---|---|---|
| H3 | `apic.rs` | SVR com vetor 0 → #DE espúrio | SVR = `(svr & 0xFFFFFF00) \| 0xFF \| 0x100` |
| H4 | `interrupts.rs` | IDT sem handlers para exceções 0-31 | 32 handlers nomeados com dump textual |
| H5 | `interrupts.rs` | EOI não enviado ao PIC escravo | `send_eoi(vector)` envia EQI ao 0xA0 se >= 40 |
| H11 | `pci.rs` | PCI scaneia funções 1-7 mesmo sem multifunction | `header_type` bit 7 verificado |
| H12 | `apic.rs` | IOAPIC RTEs 2-23 desmascaradas | Todas RTEs inicializadas com MASK=1 |

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

## Próximo
- VirtIO-GPU: debugar GET_DISPLAY_INFO (resposta 0x0)
- PCI capabilities: suporte a cfg_type=5 (PCI config access)
- Drivers GPU nativos: postergado (inviável sem crate compatível)
