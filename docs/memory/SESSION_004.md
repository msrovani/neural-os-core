# Session 004 — Sprint 4: Alocação Dinâmica e Heap

**Date:** 2026-06-21
**Goal:** Habilitar a crate `alloc` do Rust construindo paginação e alocador global para suportar alocação dinâmica de memória (instanciação de tensores e estruturas de modelo de IA no futuro).

---

## Accomplished

### 1. Page Table Module (`src/memory.rs`)
- `OffsetPageTable` — criado via `Cr3::read()` + `physical_memory_offset`
  - `Cr3` retorna o frame físico da Level 4 Page Table
  - Converte para virtual usando o offset: `VirtAddr::new(offset) + frame.start_address()`
  - Cria `&mut PageTable` a partir do endereço virtual
  - `OffsetPageTable::new(table, offset)` — implementa `Mapper<Size4KiB>` + `Translate`
- `BootInfoFrameAllocator` — implementa `FrameAllocator<Size4KiB>`
  - Lê `BootInfo.memory_map` (fornecido pela UEFI/BIOS)
  - Filtra regiões `MemoryRegionType::Usable`
  - Converte `FrameRange.start_addr()..end_addr()` em iterador de `PhysFrame`

### 2. Heap Allocator (`src/allocator.rs`)
- `ALLOCATOR: LockedHeap` com `#[global_allocator]`
  - Usa `linked_list_allocator` v0.9.1 (free-list allocator)
  - 100 KB iniciais em `0x_4444_4444_0000`
- `init_heap(mapper, frame_allocator)`:
  - Calcula page range (25 páginas de 4KB)
  - Para cada página: aloca frame → `map_to` com `PRESENT | WRITABLE` → flush TLB
  - Chama `ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE)`

### 3. Integração no Boot Flow
- `extern crate alloc;` no topo de `main.rs`
- Boot sequence: VGA → IDT → `init_memory()` → `BootInfoFrameAllocator::init()` → `init_heap()`
- Testes:
  - `Box::new(41)` → `*boxed_val = 41` (VGA + serial)
  - `Vec::push([10, 20, 30])` → `[10, 20, 30]` (VGA + serial)

### 4. Nova Dependência
- `linked_list_allocator = "0.9"` (resolve para v0.9.1, compatível com spin v0.9)

### 5. Documentação
- ADR-0004: Memory Paging and Heap Allocation

---

## Boot Output (QEMU - serial)

```
[SYSTEM] Neural Microkernel Iniciado. Aguardando integracao NPU/Ring 0.
[TEST] Forcando Breakpoint (int3)...
[EXCEPTION] Breakpoint Detectado
[TEST] Box::new(41) = 41
[TEST] Vec = [10, 20, 30]
```

---

## Problems Encountered & Resolutions

| # | Problem | Root Cause | Resolution |
|---|---|---|---|
| 1 | `BootInfo` memory map API desconhecida | Tipos `FrameRange`, `MemoryRegionType` são específicos do bootloader v0.9.34 | Lido código-fonte da crate para mapear `MemoryRegion.range.start_frame_number` etc. |
| 2 | `init_memory()` return type mismatch | Retornava `&'static mut OffsetPageTable` mas `OffsetPageTable::new` retorna owned value | Mudado para `-> OffsetPageTable<'static>` |
| 3 | `Mapper` trait bound | `&mut mapper` tem tipo `&mut OffsetPageTable`, que implementa `Mapper<Size4KiB>` | Resolvido após correção do return type (erro cascata) |
| 4 | `Step` trait needed? | `Page::range_inclusive` depende de `Step` trait feature | `PageRangeInclusive` tem `Iterator` impl próprio, não precisa de `step_trait` |

---

## Key Architectural Decisions

1. **`OffsetPageTable` via `Cr3` + `physical_memory_offset`** — A bootloader mapeia memória física inteira. Lemos CR3 para achar a P4 table, computamos seu endereço virtual e criamos o mapper. Sem necessidade de `recursive_page_table`.
2. **`BootInfoFrameAllocator` sequencial** — Avança índice a cada `allocate_frame()`. Simples e suficiente para boot. Futuramente slab allocator reaproveitará frames.
3. **100 KB heap em `0x4444_4444_0000`** — Endereço alto, fora do range do kernel e do bootloader. Fácil de estender.
4. **`linked_list_allocator` v0.9** — Free-list simples, compatível com `spin` v0.9, sem dependências adicionais.
