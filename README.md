# Neural OS Hermes ⚡ — v0.45.0 — AGENT/SKILL-FIRST + VIRTIO-GPU + BUGFIX 🏆

**The first AI-native operating system. Bare-metal Rust. No Linux. No POSIX. No legacy.**

```
"We don't need an OS that runs AI.
 We need an OS that IS AI."
```

## 🔥 O que o torna único

### 0. Tudo é Agente ou Skill
**Bloco 11 (Sprints 39-42 consolidado):** Não existem "tasks", "services" ou "drivers" como conceitos separados. Cada entidade é um **Agente** com manifesto, ScheduleKind e ciclo de vida. Habilidades (**Skills**) são a interface de requisição-resposta.

16 agentes nativos (v0.45.0):
| Código | Agente | Tipo | Driver/Driver manual |
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
| A-011–A-016 | Boot agents | System/Driver | PCI, SMP, Trust, GPU |

### 1. Kernel que SE CURA
Quando um erro ocorre (Page Fault, GPF, OOM), o kernel não dá BSOD:
```
[PANIC] → FailureClass::classify() → SelfHeal::analyze() → RecoveryAction
  → restart_daemon | create_skill | log_and_continue
  → KERNEL_ERROR no EventBus → LLM analisa → sugere correção
  → Se falhar: lessons.push() → próxima tenta estratégia DIFERENTE
```

### 2. LLM que entende HARDWARE
Modelo treinado na GTX 1050 com **66.780 pares**:
```
PCI IDs (23.858) + USB IDs (23.963) + SMBIOS + Kernel + Git
+ Capabilities (25) + Error Recovery (16) + Learning (5)
```

### 3. Skills em Runtime (não compile-time)
Skills são carregadas em runtime via `SKILL_STORAGE` global. Usuário pode criar skills digitando `/add_skill <nome> <desc>` — a LLM gera automaticamente a skill em formato SKILL.md.

### 4. Skills editáveis sem recompilar
```
/show_skills      → lista skills ativas
/add_skill nome   → LLM gera skill baseada na descrição
/rm_skill nome    → remove skill
/reload_skills    → recarrega do seed
```

## 🧠 Self-Healing Architecture

```
ERRO OCORRE (Ring 0/1/2)
  ↓
FailureClass::classify()
  ├── MemoryFault     (Page Fault, OOM)
  ├── ExecutionFault  (GPF, Double Fault)
  ├── ResourceFault   (skill not found, timeout)
  ├── LogicFault      (assertion failed)
  ├── ExternalFault   (network, device)
  └── UnknownFault    (LLM consultado)
  ↓
SelfHeal::analyze(ctx, recover=true)
  ├── already_tried()? → estratégia ALTERNATIVA
  ├── RestartDaemon    → respawn da task
  ├── CreateSkill      → skill sob demanda
  └── LogAndContinue   → não fatal, segue
  ↓
KERNEL_ERROR → EventBus → LLM_REQUEST → LLM analisa
  ↓
SelfHeal::record_failure() → lessons.push()
  → Próximo erro similar: already_tried()=true → action DIFERENTE
```

## 🏗️ O que foi construído (45 sprints / 13 blocos)

| Bloco | Sprints | v | O que |
|---|---|---|---|
| Chassi | 1-17 | 0.1–0.12 | VGA, heap, EventBus, IPC, SMP, APIC |
| Discovery | 18-22 | 0.13–0.17 | PCI, ACPI, MHI, Trust, LAPIC |
| Rede | 23-24 | 0.23–0.24 | RTL8139, smoltcp |
| Transformer | 26-27 | 0.26–0.27 | Attention BitNet |
| HW-Aware LLM | 28-30 | 0.28–0.30 | PCI+USB training |
| Capabilities | 31 | 0.31 | HW mapping |
| Self-Healing | 32-37 | 0.32–0.37 | Failure taxonomy |
| Agent/Skill-First | 39-42 | 0.39–0.40 | Agent trait, 15 agentes |
| Network Evo | 43-44 | 0.41–0.42 | DHCP, ARP, VirtIO-net, NetPhy |
| **Display+Bugfix** | **45** | **0.43–0.45** | **Framebuffer, VirtIO-GPU, 5 bugs** |

## 🔬 Sources de conhecimento do LLM

| Fonte | Pares | O que aprendeu |
|---|---|---|
| PCI IDs | 23.858 | "8086:1237 → Intel 82441FX PMC" |
| USB IDs | 23.963 | "0781:5581 → SanDisk Ultra Fit" |
| SMBIOS | 21 | "SeaBIOS rel-1.16" |
| Kernel code | 31 | "O que é o executor?" |
| Git history | 100 | "Commit 8bedc80: smoltcp integrado" |
| Capabilities | 25 | "USB class 08 → Mass Storage, MHI HDD" |
| Error recovery | 16 | "Page Fault → compactar heap, restart daemon" |
| Learning | 5 | "feedback loop → already_tried → alternativa" |

## 📚 Module Map

| Módulo | Linhas | Função |
|---|---|---|
| `cortex.rs` | 360 | Transformer 4 layers, generate_text(), Tokenizer, model loader |
| `netstack.rs` | 321 | smoltcp Device trait, HTTP non-blocking, DNS resolve |
| `rtl8139.rs` | 250 | RTL8139 driver via I/O ports |
| `xhci.rs` | 118 | xHCI USB port scan, speed detection |
| `self_heal.rs` | 100 | FailureClass, SelfHeal, RecoveryAction, lessons |
| `memory.rs` | 253 | BitmapFrameAllocator, page table walk |
| `apic.rs` | 316 | LAPIC timer, IOAPIC, SMP IPI |
| `conversation.rs` | 79 | EventLog com KernelError |

## 🛠️ Quick Start

```powershell
cargo bootimage --release
qemu-system-x86_64 -m 2G -serial stdio -nic user,model=rtl8139 `
  -drive format=raw,file=bootimage-neural-kernel.bin -no-reboot -smp 2 -nographic
```

## ⚡ License — MIT
