# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.59.2 🏆
#   ECOSYSTEM BATCH 3 — 12 REPOS PORTADOS, 173 AGENTES
#   Bootloader 0.11 + Framebuffer 1280×720
#   HW Agents + The Agency (147 agents)
# ════════════════════════════════════════════════════════

## 🏆 Marcos Acumulados
- **v0.56.0** — Medusa 3-head speculative decoding + Pipeline + Memory Tree + KG + DAG
- **v0.57.0** — Bloco 15+16+17: Memory Systems + Ecosystem + LLM v2
- **v0.57.1** — Consolidation: Plugin Hub, x2APIC, Ed25519, SMP stacks
- **v0.58.0** — 🏆 Boot em Hardware Real (SDHC USB) + xHCI + FAT12 + ATA + CAD
- **v0.59.0** — 🏆 Bootloader 0.11.15 + Framebuffer 1280×720 UEFI
- **v0.59.1** — The Agency (147 agents) + HW Agents
- **v0.59.2** — Ecosystem Batch 3: 12 repos portados (redox, Theseus, Embassy, Tock, Swarm, RagaAI, Swarms, SuperAGI)

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.**
173+ agentes: 20 nativos + 147 The Agency + ~6 HW agents.
Bootloader 0.11.15 com `bootloader_api`. Framebuffer 1280×720.
Build via `cargo build --release` + `tools/build_image.py`.

## Agent Landscape (173+ agentes — v0.59.2)
| Código | Agente | Tipo | Função |
|---|---|---|---|
| A-001 | SystemAgent | System (Oneshot) | Init, EchoSkill |
| A-002 | MonitorAgent | System (Oneshot) | SYSTEM_READY |
| A-003 | HwBridgeAgent | Router (Continuous) | Scancode IRQ bridge |
| A-004 | NetAgent | Network (Continuous) | smoltcp poll (RTL8139) |
| A-005 | InputAgent | Console (Continuous) | Keyboard (PS/2 + USB xHCI) |
| A-006 | CortexAgent | Inference (Continuous) | LLM transformer + Medusa speculative |
| A-007 | HermesAgent | Router (Continuous) | Intent routing + ReAct + Council + Handoff |
| A-008 | DisplayAgent | Console (Continuous) | Framebuffer NeuralConsole 1280×720 |
| A-009 | NetDriverAgent | Driver (Oneshot) | RTL8139 + VirtIO-net |
| A-010 | UsbDriverAgent | Driver (Oneshot) | xHCI init |
| A-011 | BootSelfHealAgent | System (Oneshot) | SelfHeal init |
| A-012 | BootTrustAgent | System (Oneshot) | TrustCache Ed25519 |
| A-013 | PlatformAgent | System (Oneshot) | PCI+ACPI+APIC+SMP+x2APIC |
| A-014 | MemoryAgent | System (Oneshot) | MHI + Arch |
| A-015 | GpuDriverAgent | Driver (Oneshot) | VirtIO-GPU + framebuffer |
| A-016 | HwDetectAgent | System (Oneshot) | HwIdentifySkill |
| A-017 | CronAgent | System (Continuous) | Cron Scheduler |
| A-018 | SecurityAgent | System (Continuous) | Security Pipeline |
| A-019 | SafetyAgent | System (Continuous) | Asimov 4 Laws |
| A-020 | OptimizerAgent | System (Continuous) | Self-Optimization |
| A-021+ | The Agency (147) | Specialist (Passive) | 12 divisões, por demanda |
| A-168+ | HW Agents (~6) | HwSpecialist (Passive) | Por dispositivo PCI, activate_for_intent |

## Blocos Completos (22 blocos, 59 sprints)
| Bloco | Sprints | v | Foco |
|---|---|---|---|
| 1-14 | 1-51 | 0.1-0.55 | Todo desenvolvimento ate Hermes Cognitive |
| 15 | 52-54 | 0.57 | Memory Systems (Dedup, Privacy, Atkinson-Shiffrin) |
| 16 | 55 | 0.57 | Ecosystem Integration (SuperContext, SkillIndex) |
| 17 | 56 | 0.57 | Cortex LLM v2 (Sampling, Codebook VQ, Medusa) |
| 18 | 57 | 0.56 | Ecosystem Batch (Pipeline, DAG, Dashboard) |
| **19. HW Real** | **58** | **0.58** | **Boot HW real, xHCI, FAT12, ATA, CAD** |
| **20. Bootloader 0.11** | **59** | **0.59** | **Framebuffer UEFI, bootloader 0.11** |
| **21. The Agency** | **59** | **0.59.1** | **147 agents + HW Agents** |
| **22. Ecosystem Batch 3** | **59** | **0.59.2** | **12 repos portados** |

## Aprendizados Chave (Bootloader 0.11)
1. BootloaderConfig necessario para physical_memory e stack size
2. Stack probe de 256KB exige kernel_stack_size >= 512KB
3. SS precisa ser recarregado apos init_idt() (evita #GP no iretq)
4. Framebuffer stride em PIXELS (multiplicar por bpp)
5. Pixel format BGR (3 bytes) vs BGRA32 (4 bytes)
6. build_image.py substitui bootimage tool
7. MinGW + caminho com acentos = linker failure

## Pendente Técnico
- **Cintilação no framebuffer**: NeuralConsole limpa tela inteira a cada tick (double buffering)
- **VGA scroll em HW real**: Notebook moderno sem CSM (framebuffer deve resolver)
- **FAT12 log via USB Mass Storage**: stub `usb_msc.rs` precisa de driver BOT/UFI (~400 LOC)
- **Rede real**: apenas RTL8139 (QEMU), falta e1000/r8169 (~300 LOC)
- **GGUF loader**: modelos 9B+ precisam de heap >5GB
- **SmileyOS shell**: port de 40+ comandos (ls, cat, ps, uptime, theme)
