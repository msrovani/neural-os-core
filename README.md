# neural-os-core

**AI Operating System — bare-metal Rust microkernel for neural inference orchestration.**

neural-os-core is an experimental AI Operating System (AIOS) being developed from scratch in Rust. It targets AMD Unified Memory Architecture (APU) with a three-ring hardware abstraction:

| Ring | Hardware | Component |
|---|---|---|
| 0 | NPU | Neural Microkernel — intent routing, context memory |
| 1 | GPU | Tensor execution and heavy lifting |
| 2 | CPU | Wasmtime execution of Daemons/Agents |

## Current Status — Sprint 2

| Category | Status |
|---|---|
| Boot | ✅ QEMU `x86_64` — bootloader + kernel |
| VGA Output | ✅ 80×25 text mode via `map_physical_memory` |
| Serial Logging | ✅ 16550 UART (COM1, port `0x3F8`) |
| Panic Handler | ✅ Dual output: VGA + serial |
| Remaining | Interrupts, page tables, NPU interface |

## Prerequisites

| Tool | Version | Installation |
|---|---|---|
| Rust | nightly (MSVC or GNU) | `rustup toolchain install nightly` |
| `llvm-tools-preview` | — | `rustup component add llvm-tools-preview` |
| `bootimage` | 0.10.x | `cargo install bootimage` |
| QEMU | 7+ | `winget install QEMU` or manual |
| C linker | MSVC or MinGW | VS Build Tools or MSYS2 + MinGW-w64 |

> **Windows without MSVC:** If Visual Studio Build Tools are not installed, install MSYS2 and MinGW-w64:
> ```powershell
> # Install MSYS2 from https://www.msys2.org/
> # Then in MSYS2 terminal:
> pacman -S mingw-w64-x86_64-gcc
> # Add C:\msys64\mingw64\bin to your PATH
> ```

## Quick Start

```powershell
# Clone and enter
git clone https://github.com/msrovani/neural-os-core
cd neural-os-core

# Build and boot in QEMU
cargo run
```

Expected output on the serial console:

```
[SYSTEM] Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.
```

The same message appears in the QEMU VGA window. Close the window to stop.

### Other Commands

```powershell
cargo build          # compile only
cargo bootimage      # create bootable disk image (without running)
cargo check          # type-check without codegen
```

## Architecture

### Boot Sequence

```
cargo run
  └─ cargo build (kernel → x86_64-unknown-none)
     └─ bootimage runner
        ├─ build bootloader (v0.9.34)
        ├─ combine kernel + bootloader → bootimage.bin
        └─ qemu-system-x86_64 -m 2G -serial stdio
           └─ bootloader sets up long mode, page tables
              └─ calls kernel_main(&BootInfo)
                 ├─ vga_buffer::init(physical_memory_offset)
                 ├─ println!("...")       → VGA window
                 └─ serial_println!("...") → host terminal
```

### Ring 0 Observability

| Channel | Backend | Macro | Target |
|---|---|---|---|
| VGA Text | `0xB8000` via `map_physical_memory` | `print!` / `println!` | QEMU window |
| Serial | 16550 UART @ `0x3F8` | `serial_print!` / `serial_println!` | Host terminal |

## Project Structure

```
neural-os-core/
├── .cargo/config.toml          # target, runner, rustflags
├── src/
│   ├── main.rs                 # entry_point!, panic handler, kernel_main
│   ├── vga_buffer.rs           # VGA Writer, Color, print!/println!
│   └── serial.rs               # 16550 UART, serial_print!/serial_println!
├── docs/
│   ├── architecture/
│   │   ├── 0001-initial-architecture-and-toolchain.md
│   │   └── 0002-vga-and-serial-logging.md
│   └── memory/
│       ├── STATE.md            # project state tracker
│       ├── SESSION_001.md      # Sprint 1 detailed log
│       └── SESSION_002.md      # Sprint 2 detailed log
├── Cargo.toml
├── rust-toolchain.toml
├── AGENTS.md                   # rules for AI-assisted IDEs
└── README.md
```

## Documentation Protocol (ADR)

Architectural decisions are recorded in `docs/architecture/`:

| ADR | Title |
|---|---|
| 0001 | Initial Architecture and Toolchain |
| 0002 | VGA and Serial Logging Infrastructure |

## License

MIT
