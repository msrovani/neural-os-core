# Session 003 — Sprint 3: Captura de Exceções da CPU

**Date:** 2026-06-21
**Goal:** Proteger o Ring 0 contra falhas de CPU implementando a Interrupt Descriptor Table (IDT), habilitando captura de Breakpoint e Double Fault com stack switching.

---

## Accomplished

### 1. Nova Dependência: `x86_64` v0.14.11
- Crate que fornece tipos seguros para estruturas nativas do processador x86-64
- IDT, GDT, TSS, segment registers, instructions

### 2. Módulo de Interrupções (`src/interrupts.rs`)
- **IDT** via `lazy_static!` com duas entradas configuradas:

| Exceção | Vetor | Handler | Comportamento |
|---|---|---|---|
| Breakpoint (`#BP`) | 3 | `breakpoint_handler` | Loga VGA + serial, retorna (continua) |
| Double Fault (`#DF`) | 8 | `double_fault_handler` | Loga VGA + serial, `panic!` (aborta) |

- **TSS** com IST entry 0 contendo pilha dedicada de 20KB para Double Fault
- **GDT** customizada com code segment (ring 0) + TSS descriptor
- `init_idt()` — sequência: `GDT.load() → set_cs → load_tss → IDT.load()`

### 3. Integração no Boot (`src/main.rs`)
- `#![feature(abi_x86_interrupt)]` — ABI experimental do nightly para `extern "x86-interrupt"`
- `interrupts::init_idt()` chamado logo após `vga_buffer::init()`
- `int3()` forçado para testar captura de Breakpoint

### 4. Documentação
- ADR-0003: Interrupt Descriptor Table

---

## Boot Output (QEMU - serial)

```
[SYSTEM] Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.
[TEST] Forcando Breakpoint (int3)...
[EXCEPTION] Breakpoint Detectado
```

Breakpoint capturado → handler executou → `iretq` retornou → loop infinito.

---

## Problems Encountered & Resolutions

| # | Problem | Root Cause | Resolution |
|---|---|---|---|
| 1 | `extern "x86-interrupt"` not recognized | ABI is experimental on nightly | Added `#![feature(abi_x86_interrupt)]` |
| 2 | `println!` not in scope in `interrupts.rs` | Macros are `#[macro_export]` but module doesn't import them | Added `use crate::{println, serial_println}` |
| 3 | Handler signature mismatch (`&mut` vs value) | x86_64 v0.14.13 passes `InterruptStackFrame` by value | Changed to `fn(InterruptStackFrame)` |
| 4 | `set_cs` deprecated | x86_64 v0.14.13 renamed to `CS::set_reg()` | Replaced with `x86_64::instructions::segmentation::CS::set_reg()` (needs `Segment` trait in scope) |
| 5 | `static_mut_refs` warning | Rust 2024 warns on `&STACK` for mutable static | Replaced with `core::ptr::addr_of!(STACK)` |
| 6 | QEMU not found in PATH | `C:\Program Files\qemu` not in PATH | Temporary: `$env:Path += ";C:\Program Files\qemu"` |

---

## Key Architectural Decisions

1. **`lazy_static!` for IDT, GDT, TSS** — All three need to persist in memory (CPU reads them at runtime). `lazy_static!` ensures a stable address.
2. **IST (Interrupt Stack Table) for Double Fault** — Sem stack switch, DF corrupts the current stack → Triple Fault. A 20KB static buffer via TSS.IST[0] é o mínimo necessário.
3. **GDT recriada (não extendida)** — Bootloader provê GDT mínima. Criamos uma nova com code + TSS descritores e recarregamos CS via `set_reg()`.
4. **`spin::Mutex` aceito como risco** — Sabemos que deadlock se exceção acontecer enquanto VGA lock estiver held. Será resolvido no Sprint 4 com `lock_api` + IRQ safety.
