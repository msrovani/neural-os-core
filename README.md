# Neural OS Hermes ⚡

**The first AI-native operating system. Bare-metal Rust. No Linux. No POSIX. No legacy.**

> **🧠 AI/Dev Quick Start:** `SUMMARY.md` (resumo) · `AGENTS.md` (regras) · `docs/memory/IDEA_BANK.md` (ideias) · `docs/architecture/0015-curso-correcao-mvp.md` (plano)

```






           ╔═══════════════════════════════════════════════════════════╗
           ║  "We don't need an OS that runs AI.                     ║
           ║   We need an OS that IS AI."                            ║
           ╚═══════════════════════════════════════════════════════════╝
```

Windows, Linux, and macOS are all the same OS designed 50 years ago with different colors. They bolt AI on top — a chatbot in the taskbar, a copilot in the browser, a GPU driver that lets PyTorch run. The kernel doesn't know what AI is. It never will.

**Neural OS Hermes is different.** The kernel itself is a neural inference engine. MLPs make architectural decisions. A Memory Hierarchy Index treats VRAM, DRAM, NVMe, and HDD as a single AI-routed tiered pool. The boot sequence is an AI pipeline: detect hardware → decide config → chat with the user → execute skills.

No drivers. No syscalls. No kernel modules. Just tensors, events, and intent.

---

## 🏗️ What's Been Built (Sprint 21 — Block 4)

The kernel discovers hardware, detects architecture, builds a memory hierarchy, and boots 6 cooperative AI agents:

```
[SYSTEM]  Neural Microkernel Iniciado.
[TEST]    Breakpoint, Box, Vec, Tensor, SiLU, RMSNorm, Intent Router
[BITNET]  Inferencia 2-bit concluida. Output: [-0.5, -2.0]
[KERNEL]  Bitmap Allocator: 1000 iteracoes estaveis
[PCI]     Scan: 4 dispositivos (QEMU q35)
[ACPI]    RSDP + RSDT + MADT (LAPIC/IOAPIC)
[APIC]    LAPIC + IOAPIC + PIC disable + timer/keyboard routing
[SMP]     AP 1 entrou em modo 64-bit Rust! (2 cores)
[ARCH]    ring0=0 ring1=1 heap=512MB trust=1 power=0 tensor=0
[MHI]     1 tier(s). Best: Dram (2140823552 bytes avail)
[EXECUTOR] 6 tasks spawned (system, monitor, hw_bridge, input, cortex, console)
[DAEMON]  Agente assincrono inicializado. Aguardando SYSTEM_READY...
[MONITOR] Evento SYSTEM_READY publicado.
[IPC]     Evento recebido no topico SYSTEM_READY. Token: 1
[SKILL]   EchoSkill executada. Output reverso: [3, 2, 1]
[DAEMON]  SYSTEM_READY confirmado. Ciclo de inicializacao completo.
```

### What each module does

| Module | What | How |
|---|---|---|
| `pci.rs` | PCI scan via CF8/CFC | 256 busses × 32 devices, vendor/device/class/BARs |
| `acpi.rs` | ACPI parser | RSDP search (EBDA + BIOS), RSDT/XSDT, MADT (LAPIC/IOAPIC) |
| `apic.rs` | APIC init | LAPIC (SVR, TPR), IOAPIC (IRQ redirection), PIC disable |
| `smp/` | SMP multi-core | PerCpu GS.base, trampoline 16→64, INIT-SIPI-SIPI |
| `mhi.rs` | Memory Hierarchy Index | `AllocTier` (Dram/Vram/Nvme/Hdd), `alloc_by_tier()` |
| `inventory.rs` | Hardware Inventory | `HardwareInventory::collect()`, `SystemArchitecture::infer()` |
| `hermes.rs` | Hermes Chat | MLP intent router, `/status`, `/echo`, `/help` |
| `interrupts.rs` | Dual EOI | `USING_APIC` atomic flag → APIC or PIC EOI per interrupt |

### Next: Block 5 — Skills + Trust Cache (Sprint 22)

SystemStatusSkill consumir MHI, HardwareInfoSkill, TrustCache com TTL, ISO bootável.

---

## 🔥 The Vision

### What if memory was managed by... usabilidade?

Every OS today treats all memory the same. A byte in VRAM (112 GB/s) and a byte on HDD (0.1 GB/s) are accessed through different APIs, different drivers, different filesystems. The programmer decides where data lives.

**Neural OS Hermes doesn't work that way.** The Memory Hierarchy Index (MHI) is a first-class OS primitive:

