# neural-os-core

**AI Operating System — bare-metal Rust microkernel for neural inference orchestration.**

neural-os-core is an experimental AI Operating System (AIOS) built from scratch in Rust (`#![no_std]`, `x86_64-unknown-none`). It replaces traditional kernel schedulers with a Neural Cortex — an MLP-based intent router that classifies user embeddings into kernel actions. The architecture is organized as three hardware rings:

| Ring | Hardware | Role |
|---|---|---|
| 0 | NPU | Neural Microkernel — intent routing, context memory, NN primitives |
| 1 | GPU | Tensor execution and heavy lifting (future: SIMD/AVX matmul) |
| 2 | CPU | Wasmtime execution of ephemeral Daemons/Agents |

## Current State — Sprint 17 (TicketLock FIFO & Concurrency Refactor)

All 17 subsystems booting and verified in QEMU. The kernel boots, initializes memory + interrupts + SIMD, runs neural diagnostics (SiLU, RMSNorm, Tensor matmul, ternary BitNet), then spawns 5 cooperative async agents communicating via EventBus IPC:

```
[SYSTEM]  Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.
[EXCEPTION] Breakpoint Detectado
[TEST]    Box::new(41) = 41
[TEST]    SiLU([-1, 0, 1]) = [-0.26894143, 0.0, 0.7310586]
[ROUTER]  Intencao processada. Acao escolhida: 0 (0=Daemon, 1=Halt)
[BITNET]  Inferencia 2-bit concluida. Tamanho comprimido: 2 bytes. Output: [-0.5, -2.0]
[PIC]     8259A remapeado: PIC1 offset 32, PIC2 offset 40.
[CPU]     Interrupcoes de hardware habilitadas (IF=1).
[AGENT]   Spawn hw_bridge_daemon (poll scancode → EventBus)
[AGENT]   Spawn input_daemon (scan → buffer → USER_INTENT)
[AGENT]   Spawn intent_router_daemon (Cortex)
[AGENT]   Spawn system_daemon (SYSTEM_READY → EchoSkill)
[AGENT]   Spawn hardware_monitor_daemon (publish SYSTEM_READY)
[SKILL]   EchoSkill executada. Output reverso: [3, 2, 1]
[WATCHDOG] Ticks do temporizador: 100
```

| Component | Status |
|---|---|
| Bootloader (UEFI/BIOS) | ✅ `bootloader` v0.9.34, `map_physical_memory` |
| VGA Output | ✅ 80×25 text mode, scrolling, `print!/println!` |
| Serial Logging | ✅ 16550 UART (COM1, `0x3F8`), `serial_print!/serial_println!` |
| IDT (BP, DF, PF, GPF, NP, SS, TS, AC) | ✅ 8 exception handlers + IST for DF |
| GDT + TSS | ✅ Custom GDT with TSS descriptor |
| Page Tables | ✅ `OffsetPageTable` via `Cr3` + `physical_memory_offset` |
| Frame Allocator | ✅ `BitmapFrameAllocator` (128 KB bitmap, 4 GB, alloc+dealloc real) |
| Contiguous Alloc | ✅ `allocate_contiguous(count)` for Huge Pages |
| `alloc` crate | ✅ `LockedHeap`, 100 KB, `Box`/`Vec` tested |
| FPU/SSE (SIMD) | ✅ CR0/CR4 ativados, `f32`/`f64` native |
| Tensor Engine | ✅ `Tensor` struct, `matmul()`, `transposed()`, `apply<F>` |
| Neural Primitives | ✅ SiLU, RMSNorm, Linear layer, argmax |
| Intent Router | ✅ MLP 1×3 → 2, forward pass, argmax decision |
| PIC 8259A | ✅ Remapped (PIC1→32, PIC2→40), watchdog timer |
| PIT Watchdog | ✅ ~18.2 Hz atomic counter, EOI |
| Page Fault Handler | ✅ CR2 read, security log, hlt abort |
| Frame Deallocator | ✅ `BitmapFrameAllocator` implements `FrameDeallocator` — real reuse |
| TernaryTensor | ✅ `Vec<i8>` weights, 4× compression vs f32 |
| Hybrid MatMul | ✅ ADD/SUB-only kernel — zero FPU multiplications |
| BitLinear Layer | ✅ Ternary dense layer with `matmul_hybrid()` |
| PackedTernaryTensor | ✅ 2-bit packing — 4 weights/byte, 12× vs f32 |
| Quantization | ✅ `quantize_to_packed(f32, Δ)` — calibration pass |
| Comp. BitLinear | ✅ `PackedTernaryTensor` — inference from 2-bit storage |
| TicketLock FIFO | ✅ `ticket-lock` crate — `AtomicUsize ticket/serving`, spin loop justo |
| EventBus IPC | ✅ `event-bus` crate — publish/subscribe com `CapabilityToken` |
| Skill Registry MCP | ✅ `skill-registry` crate — `Skill` trait + `McpManifest` + zero-trust |
| Async Executor | ✅ `NeuralExecutor` — `VecDeque<AgentTask>` loop cooperativo |
| Agent: HW Bridge | ✅ Poll `LAST_SCANCODE` → publica `RAW_HW_IRQ1` no EventBus |
| Agent: Input Daemon | ✅ `RAW_HW_IRQ1` → buffer String → `USER_INTENT` |
| Agent: Cortex Router | ✅ `USER_INTENT` → mock inference → `SkillRegistry` exec |
| Agent: System Daemon | ✅ Assina `SYSTEM_READY` → executa `EchoSkill` |
| Top-Half/Bottom-Half | ✅ Interrupt handler µs (port read + atomic + EOI); HW Bridge em user-context |
| Ciclo Intenção Fechado | ✅ Teclado → buffer → USER_INTENT → Córtex → Skill Registry → log RAM |

