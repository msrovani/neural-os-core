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

## 🏗️ What's Been Built (Sprint 22 — Hermes Chat + Skills + Trust Cache)

The kernel discovers hardware, detects architecture, builds a memory hierarchy, boots 6 cooperative AI agents, and executes verified skills with trust cache. Ecosystem analysis complete across 5 tiers (136 repos, 249 ideas cataloged):

```
[SYSTEM]  Neural Microkernel Iniciado.
[TEST]    Breakpoint, Box, Vec, Tensor, SiLU, RMSNorm, Intent Router
[BITNET]  Inferencia 2-bit concluida. Output: [-0.5, -2.0]
[KERNEL]  Bitmap Allocator: 1000 iteracoes estaveis
[PCI]     Scan: 4 dispositivos (QEMU q35)
[ACPI]    RSDP + RSDT + MADT (LAPIC/IOAPIC)
[APIC]    LAPIC timer + IOAPIC keyboard + PIC disable
[SMP]     AP 1 entrou em modo 64-bit Rust! (2 cores)
[ARCH]    ring0=0 ring1=1 heap=64MB trust=1 power=0 tensor=0
[MHI]     1 tier(s). Best: Dram (261 MB avail)
[EXECUTOR] Timer ticks: antes=58, depois=229  ← LAPIC timer @ ~100 Hz
[EXECUTOR] 6 tasks spawned (system, monitor, hw_bridge, input, cortex, console)
[MONITOR] Evento SYSTEM_READY publicado.
[SKILL]   EchoSkill executada. Output reverso: [3, 2, 1]
[DAEMON]  SYSTEM_READY confirmado. Ciclo de inicializacao completo.
[WATCHDOG] Ticks do temporizador: 2100+
[EXECUTOR] Hardware context: RAM=[0.023943, 0.000000] tasks=4
```

### Ecosystem Analysis (5 Tiers Complete — 136 repos, 249 ideas)

| Tier | Repos | Ideas | ADR | Highlight |
|------|-------|-------|-----|-----------|
| 0 (Crom) | 75 | 22 | ADR-0020 | XOR Delta, CDC Rabin, Codebook VQ, ReAct loop |
| 1 (Life OS) | 20 | 26 | ADR-0021 | Spectrum Graph, Temporal KG, Intent Transparency |
| 2 (PAI) | 21 | 15 | ADR-0022 | Skill Metadata, Audit Ring, Context Fencing |
| 3 (Memory) | 14 | 14 | ADR-0023 | SHA-256 dedup, Ebbinghaus decay, 4-tier consolidation |
| 4 (Agent Frameworks) | 6 | 22 | ADR-0024 | Tool Policy, Usage Tracker, Cron Scheduler, Cline patterns |

**Next:** Tier 5 (Language-Specific Runtimes) — WASM, Rust native agents, C ABI

### What each module does

| Module | What | How |
|---|---|---|
| `pci.rs` | PCI scan via CF8/CFC | 256 busses × 32 devices, vendor/device/class/BARs |
| `acpi.rs` | ACPI parser | RSDP search (EBDA + BIOS), RSDT/XSDT, MADT (+ Interrupt Source Override) |
| `apic.rs` | APIC init | LAPIC timer (vector 32, periodic), IOAPIC keyboard redirect, PIC disable |
| `smp/` | SMP multi-core | PerCpu GS.base, trampoline 16→64, INIT-SIPI-SIPI |
| `mhi.rs` | Memory Hierarchy Index | `AllocTier` (Dram/Vram/Nvme/Hdd), `alloc_by_tier()` |
| `inventory.rs` | Hardware Inventory | `HardwareInventory::collect()`, `SystemArchitecture::infer()` |
| `hermes.rs` | Hermes Chat | MLP intent router, `/status`, `/echo`, `/help`, `/hw`, `/trust allow/deny` |
| `trust.rs` | TrustCache | Token cache with TTL, denylist, `check_or_cache()` |
| `interrupts.rs` | Dual EOI | `USING_APIC` atomic flag → APIC or PIC EOI per interrupt |

### Next: Sprint 23 — Network Sprint + Tool Policy (#228)

