# Project State — neural-os-core

## Sprint 1 — Chassi Básico (Complete)
## Sprint 2 — Observabilidade Ring 0 (Complete)
## Sprint 3 — Captura de Exceções da CPU (Complete)
## Sprint 4 — Alocação Dinâmica e Heap (Complete)
## Sprint 5 — Ativação de SIMD e Fundação Tensorial (Complete)
## Sprint 6 — Primitivas Neurais e dependência libm (Complete)
## Sprint 7 — Intent Router MLP e Forward Pass (Complete)
## Sprint 8 — Hardware Interrupts & Memory Safety (Complete)
## Sprint 9 — Ternary Inference Engine (Complete)

### Current Status

| Category | Status |
|---|---|
| Last QEMU Boot | ✅ Boot OK — VGA + serial + Breakpoint handler |
| Compilation | ✅ `cargo check` — 0 errors, 0 warnings |
| VGA Output | ✅ Mapped via `map_physical_memory`, Writer with `print!/println!` |
| Serial Output | ✅ `uart_16550` driver, `serial_print!/serial_println!` via port `0x3F8` |
| Panic Handler | ✅ Logs to VGA and serial simultaneously |
| IDT | ✅ Breakpoint + Double Fault handlers, IST stack switch for DF |
| GDT + TSS | ✅ Custom GDT with TSS descriptor for Double Fault stack switching |
| Page Tables | ✅ `OffsetPageTable` via `Cr3` + `physical_memory_offset` |
| Frame Allocator | ✅ `BootInfoFrameAllocator` — lê mapa UEFI/BIOS, retorna frames Usable |
| Heap | ✅ `LockedHeap` global allocator (linked_list_allocator v0.9.1) |
| `alloc` crate | ✅ `Box`, `Vec` testados no boot flow |
| FPU/SSE (SIMD) | ✅ CR0: clear EMULATE_COPROC, set MONITOR + NUMERIC_ERROR |
| | ✅ CR4: set OSFXSR + OSXMMEXCPT_ENABLE |
| Tensor Engine | ✅ `Tensor` struct with f32 matmul (1×3 × 3×1 = 1×1) |
| Tensor API | ✅ `add_scalar`, `mul_scalar`, `apply<F>` |
| SiLU Activation | ✅ `nn::silu(x)` via `libm::expf` — `[-0.269, 0, 0.731]` |
| RMSNorm | ✅ `nn::rms_norm(tensor, weight, eps)` via `libm::sqrtf` |
| Tensor transpose | ✅ `transposed()` — row-major → column-major, usado em Linear |
| Linear Layer | ✅ `Linear { weights, bias }` com `forward()` = X·W^T + B |
| argmax | ✅ `nn::argmax(tensor)` — índice do maior logit |
| Intent Router | ✅ MLP 1×3 → 2, SiLU → argmax → decisão (0=Daemon, 1=Halt) |
| PIC 8259A Remap | ✅ PIC1 → vetor 32, PIC2 → vetor 40 via `ChainedPics` |
| PIT Watchdog | ✅ Timer a ~18.2 Hz, contador atômico, EOI |
| Page Fault Handler | ✅ CR2 → log → `hlt` loop (barreira Ring 2) |
| Frame Deallocator | ✅ `FrameDeallocator` trait + `EmptyFrameDeallocator` stub |
| TernaryTensor | ✅ `i8` storage, shape (in, out), `from_row_major()` |
| Hybrid MatMul | ✅ ADD/SUB-only — zero multiplicações, `match w {1 => add, -1 => sub, _ => skip}` |
| BitLinear | ✅ Camada densa ternária com `forward()` usando `matmul_hybrid()` |
| BitNet Test | ✅ Input `[1.5, -0.5, 2.0]` → saída `[-0.5, -2.0]` (verified) |
| `libm` crate | ✅ v0.2.16 — `expf`, `sqrtf` em `no_std` |
| Toolchain | ✅ nightly, bootimage v0.10.4, MinGW-w64 |

### Files

| File | Purpose |
|---|---|
| `src/main.rs` | Entry point, panic handler, boot flow with `Box`/`Vec` test |
| `src/vga_buffer.rs` | VGA Writer, `print!/println!` |
| `src/serial.rs` | 16550 UART, `serial_print!/serial_println!` |
| `src/interrupts.rs` | IDT, TSS, GDT, Breakpoint + Double Fault + Page Fault + PIT Timer + PIC remap |
| `src/memory.rs` | `OffsetPageTable`, `BootInfoFrameAllocator`, `FrameDeallocator` trait, `init_memory()` |
| `src/allocator.rs` | `LockedHeap` global allocator, `init_heap()` |
| `src/simd.rs` | `enable_simd()` — CR0/CR4 FPU/SSE enablement |
| `src/tensor.rs` | `Tensor` + `TernaryTensor` — `matmul`, `matmul_hybrid`, transpose, apply |
| `src/nn.rs` | `silu()`, `rms_norm()`, `Linear`, `BitLinear`, `argmax` — MLP + ternary layer |
| `Cargo.toml` | `bootloader` + `spin` + `lazy_static` + `uart_16550` + `x86_64` + `linked_list_allocator` + `libm` + `pic8259` |
| `.cargo/config.toml` | Target, runner, `relocation-model=static` |
| `docs/architecture/0001-*.md` to `0011-*.md` | 11 ADRs |
| `docs/memory/STATE.md` | This file |
| `docs/memory/SESSION_001.md` to `SESSION_009.md` | Sprint logs |

### Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `bootloader` | 0.9.34 | Boot image, `BootInfo`, `map_physical_memory` |
| `spin` | 0.9 | `Mutex<T>` for `no_std` sync |
| `lazy_static` | 1.5 | `lazy_static!` with `spin_no_std` |
| `uart_16550` | 0.2 | 16550 UART driver |
| `x86_64` | 0.14.11 | IDT, GDT, TSS, page tables, frame allocator trait |
| `linked_list_allocator` | 0.9.1 | `LockedHeap` global allocator |
| `libm` | 0.2.16 | `expf`, `sqrtf` — funções matemáticas em `no_std` |
| `pic8259` | 0.10.4 | Driver do controlador 8259A — remap IRQ, EOI |

### Known Issues

1. **`spin::Mutex` single-core** — deadlock if exception fires while VGA/heap lock is held.
2. **Frame allocator monotonic** — `allocate_frame()` nunca reusa frames; precisa de slab allocator.
3. **Heap 100 KB fixo** — tamanho arbitrário, precisa de budget tuning.
4. **MinGW linker required** — `bootimage` needs C linker.

### Next Steps (Sprint 10 — Phase 3 cont.)

- [x] TernaryTensor struct — `i8` storage, `from_row_major()`
- [x] `matmul_hybrid()` — ADD/SUB-only kernel (zero FPU multiplications)
- [x] BitLinear layer — `forward()` using `matmul_hybrid()` + bias
- [x] BitNet test — input `[1.5, -0.5, 2.0]` → ADD/SUB ternary → `[-0.5, -2.0]`
- [x] ADR-0011: BitLinear and Hybrid Ternary MatMul
- [ ] Bitmap/Free-list FrameDeallocator — reuso real de frames físicos
- [ ] Slab allocator — reduzir fragmentação do heap
- [ ] Calibration pass — `f32` → ternary thresholding via `Δ = α · E[|w|]`
- [ ] `TernaryTensor::packed()` — 2-bit packing (4 weights per byte) for weight storage

---

## Future Roadmap

| Phase | Title | Sprints | Target |
|---|---|---|---|
| **3** | Ternary Inference (BitNet b1.58) | 9–11 | Q3 2026 |
| **4** | Zero-Copy Semantic File System | 12–15 | Q4 2026 |
| **5** | Skills-as-Modules (WASM Component Model) | 16–18 | Q1 2027 |
| **6** | Hardware-Aware AIOS Syscalls (Zero-Trust) | 19–21 | Q2 2027 |

### Phase 3 — Ternary Inference (BitNet b1.58)

Eliminated `f32` matmul from weights. Quantized to {-1, 0, +1} using `i8` (future: 2-bit packing). Dot-product replaced by conditional ADD/SUB. 4× memory compression, zero FPU multiplications during inference.

**Completed:**
- `src/tensor.rs::TernaryTensor` — `i8` storage, `from_row_major()`, `matmul_hybrid()`
- ADD/SUB-only kernel: `match w { 1 => add, -1 => sub, _ => skip }`
- `src/nn.rs::BitLinear` — ternary forward pass with optional bias
- Test: input `[1.5, -0.5, 2.0]` → ternary matmul → `[-0.5, -2.0]` (no `*` ops)
- ADR-0011: BitLinear and Hybrid Ternary MatMul

**Remaining:**
- Calibration pass: `f32` → ternary via threshold `Δ`
- `TernaryTensor::packed()` — 2-bit packing (4 weights/byte) for storage efficiency
- Ternary-aware `Silu` (skip SiLU, output is already logits)

### Phase 4 — Zero-Copy Semantic File System (SFS)

Map NVMe storage directly into kernel VAS via page tables + DMA. Use `zerocopy` crate for safe transmutation. Eliminate all buffer copies between persistent storage and Ring 0 context memory.

**Key deliverables:**
- `src/nvme.rs`: minimal NVMe driver (submission/completion queues)
- SFS virtual address range (`0x5000_0000_0000 – 0x6000_0000_0000`)
- `zerocopy` integration for Tensor ↔ `&[u8]` transmutation

### Phase 5 — Skills-as-Modules (WASM Component Model)

Ephemeral skill execution via `wasmi`. No installed applications — only on-demand WASM instances that are dropped after execution, freeing all memory without GC.

**Key deliverables:**
- `src/wasm.rs`: wasmi embedder with custom tensor host functions
- Linear memory pool: pre-allocated 64-page slabs per skill
- Capability-based import validation

### Phase 6 — Hardware-Aware AIOS Syscalls (Zero-Trust)

Every privilege-escalating call from Ring 2 is intercepted and evaluated by the Neural Cortex (LLM) before hardware execution. Default-deny policy; skills request capabilities explicitly.

**Key deliverables:**
- `src/syscall.rs`: syscall dispatch table
- Capability token per skill instance
- Cortex evaluation bridge (Linear → argmax for allow/deny)

---

*See ADR-0010 for complete architectural details.*
