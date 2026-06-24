# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/)
with [Conventional Commits](https://www.conventionalcommits.org/).

## [0.17.0] — 2026-06-24

### Added (Sprint 22 — Block 5: Skills + Trust Cache)

- **`trust.rs`** — `TrustCache` with:
  - `is_trusted(token, skill_name, now_ticks)` — checks cache and denylist
  - `trust_allow(token, skill_name, now_ticks)` — permanent trust until explicit deny
  - `trust_deny(token, skill_name)` — revoke trust + add to denylist
  - `check_or_cache(token, skill_name, now_ticks, ttl_ticks)` — auto-cache on valid token (360 ticks ≈ 20s TTL)
- **`HardwareInfoSkill`** — new skill exposing `SystemArchitecture` (ring mode, heap size, etc.) and MHI tier info. Invoked via `/hw`, `/hardware`, or `/info` commands.
- **`SystemStatusSkill` upgraded** — now reads MHI tiers + `GLOBAL_ALLOCATOR` occupancy to report per-tier free/total RAM in MB.
- **`SkillRegistry` additions** (`registry.rs`):
  - `has_skill(name) -> bool` — check if skill exists
  - `validate_token(name, token) -> bool` — check token authorization without executing
  - `execute_skill_unchecked(name, payload)` — skip token validation (caller must validate)
- **Trust-aware Hermes commands**:
  - `/trust allow <token> <skill>` — permanently authorize a token for a skill
  - `/trust deny <token> <skill>` — revoke authorization
  - `/hw` — display hardware info and system architecture
  - All skill executions (`/status`, `/echo`, MLP-triggered) now use `execute_skill_with_trust()` helper
- **Help text updated** — lists all available commands
- Version bump: v0.16.0 → v0.17.0

## [0.16.0] — 2026-06-23

### Fixed (Sprint 21 — IOAPIC mask bug)

- **apic.rs `redirect_irq()`** — removed `(1u32 << 16)` from redirection entry low dword. Bit 16 is the MASK bit in IOAPIC redirection entries. The original code set it, masking all interrupts (timer, keyboard, etc.). Without timer interrupts, the executor's `hlt()` never woke up, stalling the system after the first poll cycle. Debug output confirmed: `IOAPIC redirection[0]: low=0x00010000` (bit 16 = masked). After fix: timer IRQ0 (vector 32) delivers at ~18.2 Hz, executor cycles normally.

### Added (Sprint 21 — Block 4: MLP + MHI + Auto-detecção)

- `mhi.rs` — Memory Hierarchy Index with:
  - `AllocTier` enum: Dram, Vram, Nvme, Hdd
  - `MemoryTier` struct: kind, capacity_bytes, bandwidth_mbs, latency_ns, name
  - `MemoryHierarchy::new()` — auto-creates Dram tier from bitmap frame allocator
  - `alloc_by_tier(Dram)` — allocates contiguous physical frames, returns PhysAddr
  - Other tiers return `None` (drivers not yet implemented)
- `inventory.rs` — Hardware Inventory & System Architecture with:
  - `HardwareInventory::collect(pci_devices, acpi_info)` — CPU count, RAM, PCI device detection (VirtIO-net/GPU, NVMe, XHCI)
  - `SystemArchitecture::infer(inv)` — rule-based heuristics: GPU detect → ring1, RAM size → heap, CPU count → power mode
  - Both pure data structures for future MLP weight training (item #51)
- `memory.rs` — `BitmapFrameAllocator::usable_memory_bytes()` public accessor
- **Adaptive boot flow** — `main.rs` now runs: PCI scan → HardwareInventory::collect() → SystemArchitecture::infer() → log to VGA+serial → MHI init → NeuralExecutor. Example output: `[ARCH] ring0=0 ring1=0 heap=2048MB` / `[MHI] 1 tier(s), X MB usable.`
- **Workspace crate versions** — `neural-kernel` bumped to v0.16.0

## [0.15.0] — 2026-06-23

### Added (Sprint 20 — Block 3: Hermes Chat)

- `hermes.rs` — Hermes Chat console module with:
  - `IntentMlp` — real MLP intent classifier: bag-of-words (16-word vocab) → Linear(16→8) → SiLU → Linear(8→3) → argmax (3 intents: chat, status, echo)
  - Hand-crafted weights for keyword-based classification (status/memory/ram/cpu/system → status intent; echo/reverse → echo intent; hello/hi/help → chat intent)
  - `parse_command()` — multi-word command parser: `/status`, `/echo <text>`, `/help`, `/stats`, `/mem`
  - `Command` enum: Status, Echo(String), Help, Chat(String)
- **scancode_to_ascii()** — expanded with digits 0-9 (0x02-0x0B) and punctuation (`- = [ ] ; ' ` \ , . /`) for full command-line input
- **intent_router_daemon** — upgraded from mock string-contains to:
  - `parse_command()` dispatches `/status` and `/echo` to SkillRegistry
  - Unrecognized text → `INTENT_MLP.classify()` → routes to SystemStatusSkill (intent 1), EchoSkill (intent 2), or default chat response (intent 0)
  - Publishes responses on `HERMES_RESPONSE` EventBus topic
- **hermes_console_daemon** — subscribes `HERMES_RESPONSE`, prints `[Hermes] <response>` to both VGA and serial
- Both new daemons spawn in the NeuralExecutor (6 tasks total)

### Changed

- `main.rs` — added `mod hermes;`, `INTENT_MLP` lazy_static, expanded scancode table, upgraded intent_router + new console daemon

## [0.14.1] — 2026-06-23

### Fixed (Sprint 19 — SMP Multi-Core Boot)

- **Root cause isolated:** bootloader identity-maps pages 0-7 only (PD[0] = 0x4023 → PT base = 0x4000). PT[64] for VA 0x40000 was `0x0000000000000000` → AP #PF on first instruction at 0x400A4 → triple fault
- **Identity-map PTE fix:** single `write_volatile` at `phys_offset + 0x4200` writes PTE `0x40000 | 0x003` (Present|Write) — AP can fetch from VA 0x40000 after enabling paging
- **CPU_COUNT race condition:** `spin::Mutex` protects `fetch_add` because QEMU TCG lacks cross-vCPU atomicity; all APs previously read same counter value
- **50ms busy-wait** after second SIPI for accurate AP count (all 3 APs finish trampoline within <20ms)
- **Slab Allocator memory corrupt fix:** `SLAB_CHUNK_SIZE` = bucket_size (not aligned to 8); free list pointer stored before chunk, retrieved via `ptr.read::<*mut u8>()`
- **asm! memcpy:** Replaced `core::intrinsics::copy_nonoverlapping` with `asm!("rep movsb")` to avoid `native_memcpy` dependency in `no_std`

### Changed

- `smp/mod.rs` — identity-map PTE written directly via raw pointer (not OffsetPageTable mapper); `AP_BOOT_LOCK: spin::Mutex<()>` around CPU_COUNT increment; 50ms busy-wait after SIPI
- `smp/trampoline.rs` — replaced `copy_nonoverlapping` with `asm!` block for zero-dependency memcpy
- `slab.rs` — `SLAB_CHUNK_SIZE` = bucket_size (not `align_up(bucket_size, 8)`); corrected `put()` free list logic

### Result

- `-smp 2`: ✅ AP 1 boots — `[SMP] AP 1 entrou em modo 64-bit Rust!` → `APs acordados: 1`
- `-smp 4`: ✅ AP 1, 2, 3 boot — `APs acordados: 3`
- `qemu_trace.log`: zero `check_exception` lines — no #UD, #PF, #GP
- Sprint 19 (Block 2) now fully operational

## [0.14.0] — 2026-06-23

### Added (Sprint 19 — Block 2: SMP + Slab + Heap 4 MB)

- `allocate_below_1mb()` — BitmapFrameAllocator aloca frame < 1 MiB para trampoline real-mode (`src/memory.rs`)
- `PHYS_MEM_OFFSET` — AtomicU64 global com offset de memória física para acesso de qualquer módulo (`src/memory.rs`)
- Slab Allocator — 8 buckets (32, 64, 128, 256, 512, 1024, 2048, 4096), free list ligada, `Mutex<SlabAllocator>` com métricas atômicas (`src/slab.rs`)
- Heap expandido de 100 KB para 4 MB — primeiros 512 KB para Slab, restante 3.5 MB para LockedHeap (`src/allocator.rs`)
- PerCpu struct (repr(C), 64 bytes) com self_ptr, cpu_id, lapic_id, bsp_flag, ring. GS.base via wrmsr(0xC0000101) (`src/smp/percpu.rs`)
- `this_cpu()` — lê gs:[0] para obter ponteiro PerCpu. `cpu_id()` lê gs:[8]
- Trampoline assembly (global_asm!) — 16-bit → 32-bit protected → PAE → EFER.LME → paging → 64-bit long mode → Rust entry. Header patcheable de 48 bytes com campos jmp32/jmp64/cr3/stack/percpu/entry_fn (`src/smp/trampoline.rs`)
- INIT-SIPI-SIPI via LAPIC ICR — `send_init_ipi()`, `send_sipi(vector)` com entrega via shorthand "all excluding self" (`src/apic.rs`)
- `wait_for_ipi_delivery()` — spin até ICR delivery status clear. `lapic_id()` — LAPIC ID register (offset 0x20)
- SMP orchestrator — `init_smp()` aloca trampoline, identity-maps, patcha, dispara INIT-SIPI-SIPI (`src/smp/mod.rs`)
- `ap_entry()` — entry point chamado pelos APs em modo 64-bit

### Changed

- `main.rs` — `mapper` scoped no boot flow para evitar aliasing com o mapper do SMP init
- Boot flow: adicionados `mod smp`, `mod slab`, `crate::smp::init_smp()` antes do NeuralExecutor

## [0.13.0] — 2026-06-23

### Added (Sprint 18 — Block 1)

- PCI scan — CF8/CFC config space access, 256 bus × 32 device enumeration, vendor/device/class/BARs (`crates/neural-kernel/src/pci.rs`)
- ACPI parser — RSDP discovery (EBDA + BIOS area), RSDT/XSDT walking, MADT LAPIC/IOAPIC/x2APIC parsing (`crates/neural-kernel/src/acpi.rs`)
- APIC init — LAPIC SVR + TPR + timer masked, IOAPIC IRQ0→vec32 + IRQ1→vec33, PIC disable (`crates/neural-kernel/src/apic.rs`)
- Dual EOI — `USING_APIC: AtomicBool` + `send_eoi()` com fallback APIC/PIC para handlers
- Boot flow: `init_pci()` → `init_acpi()` → `init_apic(info)` (fallback PIC se sem ACPI)

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