VirtIO-net + smoltcp TCP/IP + DNS + HTTP client per ADR-0016. Terminal Hermes remoto sobre TCP. Also: Tool Policy Registry (#228), Usage Tracker (#229), Auto-Compact Buffer (#230), Event-Sourced Conversation (#231).

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
Block 5 (Skills)    ─── Skills + Trust Cache + LAPIC timer
Block 6 (Network)   ─── VirtIO-net + TCP/IP + DNS + HTTP
```

Each block builds on the previous. No shortcuts. No copilot. No bloat.

---

## 🚀 Current State

The chain is at **Block 5 (Skills + Trust Cache)** — Sprint 22 complete. Here's what boots in QEMU right now:

```
[SYSTEM]  Neural Microkernel Iniciado.
[APIC]    LAPIC timer (vetor 32, count=8M, div=1) — ~100 Hz
[TIMER]   Interrupt fired! tick=0..4 → 171 ticks in busy wait
[SMP]     AP 1 boot (2 cores)
[ARCH]    ring0=0 ring1=1 heap=64MB trust=1 power=0 tensor=0
[MHI]     1 tier, Dram ~261 MB avail
[EXECUTOR] Timer ticks: antes=58, depois=229
[EXECUTOR] 6 tasks → SYSTEM_READY → EchoSkill → Watchdog 2100+
[WATCHDOG] Ticks do temporizador: 2100
[EXECUTOR] Hardware context: RAM=[0.023943, 0.000000] tasks=4
```

**Sprint 22 capability:**
- ✅ VGA 80×25 + Serial 0x3F8 (dual output)
- ✅ IDT with 8 exception handlers + keyboard
- ✅ Bitmap Frame Allocator (128 KB bitmap, covers 4 GB)
- ✅ FPU/SSE + Tensor f32 matmul, SiLU, RMSNorm
- ✅ TernaryTensor + PackedTernaryTensor (2-bit, 12× compression)
- ✅ TicketLock FIFO (SMP-safe synchronization)
- ✅ EventBus IPC + CapabilityToken (zero-trust messaging)
- ✅ Skill Registry + MCP Layer (EchoSkill, SystemStatus, HardwareInfo)
- ✅ NeuralExecutor with 6 cooperative agents
- ✅ PCI scan (CF8/CFC) + ACPI (RSDP/MADT + Interrupt Source Override) + APIC
- ✅ SMP multi-core (trampoline, GS.base, INIT-SIPI-SIPI)
- ✅ Slab allocator (8 buckets) + 4 MB heap
- ✅ Hermes Chat (MLP intent router, `/status`, `/echo`, `/help`, `/hw`, `/trust allow/deny`)
- ✅ MHI (MemoryTier, AllocTier, `alloc_by_tier(Dram)`)
- ✅ HardwareInventory + SystemArchitecture auto-detection
- ✅ LAPIC timer (periodic, vetor 32) — substitui PIT → IOAPIC
- ✅ TrustCache (allow/deny/check, TTL 360 ticks, denylist)
- ✅ MADT parsing: Interrupt Source Override + x2APIC

---

## 📐 Architecture

```
┌──────────────────────────────────────────────────────────┐
│                   Neural OS Hermes — AIOS                 │
├──────────────────────────────────────────────────────────┤
│  Ring 0 (Reflex)    Tiny MLP 16→8→3  sub-µs             │
│  Ring 1 (Cortex)    BitNet LLM 1.5B  ~5-15 tok/s        │
│  Ring 2 (Action)    WASM Skills      SkillRegistry       │
├──────────────────────────────────────────────────────────┤
│  Memory Hierarchy Index     alloc_by_tier()              │
│  Event Bus                  pub/sub + CapabilityToken    │
│  Skill Registry             zero-trust + MCP layer       │
│  PCI/ACPI/APIC/SMP          hardware discovery           │
├──────────────────────────────────────────────────────────┤
│  Bootloader (UEFI/BIOS)     bootloader crate v0.9.34    │
│  no_std Rust                x86_64-unknown-none          │
│  Target                     QEMU → AMD APU (real HW)    │
└──────────────────────────────────────────────────────────┘

Decision Pipeline:
  Event → Ring 0 Reflex MLP → "precisa pensar?" → 
    ├── Não (confiança > 90%) → executa ação direta
    └── Sim → Ring 1 BitNet LLM → intenção → Ring 2 Skill → executa
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
|---|---|---|---|
| 0 | 1-17 | VGA, serial, heap, EventBus, agents | ✅ |
| 1 | 18 | PCI scan + ACPI + APIC | ✅ |
| 2 | 19 | PerCpu + SMP + Slab allocator (4 MB heap) | ✅ |
| 3 | 20 | Hermes Chat (MLP intent router, commands) | ✅ |
| 4 | 21 | MHI + HardwareInventory + SystemArchitecture | ✅ |
| 5 | 22 | Skills + Trust Cache + LAPIC timer | ✅ |
| — | — | Ecosystem analysis (Tiers 0-4, 136 repos, 249 ideas) | ✅ |
| 6 | 23 | Network (VirtIO-net + smoltcp + HTTP) + Sprint 23 items (#228-#231) | 🟡 Próximo |
| 7 | 24 | Network completion + Sprint 24 items (#232-#235) | 🟡 |
| **8** | **25** | **Transformer Engine + Sprint 25 items (#236-#240)** | **🆕 Planejado** |
| **9** | **26** | **Cortex Daemon + Sprint 26+ items (#241-#244)** | **🆕 Planejado** |
| **10** | **27+** | **Reflex tuning + Success Engine** | **🆕 Planejado** |
| 11 | 28+ | WASM + TLS + multi-agent | ⏳ |

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
|---|---|---|
| 0001 | [Initial Architecture and Toolchain](docs/architecture/0001-initial-architecture-and-toolchain.md) |
| 0002 | [VGA and Serial Logging](docs/architecture/0002-vga-and-serial-logging.md) |
| 0003 | [Interrupt Descriptor Table](docs/architecture/0003-interrupt-descriptor-table.md) |
| 0004 | [Memory Paging and Heap](docs/architecture/0004-memory-paging-and-heap.md) |
| 0005 | [SIMD and FPU Enablement](docs/architecture/0005-simd-and-fpu-enablement.md) |
| 0006 | [Neural Primitives and libm](docs/architecture/0006-neural-primitives-and-libm.md) |
| 0007 | [Intent Router MLP](docs/architecture/0007-intent-router-mlp.md) |
| 0009 | [PIC, Watchdog and Page Fault](docs/architecture/0009-pic-watchdog-and-page-fault.md) |
| 0010 | [Strategic Roadmap](docs/architecture/0010-strategic-roadmap-and-innovations.md) |
| 0011 | [BitLinear and Ternary MatMul](docs/architecture/0011-bitlinear-and-hybrid-matmul.md) |
| 0012 | [2-bit Packing and Quantization](docs/architecture/0012-2bit-packing-quantization.md) |
| 0013 | [Executive Summary / SotA 2026](docs/architecture/0013-neural-os-executive-summary.md) |
| 0014 | [Hardware Evolution Ideas (post-MVP)](docs/architecture/0014-ideias-hardware.md) |
| 0015 | [Course Correction → MVP Hermes](docs/architecture/0015-curso-correcao-mvp.md) |
| 0016 | [Network Strategy](docs/architecture/0016-network-strategy.md) |
| 0017 | [Critical Bugfix Sprint](docs/architecture/0017-critical-bugfix-sprint.md) |
| 0018 | [Sprint 24 Plan](docs/architecture/0018-sprint-24-plan.md) |
| 0019 | [Neural Cortex — BitNet LLM Integration](docs/architecture/0019-neural-cortex-bitnet-llm.md) |
| 0020 | [Crom Ecosystem Rust Analysis](docs/architecture/0020-crom-ecosystem-analysis.md) |
| 0021 | [Life OS Ecosystem Analysis](docs/architecture/0021-life-os-ecosystem-analysis.md) |
| 0022 | [Personal AI Assistant Ecosystem](docs/architecture/0022-personal-ai-assistant-ecosystem-analysis.md) |
| 0023 | [Memory Systems & Second Brain](docs/architecture/0023-memory-systems-second-brain-analysis.md) |
| **0024** | **[Agent Frameworks Analysis](docs/architecture/0024-agent-frameworks-analysis.md)** ← **You are here** |

---

## ⚡ License

MIT — because revolutions need to be forkable.
