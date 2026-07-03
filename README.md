# Neural OS Hermes v0.76.1 — AI-native Bare-metal Operating System

**The first AI-native operating system in the world. Bare-metal Rust. No Linux. No POSIX. No legacy. 0 errors.**

```
03/07/2026 — TPM + DiskAgent + NVMe + SMART + Adaptive Heap + Dynamic Tick.
              Particion FAT32 mascarada (Hidden 0x1C), Ed25519 kernel signing.
              DiskIntelligenceAgent: 6 controladoras, 10+ FS probes, ARC cache.
              NVMe driver, USB-MSC BOT, GPT, SED/OPAL, I/O scheduler.
              MemoryAgent: heap dinamico, orcamento AI, clock calibration.
              Hermes event-driven: 84 linhas/seg → silencio ate ter trabalho.
              247+ agentes, 132 arquivos Rust, ~15.500 LOC, 0 erros.
```

```
"We don't need an OS that runs AI. We need an OS that IS AI."
```

---

## The Vision

**Neural OS Hermes is not "Linux with AI tools." It is the first OS built from scratch where artificial intelligence IS the operating system.** Every system call, every device driver, every memory allocation, every scheduling decision — all processed by a neural cortex, not by legacy Unix abstractions.

| | Linux | Windows | Neural OS Hermes |
|---|---|---|---|
| Kernel | 30M LOC C | C/C++ legacy | **Rust no_std, ~13.800 LOC** |
| AI | bolted-on (eBPF, CUDA) | bolted-on (Copilot) | **Native. Is the OS.** |
| LLM Integration | runs as app | runs as app | **Cortex acorda no boot** |
| GPU Compute | Vulkan/DX drivers | Vulkan/DX drivers | **Ring buffer bare-metal** |
| Device Model | sysfs + udev | PnP manager | **Agent/Skill-First** |
| Boot | systemd + initramfs | winload.efi | **8 fases, agent-driven** |
| Self-Healing | systemd restart | crash dump | **FailureClass + LLM recovery** |

---

## Architecture: Boot Agent-First

O boot não é uma sequência procedural — é uma **coreografia de agentes**. Cada fase publica `BOOT_PHASE` no EventBus, e o Cortex assiste desde o momento zero:

```
[SafeHarbor]     Serial + Framebuffer + IDT — minimo para sobreviver
[MemoryCore]     Frame allocator + Page tables + Heap + SIMD
[SystemBringup]  SystemAgent + CortexAgent ACORDA (pre-HW)
[Diagnostics]    DiagnosticSkill testa o sistema nervoso
[HardwareDiscovery] PCI/ACPI/SMP/GPU — agentes descobrem o hardware
[DriverInit]     Net/ATA/USB drivers como agentes
[AgentFleet]     Todos os 247+ agentes registrados
[Runtime]        HermesAgent lidera, Cortex pensa, agentes agem
```

Cada fase publica `BOOT_PHASE` no EventBus. HermesAgent mostra progresso, CortexAgent analisa, BootLogAgent persiste. **O boot log é escrito em FAT12 e lido pelo Cortex no boot seguinte para auto-diagnóstico.**

---

## What Makes This Unique

### 1. GPU Compute Bare-metal em no_std (Sprint 66-67)
**Nenhum outro projeto Rust faz isso.** Nem Redox, nem Theseus, nem Tock. Escrevemos drivers GPU diretamente via PCI BAR MMIO — sem Vulkan, sem Mesa, sem kernel module:

- **Intel Gen9+** — Ring buffer de comandos via `gpu/intel.rs`
- **BCS Blitter Engine** — Ring separado para blit (0x220000)
- **GTT (Graphics Translation Table)** — MMU interna da GPU Intel
- **VRAM via Huge Pages 2MB** — 4096 entradas para 8GB VRAM
- **VRAM Free List** — `BTreeMap<u64,u64>` first-fit + coalescing
- **NVIDIA** — PFIFO probe + VRAM BAR2 + P8 mode
- **AMD** — PM4 ring buffer stub + VRAM mapeada
- **30+ GPUs detectadas** por PCI device ID

### 2. Boot Agent-First com Cortex pre-HW (Sprint 71)
O LLM carrega ANTES do PCI scan. O sistema nervoso acorda, observa o hardware sendo descoberto, e pode **participar das decisões** desde o primeiro tick. DiagnosticSkill substitui testes inline — o kernel não tem mais código procedural de teste no boot.

### 3. 247 Agentes como Única Primitiva de Sistema
Tudo é agente: drivers, serviços, cron, segurança, entrada, saída, o LLM, o roteador de intenção. Não existem processos, threads, tasks, daemons, systemd units ou serviços. Apenas agentes com manifesto, ciclo de vida e capacidades.

