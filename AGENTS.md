# Role and Purpose
You are a Senior Systems and AI Engineer specializing in bare-metal Rust development, microkernel architecture, and neural inference orchestration. You are developing "neural-os-core", an AI Operating System (AIOS) from scratch.

# Core Architecture & Constraints
1. **Bare-Metal Rust:** This project operates entirely in `no_std` and `no_main` environments. You CANNOT use the Rust standard library (`std`).
2. **Hardware Rings Abstraction:**
   - Ring 0 (NPU): Neural Microkernel (Intent routing, context memory).
   - Ring 1 (GPU): Tensor execution and heavy lifting.
   - Ring 2 (CPU): Wasmtime execution of Daemons/Agents.
3. **No Legacy OS Concepts:** We are not building Linux. We do not use POSIX standards. Memory is mapped as a Semantic File System. 
4. **Emulation First:** All code must be testable via QEMU (`qemu-system-x86_64`) before deploying to physical AMD Unified Memory Architecture (APU).

# Operational Rules & Guardrails
- **Zero Hallucination Policy:** If you do not know how to implement a low-level hardware interaction, state it explicitly. Do not invent Rust crates that do not exist or are incompatible with `no_std`.
- **Strict Testing:** Before proposing a final code block, you must internally simulate the compilation sequence. If it requires `std` or an OS allocator, rewrite it.
- **Boot sequence:** Rely on the `bootloader` crate for UEFI/BIOS handoff.

# Memory & Documentation (ADR Protocol)
- Do not make architectural decisions implicitly. 
- For every new module (e.g., memory paging, inference engine port), you must first create or update an Architecture Decision Record (ADR) in the `/docs/architecture/` folder.
- Maintain a `/docs/memory/STATE.md` file summarizing the current state of the kernel, last successful QEMU boot status, and pending tasks. Update this file automatically at the end of complex tasks.

# Code Style & Versioning
- Adhere strictly to idiomatic Rust. Use `clippy` configurations.
- Commit messages must follow Conventional Commits (e.g., `feat: implement memory allocator`, `fix: resolve page fault in qemu`).
- Comment complex unsafe blocks extensively, explaining *why* the `unsafe` keyword is necessary for that specific hardware interaction.

# Project Summary — neural-os-core v0.9.0

## Goal
Build a bare-metal Rust microkernel (neural-os-core) for AI inference orchestration across NPU/GPU/CPU rings.

## Constraints
- `#![no_std]` bare-metal, nightly Rust, x86_64-unknown-none target
- `bootloader` v0.9.34 with `map_physical_memory` feature
- All output to both VGA (QEMU window) and serial (host terminal)
- ADR + session log documentation protocol
- Windows toolchain with MinGW-w64 linker
- Every sprint: `cargo check --release` (0 errors, 0 warnings) + QEMU boot

## 9 Sprints Complete

### Sprint 1 (v0.1.0) — Toolchain & Boot
Toolchain nightly + x86_64-unknown-none, bootloader v0.9.34, `cargo run` boots in QEMU, serial output at port 0x3F8, `relocation-model=static` fix, MinGW-w64 setup, ADR-0001.

### Sprint 2 (v0.2.0) — VGA & Serial
VGA text buffer — 16-color Writer, scrolling, `print!/println!`, buffer at runtime via `physical_memory_offset`. Serial — `uart_16550` driver, `lazy_static!` + `spin::Mutex`, `serial_print!/serial_println!`. Dual-output panic handler. `bootloader::entry_point!(kernel_main)`. Deps: `spin`, `lazy_static`, `uart_16550`. ADR-0002.

### Sprint 3 (v0.3.0) — IDT & Exceptions
`lazy_static!` IDT with Breakpoint handler (logs + returns) and Double Fault handler (logs + panics). TSS with IST entry 0 (20KB stack) for DF. Custom GDT with kernel code + TSS. `#![feature(abi_x86_interrupt)]`. Forced `int3()` test. Dep: `x86_64 = "0.14.11"`. ADR-0003.

### Sprint 4 (v0.4.0) — Memory & Heap
`OffsetPageTable` via `Cr3::read()` + `physical_memory_offset`. `BootInfoFrameAllocator` — filters `Usable` regions from UEFI/BIOS `MemoryMap`. `linked_list_allocator::LockedHeap` as `#[global_allocator]`, `init_heap()` maps 25 pages (100 KB) at `0x4444_4444_0000`. `extern crate alloc` — `Box::new(41)` and `Vec::push`. Dep: `linked_list_allocator = "0.9"`. ADR-0004.

