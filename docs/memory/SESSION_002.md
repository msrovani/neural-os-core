# Session 002 — Sprint 2: Observabilidade Ring 0

**Date:** 2026-06-21
**Goal:** Establish persistent output channels (VGA + serial), safe global Writer, and dual-output panic handler.

---

## Accomplished

### 1. VGA Text Buffer Module (`src/vga_buffer.rs`)
- Color enum (16 VGA colors), `ColorCode` wrapper, `ScreenChar` struct
- `VgaBuffer` mapped at `0xB8000 + physical_memory_offset` (runtime-computed)
- `Writer` with `write_byte`, `write_string`, `new_line` (scrolling), `core::fmt::Write` impl
- Global `WRITER: Mutex<Option<Writer>>` — initialized via `vga_buffer::init()` after boot
- Macros `print!` / `println!` for kernel-wide VGA output

### 2. Serial Module (`src/serial.rs`)
- `uart_16550::SerialPort` at `0x3F8` with `lazy_static! + Mutex`
- Macros `serial_print!` / `serial_println!` for host-terminal logging

### 3. Dual-Output Panic Handler
- `panic!()` now calls `println!("[PANIC] {}", info)` for VGA
- AND `serial_println!("[PANIC] {}", info)` for serial
- Enables crash dump viewing both on-screen and in the host terminal

### 4. Bootloader Integration
- Enabled `map_physical_memory` feature on both build-dep and regular dep
- Added `bootloader` as regular dependency for `BootInfo` type
- Migrated from `extern "C" fn _start()` to `bootloader::entry_point!(kernel_main)`
- `vga_buffer::init()` receives `physical_memory_offset` from `BootInfo`

### 5. New Crate Dependencies

| Crate | Version | Feature | Purpose |
|---|---|---|---|
| `bootloader` (reg) | 0.9.34 | `map_physical_memory` | Kernel-side BootInfo type |
| `spin` | 0.9 | — | `Mutex<T>` for no_std sync |
| `lazy_static` | 1.5 | `spin_no_std` | Lazy init for SerialPort |
| `uart_16550` | 0.2 | — | 16550 UART driver |

---

## Problems Encountered & Resolutions

| # | Problem | Root Cause | Resolution |
|---|---|---|---|
| 1 | `i8` register mismatch in inline asm | `out dx, al` expects `i8`, Rust literals are `i32` | Replaced raw asm with `uart_16550` crate |
| 2 | VGA write page-faults | Bootloader doesn't identity-map `0xB8000` for kernel | Enabled `map_physical_memory` feature; compute VGA addr via `0xB8000 + offset` |
| 3 | `BootInfo` struct field missing | Feature flags must match between build-dep and regular dep | Added `features = ["map_physical_memory"]` to both |
| 4 | `entry_point!` macro needs exact signature | Macro validates `fn(&BootInfo) -> !` at compile time | Changed `kernel_main` signature to match |

---

## Key Architectural Decisions

1. **`physical_memory_offset` at runtime** — VGA buffer address is not hardcoded; computed in `vga_buffer::init()` from boot info. This is mandatory with `map_physical_memory`.
2. **`Mutex<Option<Writer>>` over `lazy_static!` for VGA** — Writer depends on a runtime value (the offset), so it can't be initialized at compile time via `lazy_static!`.
3. **`lazy_static!` for Serial** — SerialPort initialization is safe to run once and doesn't depend on kernel parameters.
4. **`spin::Mutex` over `lock_api`** — Simpler API, no external trait dependencies, sufficient for single-core.
5. **`bootloader` as regular dep** — Required to use `entry_point!` macro and `BootInfo` type. Without it, we'd have to manually parse the boot info struct layout.