```
[MHI] Memory Hierarchy:
  tier[0] VRAM:  4 GB @ 112 GB/s  ← tensor_active (MLP decided)
  tier[1] DRAM: 16 GB @  19 GB/s  ← heap, kv_cache, sfs_active
  tier[2] NVMe: 256 GB @ 3.5 GB/s ← sfs_cold, tensor_swap
  tier[3]  HDD:   1 TB @ 0.1 GB/s ← episodic_memory, logs

> alloc_by_tier(Tensor, 4MB)
  → VRAM (fastest tier with space)
```

The programmer (or the agent) says *what* they need. The MHI decides *where* it goes, based on access patterns, bandwidth, and capacity. No `cudaMemcpy`, no `mmap`, no `O_DIRECT`. Just tiers.

**This doesn't exist anywhere else.** Not on Linux. Not on Windows. Not on macOS.

### What if there were no drivers?

Every device in Neural OS is a **skill** loaded on demand by the Neural Cortex, not a kernel module compiled for a specific OS version. USB mouse? Skill `hid_mouse.wasm`. NVMe storage? Skill `nvme_driver.wasm`. GPU compute? Skill `gpu_compute.wasm`.

No autorun. No implicit trust. The Cortex decides what to load, when, and whether the user has intent.

### What if the OS was 2 MB?

A Linux kernel is ~10 MB. A Windows kernel is larger. A typical OS install is 10-50 GB.

Neural OS Hermes is **< 2 MB**. The entire kernel, MLP weights, MHI, EventBus, Skill Registry, and chat interface fit on a floppy disk.

---

## 🧬 Core Innovation: The Chain

Every feature is a dependency in a cryptographic chain. You can't skip blocks:

```
Block 0 (Genesis)  ─── VGA + serial + heap + EventBus + 5 agents
Block 1 (PCI+ACPI) ─── Hardware discovery (CF8/CFC, MADT, LAPIC)
Block 2 (SMP+Heap) ─── Multi-core + slab allocator (4 MB heap)
Block 3 (Hermes UI) ─── Chat loop + Intent Router (neural terminal)
Block 4 (Auto-Conf) ─── MLP architecture detector + MHI
Block 5 (Skills)    ─── Hardware control + Trust Cache
Block 6 (MVP)       ─── Bootable ISO on any x86-64 UEFI
```

Each block builds on the previous. No shortcuts. No copilot. No bloat.

---

## 🚀 Current State

The chain is at **Block 4 (Auto-Config)** — Sprint 21 complete. Here's what boots in QEMU right now:

```
[SYSTEM]  Neural Microkernel Iniciado.
[TEST]    Breakpoint, Box, Vec, Tensor, SiLU, RMSNorm, Intent Router
[BITNET]  Inferencia 2-bit concluida. Output: [-0.5, -2.0]
[KERNEL]  Bitmap Allocator: 1000 iteracoes estaveis
[PCI]     Scan: 4 dispositivos QEMU q35
[ACPI]    RSDP + RSDT + MADT parsing
[APIC]    LAPIC + IOAPIC + PIC disable
[SMP]     AP 1 boot (2 cores)
[ARCH]    ring0=0 ring1=1 heap=512MB trust=1 power=0 tensor=0
[MHI]     1 tier, Dram ~2 GB
[EXECUTOR] 6 tasks → pipeline completo → idle
[SKILL]   EchoSkill executada. Output reverso: [3, 2, 1]
```

**Sprint 21 capability:**
- ✅ VGA 80×25 + Serial 0x3F8 (dual output)
- ✅ IDT with 8 exception handlers + PIT + keyboard
- ✅ Bitmap Frame Allocator (128 KB bitmap, covers 4 GB)
- ✅ FPU/SSE + Tensor f32 matmul, SiLU, RMSNorm
- ✅ TernaryTensor + PackedTernaryTensor (2-bit, 12× compression)
- ✅ TicketLock FIFO (SMP-safe synchronization)
- ✅ EventBus IPC + CapabilityToken (zero-trust messaging)
- ✅ Skill Registry + MCP Layer (EchoSkill, SystemStatus)
- ✅ NeuralExecutor with 6 cooperative agents
- ✅ PCI scan (CF8/CFC) + ACPI (RSDP/MADT) + APIC (LAPIC/IOAPIC)
- ✅ SMP multi-core (trampoline, GS.base, INIT-SIPI-SIPI)
- ✅ Slab allocator (8 buckets) + 4 MB heap
- ✅ Hermes Chat (MLP intent router, `/status`, `/echo`, `/help`)
- ✅ MHI (MemoryTier, AllocTier, `alloc_by_tier(Dram)`)
- ✅ HardwareInventory + SystemArchitecture auto-detection

---

