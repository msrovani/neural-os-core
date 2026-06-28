# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.58.0 🏆
#   BOOT EM HARDWARE REAL + 20 AGENTES + TUDO COMPLETO
#   USB keyboard, FAT12 log, ATA, MBR, CAD
# ════════════════════════════════════════════════════════

## 🏆 Marco: Primeiro Boot em Hardware Real
**28/06/2026** — Neural OS Hermes bootou em notebook físico x86-64
via SDHC USB. VGA, PCI, ACPI, APIC, SMP, Hermes rodando. Zero panics.

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.**
20 agentes nativos. 16 blocos concluídos em 58 sprints.

## Agent Landscape (20 agentes — v0.58.0)
| Código | Agente | Tipo | Função |
|---|---|---|---|
| A-001 | SystemAgent | System (Oneshot) | Init, EchoSkill |
| A-002 | MonitorAgent | System (Oneshot) | SYSTEM_READY |
| A-003 | HwBridgeAgent | Router (Continuous) | Scancode IRQ bridge |
| A-004 | NetAgent | Network (Continuous) | smoltcp poll |
| A-005 | InputAgent | Console (Continuous) | Keyboard (PS/2 + USB xHCI) |
| A-006 | CortexAgent | Inference (Continuous) | LLM transformer + Medusa |
| A-007 | HermesAgent | Router (Continuous) | Intent routing, ReAct, Council |
| A-008 | DisplayAgent | Console (Continuous) | Framebuffer + VGA |
| A-009 | NetDriverAgent | Driver (Oneshot) | RTL8139 + VirtIO-net |
| A-010 | UsbDriverAgent | Driver (Oneshot) | xHCI init |
| A-011 | BootSelfHealAgent | System (Oneshot) | SelfHeal init |
| A-012 | BootTrustAgent | System (Oneshot) | TrustCache init |
| A-013 | PlatformAgent | System (Oneshot) | PCI+ACPI+APIC+SMP |
| A-014 | MemoryAgent | System (Oneshot) | MHI + Arch |
| A-015 | GpuDriverAgent | Driver (Oneshot) | VirtIO-GPU |
| A-016 | HwDetectAgent | System (Oneshot) | HwIdentifySkill |
| A-017 | CronAgent | System (Continuous) | Cron Scheduler |
| A-018 | SecurityAgent | System (Continuous) | Security Pipeline |
| A-019 | SafetyAgent | System (Continuous) | Asimov 4 Laws |
| A-020 | OptimizerAgent | System (Continuous) | Self-Optimization |

## Módulos do Kernel (v0.58.0, ~8000+ LOC)
| Módulo | LOC | Função |
|---|---|---|
| `cortex.rs` | 400+ | Transformer 4 layers, Medusa 3 heads, sample |
| `xhci.rs` | 160 | xHCI USB HID keyboard driver |
| `fat.rs` | 110 | MBR parser + FAT12 append |
| `ata.rs` | 110 | ATA PIO read/write |
| `virtio_gpu.rs` | 440 | VirtIO-GPU PCI caps + MMIO |
| `virtio_net.rs` | 344 | VirtIO-net manual |
| `rtl8139.rs` | 246 | RTL8139 via I/O ports |
| `apic.rs` | 430 | LAPIC/IOAPIC/x2APIC, IPI, MMIO |
| `interrupts.rs` | 235 | IDT 0-31, PIC/APIC EOI |
| `pci.rs` | 220 | PCI scan, capabilities, bridges |
| `self_heal.rs` | 230 | FailureClass, checkpoint, snapshot |
| `trust.rs` | 200 | TrustCache, PermissionMode, Ed25519 |
| `security.rs` | 130 | 5 detectores + correlação |
| `display/` | 500 | Framebuffer, GpuDevice, console, font |
| `agent-core/` | 420 | Agent trait, Pipeline, DAG, Dashboard |
| `event-bus/` | 260 | EventBus, Memory Tree, KG, 7 sub-módulos |
| `identity.rs` | 45 | Ed25519 via ed25519-compact |
| `plugin_hub.rs` | 63 | Plugin install/remove/scan |
| `serial.rs` | 73 | Boot log 64KB com timestamp |

## Blocos Completos (16 blocos)
| Bloco | Sprints | v | Foco |
|---|---|---|---|
| Chassi | 1-17 | 0.1-0.12 | VGA, heap, EventBus, SMP, APIC |
| Discovery | 18-22 | 0.13-0.17 | PCI, ACPI, MHI, Trust |
| Rede | 23-24 | 0.23-0.24 | RTL8139, smoltcp |
| Transformer | 26-27 | 0.26-0.27 | Attention BitNet |
| HW-Aware LLM | 28-30 | 0.28-0.30 | PCI+USB training |
| Capabilities | 31 | 0.31 | HW→skill mapping |
| Self-Healing | 32-37 | 0.32-0.37 | Failure taxonomy |
| Agent/Skill-First | 39-42 | 0.39-0.40 | Agent trait, 18 agents |
| Network Evolution | 43-44 | 0.41-0.42 | DHCP, VirtIO-net |
| Display+Bugfix | 45 | 0.43-0.45 | Framebuffer, VirtIO-GPU |
| CDC+Delta+Locks | 46-47 | 0.46-0.47 | IrqSafeLock, Rabin, XOR |
| Network+Platform | 48 | 0.48 | x2APIC, Huge Pages, Cron |
| Trust & Security | 49-50 | 0.49-0.50 | Ed25519, Security Pipeline |
| Hermes Cognitive | 51-55 | 0.51-0.55 | SDD, ReAct, Council, Self-Opt |
| Memory+Ecosystem | 56-57 | 0.56-0.57 | Medusa, MemTree, KG, Medusa |
| **HW Real + USB** | **58** | **0.58** | **🏆 Boot HW real, xHCI, FAT12, ATA, CAD** |

## Modelo
- 66.780 pares, GTX 1050, loss 1.156, 68 KB .bitnet
- Medusa 3-head speculative decoding
- Sampling: argmax, top-k, temperature

## Pendente Técnico
- **Prompt interativo >** — Hermes aguardar input do usuário (~50 LOC)
- **Framebuffer UEFI** — upgrade bootloader 0.9.34 → 0.11+
- **Driver e1000/r8169** — rede em hardware real
- **Plugin Hub MCP Index (#236)** — conclusão
- **WASM sandbox** — wasmi embedder (~1500 LOC)
