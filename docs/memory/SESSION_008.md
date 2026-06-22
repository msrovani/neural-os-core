# SESSION_008 — Hardware Interrupts & Memory Safety

**Date:** 2026-06-21  
**Objective:** Isolar heranças do x86_64 (PIC 8259A), gerenciar limpeza de memória (FrameDeallocator), e configurar o Watchdog do sistema antes de avançar para Ring 2.

## Changes

### New
- `pic8259 = "0.10"` em `Cargo.toml` — driver do 8259A
- `interrupts.rs:init_pics()` — remapeia PIC1 → vetor 32, PIC2 → vetor 40
- `interrupts.rs:timer_handler()` — handler do PIT (IRQ 0 → vetor 32)
  - Incrementa `TIMER_TICKS` (AtomicUsize)
  - Envia EOI ao PIC
- `interrupts.rs:page_fault_handler()` — handler do `#PF` (vetor 14)
  - Lê CR2 → imprime endereço + "Acesso negado" → hlt loop
- `interrupts.rs:enable_interrupts()` — `sti` (IF=1)
- `memory.rs:FrameDeallocator` trait — `deallocate_frame()` stub
- `memory.rs:EmptyFrameDeallocator` — no-op até implementação do bitmap

### Modified
- `src/interrupts.rs` — IDT estendida com `page_fault` + `idt[32]`
- `src/main.rs` — `init_pics()` + `enable_interrupts()` + watchdog hlt loop

## Verification

QEMU output (serial):

```
[PIC] 8259A remapeado: PIC1 offset 32, PIC2 offset 40.
[CPU] Interrupcoes de hardware habilitadas (IF=1).
[WATCHDOG] Ticks do temporizador: 100
[WATCHDOG] Ticks do temporizador: 200
[WATCHDOG] Ticks do temporizador: 300
```

Watchdog: PIT a ~18.2 Hz, print a cada 100 ticks (~5.5s) ✅

## Validation Criteria
- ✅ `cargo check --release` — 0 errors, 0 warnings
- ✅ QEMU boot — all 8 subsystems operational
- ✅ PIC remap: IRQs 0-15 movidos para vetores 32-47 (sem conflito com exceções)
- ✅ PIT timer handler ativo com contador atômico
- ✅ Page Fault handler com diagnóstico CR2
- ✅ ADR-0009 documentado

## Next Sprint (Sprint 9)
- Bitmap/Free-list FrameDeallocator
- Slab allocator for heap
- Preparação para Ring 2 (WASM)
