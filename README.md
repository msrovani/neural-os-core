# neural-os-core

**AI Operating System — bare-metal Rust microkernel for neural inference orchestration.**

neural-os-core is an experimental AI Operating System (AIOS) being developed from scratch in Rust. It targets AMD Unified Memory Architecture (APU) with a three-ring hardware abstraction:

| Ring | Hardware | Component |
|---|---|---|
| 0 | NPU | Neural Microkernel — intent routing, context memory |
| 1 | GPU | Tensor execution and heavy lifting |
| 2 | CPU | Wasmtime execution of Daemons/Agents |

## Current Status — Sprint 4

| Category | Status |
|---|---|
| Boot | ✅ QEMU `x86_64` — bootloader + kernel |
| VGA Output | ✅ 80×25 text mode via `map_physical_memory` |
| Serial Logging | ✅ 16550 UART (COM1, port `0x3F8`) |
| Panic Handler | ✅ Dual output: VGA + serial |
| IDT (Breakpoint, Double Fault) | ✅ Captura e log com IST stack switching |
| GDT + TSS | ✅ Custom GDT with TSS descriptor |
| Page Tables (OffsetPageTable) | ✅ Via `Cr3::read()` + `physical_memory_offset` |
| Frame Allocator | ✅ `BootInfoFrameAllocator` — lê mapa UEFI/BIOS |
| Heap (alloc crate) | ✅ `LockedHeap` 100 KB, `Box`/`Vec` testados |
| Next | PIC remap, PIT timer, Page Fault handler |

## Prerequisites

| Tool | Version | Installation |
|---|---|---|
| Rust | nightly | `rustup toolchain install nightly` |
| `llvm-tools-preview` | — | `rustup component add llvm-tools-preview` |
| `bootimage` | 0.10.x | `cargo install bootimage` |
| QEMU | 7+ | `winget install QEMU` or manual (add to PATH) |
| C linker | MSVC or MinGW | VS Build Tools or MSYS2 + MinGW-w64 |

> **Windows without MSVC:** Install MSYS2 then `pacman -S mingw-w64-x86_64-gcc`, add `C:\msys64\mingw64\bin` to PATH.
> **QEMU path:** If `cargo run` fails with "program not found", add QEMU dir to PATH:
> ```powershell
> $env:Path += ";C:\Program Files\qemu"
> ```

## Quick Start

```powershell
git clone https://github.com/msrovani/neural-os-core
cd neural-os-core
cargo run
```

Expected serial output:

```
[SYSTEM] Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.
[TEST] Forcando Breakpoint (int3)...
[EXCEPTION] Breakpoint Detectado
[TEST] Box::new(41) = 41
[TEST] Vec = [10, 20, 30]
```

Same appears in QEMU VGA window. Close window to stop.

### Commands

```powershell
cargo run          # build + boot in QEMU
cargo build        # compile only
cargo bootimage    # create bootable image (without running)
cargo check        # type-check without codegen
```

## Architecture

### Boot Sequence

```
cargo run
  └─ cargo build → x86_64-unknown-none
     └─ bootimage runner
        ├─ build bootloader v0.9.34
        ├─ combine → bootimage.bin
        └─ qemu-system-x86_64 -m 2G -serial stdio
           └─ bootloader → long mode, page tables
              └─ kernel_main(&BootInfo)
                 ├─ vga_buffer::init(offset)
                 ├─ interrupts::init_idt()   ← NEW
                 │   ├─ GDT.load + set_cs
                 │   ├─ load_tss
                 │   └─ IDT.load (lidt)
                 ├─ println! / serial_println!
                  ├─ int3() → Breakpoint handler → log → ret
                  ├─ memory::init_memory(offset)   ← NEW
                  ├─ BootInfoFrameAllocator::init
                  ├─ allocator::init_heap(mapper, alloc)
                  ├─ Box::new(41) → Vec::push → log
                  └─ loop
```

### Exception Handling

| Exception | Vector | Behavior |
|---|---|---|
| Breakpoint (`#BP`) | 3 | Logs, returns (continues execution) |
| Double Fault (`#DF`) | 8 | Logs, panics (aborts system) |

Double Fault uses IST (Interrupt Stack Table) entry 0 with a dedicated 20KB stack to prevent Triple Fault.

### Heap Allocation

| Component | Address | Size | Allocator |
|---|---|---|---|
| Heap | `0x4444_4444_0000` | 100 KB | `linked_list_allocator::LockedHeap` (free-list) |

Frames são alocados via `BootInfoFrameAllocator` que lê regiões `Usable` do mapa de memória física fornecido pela UEFI/BIOS. Páginas são mapeadas com `PRESENT | WRITABLE` via `OffsetPageTable`.

## Project Structure

```
neural-os-core/
├── .cargo/config.toml          # target, runner, rustflags
├── src/
│   ├── main.rs                 # entry_point!, panic handler, kernel_main, int3, alloc test
│   ├── vga_buffer.rs           # VGA Writer, print!/println!
│   ├── serial.rs               # 16550 UART, serial_print!/serial_println!
│   ├── interrupts.rs           # IDT, TSS, GDT, handlers (Breakpoint, Double Fault)
│   ├── memory.rs               # OffsetPageTable, BootInfoFrameAllocator
│   └── allocator.rs            # LockedHeap global allocator, init_heap()
├── docs/
│   ├── architecture/
│   │   ├── 0001-initial-architecture-and-toolchain.md
│   │   ├── 0002-vga-and-serial-logging.md
│   │   ├── 0003-interrupt-descriptor-table.md
│   │   └── 0004-memory-paging-and-heap.md
│   └── memory/
│       ├── STATE.md
│       ├── SESSION_001.md
│       ├── SESSION_002.md
│       ├── SESSION_003.md
│       └── SESSION_004.md
├── Cargo.toml
├── CHANGELOG.md
├── rust-toolchain.toml
├── AGENTS.md
└── README.md
```

## Architectural Decisions (ADRs)

| ADR | Title |
|---|---|
| 0001 | Initial Architecture and Toolchain |
| 0002 | VGA and Serial Logging Infrastructure |
| 0003 | Interrupt Descriptor Table |
| 0004 | Memory Paging and Heap Allocation |

## Crate Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `bootloader` | 0.9.34 | Boot image + BootInfo |
| `spin` | 0.9 | `Mutex<T>` for `no_std` |
| `lazy_static` | 1.5 | Lazy initialization |
| `uart_16550` | 0.2 | 16550 UART driver |
| `x86_64` | 0.14.11 | IDT, GDT, TSS, page tables, CPU instructions |
| `linked_list_allocator` | 0.9.1 | `LockedHeap` global allocator |

## License

MIT
