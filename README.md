# Neural OS Hermes ⚡ — v0.40.0 — AGENT/SKILL-FIRST 🏆

**The first AI-native operating system. Bare-metal Rust. No Linux. No POSIX. No legacy.**

```
"We don't need an OS that runs AI.
 We need an OS that IS AI."
```

## 🔥 O que o torna único

### 0. Tudo é Agente ou Skill
**Bloco 11 (Sprints 39-42 consolidado):** Não existem "tasks", "services" ou "drivers" como conceitos separados. Cada entidade é um **Agente** com manifesto, ScheduleKind e ciclo de vida. Habilidades (**Skills**) são a interface de requisição-resposta.

16 agentes (Sprint 40 — SystemAgent nativo, 7 LegacyTaskAgent wrapper):
| Código | Agente | Status | Tipo |
|---|---|---|---|
| A-001 | **SystemAgent** | ✅ Agent | Oneshot → SYSTEM_READY |
| A-002 | MonitorAgent | 🟡 wrapper | LegacyTaskAgent |
| A-003 | HwBridgeAgent | 🟡 wrapper | LegacyTaskAgent |
| A-004 | NetAgent | 🟡 wrapper | LegacyTaskAgent |
| A-005 | InputAgent | 🟡 wrapper | LegacyTaskAgent |
| A-006 | CortexAgent | 🟡 wrapper | LegacyTaskAgent |
| A-007 | HermesAgent | 🟡 wrapper | LegacyTaskAgent |
| A-008 | ConsoleAgent | 🟡 wrapper | LegacyTaskAgent |
| A-009..A-016 | DriverAgents + SystemAgents | ✅ struct | PCI, SMP, Trust, Memory |

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

## 🏗️ O que foi construído (40 sprints / 11 blocos)

| Bloco | Sprints | O que |
|---|---|---|
| Chassi | 1-17 | VGA, heap, EventBus, IPC, SMP, APIC |
| Discovery | 18-22 | PCI, ACPI, MHI, Trust, LAPIC timer |
| Rede | 23-24 | RTL8139, smoltcp, e1000 removal |
| Transformer | 26-27 | Attention, 4 layers BitNet, generate_text() |
| HW-Aware LLM | 28-30 | PCI+USB+SMBIOS training, xHCI driver |
| Capabilities | 31 | O que cada hardware FAZ + skills + MHI |
| Self-Healing | 32-37 | Panic → LLM → recovery, Failure Taxonomy, checkpoint |
| **Agent/Skill-First** | **39-42** | **Agent trait, AgentRegistry, skill_loader, runtime skills** |

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
