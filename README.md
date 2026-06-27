# Neural OS Hermes v0.55.0 - HERMES COGNITIVE + SELF-OPTIMIZATION

**The first AI-native operating system. Bare-metal Rust. No Linux. No POSIX. No legacy.**

```
"We don't need an OS that runs AI.
 We need an OS that IS AI."
```

## ­ƒöÑ O que o torna ├║nico

### 0. Tudo ├® Agente ou Skill
**Bloco 11 (Sprints 39-42 consolidado):** N├úo existem "tasks", "services" ou "drivers" como conceitos separados. Cada entidade ├® um **Agente** com manifesto, ScheduleKind e ciclo de vida. Habilidades (**Skills**) s├úo a interface de requisi├º├úo-resposta.

16 agentes nativos (v0.55.0):
| C├│digo | Agente | Tipo | Driver/Driver manual |
|---|---|---|---|
| A-001 | SystemAgent | System | Init, EchoSkill |
| A-002 | MonitorAgent | System | SYSTEM_READY |
| A-003 | HwBridgeAgent | Router | IRQ bridge |
| A-004 | NetAgent | Network | smoltcp poll |
| A-005 | InputAgent | Console | Keyboard |
| A-006 | CortexAgent | Inference | LLM transformer |
| A-007 | HermesAgent | Router | Intent + skills |
| A-008 | **DisplayAgent** | Console | **Framebuffer BGRA32** |
| A-009 | NetDriverAgent | Driver | RTL8139 + VirtIO-net |
| A-010 | UsbDriverAgent | Driver | xHCI USB |
| A-011ÔÇôA-016 | Boot agents | System/Driver | PCI, SMP, Trust, GPU |

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

## ­ƒÅù´©Å O que foi constru├¡do (45 sprints / 13 blocos)

| Bloco | Sprints | v | O que |
|---|---|---|---|
| Chassi | 1-17 | 0.1ÔÇô0.12 | VGA, heap, EventBus, IPC, SMP, APIC |
| Discovery | 18-22 | 0.13ÔÇô0.17 | PCI, ACPI, MHI, Trust, LAPIC |
| Rede | 23-24 | 0.23ÔÇô0.24 | RTL8139, smoltcp |
| Transformer | 26-27 | 0.26ÔÇô0.27 | Attention BitNet |
| HW-Aware LLM | 28-30 | 0.28ÔÇô0.30 | PCI+USB training |
| Capabilities | 31 | 0.31 | HW mapping |
| Self-Healing | 32-37 | 0.32ÔÇô0.37 | Failure taxonomy |
| Agent/Skill-First | 39-42 | 0.39ÔÇô0.40 | Agent trait, 15 agentes |
| Network Evo | 43-44 | 0.41ÔÇô0.42 | DHCP, ARP, VirtIO-net, NetPhy |
| **Display+Bugfix** | **45** | **0.43ÔÇô0.45** | **Framebuffer, VirtIO-GPU, 5 bugs** |

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

## ­ƒôÜ Module Map

| M├│dulo | Linhas | Fun├º├úo |
|---|---|---|
| `cortex.rs` | 360 | Transformer 4 layers, generate_text(), Tokenizer, model loader |
| `netstack.rs` | 321 | smoltcp Device trait, HTTP non-blocking, DNS resolve |
| `rtl8139.rs` | 250 | RTL8139 driver via I/O ports |
| `xhci.rs` | 118 | xHCI USB port scan, speed detection |
| `self_heal.rs` | 100 | FailureClass, SelfHeal, RecoveryAction, lessons |
| `memory.rs` | 253 | BitmapFrameAllocator, page table walk |
| `apic.rs` | 316 | LAPIC timer, IOAPIC, SMP IPI |
| `conversation.rs` | 79 | EventLog com KernelError |

## ­ƒøá´©Å Quick Start

```powershell
cargo bootimage --release
qemu-system-x86_64 -m 2G -serial stdio -nic user,model=rtl8139 `
  -drive format=raw,file=bootimage-neural-kernel.bin -no-reboot -smp 2 -nographic
```

## ÔÜí License ÔÇö MIT




