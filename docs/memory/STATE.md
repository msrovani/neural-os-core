# Project State — neural-os-core

## Sprint 1 — Chassi Básico (Complete)
## Sprint 2 — Observabilidade Ring 0 (Complete)

### Current Status

| Category | Status |
|---|---|
| Last QEMU Boot | ✅ Boot OK — VGA + serial messages displayed |
| Compilation | ✅ `cargo check` — 0 errors |
| VGA Output | ✅ Mapped via `map_physical_memory`, Writer with `print!/println!` |
| Serial Output | ✅ `uart_16550` driver, `serial_print!/serial_println!` via port `0x3F8` |
| Panic Handler | ✅ Logs to VGA and serial simultaneously |
| Toolchain | ✅ nightly, bootimage v0.10.4, MinGW-w64 |

### Files

| File | Purpose |
|---|---|
| `src/main.rs` | Entry point (`entry_point!` macro), panic handler dual output |
| `src/vga_buffer.rs` | VGA Writer, Color/ScreenChar/VgaBuffer structs, `print!/println!` |
| `src/serial.rs` | 16550 UART via `uart_16550`, `serial_print!/serial_println!` |
| `Cargo.toml` | `bootloader` + `spin` + `lazy_static` + `uart_16550` |
| `.cargo/config.toml` | Target, runner, `relocation-model=static` |
| `docs/architecture/0001-initial-architecture-and-toolchain.md` | ADR-0001 |
| `docs/architecture/0002-vga-and-serial-logging.md` | ADR-0002 |
| `docs/memory/STATE.md` | This file |
| `docs/memory/SESSION_001.md` | Sprint 1 session log |
| `docs/memory/SESSION_002.md` | Sprint 2 session log |

### Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `bootloader` (build-dep) | 0.9.34 | Boot image creation, `binary + map_physical_memory` |
| `bootloader` (dep) | 0.9.34 | Kernel-side `BootInfo` type + `entry_point!` macro |
| `spin` | 0.9 | `Mutex<T>` for `no_std` synchronization |
| `lazy_static` | 1.5 | `lazy_static!` with `spin_no_std` feature |
| `uart_16550` | 0.2 | Serial port driver for 16550 UART |

### Known Issues

1. **`spin::Mutex` single-core** — will deadlock in interrupt handler if the interrupted code holds the same lock.
2. **VGA init after boot** — early bootloader panics (before `kernel_main`) only visible via bootloader's own VGA.
3. **MinGW linker required** — `bootimage` compilation needs C linker; MSVC alternative requires Visual Studio Build Tools.

### Next Steps (Sprint 3)

- [ ] Add interrupt handling (IDT + PIC) with proper lock safety
- [ ] Implement recursive page table access for manual page mapping
- [ ] Introduce a framebuffer abstraction layer (beyond VGA text mode)
- [ ] Outline NPU Ring 0 interface specification
- [ ] Replace `spin::Mutex` with `lock_api` for IRQ safety
