# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/)
with [Conventional Commits](https://www.conventionalcommits.org/).

## [0.13.0] — 2026-06-23

### Added

- Hardware Neural Routing — IRQ1 keyboard → EventBus → Agent pipeline (`crates/neural-kernel/src/main.rs`)
  - Top-Half: `keyboard_interrupt_handler` (IDT[33]) lê porta 0x60 → `LAST_SCANCODE` (AtomicU8, Release) → EOI raw
  - Bottom-Half: `hw_bridge_daemon` (async task) poll AtomicU8 → publica `RAW_HW_IRQ1` no EventBus
  - `input_daemon` (async task) subscreve RAW_HW_IRQ1 → buffer String → `scancode_to_ascii()` → ENTER publica `USER_INTENT`
  - `intent_router_daemon` (Cortex) subscreve USER_INTENT → mock inference → `SkillRegistry::execute_skill`
- Closed Intent Pipeline (Sprint 16)
  - `SystemStatusSkill` — lê `global_hardware_context()` via TicketLock, loga `"Memoria RAM: {:.2}%"`
  - 5 tasks spawnadas (3 persistentes), 1000+ PIT ticks estáveis, zero Double Faults
- `TicketLock` FIFO crate (`crates/ticket-lock/src/lib.rs`)
  - `TicketLock<T>` — `AtomicUsize ticket/serving`, `UnsafeCell<T>`, spin loop justo
  - Garantia FIFO, `Send` + `Sync` para T: Send
  - `TicketLockGuard` com `Deref`/`DerefMut` e incremento `serving` no Drop
- EventBus refatorado para TicketLock
  - `EventBus.subscribers`: `spin::Mutex` → `TicketLock<BTreeMap<...>>`
  - `Receiver.queue`: `Arc<TicketLock<VecDeque<Event>>>`
  - ID counter: `Arc<AtomicU64>` (was raw u64)
- `GLOBAL_ALLOCATOR: TicketLock<Option<BitmapFrameAllocator>>` — frame allocator encapsulado
- `init_global_allocator()` — migra frame allocator para TicketLock pós-boot
- `global_hardware_context()` — acesso thread-safe via TicketLock
- NeuralExecutor simplificado: campo `frame_allocator` removido, usa `global_hardware_context()`
- `sync` module (`crates/neural-kernel/src/sync/`) — re-exporta `ticket_lock::*`
- ADR-0013: Neural OS Executive Summary (SotA 2026)

### Changed

- EventBus modernizado: `spin::Mutex` substituído por `TicketLock` (Sprint 17)
- BitmapFrameAllocator agora protegido por `TicketLock` (não mais por `spin::Mutex`)
- NeuralExecutor não gerencia mais frame_allocator — acesso global via TicketLock
- `interrupts.rs` — expandido com handlers: GPF, Stack Segment, Segment Not Present, Invalid TSS, Alignment Check

## [0.12.0] — 2026-06-22

### Added

- Async Neural Executor (`crates/neural-kernel/src/task/`)
  - `pub struct AgentTask { id: u64, future: Pin<Box<dyn Future>> }` — with `AtomicU64` ID generation
  - `pub struct NeuralExecutor { task_queue: VecDeque<AgentTask> }` — cooperative polling loop
  - `DummyWaker` via `RawWakerVTable` — no-op waker for `no_std` environments
  - `pub fn run(&mut self)` — replaces `loop { hlt() }`; polls tasks, logs hardware context every 100 iterations
- Event Bus IPC (`crates/event-bus/`)
  - `CapabilityToken`, `Event`, `EventBus` with publish/subscribe via `BTreeMap + spin::Mutex`
  - `Receiver::try_receive()` for non-blocking polling
  - `yield_now().await` for explicit cooperation
- Skill Registry & MCP Layer (`crates/skill-registry/`)
  - `trait Skill: Send + Sync` with `manifest()` + `execute()`
  - `SkillRegistry` with Zero-Trust CapabilityToken validation
  - `EchoSkill` — reverses payload bytes
  - `SystemStatusSkill` — logs RAM occupancy via hardware context
- `async fn system_daemon()` — test agent that spawns, executes, and completes
- `async fn hardware_monitor_daemon()` — publishes SYSTEM_READY with Token(1)
- Boot sequence: `NeuralExecutor::run()` instead of raw `hlt` loop

## [0.11.0] — 2026-06-22

### Added

- `BitmapFrameAllocator` — 128 KB `.bss` bitmap covering 4 GB physical memory
- `init(&mut self, memory_map)` — varre UEFI MemoryMap, marca `Usable` como livre, o resto ocupado
- `FrameAllocator<Size4KiB>` + `FrameDeallocator<Size4KiB>` — alloc/dealloc reais com busca linear
- `allocate_contiguous(count)` — aloca N frames contíguos para Huge Pages (2 MiB / 1 GiB)
- `hardware_context_tensor() -> [f32; 2]` — `[taxa_ocupacao, 0.0]` via contador de alocações
- Stress test: 1000 alloc/dealloc estáveis, 0% leak, RAM Tensor confirmado em QEMU
- `PackedTernaryTensor` struct (`crates/neural-kernel/src/tensor.rs`) — 2-bit per weight, 4 weights per byte
- `pack_weights()` + `get_weight()` — pack/extract 2-bit ternary values
- `matmul_hybrid()` on `PackedTernaryTensor` — reads weights bit-by-bit from packed storage
- `quantize_to_packed(tensor, threshold)` — f32→ternary calibration
- ADR-0012: 2-bit Packing and Ternary Quantization

### Changed

- `nn::BitLinear` — `weights` field changed from `TernaryTensor` to `PackedTernaryTensor`
- `main.rs` — BitNet test now uses quantization + packed inference flow
- Monorepo workspace: `src/` movido para `crates/neural-kernel/src/`

## [0.10.0] — 2026-06-21

### Added

- `TernaryTensor` struct (`src/tensor.rs`) — weight storage as `Vec<i8>` with values in {-1, 0, 1}
- `TernaryTensor::from_row_major()` — constructor with shape validation
- `TernaryTensor::matmul_hybrid(input: &Tensor) -> Option<Tensor>` — ADD/SUB-only kernel
  - Weight `+1` → `accumulator += input[t]`
  - Weight `-1` → `accumulator -= input[t]`
  - Weight `0` → skip (no multiplication)
- `nn::BitLinear` struct (`src/nn.rs`) — ternary dense layer
  - `forward()` = `matmul_hybrid()` + optional bias
- BitNet hybrid inference test in boot flow
  - Input `[1.5, -0.5, 2.0]` × TernaryTensor(3×2) → `[-0.5, -2.0]`
  - Zero multiplication operators in the inner loop
- ADR-0011: BitLinear and Hybrid Ternary MatMul

## [0.8.0] — 2026-06-21

### Added

- `pic8259 = "0.10"` dependency — 8259A PIC driver with `ChainedPics`
- PIC remap (PIC1 → vector 32, PIC2 → vector 40) — `interrupts::init_pics()`
- PIT Timer watchdog handler (IRQ 0, vector 32) — atomic `TIMER_TICKS` counter + EOI
- Page Fault handler (vector 14) — reads `CR2`, logs fault address, halts via `hlt`
- `interrupts::enable_interrupts()` — `sti` instruction sets IF=1
- `memory.rs:FrameDeallocator` trait — `deallocate_frame()` for future frame recycling
- `EmptyFrameDeallocator` — no-op stub until bitmap allocator
- ADR-0009: PIC Watchdog and Page Fault Safety

### Changed

- `src/interrupts.rs` — IDT extended with `page_fault` and `idt[32]` (timer)
- `src/main.rs` — `init_pics()` + `enable_interrupts()` + watchdog `hlt` loop
- `src/memory.rs` — `FrameDeallocator` trait + `EmptyFrameDeallocator` added

## [0.7.0] — 2026-06-21

### Added

- `Tensor::transposed()` — row-major to column-major transposition (W^T support)
- `nn::Linear` struct with `weights: Tensor` and `bias: Option<Tensor>`
  - `forward(&self, input) -> Tensor` implements Y = X·W^T + B
- `nn::argmax(tensor) -> usize` — returns index of highest logit
- Intent Router MLP in boot flow
  - Input embedding + Linear(3→2) + SiLU + argmax = kernel decision
  - Tested: `[1.0, -0.5, 0.3]` → action 0 (Acionar Daemon Ring 2)
- ADR-0007: Intent Router MLP — Primeiro Córtex Primitivo

## [0.6.0] — 2026-06-21

### Added

- `libm = "0.2"` dependency for `no_std` math functions (`expf`, `sqrtf`)
- Neural primitives module (`src/nn.rs`)
  - `silu(x)` activation via `libm::expf` — tested: `[-1, 0, 1] → [-0.269, 0, 0.731]`
  - `rms_norm(tensor, weight, eps)` via `libm::sqrtf` — tested: RMSNorm of SiLU output
- `Tensor::add_scalar`, `Tensor::mul_scalar`, `Tensor::apply<F>` (generic closure)
- `nn::silu` used as closure arg to `Tensor::apply` in boot test
- ADR-0006: Neural Primitives and libm

## [0.5.0] — 2026-06-21

### Added

- SIMD enablement module (`src/simd.rs`)
  - `enable_simd()` — CR0: clear `EMULATE_COPROCESSOR`, set `MONITOR_COPROCESSOR` + `NUMERIC_ERROR`
  - CR4: set `OSFXSR` + `OSXMMEXCPT_ENABLE`
  - `f32`/`f64` operations now execute natively without `#NM` exceptions
- Tensor Engine module (`src/tensor.rs`)
  - `Tensor` struct with `shape: (usize, usize)` and `data: Vec<f32>`
  - `from_row_major()`, `matmul()` — dot product multiplication
  - Tested: 1×3 × 3×1 → 1×1 = `[32.0]`
- `simd::enable_simd()` call in boot flow after heap init
- ADR-0005: SIMD and FPU Enablement

### Changed

- `main.rs`: added `mod simd; mod tensor;` + tensor matmul test

## [0.4.0] — 2026-06-21

### Added

- Memory module (`src/memory.rs`)
  - `OffsetPageTable` — cria mapper via `Cr3::read()` + `physical_memory_offset`
  - `BootInfoFrameAllocator` — implementa `FrameAllocator<Size4KiB>` iterando mapa UEFI/BIOS
  - `init_memory(offset)` — retorna `OffsetPageTable<'static>` pronto
- Heap allocator module (`src/allocator.rs`)
  - `LockedHeap` como `#[global_allocator]` via `linked_list_allocator` v0.9.1
  - `init_heap(mapper, frame_allocator)` — mapeia 25 páginas (100 KB) em `0x4444_4444_0000`
- `extern crate alloc` ativado — `Box::new(41)` e `Vec::push([10, 20, 30])` testados em QEMU
- `linked_list_allocator = "0.9"` dependency
- ADR-0004: Memory Paging and Heap Allocation
- SESSION_004.md: Sprint 4 detailed log

## [0.3.0] — 2026-06-21

### Added

- IDT (Interrupt Descriptor Table) module (`src/interrupts.rs`)
  - Breakpoint handler (`#BP`, vector 3) — logs VGA + serial, returns
  - Double Fault handler (`#DF`, vector 8) — logs VGA + serial, panics
  - TSS with IST entry 0 (20KB dedicated stack) for Double Fault stack switching
  - GDT with kernel code segment + TSS descriptor
  - `init_idt()` — loads GDT, sets CS, loads TSS, loads IDT
- `x86_64` crate v0.14.11 dependency (IDT, GDT, TSS, CPU instructions)
- `#![feature(abi_x86_interrupt)]` for `extern "x86-interrupt"` calling convention
- Forced `int3()` breakpoint test in boot flow
- ADR-0003: Interrupt Descriptor Table
- SESSION_003.md: Sprint 3 detailed log
- QEMU path added to `PATH` documentation for Windows

### Fixed

- Handler signature adapted to `x86_64` v0.14.13 API (`InterruptStackFrame` by value)
- `static_mut_refs` warning — replaced `&STACK` with `core::ptr::addr_of!(STACK)`
- Deprecated `set_cs` — replaced with `CS::set_reg()` via `Segment` trait
- Macro scoping — explicit `use crate::{println, serial_println}` in interrupts module

## [0.2.0] — 2026-06-21

### Added

- VGA text mode output via `map_physical_memory` feature (`vga_buffer.rs`)
  - `Writer` with scrolling, 16-color support, `core::fmt::Write` impl
  - Macros `print!` / `println!` for kernel-wide use
  - Buffer mapped at runtime using `physical_memory_offset` from `BootInfo`
- Serial port logging via `uart_16550` crate (`serial.rs`)
  - 16550 UART initialization at port `0x3F8`
  - `lazy_static!` + `spin::Mutex` for safe global access
  - Macros `serial_print!` / `serial_println!`
- Dual-output panic handler in `main.rs`
  - `panic!()` writes to both VGA and serial simultaneously
- New crate dependencies: `spin` v0.9, `lazy_static` v1.5, `uart_16550` v0.2
- `bootloader` as regular dependency (kernel-side `BootInfo` type with `map_physical_memory`)
- ADR-0002: VGA and Serial Logging Infrastructure

### Changed

- Entry point migrated from raw `extern "C" fn _start()` to `bootloader::entry_point!(kernel_main)`
- VGA base address computed as `0xB8000 + physical_memory_offset` (runtime, not hardcoded)
- `STATE.md` updated with Sprint 2 progress

## [0.1.0] — 2026-06-21

### Added

- Initial bare-metal Rust kernel scaffold
  - `#![no_std]` + `#![no_main]` environment
  - Minimal panic handler (infinite loop)
  - Serial init and output via raw port I/O
- Bootloader integration (`bootloader` v0.9.34 build-dep)
  - `bootimage runner` for automated QEMU launch
  - `relocation-model=static` to produce `ET_EXEC` ELF (fixes bootloader compatibility)
- Toolchain configuration
  - `rust-toolchain.toml` pinned to nightly
  - `.cargo/config.toml` with target and runner
- Documentation protocol
  - ADR-0001: Initial Architecture and Toolchain
  - State tracker (`STATE.md`)
  - Session log (`SESSION_001.md`)
- MSYS2 + MinGW-w64 setup for Windows toolchain without MSVC
- `AGENTS.md` — system rules for AI-assisted IDEs
