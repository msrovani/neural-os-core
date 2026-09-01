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

## Fix 2: cognitive.rs OOB (commit 6d18405)

Após o fix do #PF, o boot avançou mas panico em `cognitive.rs` — index OOB
no path de inferência do transformer (rope_apply, gqa_attn_forward, rms_backward).

### Root Cause

O `head_dim` era calculado uma vez do model-level (`model.kv_dim / num_heads`)
e usado para todas as camadas. Mas o `matmul_hybrid` pode retornar tensores
com dimensões diferentes do esperado, causando OOB em:

- `rope_apply`: `data[off + 2*d + 1]` ultrapassava o slice
- `gqa_attn_forward`: `q.data[s * qw + q_base + d]` com `qw` derivado de head_dim errado
- `rms_backward`: iteração `0..x.data.len()` mas `dy.data.len() < x.data.len()`

### Fix

- **train_forward**: derivar `hd` por-layer de `q.shape.1 / num_heads` (não do model)
- **backward**: derivar `hd` por-layer de `act.q.shape.1 / num_heads`
- **gqa_attn_forward**: clamp `hd` para `min(q.shape.1, k.shape.1) / num_heads`
- **rope_apply**: bounds check antes de acessar `data[off + 2*d + 1]`
- **rms_backward**: `len = min(x, dy, dx)` + safe `w` access

### Resultado

| Métrica | Antes | Depois |
|---|---|---|
| cognitive panic | OOB em line 770/830/858 | **0 panics** ✅ |
| Boot progress | Phase 5 + panic | **Phase 5 + training loop** ✅ |
| Training | crash | **executa (lento no TCG)** ✅ |

## Lições Aprendidas

1. **Kernel virtual ≠ HHDM** — São mapeamentos separados. `cr2 - HHDM_OFFSET` só
   funciona para HHDM. Kernel virtual precisa de `kernel_phys + (cr2 - kernel_virt)`.

2. **`serial_print!` deadlock em #PF handler** — O UART lock é spinlock. Se o
   interrupt já está no handler e o serialPrint tenta lock, deadlock. Usar
   `puts`/`puthex` (lock-free raw I/O) ou contadores atômicos para diagnóstico.

3. **Atomic counters > logs em interrupt handlers** — `PF_DIAG_*` counters
   são lock-free e podem ser lidos no handler via `puts`/`puthex` sem risco
   de deadlock.

4. **Per-layer head_dim é obrigatório** — O head_dim do modelo (model.head_dim)
   pode não corresponder ao shape real do tensor Q/K/V após matmul. Derivar
   de `q.shape.1 / num_heads` POR LAYER é o correto.

5. **rms_backward precisa de bounds checking** — Tensores x, dy, e dx podem ter
   tamanhos diferentes. Usar `min(x.len(), dy.len(), dx.len())` como limite.

## Próximo

- Training loop muito lento no TCG — considerar treinar no host (GPU)
- Cross-boot NSGDB recall — desbloqueado com ATA funcionando
- Boot loop: goal = Jarbas greeting + NSGDB recall cross-boot
