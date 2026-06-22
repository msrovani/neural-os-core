# Project State — neural-os-core

## Sprint 1 — Chassi Básico (Complete)
## Sprint 2 — Observabilidade Ring 0 (Complete)
## Sprint 3 — Captura de Exceções da CPU (Complete)
## Sprint 4 — Interrupções de Hardware e Page Faults (Pending)

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
| Toolchain | ✅ nightly, bootimage v0.10.4, MinGW-w64 |

### Files

| File | Purpose |
|---|---|
| `src/main.rs` | Entry point (`entry_point!`), panic handler dual output, `int3()` test |
| `src/vga_buffer.rs` | VGA Writer, Color/ScreenChar/VgaBuffer, `print!/println!` |
| `src/serial.rs` | 16550 UART via `uart_16550`, `serial_print!/serial_println!` |
| `src/interrupts.rs` | IDT, TSS, GDT init; Breakpoint + Double Fault handlers |
| `Cargo.toml` | `bootloader` + `spin` + `lazy_static` + `uart_16550` + `x86_64` |
| `.cargo/config.toml` | Target, runner, `relocation-model=static` |
| `docs/architecture/0001-initial-architecture-and-toolchain.md` | ADR-0001 |
| `docs/architecture/0002-vga-and-serial-logging.md` | ADR-0002 |
| `docs/architecture/0003-interrupt-descriptor-table.md` | ADR-0003 |
| `docs/memory/STATE.md` | This file |
| `docs/memory/SESSION_001.md` | Sprint 1 |
| `docs/memory/SESSION_002.md` | Sprint 2 |
| `docs/memory/SESSION_003.md` | Sprint 3 |

### Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `bootloader` (build-dep) | 0.9.34 | Boot image creation, `binary + map_physical_memory` |
| `bootloader` (dep) | 0.9.34 | Kernel-side `BootInfo` type + `entry_point!` macro |
| `spin` | 0.9 | `Mutex<T>` for `no_std` synchronization |
| `lazy_static` | 1.5 | `lazy_static!` with `spin_no_std` feature |
| `uart_16550` | 0.2 | Serial port driver for 16550 UART |
| `x86_64` | 0.14.11 | IDT, GDT, TSS structures, CPU instructions |

### Known Issues

1. **`spin::Mutex` single-core** — deadlock if exception fires while VGA lock is held.
2. **VGA init after boot** — early bootloader panics only visible via bootloader's own VGA.
3. **MinGW linker required** — `bootimage` needs C linker; MSVC alternative requires VS Build Tools.
4. **Double Fault stack static 20KB** — overflow corrupts adjacent memory.

### Next Steps (Sprint 4)

- [ ] PIC remap (8259A) — reencaminhar IRQs de hardware para vetores ≥ 32
- [ ] PIT timer handler — interrupção periódica para preempção
- [ ] Page Fault handler — capturar e tratar `#PF` (pré-requisito para memória semântica)
- [ ] Replace `spin::Mutex` with `lock_api` for IRQ safety
- [ ] Outline NPU Ring 0 interface specification
