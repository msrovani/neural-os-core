# Neural OS Hermes ⚡

**The first AI-native operating system. Bare-metal Rust. No Linux. No POSIX. No legacy.**

> **🧠 AI/Dev Quick Start:** `SUMMARY.md` · `AGENTS.md` · `docs/memory/IDEA_BANK.md` · `docs/memory/STATE.md` · `docs/memory/STATE.md`

```
           ╔═══════════════════════════════════════════════════════════╗
           ║  "We don't need an OS that runs AI.                     ║
           ║   We need an OS that IS AI."                            ║
           ╚═══════════════════════════════════════════════════════════╝
```

Windows, Linux, and macOS bolt AI on top — a chatbot in the taskbar, a copilot in the browser. The kernel doesn't know what AI is.

**Neural OS Hermes is different.** The kernel itself is a neural inference engine. The Cortex classifies every user intent. The Memory Hierarchy Index treats VRAM, DRAM, NVMe, and HDD as a single AI-routed tiered pool. Seven cooperative AI agents run the system — no shell, no init, no systemd.

No drivers. No syscalls. No kernel modules. Just tensors, events, and intent.

---

## 🚀 Boot Timeline — What the System Does

From power-on to interactive Hermes console, here's every step:

### 1. Bootloader (UEFI/BIOS → 64-bit)
```
SeaBIOS → iPXE → bootloader crate → kernel_main
```
The bootloader (`bootloader` v0.9.34) sets up page tables with `map_physical_memory`, passes `BootInfo` with memory map. The kernel starts at the `kernel_main` entry point in long mode.

### 2. VGA + Serial (Dual Output)
```
vga_buffer::init(offset) → VGA 80×25 color text
serial::init() → COM1 at 0x3F8 (host terminal)
```
Every `print!`/`println!` writes to both the QEMU window (VGA) and the host terminal (serial). The panic handler also dual-outputs.

### 3. IDT + GDT + TSS (Exception Handling)
```
interrupts::init_idt() → 8 handlers + Breakpoint (logs + returns)
                       → Double Fault (IST stack, 20KB buffer)
                       → Page Fault (CR2 → log → hlt)
                       → Timer (PIT → LAPIC, atomic counter)
                       → Keyboard (IRQ1 → AtomicU8 release)
```
The GDT is recreated from scratch (bootloader GDT is minimal). The TSS provides an Interrupt Stack Table for Double Fault recovery.

### 4. Memory Init (Page Tables + Heap)
```
memory::init_memory(offset) → OffsetPageTable via CR3
allocator::init_heap() → 4 MB heap at 0x4444_4444_0000
                       → LockedHeap (3.5 MB) + Slab (512 KB)
```
Page tables are accessed through the physical memory offset (bootloader feature `map_physical_memory`). The heap uses a linked-list allocator for general allocations and a slab allocator for small objects (32-4096 bytes, 8 buckets).

### 5. SIMD + Tensor Core (Neural Foundation)
```
enable_simd() → CR0/CR4: FXSAVE/FXRSTOR, SSE, SSE2
→ Tensor matmul, SiLU, RMSNorm, 2-bit PackedTernaryTensor
→ BitNet Linear: matmul_hybrid() — ADD/SUB only, zero multiplications
```
The entire neural stack (used by Cortex intent router) is pure `no_std` Rust using the `libm` crate for `expf`/`sqrtf`.

### 6. PCI Scan + ACPI + APIC (Hardware Discovery)
```
PCI: CF8/CFC scan → 256 busses, vendor/device/class/BARs
ACPI: RSDP (EBDA/BIOS) → RSDT/XSDT → MADT (LAPIC/IOAPIC)
APIC: LAPIC timer (periodic, ~100 Hz) + IOAPIC keyboard redirect + PIC disable
```
The system discovers QEMU's virtual hardware: 4 devices (host bridge, ISA bridge, VGA, RTL8139 NIC). The LAPIC timer replaces the PIT for precise interrupt generation.

### 7. SMP Multi-Core (AP Boot)
```
INIT-SIPI-SIPI → trampoline at 0x40000 → 3 APs in 64-bit Rust
Stack: PerCpu (GS.base) → hlt loop
```
All 4 cores (BSP + 3 APs) boot via `OffsetPageTable::map_to()` — this correctly handles 2MB/1GB huge pages (critical bug fixed in Sprint 24).

### 8. Architecture Inference + Memory Hierarchy
```
SystemArchitecture::infer() → heuristic: GPU? RAM? Cores?
MemoryHierarchy::new() → Dram tier from bitmap allocator
```
The system classifies itself and decides heap size, ring assignment, and tensor compute resources.

### 9. RTL8139 Network + smoltcp TCP/IP
```
init_driver_rtl8139() → I/O ports, 4 TX desc, RX ring buffer
NetStack lazy init → smoltcp Interface + Device trait
NetworkAgent → DNS resolve → HTTP connect (non-blocking)
```
The kernel detects the RTL8139 NIC, initializes it, and delegates TCP/IP to smoltcp. The network agent polls the interface once per tick and can make HTTP requests.

### 10. NeuralExecutor — 7 Cooperative Agents
```
NeuralExecutor::run() → VecDeque<AgentTask> polling loop

  task 1: system_daemon       → publish SYSTEM_READY
  task 2: hardware_monitor    → context tensor every 100 ticks
  task 3: hw_bridge           → scancode → EventBus
  task 4: network_agent       → smoltcp poll → HTTP
  task 5: input_daemon        → ASCII buffer → ENTER → USER_INTENT
  task 6: intent_router       → Cortex.think() → SkillRegistry
  task 7: hermes_console      → display [Hermes] responses
```
Each task runs cooperatively — `yield_now().await` after every iteration. No preemption. No race conditions.