## 📐 Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Neural OS Hermes                      │
├─────────────────────────────────────────────────────────┤
│  Ring 0 (Intent Router)     MLP 3→2 ternário (< 1 µs)   │
│  Ring 1 (Tensor Engine)     matmul + neural primitives  │
│  Ring 2 (Agent Runtime)     EventBus + Skills + WASM     │
├─────────────────────────────────────────────────────────┤
│  Memory Hierarchy Index     alloc_by_tier()              │
│  Event Bus                  pub/sub + CapabilityToken    │
│  Skill Registry             zero-trust + MCP layer       │
│  PCI/ACPI/APIC              hardware discovery           │
├─────────────────────────────────────────────────────────┤
│  Bootloader (UEFI/BIOS)     bootloader crate v0.9.34    │
│  no_std Rust                x86_64-unknown-none          │
│  Target                     QEMU → AMD APU (real HW)    │
└─────────────────────────────────────────────────────────┘
```

---

## 🛠️ Quick Start

```powershell
# Prerequisites
rustup toolchain install nightly
rustup component add llvm-tools-preview
cargo install bootimage
winget install QEMU

# Build and boot
cargo run
```

QEMU window opens. VGA output on screen, serial output in terminal. Type your intent.

---

## 🎯 Roadmap to MVP

| Block | Sprint | Deliverable | Status |
|---|---|---|---|---|
| 0 | 1-17 | VGA, serial, heap, EventBus, agents | ✅ Concluído |
| 1 | 18 | PCI scan + ACPI + APIC | ✅ Concluído |
| 2 | 19 | PerCpu + SMP + Slab allocator (4 MB heap) | ✅ Concluído |
| 3 | 20 | Hermes Chat (MLP intent router, commands) | ✅ Concluído |
| 4 | 21 | MHI + HardwareInventory + SystemArchitecture | ✅ Concluído |
| 5 | 22 | TrustCache + MHI Skills + HardwareInfoSkill | ✅ Concluído |
| 6 | **23** | **Network Sprint (VirtIO-net + smoltcp + HTTP)** | 🟡 Próximo |
| 7 | 24 | NVMe + SFS persistente | ⏳ |
| 8+ | 25+ | WASM + TLS + multi-agent | ⏳ |

---

## 🤝 For Innovators

You've read this far because you're tired of:
- **Linux** — 30M lines of C, 40+ years of ABI debt, systemd, containers for isolation
- **Windows** — registry, BSOD, driver hell, telemetry as architecture
- **macOS** — locked down, proprietary, XNU is microkernel-ish but not really

Neural OS Hermes is for people who believe:
- An OS should be **2 MB**, not 10 GB
- Memory management by **bandwidth and latency**, not by hardware slot
- Devices should **ask permission** before they work
- The kernel **is** the AI — not "AI added to kernel"

This is not a Linux distribution. This is not a Unix clone. This is a ground-up reimagining of what an OS is, written in bare-metal Rust, designed for an age where memory is tiered, compute is heterogeneous, and everyone is talking about AI but nobody has built the OS for it.

**We're building it.**

---

## 📚 Architecture Decision Records

| ADR | Title |
|---|---|
| 0001 | [Initial Architecture and Toolchain](docs/architecture/0001-initial-architecture-and-toolchain.md) |
| 0002 | [VGA and Serial Logging](docs/architecture/0002-vga-and-serial-logging.md) |
| 0003 | [Interrupt Descriptor Table](docs/architecture/0003-interrupt-descriptor-table.md) |
| 0004 | [Memory Paging and Heap](docs/architecture/0004-memory-paging-and-heap.md) |
| 0005 | [SIMD and FPU Enablement](docs/architecture/0005-simd-and-fpu-enablement.md) |
| 0006 | [Neural Primitives and libm](docs/architecture/0006-neural-primitives-and-libm.md) |
| 0007 | [Intent Router MLP](docs/architecture/0007-intent-router-mlp.md) |
| 0010 | [Strategic Roadmap](docs/architecture/0010-strategic-roadmap-and-innovations.md) |
| 0011 | [BitLinear and Ternary MatMul](docs/architecture/0011-bitlinear-and-hybrid-matmul.md) |
| 0012 | [2-bit Packing and Quantization](docs/architecture/0012-2bit-packing-quantization.md) |
| 0013 | [Executive Summary / SotA 2026](docs/architecture/0013-neural-os-executive-summary.md) |
| 0014 | [Hardware Evolution Ideas (post-MVP)](docs/architecture/0014-ideias-hardware.md) |
| **0015** | **[Course Correction → MVP Hermes](docs/architecture/0015-curso-correcao-mvp.md)** ← **You are here** |

---

## ⚡ License

MIT — because revolutions need to be forkable.
