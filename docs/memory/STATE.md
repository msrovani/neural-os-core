# Project State — neural-os-core

## Sprint 1 — Chassi Básico (Complete)

### Current Status
| Category | Status |
|---|---|
| Last QEMU Boot | ✅ Boot OK — serial message displayed |
| Compilation | ✅ `cargo build` — 0 errors |
| Serial Output | ✅ `[SYSTEM] Neural Microkernel Iniciado...` |
| VGA Output | ❌ Not mapped by bootloader (using serial) |
| Toolchain | ✅ nightly-2026-06-21, MinGW-w64, bootimage v0.10.4 |

### Architecture Overview

```
User: cargo run
  └─ cargo compiles kernel for x86_64-unknown-none
     └─ bootimage runner:
        ├─ compiles bootloader v0.9.34
        ├─ combines into bootable disk image
        └─ launches qemu-system-x86_64
           ├─ -m 2G
           └─ -serial stdio
              └─ bootloader → _start(boot_info) → serial message → loop
```

### Files

| File | Purpose |
|---|---|
| `src/main.rs` | Kernel entry point, serial init/write, panic handler |
| `Cargo.toml` | Package + bootloader build-dep + bootimage config |
| `.cargo/config.toml` | Target, runner, relocation-model=static |
| `rust-toolchain.toml` | Nightly pinned, llvm-tools-preview |
| `AGENTS.md` | System rules for AI-assisted IDEs |
| `docs/architecture/0001-*.md` | ADR-0001: Initial Architecture and Toolchain |
| `docs/memory/STATE.md` | This file — project state tracker |
| `docs/memory/SESSION_001.md` | Detailed session log with problems/solutions |

### Environment Dependencies

| Tool | Path | Purpose |
|---|---|---|
| Rust nightly | via rustup | Kernel + bootloader compilation |
| MSYS2 | `C:\msys64` | MinGW-w64 C linker for bootimage |
| MinGW-w64 | `C:\msys64\mingw64\bin` | gcc, linker for native tool compilation |
| bootimage | `~/.cargo/bin` | Creates bootable disk image from kernel |
| QEMU | `C:\Program Files\qemu` | x86_64 emulation for testing |

### Known Issues
1. **No VGA** — bootloader v0.9 does not map `0xB8000` into kernel page tables. Serial output only.
2. **MinGW dependency** — bootimage compiled with GNU toolchain; MSVC path untested.
3. **Nightly-bound** — bootloader v0.9.34 may break with future nightly changes (especially ELF format).

### Next Sprint (Planned)
- [ ] Map VGA buffer via `map_physical_memory` feature
- [ ] Add interrupt handling (IDT + PIC)
- [ ] Introduction to NPU Ring 0 interface specification
- [ ] Replace busy-loop with power-saving idle
