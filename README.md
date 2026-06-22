# neural-os-core

**AI Operating System — bare-metal Rust microkernel for neural inference orchestration.**

neural-os-core is an experimental AI Operating System (AIOS) built from scratch in Rust (`#![no_std]`, `x86_64-unknown-none`). It replaces traditional kernel schedulers with a Neural Cortex — an MLP-based intent router that classifies user embeddings into kernel actions. The architecture is organized as three hardware rings:

| Ring | Hardware | Role |
|---|---|---|
| 0 | NPU | Neural Microkernel — intent routing, context memory, NN primitives |
| 1 | GPU | Tensor execution and heavy lifting (future: SIMD/AVX matmul) |
| 2 | CPU | Wasmtime execution of ephemeral Daemons/Agents |

## Current State — Sprint 9 (Ternary Inference Engine)

All 9 subsystems booting and verified in QEMU:

```
[SYSTEM]  Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.
[EXCEPTION] Breakpoint Detectado
[TEST]    Box::new(41) = 41
[TEST]    SiLU([-1, 0, 1]) = [-0.26894143, 0.0, 0.7310586]
[ROUTER]  Intencao processada. Acao escolhida: 0 (0=Daemon, 1=Halt)
[BITNET]  Inferencia Hibrida concluida. Resultado: [-0.5, -2.0]
[PIC]     8259A remapeado: PIC1 offset 32, PIC2 offset 40.
[WATCHDOG] Ticks do temporizador: 100
```

| Component | Status |
|---|---|
| Bootloader (UEFI/BIOS) | ✅ `bootloader` v0.9.34, `map_physical_memory` |
| VGA Output | ✅ 80×25 text mode, scrolling, `print!/println!` |
| Serial Logging | ✅ 16550 UART (COM1, `0x3F8`), `serial_print!/serial_println!` |
| IDT (BP, DF, PF) | ✅ IST stack switching, CR2 diagnostics |
| GDT + TSS | ✅ Custom GDT with TSS descriptor |
| Page Tables | ✅ `OffsetPageTable` via `Cr3` + `physical_memory_offset` |
| Frame Allocator | ✅ `BootInfoFrameAllocator` (UEFI/BIOS memory map) |
| `alloc` crate | ✅ `LockedHeap`, 100 KB, `Box`/`Vec` tested |
| FPU/SSE (SIMD) | ✅ CR0/CR4 ativados, `f32`/`f64` native |
| Tensor Engine | ✅ `Tensor` struct, `matmul()`, `transposed()`, `apply<F>` |
| Neural Primitives | ✅ SiLU, RMSNorm, Linear layer, argmax |
| Intent Router | ✅ MLP 1×3 → 2, forward pass, argmax decision |
| PIC 8259A | ✅ Remapped (PIC1→32, PIC2→40), watchdog timer |
| PIT Watchdog | ✅ ~18.2 Hz atomic counter, EOI |
| Page Fault Handler | ✅ CR2 read, security log, hlt abort |
| Frame Deallocator | ✅ `FrameDeallocator` trait + stub |
| TernaryTensor | ✅ `Vec<i8>` weights, 4× compression vs f32 |
| Hybrid MatMul | ✅ ADD/SUB-only kernel — zero FPU multiplications |
| BitLinear Layer | ✅ Ternary dense layer with `matmul_hybrid()` |

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
                      │    └─ IDT.load
                      ├─ memory::init_memory(offset)
                      ├─ BootInfoFrameAllocator::init
                      ├─ allocator::init_heap(mapper, alloc)
                      ├─ simd::enable_simd()
                      ├─ int3() → Breakpoint handler → log → ret
                       ├─ Box/Vec/Tensor/SiLU/RMSNorm tests
                       ├─ Intent Router: Linear(3→2) → SiLU → argmax
                       ├─ BitNet: BitLinear ternary matmul ← Sprint 9
                       ├─ interrupts::init_pics()
                       ├─ interrupts::enable_interrupts()
                       └─ loop { hlt(); watchdog ticks }
```

### Hardware Interrupt Handling

| Vector | Source | Handler |
|---|---|---|
| 3 | `#BP` (Breakpoint) | Log + return |
| 8 | `#DF` (Double Fault) | Log + panic + hlt |
| 14 | `#PF` (Page Fault) | CR2 read + security log + hlt |
| 0–15 → 32–47 | PIC (8259A) | Remapped, no conflict |
| 32 | PIT Timer (IRQ 0) | Atomic counter + EOI |

### Heap & Memory

| Component | Address | Size |
|---|---|---|
| Heap | `0x4444_4444_0000` | 100 KB |
| Heap allocator | — | `linked_list_allocator::LockedHeap` |
| Frame allocator | — | `BootInfoFrameAllocator` (monotonic; dealloc stub) |
| Page table mapper | — | `OffsetPageTable<'static>` |

## Innovation Roadmap

| Phase | Title | Target |
|---|---|---|
| **3** | [Ternary Inference (BitNet b1.58)](docs/architecture/0010-strategic-roadmap-and-innovations.md#phase-3--ternary-inference-bitnet-b158) — Hybrid engine done | Q3 2026 |
| **4** | [Zero-Copy Semantic File System](docs/architecture/0010-strategic-roadmap-and-innovations.md#phase-4--zero-copy-semantic-file-system) | Q4 2026 |
| **5** | [Skills-as-Modules (WASM Component Model)](docs/architecture/0010-strategic-roadmap-and-innovations.md#phase-5--skills-as-modules-wasm-component-model) | Q1 2027 |
| **6** | [Hardware-Aware AIOS Syscalls](docs/architecture/0010-strategic-roadmap-and-innovations.md#phase-6--hardware-aware-aios-syscalls-zero-trust) | Q2 2027 |

See [ADR-0010](docs/architecture/0010-strategic-roadmap-and-innovations.md) for full architectural details.

## Project Structure

```
neural-os-core/
├── .cargo/config.toml          # target, runner, rustflags
├── src/
│   ├── main.rs                 # entry_point!, panic handler, boot flow
│   ├── vga_buffer.rs           # VGA text mode, print!/println!
│   ├── serial.rs               # 16550 UART, serial_print!/serial_println!
│   ├── interrupts.rs           # IDT, TSS, GDT, PIC, PIT, handlers
│   ├── memory.rs               # OffsetPageTable, FrameAllocator, FrameDeallocator
│   ├── allocator.rs            # LockedHeap, init_heap()
│   ├── simd.rs                 # FPU/SSE enablement via CR0/CR4
│   ├── tensor.rs               # Tensor struct, matmul, transpose, apply
│   └── nn.rs                   # SiLU, RMSNorm, Linear, argmax
├── docs/
│   ├── architecture/           # ADRs (0001–0010)
│   ├── memory/                 # STATE.md + SESSION_*.md logs
├── Cargo.toml
├── CHANGELOG.md
├── rust-toolchain.toml
├── AGENTS.md
└── README.md
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

## Crate Dependencies

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

## License

MIT