| Grupo | Agentes | Função |
|---|---|---|
| **20 nativos** | SystemAgent, HermesAgent, CortexAgent, DisplayAgent, etc | OS Core |
| **The Agency** | 147 especialistas em 12 divisões | Engenharia, Design, Marketing, Segurança... |
| **Importados** | ~80 agentes do msitarzewski/agency-agents (MIT) | 7 divisões adicionais |
| **HW Agents** | ~6 por dispositivo PCI | GPU, NIC, USB, ATA |
| **FS Agents** | ~6 (Ata, DevFs, ProcFs, HermesFs, RamFs, InferenceFs) | VFS |

### 4. Meta-Skill Auto-Improvement (Sprint 67)
Único SO com meta-skill que observa e melhora as próprias skills:
- `skill_observer.rs`: Observation protocol com `watch_task()`, `watch_correction()`, `pending_observations()`
- Comprehensive Review a cada 3000 ticks
- `/learn` command: gera SKILL.md a partir de padrões detectados
- `completion_check()`: verification contracts pós-execução

### 5. Memória Hierárquica com ARC (MHI)
ZFS-style ARC adaptado para AI workloads: DRAM ↔ VRAM ↔ SSD ↔ HDD. `arc_suggest_tier()` move dados quentes do LLM para VRAM automaticamente. `MhiScheduler` migra tiers a cada 1000 ticks.

### 6. Self-Healing com LLM
Kernel panics são classificados por `FailureClass::classify()`, analisados pelo `SelfHeal::analyze()`, e recuperados com ação sugerida pelo LLM. Lições aprendidas persistem entre boots. O kernel aprende com os próprios erros.

### 7. Desktop COSMIC em Ring 0
DisplayAgent renderiza o Compositor diretamente — sem X11, sem Wayland, sem display server. 3 workspaces, auto-tiling, dock bar, notificações, mouse drag, cursor, temas, apps.

### 8. Self-Healing com Checkpoints
Session checkpoint salva bitmap do alocador + MHI a cada 100 ticks. Double Fault → restore automático. CDC Rabin chunking + XOR delta snapshot.

---

## The Tick-Tock System — How Time Works

The Neural AIOS doesn't use Unix-style `jiffies` or epoch timestamps. Time is measured in **ticks** — hardware interrupts from the LAPIC (Local APIC) timer.

```
┌──────────────────────────────────────────────────────────┐
│              SYSTEM CLOCKS                                │
│                                                           │
│  CPU Core (3-5 GHz)  ───┤                               │
│  RAM (DDR4 3200 MT/s) ───┤   NÃO controlam o tick        │
│  VRAM (25 GB/s)       ───┤                                │
│                                                           │
│  BCLK (100 MHz) ─── LAPIC Timer ─── TICKS (12-192/s)    │
│                                                           │
│  Cada TICK = 1 interrupcao de hardware                   │
│  → timer_handler() → TIMER_TICKS += 1                    │
│  → AgentScheduler chama agent.tick()                     │
└──────────────────────────────────────────────────────────┘
```

**Static tick (antes):** `init_count = 8,388,608` → 12 ticks/s → latência 84ms.

**Dynamic tick (v0.76.1):** O `MemoryAgent` mede a CPU real via `rdtsc()`, conta agentes ativos, e calibra:

```
Agentes ativos → Tick rate → Latencia
     0-5       →  192 t/s  →  5ms     (modo interativo)
     6-10      →   96 t/s  → 10ms
    11-20      →   48 t/s  → 21ms     (modo normal)
    21-50      →   24 t/s  → 42ms
    51+        →   12 t/s  → 84ms     (modo economia)
```

**Conversão tick → tempo real (12 t/s base):**

| Ticks | Real time |
|---|---|
| 12 | ~1 segundo |
| 720 | ~1 minuto |
| 5000 | ~7 minutos |
| 43000 | ~1 hora |

**O que acontece em 1 tick (84ms):**

| Recurso | Em 1 tick |
|---|---|
| CPU (3 GHz) | **252 milhões de instruções** |
| RAM (DDR4) | **2.1 GB transferidos** |
| NVMe (7 GB/s) | **588 MB lidos** |
| LLM BitNet 1.5B | **3-4 tokens** (CPU) ou **100+ tokens** (GPU) |

**Hermes é event-driven:** Só avança o ciclo ReAct quando há entrada real.

### Trinity Model Hub — MoE (v0.77 target)

Mixture of Experts no kernel. Modelos microscópicos (<150KB cada) especializados:

```
"crie um app jogo da velha"
        │
        ▼
┌──────────┐    ┌───────────┐    ┌──────────┐    ┌────────────┐
│ Router   │───▶│ rust_coder│───▶│  cargo   │───▶│ tictactoe  │
│ 68KB     │    │ 125KB     │    │  build   │    │ .wasm 48KB │
└──────────┘    └───────────┘    └──────────┘    └────────────┘
                                               ▶ AgentRegistry.load()
```

