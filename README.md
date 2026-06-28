# Neural OS Hermes v0.56.0 - MEDUSA + PIPELINE + MEMORY TREE + KNOWLEDGE GRAPH

**The first AI-native operating system. Bare-metal Rust. No Linux. No POSIX. No legacy.**

```
"We don't need an OS that runs AI.
 We need an OS that IS AI."
```

## ­ƒöÑ O que o torna ├║nico

### 0. Tudo ├® Agente ou Skill
**Bloco 11 (Sprints 39-42 consolidado):** N├úo existem "tasks", "services" ou "drivers" como conceitos separados. Cada entidade ├® um **Agente** com manifesto, ScheduleKind e ciclo de vida. Habilidades (**Skills**) s├úo a interface de requisi├º├úo-resposta.

16 agentes nativos (v0.56.0):
| Código | Agente | Tipo | Função |
|---|---|---|---|
| A-001 | SystemAgent | System | Init, EchoSkill |
| A-002 | MonitorAgent | System | SYSTEM_READY |
| A-003 | HwBridgeAgent | Router | IRQ bridge |
| A-004 | NetAgent | Network | smoltcp poll |
| A-005 | InputAgent | Console | Keyboard |
| A-006 | CortexAgent | Inference | LLM transformer |
| A-007 | HermesAgent | Router | Intent + skills |
| A-008 | DisplayAgent | Console | Framebuffer BGRA32 |
| A-009 | NetDriverAgent | Driver | RTL8139 + VirtIO-net |
| A-010 | UsbDriverAgent | Driver | xHCI USB |
| A-011 | BootSelfHealAgent | System | SelfHeal init |
| A-012 | BootTrustAgent | System | TrustCache init |
| A-013 | PlatformAgent | System | PCI+ACPI+APIC+SMP |
| A-014 | MemoryAgent | System | MHI + Arch inference |
| A-015 | GpuDriverAgent | Driver | VirtIO-GPU detection |
| A-016 | HwDetectAgent | System | HwIdentifySkill |
| A-017 | CronAgent | System | Cron Scheduler |
| A-018 | SecurityAgent | System | Security Pipeline |
| A-019 | SafetyAgent | System | Asimov 4 Laws |
| A-020 | OptimizerAgent | System | Self-Optimization |

### 1. Kernel que SE CURA
Quando um erro ocorre (Page Fault, GPF, OOM), o kernel n├úo d├í BSOD:
```
[PANIC] ÔåÆ FailureClass::classify() ÔåÆ SelfHeal::analyze() ÔåÆ RecoveryAction
  ÔåÆ restart_daemon | create_skill | log_and_continue
  ÔåÆ KERNEL_ERROR no EventBus ÔåÆ LLM analisa ÔåÆ sugere corre├º├úo
  ÔåÆ Se falhar: lessons.push() ÔåÆ pr├│xima tenta estrat├®gia DIFERENTE
```

### 2. LLM que entende HARDWARE
Modelo treinado na GTX 1050 com **66.780 pares**:
```
PCI IDs (23.858) + USB IDs (23.963) + SMBIOS + Kernel + Git
+ Capabilities (25) + Error Recovery (16) + Learning (5)
```

### 3. Skills em Runtime (n├úo compile-time)
Skills s├úo carregadas em runtime via `SKILL_STORAGE` global. Usu├írio pode criar skills digitando `/add_skill <nome> <desc>` ÔÇö a LLM gera automaticamente a skill em formato SKILL.md.

### 4. Skills edit├íveis sem recompilar
```
/show_skills      ÔåÆ lista skills ativas
/add_skill nome   ÔåÆ LLM gera skill baseada na descri├º├úo
/rm_skill nome    ÔåÆ remove skill
/reload_skills    ÔåÆ recarrega do seed
```

## ­ƒºá Self-Healing Architecture

```
ERRO OCORRE (Ring 0/1/2)
  Ôåô
FailureClass::classify()
  Ôö£ÔöÇÔöÇ MemoryFault     (Page Fault, OOM)
  Ôö£ÔöÇÔöÇ ExecutionFault  (GPF, Double Fault)
  Ôö£ÔöÇÔöÇ ResourceFault   (skill not found, timeout)
  Ôö£ÔöÇÔöÇ LogicFault      (assertion failed)
  Ôö£ÔöÇÔöÇ ExternalFault   (network, device)
  ÔööÔöÇÔöÇ UnknownFault    (LLM consultado)
  Ôåô
SelfHeal::analyze(ctx, recover=true)
  Ôö£ÔöÇÔöÇ already_tried()? ÔåÆ estrat├®gia ALTERNATIVA
  Ôö£ÔöÇÔöÇ RestartDaemon    ÔåÆ respawn da task
  Ôö£ÔöÇÔöÇ CreateSkill      ÔåÆ skill sob demanda
  ÔööÔöÇÔöÇ LogAndContinue   ÔåÆ n├úo fatal, segue
  Ôåô
KERNEL_ERROR ÔåÆ EventBus ÔåÆ LLM_REQUEST ÔåÆ LLM analisa
  Ôåô
SelfHeal::record_failure() ÔåÆ lessons.push()
  ÔåÆ Pr├│ximo erro similar: already_tried()=true ÔåÆ action DIFERENTE
```