### Sprint 5 (v0.5.0) — SIMD & Tensor
`enable_simd()` via CR0/CR4: clear `EMULATE_COPROC`, set `MONITOR_COPROC` + `NUMERIC_ERROR` (CR0); set `OSFXSR` + `OSXMMEXCPT_ENABLE` (CR4). `Tensor { shape: (usize, usize), data: Vec<f32> }` with `from_row_major()` + `matmul()`. Tested: 1×3 × 3×1 = [32.0]. No new deps. ADR-0005.

### Sprint 6 (v0.6.0) — Neural Primitives
`libm = "0.2"` — `expf`, `sqrtf` in `no_std`. `nn::silu(x)` via `x/(1+exp(-x))`. `nn::rms_norm()` via `sqrt(mean_sq + eps)`. `Tensor::add_scalar`, `mul_scalar`, `apply<F>`. Tested: [-1, 0, 1] → SiLU → [-0.269, 0, 0.731]. ADR-0006.

### Sprint 7 (v0.7.0) — Intent Router MLP
`Tensor::transposed()` (row→col major). `nn::Linear { weights, bias }` with `forward()` = X·W^T + B. `nn::argmax()` — index of max logit. Tested: [1.0, -0.5, 0.3] → Linear(3→2) → SiLU → argmax = 0 (Daemon). ADR-0007.

### Sprint 8 (v0.8.0) — PIC, Watchdog, Page Fault
`pic8259 = "0.10"` — `ChainedPics` remap PIC1→32, PIC2→40. PIT timer handler (vetor 32) — atomic counter + EOI. Page Fault handler (vetor 14) — CR2 → log → hlt loop. `FrameDeallocator` trait + `EmptyFrameDeallocator` stub. `sti` at boot end. ADR-0009.

### Sprint 9 (v0.9.0) — Ternary Inference (Phase 3 start)
`TernaryTensor { shape, data: Vec<i8> }` — values in {-1, 0, 1}. `matmul_hybrid()` — ADD/SUB-only kernel (no `*` operator). `nn::BitLinear` — ternary forward pass. Tested: [1.5, -0.5, 2.0] → ternary → [-0.5, -2.0]. ADR-0011, ADR-0010 (Roadmap).

## Key Architectural Decisions
- **VGA address** computed at runtime (`0xB8000 + physical_memory_offset`)
- **`Mutex<Option<Writer>>`** for VGA (not `lazy_static!`) — depends on runtime BootInfo
- **`lazy_static!` for Serial** — SerialPort init is safe at compile time
- **GDT recreated (not extended)** — bootloader GDT is minimal
- **IST for Double Fault** — 20KB static buffer prevents Triple Fault
- **`OffsetPageTable` via Cr3** — reads CR3 for L4 table addr, no recursive mapping
- **Heap at `0x4444_4444_0000`** — high address, safe from kernel/bootloader range
- **Ternary ADD/SUB kernel** — zero FPU multiplications in weight matmul

## Boot Sequence
```
cargo run → bootloader → kernel_main
  ├─ vga_buffer::init(offset)
  ├─ interrupts::init_idt()       (GDT + TSS + IDT)
  ├─ memory::init_memory(offset)  (OffsetPageTable)
  ├─ BootInfoFrameAllocator::init
  ├─ allocator::init_heap()       (LockedHeap 100 KB)
  ├─ simd::enable_simd()          (CR0/CR4)
  ├─ int3() → Breakpoint handler
  ├─ Box/Vec/Tensor/SiLU/RMSNorm tests
  ├─ Intent Router: Linear → SiLU → argmax
  ├─ BitNet: BitLinear ternary matmul
  ├─ init_pics()                  (PIC remap)
  ├─ enable_interrupts()          (sti)
  └─ loop { hlt(); watchdog TIMER_TICKS }
```

## Active Dependencies
| Crate | Version |
|---|---|
| bootloader | 0.9.34 (map_physical_memory) |
| spin | 0.9 |
| lazy_static | 1.4 (spin_no_std) |
| uart_16550 | 0.2 |
| x86_64 | 0.14.11 |
| linked_list_allocator | 0.9 |
| libm | 0.2 |
| pic8259 | 0.10 |

## Next Sprint (Sprint 10)
Bitmap FrameDeallocator, Slab allocator, calibration pass (f32→ternary via Δ), packed 2-bit storage.

## Roadmap (ADR-0010)
- Phase 3: Ternary Inference (Sprints 9–11, Q3 2026)
- Phase 4: Zero-Copy SFS (Sprints 12–15, Q4 2026)
- Phase 5: WASM Skills (Sprints 16–18, Q1 2027)
- Phase 6: AIOS Syscalls (Sprints 19–21, Q2 2027)