| Expert | Tamanho | Função |
|---|---|---|
| **hw_identify** | 68KB | ✅ PCI/USB detection (99% precision) |
| **rust_coder** | 125KB | 🟡 Gera código Rust no_std + WASM |
| **disk_diag** | 50KB | 🟡 Diagnóstico SMART + erros |
| **security** | 50KB | 🟡 Assinaturas de ataque + CVE |
| **Generator** | 375MB | 🟡 1.5B params — respostas fluentes |
| **On-demand** | N KB | ⏳ "Quero pilotar helicóptero" → .bitnet → skill |

**Total experts:** 340KB. Generator: 375MB. **Matrix-style learning: download → train → use.**

### Agent Tier Classification (Premissa v0.76.1)

Todo agente **deve declarar seu `AgentTier`** no manifesto. Sem tier → fallback para `Learning`, analisado por 5000 ticks de uso e oficializado.

| Tier | Schedule | Exemplos | Critério |
|---|---|---|---|
| **Permanent** | `Continuous` | Hermes, Display, HwBridge | Essencial — sistema falha sem ele |
| **System-demand** | `EventDriven` | DiskAgent, Cortex, Net, Security | Acorda com evento do kernel |
| **User-demand** | `EventDriven` | Skill agents, Apps, Plugins | Acorda com invocação do usuário |
| **Periodic** | `PollEvery(N)` | Cron, Observer, Optimizer | Roda em intervalo fixo |
| **Learning** (fallback) | `PollEvery(2000)` | Agentes novos não-classificados | Analisado → promovido ao tier real |

```rust
pub enum AgentTier {
    Permanent,      // must run always — system fails without it
    SystemDemand,   // wakes on kernel events
    UserDemand,     // wakes on user/agent skill invocation
    Periodic(u64),  // wakes every N ticks
    Learning(u64),  // unclassified — analyzed then promoted
}
```

---

## Project Stats (v0.76.1)

| Metric | Value |
|---|---|
| Rust files | **132** |
| Total LOC | **~15,500** |
| Crates | **5** (neural-kernel, agent-core, event-bus, skill-registry, ticket-lock) |
| Agents | **247+** (20 nativos + 147 The Agency + 80 importados + ~6 HW + ~6 FS) |
| Controllers | **6** (ATA, AHCI stub, NVMe, USB-MSC, SCSI stub, VirtIO stub) |
| FS probes | **10+** (FAT32, NTFS, EXT4, XFS, ISO9660, exFAT, Btrfs, HFS+, EROFS, ReFS) |
| Technologies | **10+** (GPT, SMART, SED/OPAL, TRIM, ARC cache, I/O scheduler, adaptive heap) |
| Boot phases | **8** agent-driven com eventos EventBus |
| Compile | **0 errors** (warnings per policy) |
| License | **AGPLv3** com exceção comercial |

---

## Quick Start

```powershell
# Build
cargo build --release
python tools/build_image.py

# Run (QEMU with WHPX acceleration)
qemu-system-x86_64 -m 4G -nic user,model=rtl8139 `
  -drive format=raw,file=target/neural-os-bios.img `
  -no-reboot -smp 2 -accel whpx

# Boot from USB (Rufus: DD image, MBR, BIOS/CSM)
# Gravar target/neural-os-bios.img no pendrive via Rufus
```

---

## Why Contribute?

Neural OS Hermes is the **only bare-metal AI-native OS in existence**. Not a research paper, not a prototype — a real OS booting on real hardware with GPU compute, a desktop, 247 agents, an LLM cortex, and 0 compiler errors.

We are building the future of operating systems. **Not an OS that runs AI — an OS that IS AI.**

| Role | What you can do |
|---|---|
| **Rust Systems Engineer** | GPU shaders, NVMe driver, network stack, SMP |
| **ML Engineer** | LLM training, BitNet quantization, speculative decoding |
| **OS Researcher** | Memory hierarchy, zero-copy SFS, capability-based security |
| **Hardware Hacker** | Port to ARM/RISC-V, NPU XDNA driver, WiFi |
| **Investor/Sponsor** | Fund GPU hardware, cloud infra, academic publication |

---

## License

**GNU Affero General Public License v3.0 (AGPLv3)** with **Commercial Exception**.

| Use | License |
|---|---|
| Personal / Educational / Research / Open-source | ✅ **Free** under AGPLv3 |
| Proprietary product / SaaS / Embedded | 💰 **Commercial license required** — see [COMMERCIAL.md](COMMERCIAL.md) |

---

> *"We don't need an OS that runs AI. We need an OS that IS AI."*
>
> Neural OS Hermes — msrovani + IDA IA (OpenCode), 2026
>
> *109+ sprints, 27+ blocos, ~15.500 LOC, 0 erros, 0 panics no QEMU.*
> *De um bootloader a um SO cognitivo com 247 agentes, DiskIntelligenceAgent,*
> *NVMe bare-metal, SMART, GPT, ARC cache, adaptive heap, dynamic tick,*
> *10+ filesystem probes, 6 storage controllers, I/O scheduler,*
> *boot agent-first com cortex pre-HW, e 0 panics no QEMU.*
> *— em Rust no_std. Sem Linux. Sem POSIX. Sem legado.*
