# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.59.0 🏆
#   BOOTLOADER 0.11 + FRAMEBUFFER UEFI + HERMES GRAFICO
#   20 agentes, tudo completo
# ════════════════════════════════════════════════════════

## 🏆 Marco: Bootloader 0.11.15 + Framebuffer UEFI
**29/06/2026** — Bootloader 0.9.34 → 0.11.15. Framebuffer 1280×720 funcional.
Hermes Cognitive rodando com display grafico (NeuralConsole).

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.**
20 agentes nativos. Bootloader 0.11.15 com `bootloader_api`.

## Agent Landscape (20 agentes — v0.59.0)
| Código | Agente | Tipo | Função |
|---|---|---|---|
| A-001 | SystemAgent | System (Oneshot) | Init, EchoSkill |
| A-002 | MonitorAgent | System (Oneshot) | SYSTEM_READY |
| A-003 | HwBridgeAgent | Router (Continuous) | Scancode IRQ bridge |
| A-004 | NetAgent | Network (Continuous) | smoltcp poll |
| A-005 | InputAgent | Console (Continuous) | Keyboard (PS/2 + USB xHCI) |
| A-006 | CortexAgent | Inference (Continuous) | LLM transformer + Medusa |
| A-007 | HermesAgent | Router (Continuous) | Intent routing, ReAct, Council |
| A-008 | DisplayAgent | Console (Continuous) | Framebuffer 1280x720 |
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

## Blocos Completos (17 blocos, 59 sprints)
| Bloco | Sprints | v | Foco |
|---|---|---|---|
| 1-15 | 1-57 | 0.1-0.57 | Todo o desenvolvimento ate Medusa+Ecosystem |
| **16. HW Real + USB** | **58** | **0.58** | Boot HW real, xHCI, FAT12, ATA, CAD |
| **17. Bootloader 0.11** | **59** | **0.59** | **Framebuffer UEFI, bootloader 0.11, Hermes grafico** |

## Aprendizados Chave (Bootloader 0.11)
1. BootloaderConfig necessario para physical_memory e stack size
2. Stack probe de 256KB exige kernel_stack_size >= 512KB
3. SS precisa ser recarregado apos init_idt() (evita #GP no iretq)
4. Framebuffer stride em PIXELS (multiplicar por bpp)
5. Pixel format BGR (3 bytes) vs BGRA32 (4 bytes)
6. build_image.py substitui bootimage tool
7. MinGW + caminho com acentos = linker failure

## Pendente Tecnico
- **Cintilacao no render**: NeuralConsole limpa tela inteira a cada tick
- **Framebuffer em HW real**: testar em notebook sem CSM
- **USB keyboard**: testar driver no notebook
- **USB Mass Storage BOT**: ~400 LOC para log persistente
- **Rede real**: driver e1000/r8169
