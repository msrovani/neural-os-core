# Neural OS Hermes ⚡ — v0.30.0 🏆 USB DETECTION + FINAL MODEL 🏆

**The first AI-native operating system. Bare-metal Rust. No Linux. No POSIX. No legacy.**

```
           ╔═══════════════════════════════════════════════════════════╗
           ║  "We don't need an OS that runs AI.                     ║
           ║   We need an OS that IS AI."                            ║
           ╚═══════════════════════════════════════════════════════════╝
```

Every OS today bolts AI on top — a chatbot in the taskbar, a copilot in the browser, a GPU driver that lets PyTorch run. The kernel doesn't know what AI is. It never will.

**Neural OS Hermes is different.** The kernel IS a neural inference engine. The Cortex classifies every user intent. **A 4-layer BitNet transformer with ~177K ternary parameters generates text from user input — trained on 66K pairs: PCI IDs, USB IDs, SMBIOS, kernel code, and git history.** An xHCI USB driver detects connected devices and the LLM identifies them. Seven cooperative AI agents run the system. No shell. No init. No systemd. Just tensors, events, and intent.

---

## 🚀 Boot Timeline — What Happens at Power-On

From firmware to interactive Hermes console, the system progresses through 11 phases:

### Phase 1 — Bootloader (UEFI → 64-bit)
```
SeaBIOS → iPXE → bootloader crate → kernel_main
```
The `bootloader` crate (v0.9.34) transitions the CPU from firmware to long mode, sets up 4-level page tables with `map_physical_memory`, and passes a `BootInfo` struct containing the memory map and physical memory offset. The kernel starts at `kernel_main`.

### Phase 2 — VGA + Serial (Dual Output)
```
vga_buffer::init(offset) → VGA 80×25 color text buffer
serial::init() → COM1 at port 0x3F8 (host terminal)
```
Every `print!`/`println!` writes to both the QEMU window and the host terminal simultaneously. The panic handler also dual-outputs, ensuring no error is ever invisible.

### Phase 3 — IDT + GDT + TSS (Exception & Interrupt Framework)
```
interrupts::init_idt() → 8 CPU exception handlers
  → Breakpoint (int3): logs and returns execution
  → Double Fault: IST stack switch → 20KB buffer → prevents triple fault
  → Page Fault: reads CR2 → logs address + error code → halts
  → Timer: LAPIC-based, atomic 64-bit counter
  → Keyboard: IRQ1 → read port 0x60 → AtomicU8 scancode release
```
The GDT is rebuilt (bootloader's minimal GDT doesn't include TSS). The TSS provides an Interrupt Stack Table that gives Double Fault its own dedicated stack, preventing a cascade into Triple Fault → reboot.

### Phase 4 — Memory & Heap (Allocator Foundation)
```
memory::init_memory(phys_offset) → OffsetPageTable via CR3
allocator::init_heap() → 4 MB heap at VirtAddr 0x4444_4444_0000
  → LockedHeap: 3.5 MB (linked_list_allocator crate)
  → SlabAllocator: 512 KB, 8 buckets (32, 64, 128, 256, 512, 1024, 2048, 4096 bytes)
```
Page tables are traversed via the physical memory offset (no recursive tables needed). The heap supports both general-purpose allocations (LockedHeap) and fast fixed-size allocations (Slab).

### Phase 5 — SIMD + Tensor Core (Neural Compute Foundation)
```
enable_simd() → CR0.EM=0, CR0.MP=1, CR4.OSFXSR=1, CR4.OSXMMEXCPT=1
→ Tensor<f32>: matmul, SiLU, RMSNorm, softmax
→ PackedTernaryTensor: 2-bit ternary weights (4 per byte), matmul_hybrid() ADD/SUB kernel
→ BitLinear: forward pass with zero multiplications
```
All neural computation uses pure `no_std` Rust. The `libm` crate provides `expf` for SiLU activation and `sqrtf` for RMSNorm. No FPU multiplications in the critical path — BitLinear's `matmul_hybrid` uses only integer ADD/SUB on packed ternary weights.

