# Project State — neural-os-core

## Sprint 1 — Chassi Básico (Complete)
## Sprint 2 — Observabilidade Ring 0 (Complete)
## Sprint 3 — Captura de Exceções da CPU (Complete)
## Sprint 4 — Alocação Dinâmica e Heap (Complete)

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
| Toolchain | ✅ nightly, bootimage v0.10.4, MinGW-w64 |

### Files

| File | Purpose |
|---|---|
| `src/main.rs` | Entry point, panic handler, boot flow with `Box`/`Vec` test |
| `src/vga_buffer.rs` | VGA Writer, `print!/println!` |
| `src/serial.rs` | 16550 UART, `serial_print!/serial_println!` |
| `src/interrupts.rs` | IDT, TSS, GDT, Breakpoint + Double Fault handlers |
| `src/memory.rs` | `OffsetPageTable`, `BootInfoFrameAllocator`, `init_memory()` |
| `src/allocator.rs` | `LockedHeap` global allocator, `init_heap()` |
| `Cargo.toml` | `bootloader` + `spin` + `lazy_static` + `uart_16550` + `x86_64` + `linked_list_allocator` |
| `.cargo/config.toml` | Target, runner, `relocation-model=static` |
| `docs/architecture/0001-*.md` to `0004-*.md` | 4 ADRs |
| `docs/memory/STATE.md` | This file |
| `docs/memory/SESSION_001.md` to `SESSION_003.md` | Sprint logs |

### Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `bootloader` | 0.9.34 | Boot image, `BootInfo`, `map_physical_memory` |
| `spin` | 0.9 | `Mutex<T>` for `no_std` sync |
| `lazy_static` | 1.5 | `lazy_static!` with `spin_no_std` |
| `uart_16550` | 0.2 | 16550 UART driver |
| `x86_64` | 0.14.11 | IDT, GDT, TSS, page tables, frame allocator trait |
| `linked_list_allocator` | 0.9.1 | `LockedHeap` global allocator |

### Known Issues

1. **`spin::Mutex` single-core** — deadlock if exception fires while VGA/heap lock is held.
2. **Frame allocator monotonic** — `allocate_frame()` nunca reusa frames; precisa de slab allocator.
3. **Heap 100 KB fixo** — tamanho arbitrário, precisa de budget tuning.
4. **MinGW linker required** — `bootimage` needs C linker.

### Next Steps (Sprint 5)

- [ ] PIC remap (8259A) — reencaminhar IRQs de hardware para vetores ≥ 32
- [ ] PIT timer handler — interrupção periódica para preempção
- [ ] Page Fault handler — capturar e tratar `#PF` (pré-requisito para memória semântica)
- [ ] Implement `FrameDeallocator` para reuso de frames
- [ ] Slab allocator para reduzir fragmentação do heap