### 11. Cortex Neural Intent Router (The Brain)
```
USER_INTENT → Cortex.think("texto") → Intent (12 categories):

  "status"      → SystemStatus  → system_status skill
  "echo hello"  → Echo          → echo skill (reverses)
  "/hw"         → HardwareInfo  → hardware_info skill
  "trust allow" → TrustAllow    → trust_allow skill
  "trust deny"  → TrustDeny     → trust_deny skill
  "ping 10..."  → Network       → net_diag skill
  "fetch url"   → HttpFetch     → http_fetch skill
  "/help"       → Help          → inline help text
  "/conv"       → Conversation  → conversation skill
  "/usage"      → Usage         → usage skill
  "hello"       → Greeting      → inline greeting
  anything else → Chat          → "entendido!"
```
The Cortex replaces the old INTENT_MLP (hand-crafted 16→8→3) with keyword-based neural classification across 12 intents. Skills are dispatched automatically when registered.

### Console Output (Full Boot)
```
[SYSTEM]  Neural Microkernel Iniciado.
[TEST]    Breakpoint, Box, Vec, Tensor, SiLU, RMSNorm, Intent Router
[BITNET]  Inferencia 2-bit concluida. Output: [-0.5, -2.0]
[KERNEL]  Bitmap Allocator: 1000 iteracoes estaveis
[PCI]     Scan: 4 dispositivos (00:03.00 10ec:8139)
[ACPI]    RSDP + RSDT + MADT (LAPIC/IOAPIC)
[APIC]    LAPIC timer + IOAPIC keyboard + PIC disable
[SMP]     APs acordados: 3 (3 cores via OffsetPageTable)
[ARCH]    ring0=0 ring1=1 heap=512MB trust=1 power=0 tensor=0
[MHI]     1 tier(s). Best: Dram (2 GB avail)
[RTL8139] Reset OK. MAC: 52:54:00:12:34:56
[EXECUTOR] 7 tasks spawned (system → monitor → hw_bridge → network → input → cortex → console)
[NET @t=10] Online. IP: 10.0.2.15
[NET @t=30] DNS google.com → timeout, using fallback
[SKILL]   EchoSkill executada. Output reverso: [3, 2, 1]
[WATCHDOG] Ticks do temporizador: 13200+
```

## 📐 Architecture

```
┌──────────────────────────────────────────────────────────┐
│               Neural OS Hermes — v0.25.0                  │
├──────────────────────────────────────────────────────────┤
│  Ring 0 (Reflex)    Cortex think()      12 intents        │
│  Ring 1 (Neural)    BitNet LLM (future) ~375 MB model     │
│  Ring 2 (Action)    SkillRegistry       5+ skills         │
├──────────────────────────────────────────────────────────┤
│  Memory Hierarchy Index     alloc_by_tier(Dram)           │
│  Event Bus                  pub/sub + CapabilityToken     │
│  smoltcp TCP/IP             RTL8139 + HTTP non-blocking   │
│  PCI/ACPI/APIC/SMP          hardware discovery            │
├──────────────────────────────────────────────────────────┤
│  Bootloader (UEFI/BIOS)     bootloader crate v0.9.34     │
│  no_std Rust                x86_64-unknown-none           │
│  Target                     QEMU → AMD APU (real HW)    │
└──────────────────────────────────────────────────────────┘
```

## 🧬 Module Map

| Module | Function | Lines |
|---|---|---|
| `main.rs` | Entry point, daemon definitions, skill registry | ~700 |
| `cortex.rs` | Neural intent router — 12 categories | 101 |
| `hermes.rs` | Command parser, ConversationTracker | 182 |
| `memory.rs` | BitmapFrameAllocator (128 KB, 4 GB) | 253 |
| `netstack.rs` | smoltcp Device trait, HTTP non-blocking | 321 |
| `network_agent.rs` | Async daemon: poll → HTTP → log | 113 |
| `rtl8139.rs` | RTL8139 driver (I/O ports) | 250 |
| `apic.rs` | LAPIC timer, IOAPIC, SMP IPI | 316 |
| `smp/` | Multi-core trampoline, PerCpu | 137+ |
| `pci.rs` | PCI scan (CF8/CFC) | 150+ |
| `acpi.rs` | RSDP, MADT, ISO parsing | 100+ |
| `nn.rs` | Linear, SiLU, RMSNorm, BitLinear | 100+ |
| `tensor.rs` | Tensor, PackedTernaryTensor, matmul | 200+ |
| `time_utils.rs` | UNIX → Brazil datetime | 22 |

## 🛠️ Quick Start

```powershell
# Build and boot in QEMU
cargo bootimage --release
qemu-system-x86_64 -m 2G -serial stdio `
  -nic user,model=rtl8139 `
  -drive format=raw,file=target\x86_64-unknown-none\release\bootimage-neural-kernel.bin `
  -no-reboot -smp 4 -nographic
```

Type in the terminal. The Cortex processes your intent and responds with `[Hermes]`.

---

## 📚 Architecture Decision Records

| ADR | Title |
|---|---|
| 0001-0015 | Core architecture, toolchain, memory, neural primitives, roadmap |
| 0016 | [Network Strategy](docs/architecture/0016-network-strategy.md) |
| 0019 | [Neural Cortex — BitNet LLM Integration](docs/architecture/0019-neural-cortex-bitnet-llm.md) |
| 0020-0024 | Ecosystem Analysis (Crom, Life OS, PAI, Memory, Agents) |
| 0025 | [Tier 3 Sandbox & Security](docs/architecture/0025-tier3-sandbox-security-analysis.md) |

---

## ⚡ License

MIT — because revolutions need to be forkable.