### Phase 6 — PCI + ACPI + APIC (Hardware Discovery)
```
PCI: CF8/CFC config space → 256 buses × 32 devices → vendor/device/class/BARs
ACPI: RSDP (EBDA + BIOS search) → RSDT/XSDT → MADT (LAPIC/IOAPIC/x2APIC)
APIC: LAPIC SVR+TPR init → IOAPIC IRQ0→vec32, IRQ1→vec33 → PIC mask
```
The kernel discovers QEMU's virtual devices. `set_page_uc()` maps APIC/IOAPIC MMIO as uncacheable (handles 2MB and 1GB huge pages). The LAPIC timer replaces the PIT.

### Phase 7 — SMP Multi-Core (All Cores Online)
```
INIT → SIPI → SIPI → trampoline at phys 0x40000 → 3 APs boot
OffsetPageTable::map_to() → handles 2MB/1GB huge page splitting
PerCpu struct at GS.base → cpu_id(), this_cpu()
```
The trampoline code transitions APs from 16-bit real mode through 32-bit protected mode to 64-bit long mode, then calls a Rust function. All 4 cores (BSP + 3 APs) enter their hlt loop.

### Phase 8 — Architecture Inference + Memory Hierarchy
```
SystemArchitecture::infer() → ring0/ring1/heap/trust/power/tensor heuristics
MemoryHierarchy::new() → Dram tier from BitmapFrameAllocator free frames
```
The system classifies itself (GPU present? >4GB RAM? >4 cores?) and configures itself accordingly. The MHI (Memory Hierarchy Index) tracks available memory per tier.

### Phase 9 — RTL8139 Network + smoltcp TCP/IP
```
init_driver_rtl8139() → I/O ports (BAR0), 4 TX descriptors, RX ring buffer
NetStack lazy init → smoltcp Interface + PhyDevice trait + SocketSet
NetworkAgent → poll per tick → DNS resolve → HTTP connect (non-blocking)
```
The RTL8139 NIC is the network hardware. smoltcp handles ARP, IP, TCP, and UDP. The network agent polls once per tick.

### Phase 10 — NeuralExecutor (7 Agents Start)
```
NeuralExecutor::run() → VecDeque<AgentTask> cooperative polling loop
  
  Agent 1: system_daemon        → publishes SYSTEM_READY
  Agent 2: hardware_monitor     → logs context tensor every 100 ticks
  Agent 3: hw_bridge            → reads scancode AtomicU8 → EventBus
  Agent 4: network_agent        → smoltcp poll → HTTP → timeline log
  Agent 5: input_daemon         → ASCII buffer → ENTER → USER_INTENT
  Agent 6: intent_router        → Cortex.think() → SkillRegistry.dispatch()
  Agent 7: hermes_console       → subscribes HERMES_RESPONSE → VGA display
```
Each agent is an `async fn` that loops forever, calling `yield_now().await` at the end of every iteration. No preemptive scheduling. No locks needed (cooperative model).

### Phase 11 — Cortex Neural Intent Router
```
USER_INTENT → Cortex.think("user text") → Intent enum (12 categories)
  → SKILL_REGISTRY.has_skill() → execute_skill_with_trust() → VGA output
```
The Cortex classifies user text and dispatches to the appropriate skill.

### Full Console Output
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
[EXECUTOR] 7 tasks spawned
[NET @t=10] Online. IP: 10.0.2.15
[SKILL]   EchoSkill executada. Output reverso: [3, 2, 1]
[WATCHDOG] Ticks do temporizador: 13200+

### 🏆 USB Detection via xHCI (26/06/2026)
```
[PCI] 00:04.00 1033:0194 class=0c subclass=03
[XHCI] Init: 16 slots, 64 portas
[USB] Porta 0: conectado
[HW-SCAN] 00:04.0 1033:0194 class=0c/03 USB 3.0 (xHCI)
```

### 🏆 HW-Aware Cortex LLM (26/06/2026)
O modelo foi treinado com 23.858 entradas PCI ID. Agora identifica hardware:
```
> identifique hardware
[CORTEX-LLM] Generating for: "Identifique estes dispositivos PCI:
8086:1237 classe 06/00
8086:7000 classe 06/01
1234:1111 classe 03/00
10ec:8139 classe 02/00"
[CORTEX-LLM] Generated: "8086 1237 intel 82441fx pmc bridge... 10ec 8139 realtek rtl8139 fast ethernet"
[Hermes] Intel 82441FX PMC — Host Bridge, class 06/00
```