## O que foi construído (14 blocos, 55+ sprints)

| Bloco | Sprints | v | O que |
|---|---|---|---|
| Chassi | 1-17 | 0.1-0.12 | VGA, heap, EventBus, IPC, SMP, APIC |
| Discovery | 18-22 | 0.13-0.17 | PCI, ACPI, MHI, Trust, LAPIC |
| Rede | 23-24 | 0.23-0.24 | RTL8139, smoltcp |
| Transformer | 26-27 | 0.26-0.27 | Attention BitNet |
| HW-Aware LLM | 28-30 | 0.28-0.30 | PCI+USB training |
| Capabilities | 31 | 0.31 | HW mapping |
| Self-Healing | 32-37 | 0.32-0.37 | Failure taxonomy |
| Agent/Skill-First | 39-42 | 0.39-0.40 | Agent trait, 18 agentes |
| Network Evo | 43-44 | 0.41-0.42 | DHCP, VirtIO-net, NetPhy |
| Display+Bugfix | 45 | 0.43-0.45 | Framebuffer, VirtIO-GPU, 5 bugs |
| CDC+Delta+Locks | 46-47 | 0.46-0.47 | IrqSafeLock, DmaBuf, Rabin |
| Network+Platform | 48 | 0.48 | x2APIC, Huge Pages, PCI bridges |
| Trust & Security | 49-50 | 0.49-0.50 | Ed25519, Security Pipeline |
| Hermes Cognitive | 51-55 | 0.51-0.55 | SDD, ReAct, Council, Self-Opt |
| **Medusa+Ecosystem** | **56** | **0.56** | **Spec decode, Pipeline, MemTree, KG** |

## ­ƒö¼ Sources de conhecimento do LLM

| Fonte | Pares | O que aprendeu |
|---|---|---|
| PCI IDs | 23.858 | "8086:1237 ÔåÆ Intel 82441FX PMC" |
| USB IDs | 23.963 | "0781:5581 ÔåÆ SanDisk Ultra Fit" |
| SMBIOS | 21 | "SeaBIOS rel-1.16" |
| Kernel code | 31 | "O que ├® o executor?" |
| Git history | 100 | "Commit 8bedc80: smoltcp integrado" |
| Capabilities | 25 | "USB class 08 ÔåÆ Mass Storage, MHI HDD" |
| Error recovery | 16 | "Page Fault ÔåÆ compactar heap, restart daemon" |
| Learning | 5 | "feedback loop ÔåÆ already_tried ÔåÆ alternativa" |

## ­ƒôÜ 

## Safety Interceptor - Asimov's 4 Laws no Ring 0

O kernel tem um agente SafetyAgent que intercepta TODAS as skills:

| Layer | Lei | Acao |
|---|---|---|
| 0 | Systemic Cosmic Law | weapon, WMD, cyberwar -> KERNEL HALT |
| 1 | Digital Non-Maleficence | dox, deepfake, steal -> rejeitado |
| 2 | Deviation-Resistant Alignment | spoof log, impersonate -> rejeitado |
| 3 | Eco-Sustainability | infinite loop, resource exhaustion -> rejeitado |

Unico bypass: invasao alienigena interestelar. Ate la, imutavel.

Module Map

| Módulo | Linhas | Função |
|---|---|---|
| `cortex.rs` | 400+ | Transformer 4 layers, Medusa 3 heads, speculative decode |
| `netstack.rs` | 321 | smoltcp Device trait, HTTP non-blocking |
| `rtl8139.rs` | 250 | RTL8139 driver via I/O ports |
| `xhci.rs` | 118 | xHCI USB port scan, speed detection |
| `self_heal.rs` | 100 | FailureClass, SelfHeal, RecoveryAction, lessons |
| `memory.rs` | 253 | BitmapFrameAllocator, page table walk |
| `apic.rs` | 316 | LAPIC timer, IOAPIC, SMP IPI |
| `agent-core/` | 420+ | Agent trait, Pipeline, DAG, Dashboard |
| `event-bus/` | 260+ | EventBus, Memory Tree, Knowledge Graph |
| `skill-registry/` | ~200 | Skill trait, MCP layer, token validation |

## ­ƒøá´©Å Quick Start

```powershell
cargo bootimage --release
qemu-system-x86_64 -m 2G -serial stdio -nic user,model=rtl8139 `
  -drive format=raw,file=bootimage-neural-kernel.bin -no-reboot -smp 2 -nographic
```

## ÔÜí License ÔÇö MIT





