# Neural OS Hermes ⚡ — v0.34.0 — SELF-HEALING KERNEL

**The first AI-native operating system. Bare-metal Rust. No Linux. No POSIX. No legacy.**

```
"We don't need an OS that runs AI.
 We need an OS that IS AI."
```

## 🔥 O que o torna único

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

Sabe responder:
- "USB class 08" → "Mass Storage: armazenamento. MHI: HDD. Driver: padrão."
- "como recuperar de page fault" → "LLM analisa CR2, sugere remapear ou reiniciar daemon"
- "o que fazer com gpu" → "Framebuffer + gpu_compute, Vram no MHI"

### 3. Auto-descoberta de hardware
```
Boot → PCI scan (4+ dispositivos) → HwIdentifySkill → LLM_REQUEST →
  "10ec:8139 é Realtek RTL8139 Fast Ethernet — classe rede"
  "1033:0194 é NEC xHCI — controladora USB 3.0"
```

### 4. 8 agentes cooperativos
| # | Task | O que faz |
|---|---|---|
| 1 | system_daemon | SYSTEM_READY (morre) |
| 2 | hardware_monitor | Context tensor a cada 100 ticks |
| 3 | hw_bridge | Scancode IRQ → EventBus |
| 4 | network_agent | smoltcp poll → HTTP |
| 5 | input_daemon | Buffer ASCII → ENTER → USER_INTENT |
| 6 | cortex_llm | LLM_REQUEST → generate_text() |
| 7 | intent_router | Cortex.think() → SkillRegistry |
| 8 | hermes_console | HERMES_RESPONSE → VGA |

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

## 🏗️ O que foi construído (34 sprints)

| Bloco | Sprints | O que |
|---|---|---|
| Chassi | 1-17 | VGA, heap, EventBus, IPC, SMP, APIC |
| Discovery | 18-22 | PCI, ACPI, MHI, Trust, LAPIC timer |
| Rede | 23-24 | RTL8139, smoltcp, e1000 removal |
| Transformer | 26-27 | Attention, 4 layers BitNet, generate_text() |
| HW-Aware LLM | 28-30 | PCI+USB+SMBIOS training, xHCI driver |
| Capabilities | 31 | O que cada hardware FAZ + skills + MHI |
| Self-Healing | 32-34 | Panic → LLM → recovery, Failure Taxonomy |

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