### 🏆 Transformer LLM Gerando Texto (26/06/2026)
```
> OI
[CORTEX-LLM] Generating for: "OI"
[CORTEX-LLM] Generated: ".$={=T*=.=.=.=.=.=.=.=.=.=.=.=.="
[Hermes] .$={=T*=.=.=.=.=.=.=.=.=.=.=.=.=
```
O primeiro bare-metal transformer a gerar texto num kernel `no_std`. Pesos aleatórios por enquanto — o próximo passo é treinar um modelo de verdade.
```

---

## 🧠 Neural Architecture — How Hermes Thinks

The system is designed as a three-ring neural decision pipeline, connected by an Event Bus, coordinated by a cooperative executor, and backed by a neural tensor engine.

### The Three Rings

```
                    ┌─────────────────────────────────────┐
                    │         USER / HARDWARE EVENT       │
                    └────────────┬────────────────────────┘
                                 │
                    ┌────────────▼────────────┐
                    │  Ring 0 — Reflex        │
                    │  Cortex.think()         │
                    │  12 intents             │
                    │  Sub-µs classification  │
                    │  "Precisa do LLM?"      │
                    └────────────┬────────────┘
                                 │ se preciso
                    ┌────────────▼────────────┐
                    │  Ring 1 — Cortex LLM    │
                    │  BitNet 1.5B params     │
                    │  2-bit ternary weights  │
                    │  ~375 MB model          │
                    │  5-15 tok/s (AVX2)      │
                    │  "Intenção, ação, tier" │
                    └────────────┬────────────┘
                                 │ decisão
                    ┌────────────▼────────────┐
                    │  Ring 2 — Skills        │
                    │  SkillRegistry          │
                    │  Zero-trust tokens      │
                    │  TrustCache             │
                    │  WASM (future)          │
                    └─────────────────────────┘
```

#### Ring 0 — Reflex (sub-µs)
The always-on, always-fast classification layer. The `Cortex::think()` function runs a keyword-based neural classifier that maps user text to one of 12 intent categories. This is the equivalent of a spinal reflex — it handles every keystroke before deciding whether the full LLM (Ring 1) is needed.

**Implemented:** Sprint 25 — `cortex.rs`, 12 intents, direct skill dispatch.

**Future:** A trained MLP (16→8→12) replaces keyword matching. Confidence threshold: >0.9 → dispatch without LLM; <0.9 → escalate to Ring 1.

#### Ring 1 — Cortex LLM (5-15 tok/s)
The BitNet 1.5B parameter ternary LLM. This is the "brain" — it handles all non-realtime decisions: complex intent routing, USB device classification, memory tier selection, power management, and skill orchestration.

**Designed:** ADR-0019 (`.bitnet` format, transformer architecture with 2-bit packed weights).

**Implemented (Sprint 26+):** Attention engine, TransformerBlock, character-level tokenizer, autoregressive `generate()`.

**Not yet in kernel:** Training pipeline (host-side Python).

#### Ring 2 — Skills (execution layer)
Skills are the "hands" — they execute actions. Each skill is a Rust struct implementing the `Skill` trait with a `manifest()` and `execute()` method. Skills are registered in the `SkillRegistry` and protected by `CapabilityToken` zero-trust validation.

**Current skills:**
| Skill | Intent Trigger | What It Does |
|---|---|---|
| `EchoSkill` | "echo hello" / "reverse 123" | Reverses the input bytes |
| `SystemStatusSkill` | "status" / "system info" | Reads MHI tiers + BitmapAllocator → reports RAM |
| `HardwareInfoSkill` | "hw" / "hardware" | Exposes SystemArchitecture (ring0, ring1, heap) |
| `NetDiagnosticSkill` | "ping" / "netdiag" | Reports NET_CONFIG (MAC, IP, gateway, DNS) |

### The Event Bus — Nervous System

```
                   ┌──────────────────────────┐
                   │        EventBus          │
                   │  TicketLock<BTreeMap>    │
                   │  Topics: SYSTEM_READY    │
                   │          USER_INTENT     │
                   │          HERMES_RESPONSE │
                   │          HW_NET_*        │
                   └────┬──────┬──────┬───────┘
                        │      │      │
              ┌─────────┘      │      └──────────┐
              ▼                ▼                  ▼
        hw_bridge        input_daemon       hermes_console
        (produces)       (transforms)       (consumes)