## Prerequisites

| Tool | Version | Install |
|---|---|---|
| Rust | nightly | `rustup toolchain install nightly; rustup component add llvm-tools-preview` |
| bootimage | 0.10.x | `cargo install bootimage` |
| QEMU | 7+ | `winget install QEMU` (or [qemu.org](https://qemu.org)) |
| Linker | MinGW-w64 | `pacman -S mingw-w64-x86_64-gcc` (MSYS2) |

> **Windows:** Add `C:\msys64\mingw64\bin` and `C:\Program Files\qemu` to `$env:Path`.

## Quick Start

```powershell
cargo run
```

QEMU window opens with VGA output. Serial output streams to terminal. Close the window to stop.

### Commands

```powershell
cargo run          # build + boot in QEMU
cargo check        # type-check (no codegen)
cargo bootimage    # create bootable image (without running)
```

## Architecture

### Boot Sequence

```
cargo run
  └─ bootimage runner
       ├─ build bootloader v0.9.34
       ├─ combine → bootimage.bin
       └─ qemu-system-x86_64 -m 2G -serial stdio
            └─ bootloader → long mode, page tables
                 └─ kernel_main(&'static BootInfo)
                      ├─ vga_buffer::init(offset)
                      ├─ interrupts::init_idt()
                      │    ├─ GDT.load + CS::set_reg
                      │    ├─ load_tss
                      │    └─ IDT.load (8 exception + PIT + keyboard handlers)
                      ├─ memory::init_memory(offset)
                      ├─ BitmapFrameAllocator::init(memory_map)
                      ├─ allocator::init_heap(mapper, alloc)
                      ├─ simd::enable_simd() — CR0/CR4
                      ├─ int3() → Breakpoint handler → log → ret
                      ├─ Neural diagnostics: Box, Vec, matmul, SiLU, RMSNorm
                      ├─ Intent Router: Linear(3→2) → SiLU → argmax
                      ├─ BitNet: quantize_to_packed() → 2-bit forward
                      ├─ Stress test: 1000 alloc/dealloc
                      ├─ init_pics() + enable_interrupts() (sti)
                      ├─ init_global_allocator(frame_allocator) ← TicketLock
                      └─ NeuralExecutor::run()
                           ├─ hw_bridge_daemon (poll scancode → EventBus)
                           ├─ input_daemon (scan → buffer → USER_INTENT)
                           ├─ intent_router_daemon (Cortex: mock inference)
                           ├─ system_daemon (EchoSkill via SYSTEM_READY)
                           ├─ hardware_monitor_daemon (publish SYSTEM_READY)
                           └─ loop { poll tasks; hlt() }
```

### Hardware Interrupt Handling

| Vector | Source | Handler |
|---|---|---|
| 3 | `#BP` (Breakpoint) | Log + return |
| 8 | `#DF` (Double Fault) | Log + panic + hlt (IST stack) |
| 10 | `#TS` (Invalid TSS) | Log + hlt |
| 11 | `#NP` (Segment Not Present) | Log + hlt |
| 12 | `#SS` (Stack Segment) | Log + hlt |
| 13 | `#GP` (General Protection) | Log + hlt |
| 14 | `#PF` (Page Fault) | CR2 read + security log + hlt |
| 17 | `#AC` (Alignment Check) | Log + hlt |
| 0–15 → 32–47 | PIC (8259A) | Remapped, no conflict |
| 32 | PIT Timer (IRQ 0) | Atomic counter + EOI |
| 33 | Keyboard (IRQ 1) | Port 0x60 → AtomicU8 + EOI |
| 34–255 | Unhandled | EOI duplo (master + slave) |

### Heap & Memory

| Component | Address | Size |
|---|---|---|
| Heap | `0x4444_4444_0000` | 100 KB |
| Heap allocator | — | `linked_list_allocator::LockedHeap` |
| Frame allocator | — | `BitmapFrameAllocator` (128 KB bitmap, 4 GB, alloc+dealloc real) |
| Page table mapper | — | `OffsetPageTable<'static>` |
| Sync primitive | — | `TicketLock<T>` (FIFO, AtomicUsize, spin loop) |

## Innovation Roadmap

| Phase | Title | Target |
|---|---|---|
| **3** | [Ternary Inference (BitNet b1.58)](docs/architecture/0010-strategic-roadmap-and-innovations.md#phase-3--ternary-inference-bitnet-b158) — Complete (Sprints 9-10) | Q3 2026 |
| **4** | [Zero-Copy Semantic File System](docs/architecture/0010-strategic-roadmap-and-innovations.md#phase-4--zero-copy-semantic-file-system) | Q4 2026 |
| **5** | [Skills-as-Modules (WASM Component Model)](docs/architecture/0010-strategic-roadmap-and-innovations.md#phase-5--skills-as-modules-wasm-component-model) | Q1 2027 |
| **6** | [Hardware-Aware AIOS Syscalls](docs/architecture/0010-strategic-roadmap-and-innovations.md#phase-6--hardware-aware-aios-syscalls-zero-trust) | Q2 2027 |

See [ADR-0010](docs/architecture/0010-strategic-roadmap-and-innovations.md) for full architectural details.  
See [docs/roadmap.md](docs/roadmap.md) for the updated bare-metal engineering order.

## Project Structure

```
neural-os-core/                          # Cargo workspace root
├── .cargo/config.toml                   # target, runner, rustflags
├── Cargo.toml                           # workspace manifest (resolver = "2")
├── CHANGELOG.md
├── rust-toolchain.toml
├── AGENTS.md
├── README.md
├── docs/
│   ├── architecture/                    # ADRs (0001–0014)
│   ├── memory/                          # STATE.md + SESSION_*.md logs
│   └── roadmap.md                       # Fases 3–7 engineering order
└── crates/
    ├── neural-kernel/                   # Bare-metal kernel (bootloader, VGA, serial,
    │   ├── src/                         # IDT, memory, SIMD, tensor, NN, async executor)
    │   └── Cargo.toml
    ├── ticket-lock/                     # TicketLock FIFO (AtomicUsize + UnsafeCell)
    │   ├── src/lib.rs
    │   └── Cargo.toml
    ├── agent-core/                      # AgentProcess trait + scheduler (stub)
    │   ├── src/lib.rs
    │   └── Cargo.toml
    ├── skill-registry/                  # Skill trait + MCP Layer (zero-trust)
    │   ├── src/ (lib, skill, mcp, registry)
    │   └── Cargo.toml
    └── event-bus/                       # IPC publish/subscribe (CapabilityToken)
        ├── src/ (lib, bus, event, capability)
        └── Cargo.toml
```

## ADR Index

| ADR | Title |
|---|---|
| 0001 | [Initial Architecture and Toolchain](docs/architecture/0001-initial-architecture-and-toolchain.md) |
| 0002 | [VGA and Serial Logging Infrastructure](docs/architecture/0002-vga-and-serial-logging.md) |
| 0003 | [Interrupt Descriptor Table](docs/architecture/0003-interrupt-descriptor-table.md) |
| 0004 | [Memory Paging and Heap Allocation](docs/architecture/0004-memory-paging-and-heap.md) |
| 0005 | [SIMD and FPU Enablement](docs/architecture/0005-simd-and-fpu-enablement.md) |
| 0006 | [Neural Primitives and libm](docs/architecture/0006-neural-primitives-and-libm.md) |
| 0007 | [Intent Router MLP](docs/architecture/0007-intent-router-mlp.md) |
| 0009 | [PIC Watchdog and Page Fault Safety](docs/architecture/0009-pic-watchdog-and-page-fault.md) |
| 0010 | [Strategic Roadmap and Innovations](docs/architecture/0010-strategic-roadmap-and-innovations.md) |
| 0011 | [BitLinear and Hybrid Ternary MatMul](docs/architecture/0011-bitlinear-and-hybrid-matmul.md) |
| 0012 | [2-bit Packing and Ternary Quantization](docs/architecture/0012-2bit-packing-quantization.md) |
| 0013 | [Neural OS Executive Summary (SotA 2026)](docs/architecture/0013-neural-os-executive-summary.md) |
| 0014 | [Ideias de Evolução de Hardware](docs/architecture/0014-ideias-hardware.md) |

## Crate Dependencies (neural-kernel)

| Crate | Version | Purpose |
|---|---|---|
| `bootloader` | 0.9.34 | Boot image + BootInfo + map_physical_memory |
| `spin` | 0.9 | `Mutex<T>` for `no_std` synchronization |
| `lazy_static` | 1.5 | Lazy initialization with `spin_no_std` |
| `uart_16550` | 0.2 | 16550 UART serial driver |
| `x86_64` | 0.14.11 | IDT, GDT, TSS, page tables, CPU instructions |
| `linked_list_allocator` | 0.9.1 | `LockedHeap` global allocator |
| `libm` | 0.2.16 | `expf`, `sqrtf` — math functions in `no_std` |
| `pic8259` | 0.10.4 | 8259A PIC driver — remap IRQ, send EOI |
| `event-bus` | 0.1.0 (workspace) | IPC publish/subscribe |
| `skill-registry` | 0.1.0 (workspace) | Skill trait + MCP zero-trust |
| `ticket-lock` | 0.1.0 (workspace) | TicketLock FIFO synchronization |

## License

MIT
