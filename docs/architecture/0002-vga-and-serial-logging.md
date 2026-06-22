# ADR-0002: VGA and Serial Logging Infrastructure

## Status
Accepted

## Context
Ring 0 (NPU Microkernel) requires observability mechanisms for debugging, boot logging, and runtime monitoring. The kernel operates in `no_std` with no OS services — we cannot use `std::sync::Mutex` or heap-allocated I/O streams.

## Decision

### Dual Output Strategy
We implement two independent output channels:

| Channel | Target | Use Case |
|---------|--------|----------|
| **VGA Text Buffer** | `0xB8000` (phys) → `0xB8000 + physical_memory_offset` (virt) | On-screen boot messages, kernel console |
| **Serial Port (COM1)** | Port `0x3F8` (16550 UART) | QEMU `-serial stdio` — logs to host terminal |

### Physical Memory Mapping for VGA
The bootloader's `map_physical_memory` feature maps all physical memory at a virtual offset. The VGA buffer at physical `0xB8000` becomes accessible at virtual `0xB8000 + boot_info.physical_memory_offset`. This virtual address is computed at runtime in `vga_buffer::init()`.

### Synchronization with `spin::Mutex`
In a bare-metal kernel with no interrupt preemption (single-core, interrupts disabled during boot):
- **`std::sync::Mutex`** — unavailable (requires `std` and OS threading primitives).
- **`spin::Mutex`** — pure spinlock, works in `no_std`. Busy-waits until the lock is released. Acceptable since the kernel is currently single-core and we have no interrupt-driven preemption.

### Lazy Initialization with `lazy_static`
- **VGA Writer**: Stored as `Mutex<Option<Writer>>` because the VGA buffer address depends on `physical_memory_offset` (runtime value from BootInfo). Initialized via `vga_buffer::init()` after boot info is available.
- **Serial Port**: Stored as `lazy_static! { Mutex<SerialPort> }` because the UART init sequence (baud rate, line control, FIFO) must run exactly once.

### Macros
- `print!` / `println!` → VGA buffer, for on-screen output.
- `serial_print!` / `serial_println!` → COM1 serial, for background logging.
- Both are available even from the panic handler, enabling crash dumps to both channels.

## Consequences
1. **VGA output is no longer hardcoded to `0xB8000`** — the address is computed via `physical_memory_offset`, which future-proofs against layout changes.
2. **Panic handler now logs** — any `panic!()` prints the error to both VGA and serial, critical for debugging in QEMU without a debugger.
3. **`spin::Mutex` is single-core only** — if we later add multi-core (SMP), we must upgrade to a proper spinlock with cooperation guarantees.
4. **Two new crate dependencies** — `spin`, `lazy_static`, `uart_16550`, and `bootloader` (as regular dependency for `BootInfo` type).
5. **No VGA in first boot stages** — the bootloader's panic handler uses VGA directly; our kernel's VGA init runs after `vga_buffer::init()` in `kernel_main`.

## Prerequisites
- `bootloader` v0.9 with `map_physical_memory` feature (both build-dep and regular dep).
- QEMU must be run with `-serial stdio` (configured in `[package.metadata.bootimage] run-args`).