```

The EventBus is the nervous system — it carries events between agents. Built with a `TicketLock`-protected `BTreeMap<String, Vec<Receiver>>`, it provides fair FIFO access without starvation. Events carry a `CapabilityToken` for zero-trust validation.

**Key event flow:**
1. `hw_bridge` publishes `RAW_HW_IRQ1` raw scancode
2. `input_daemon` subscribes, builds ASCII buffer, publishes `USER_INTENT`
3. `intent_router` subscribes, classifies via Cortex, publishes `HERMES_RESPONSE`
4. `hermes_console` subscribes, displays on VGA

### The NeuralExecutor — Heartbeat

```
loop {
    for each task in tasks {
        task.poll();  // advance 1 state
    }
    log_hardware_context(every 100 iterations);
    hlt();  // wait for next timer tick
}
```

The NeuralExecutor is a cooperative round-robin scheduler. Each task runs to completion of one state transition, then yields. There is no preemption — tasks voluntarily `yield_now().await` every iteration. This eliminates race conditions and the need for complex synchronization.

**7 tasks:**
| Task | Role | State | Persists? |
|---|---|---|---|
| system_daemon | Publish SYSTEM_READY | One-shot | Dies after first tick |
| hardware_monitor | Log context tensor | Loop | ✅ Yes |
| hw_bridge | Read scancode → EventBus | Loop | ✅ Yes |
| network_agent | smoltcp poll + HTTP | Loop | ✅ Yes |
| input_daemon | Build ASCII buffer | Loop | ✅ Yes |
| intent_router | Cortex.think() → Skill | Loop | ✅ Yes |
| hermes_console | Display [Hermes] | Loop | ✅ Yes |

### The Memory Hierarchy — Bloodstream

```
         ┌─────────┐
         │  MHI    │  MemoryHierarchy { tiers: Vec<MemoryTier> }
         └────┬────┘
              │ alloc_by_tier(Dram) → contiguous physical frames
              ▼
    ┌─────────────────┐
    │ BitmapAllocator │  128 KB bitmap → 4 GB physical
    └─────────────────┘
         │ allocate_frame()
         ▼
    ┌─────────────────┐
    │  Physical RAM   │
    └─────────────────┘
```

The Memory Hierarchy Index treats all physical memory as a single pool divided into tiers by bandwidth and latency. Currently only `Dram` is implemented (detected from the UEFI memory map). Future tiers: `Vram` (GPU), `Nvme` (SSD), `Hdd` (disk).

## 🔄 End-to-End Pipeline (Keystroke → Response)

```
1. KEYBOARD INTERRUPT (hardware)
   CPU receives IRQ1 → IDT[33] fires → keyboard_handler()
   → reads port 0x60 (scancode) → writes to LAST_SCANCODE (AtomicU8)
   → sends EOI (LAPIC write to 0xFEE000B0)

2. HW BRIDGE (agent)
   hw_bridge_daemon polls LAST_SCANCODE via AtomicU8::swap(0)
   → publishes RAW_HW_IRQ1 { scancode } on EventBus

3. INPUT DAEMON (agent)
   input_daemon receives RAW_HW_IRQ1
   → scancode_to_ascii() → appends to String buffer
   → on ENTER (0x1C): publishes USER_INTENT { "text" }

4. CORTEX (agent + neural)
   intent_router_daemon receives USER_INTENT
   → cortex.think("text") → Intent::SystemStatus | Echo | Chat | ...
   → if skill exists: SKILL_REGISTRY.execute_skill(name, payload, token)
   → if chat/greeting: inline response
   → publishes HERMES_RESPONSE { output }

5. CONSOLE (agent)
   hermes_console_daemon receives HERMES_RESPONSE
   → prints [Hermes] on VGA + serial
   → updates ConversationTracker + EventLog

6. CONTINUOUS (background)
   WATCHDOG every 100 ticks: log TIMER_TICKS + context tensor
   NETWORK every tick: smoltcp poll
   HEALTH every 200 ticks: "Health"
