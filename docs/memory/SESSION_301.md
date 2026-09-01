# SESSION_301 — #PF Fix: Kernel Virtual Range Detection

**Data:** 2026-09-01 | **Sprint:** v1.9.99-s301 | **Status:** ✅ RESOLVIDO

---

## Problema

#PF em `CR2=0xffffffffa0ceb000` (5 MB past KERNEL_END) — 11 page faults
durante boot QEMU TCG, bloqueando FAT32 mount e qualquer progresso além
da Phase 5.

## Root Cause

A detecção do range "kernel virtual" em `try_fault_in_heap` usava a fórmula
ERRADA:

```rust
// ERRADO: kernel virtual ≠ HHDM
let phys = cr2.wrapping_sub(hhdm_offset);  // 0xffffffffa0ceb000 - 0xffff800000000000
let in_kernel_virt = cr2 >= 0xffffffff80000000 && phys < 8GB;
// phys = 0x7FFFA0CEB000 (~140 PB) → condition FALSE → returns false
```

**Kernel virtual addresses (0xffffffff80000000+) são um mapeamento SEPARADO
do HHDM (0xffff800000000000+)**. A subtração `cr2 - HHDM_OFFSET` só
funciona para endereços HHDM, não para kernel virtual.

## Fix

A fórmula correta é `kernel_phys + (cr2 - kernel_virt)`:

```rust
let kphys = KERNEL_PHYS_BASE.load(Ordering::Relaxed);
let kvirt = KERNEL_VIRT_BASE.load(Ordering::Relaxed);
let phys_k = kphys + (cr2 - kvirt);
let in_kernel_virt = cr2 >= kvirt && phys_k < 8GB;
// phys_k = 0x977d2000 + (0xffffffffa0ceb000 - 0xffffffff80000000)
//        = 0x977d2000 + 0x20ceb000 = 0xB85BD000 (~2.9 GB) → TRUE
```

## Resultado

| Métrica | Antes | Depois |
|---|---|---|
| #PF count | 11 | **0** ✅ |
| Boot progress | Phase 5 + crash | **Phase 5 + FAT32 + SMP** ✅ |
| DIAG no_rng | 1 | **0** ✅ |
| DIAG ok | 0 | **2+** ✅ |

## Commits

- `b533364` — Initial diagnostics + kernel_phys/virt storage + PF_DBG instrumentation
- `67b4613` — **THE FIX**: correct kernel virtual range detection using kernel_phys

## Lições Aprendidas

1. **Kernel virtual ≠ HHDM** — São mapeamentos separados. `cr2 - HHDM_OFFSET` só
   funciona para HHDM. Kernel virtual precisa de `kernel_phys + (cr2 - kernel_virt)`.

2. **`serial_print!` deadlock em #PF handler** — O UART lock é spinlock. Se o
   interrupt já está no handler e o serialPrint tenta lock, deadlock. Usar
   `puts`/`puthex` (lock-free raw I/O) ou contadores atômicos para diagnóstico.

3. **Atomic counters > logs em interrupt handlers** — `PF_DIAG_*` counters
   são lock-free e podem ser lidos no handler via `puts`/`puthex` sem risco
   de deadlock.

## Próximo

- Panic em `cognitive.rs:770` (index out of bounds) — bug separado
- Cross-boot NSGDB recall — desbloqueado com ATA funcionando
- Boot loop: goal = Jarbas greeting + NSGDB recall cross-boot