```

## 🗺️ Roadmap — What's Next

| Sprint | Block | Deliverable | Status |
|---|---|---|---|
| 1-22 | 0-5 | MVP base (toolchain, VGA, heap, SIMD, IPC, SMP, skills, APIC) | ✅ |
| 23 | 6 | RTL8139 driver + Neural Network Agent + TCP handshake | ✅ |
| 24 | 7 | smoltcp + e1000 removal + SMP huge page fix | ✅ |
| 25 | 8 | Neural Cortex in Hermes — 12-intent router, skill dispatch | ✅ |
| 26 | 9 | Transformer Engine (Attention, generate, tokenizer, micro-model) | ✅ |
| 27 | 10 | Cortex Daemon (LLM request/response, prompt templates) | ✅ |
| **28** | **11** | **HW-Aware Cortex LLM + PCI ID training + HwIdentifySkill** | **✅ Current** |
| 29 | 12 | xHCI USB Driver + port enumeration | ✅ |
| **30** | **13** | **USB speed detection + final model 66K pairs + HW identification** | **✅ Current** |
| 31 | 14 | Full USB device descriptors (vendor/device ID via TRB) | 🟡 Próximo |
| 32+ | 15 | Networked Cortex + WASM + multi-agent | 🔲 |

## 🧬 Module Map

| Module | Lines | What |
|---|---|---|
| `main.rs` | ~700 | Entry point, lazy_static! init, 7 daemon definitions, skill registry |
| `cortex.rs` | 101 | Neural intent router — 12 categories, dispatch to skills |
| `hermes.rs` | 182 | Command parser, IntentMlp (legacy), ConversationTracker |
| `memory.rs` | 253 | BitmapFrameAllocator, PageTable walk, phys mem offset |
| `netstack.rs` | 321 | smoltcp Device trait (Rtl8139Phy), HTTP non-blocking API |
| `network_agent.rs` | 113 | Async daemon: poll → HTTP → timeline `[NET @t=NN]` |
| `rtl8139.rs` | 250 | NIC driver: I/O ports, 4 TX desc, RX ring buffer |
| `apic.rs` | 316 | LAPIC timer, IOAPIC redirects, SMP IPI, page UC mapping |
| `smp/` | 200+ | Trampoline real-mode→64-bit, PerCpu GS.base |
| `pci.rs` | 150+ | PCI config via CF8/CFC, BAR decoding |
| `acpi.rs` | 100+ | RSDP, MADT, Interrupt Source Override parsing |
| `nn.rs` | 100+ | Linear, SiLU, RMSNorm, BitLinear matmul_hybrid |
| `tensor.rs` | 200+ | Tensor f32, PackedTernaryTensor 2-bit |
| `time_utils.rs` | 22 | UNIX → Brazil datetime formatter |
| `trust.rs` | ~80 | TrustCache with TTL, allow/deny lists |
| `usage.rs` | 73 | UsageTracker: per-skill call counts, metrics tensor |
| `conversation.rs` | 79 | EventLog: VecDeque, last_n, summarize |
| `proto.rs` | 101 | Ethernet/IP/ARP/ICMP header builders and parsers |
| `xhci.rs` | 118 | xHCI USB 3.0 driver — port scan, device detect, speed detection |

## 🛠️ Quick Start

```powershell
# Prerequisites
rustup toolchain install nightly
rustup target add x86_64-unknown-none
rustup component add llvm-tools-preview
cargo install bootimage

# Install MemPalace (AI memory system)
pip install mempalace
mempalace init .

# Build and boot in QEMU
cargo bootimage --release
qemu-system-x86_64 -m 2G -serial stdio `
  -nic user,model=rtl8139 `
  -drive format=raw,file=target\x86_64-unknown-none\release\bootimage-neural-kernel.bin `
  -no-reboot -smp 4 -nographic
```

The system boots to an interactive Hermes console. Type anything — the Cortex classifies your intent and responds.

---

## 📚 Architecture Decision Records

| ADR | Title |
|---|---|
| 0001-0015 | Core architecture, toolchain, memory, neural primitives, roadmap |
| 0016 | [Network Strategy](docs/architecture/0016-network-strategy.md) |
| 0019 | [Neural Cortex — BitNet LLM Integration](docs/architecture/0019-neural-cortex-bitnet-llm.md) |
| 0020-0024 | Ecosystem Analysis (Crom, Life OS, PAI, Memory, Agent Frameworks) |
| 0025 | [Tier 3 Sandbox & Security](docs/architecture/0025-tier3-sandbox-security-analysis.md) |

---

## ⚡ License

MIT — because revolutions need to be forkable.
