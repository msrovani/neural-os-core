# ADR-0004: Memory Paging and Heap Allocation

**Status:** Accepted
**Date:** 2026-06-21
**Driver:** Necessidade de alocação dinâmica de memória para instanciar tensores e estruturas do modelo de IA no Ring 0.

## Context

O microkernel precisa suportar alocação dinâmica de memória (heap) para:
1. Instanciar tensores e pesos de modelos de IA
2. Construir estruturas de dados de tamanho variável
3. Gerenciar a memória de contexto para o sistema de arquivos semântico

A crate `alloc` do Rust fornece `Box`, `Vec`, `Arc`, etc., mas depende de:
- Um `#[global_allocator]` implementado (provê `alloc`/`dealloc`)
- Páginas virtuais mapeadas em frames físicos (para o heap residir)

## Decision

### Camada 1: Page Tables (OffsetPageTable)

A bootloader mapeia toda a memória física no espaço virtual a partir de `physical_memory_offset` (feature `map_physical_memory`). Isso permite usar `OffsetPageTable` para manipular as tabelas de página:

```
CR3 → P4 table physical addr
       ↓ + physical_memory_offset
       &mut PageTable (virtual)
       ↓
OffsetPageTable::new(table, offset)
       ↓
       Mapper<Size4KiB> + Translate
```

### Camada 2: Frame Allocator (BootInfoFrameAllocator)

O mapa de memória física fornecido pela UEFI/BIOS via `BootInfo.memory_map` lista regiões `Usable` (livres). O `BootInfoFrameAllocator` itera sobre essas regiões, calcula os endereços de frame e os retorna um a um:

```
MemoryMap.iter()
  → filter(region_type == Usable)
  → map(range.start_addr()..range.end_addr())
  → flat_map(step_by(4096))
  → map(addr → PhysFrame::containing_address(PhysAddr::new(addr)))
  → nth(next)
```

Implementa `unsafe trait FrameAllocator<Size4KiB>` — a `unsafe` decorre da exigência de que frames retornados sejam únicos e não utilizados.

### Camada 3: Heap (linked_list_allocator)

O heap é inicializado em `0x_4444_4444_0000` com 100 KB iniciais:

1. Calcular página de início e fim: `Page::range_inclusive(start, end)`
2. Para cada página no range:
   - Alocar um frame via `BootInfoFrameAllocator`
   - Mapear via `Mapper::map_to(page, frame, PRESENT | WRITABLE, allocator)`
   - Flush TLB
3. Inicializar `LockedHeap` com `init(start, size)`

O `LockedHeap` usa `linked_list_allocator` v0.9.1 que implementa um free-list allocator simples, suficiente para os primeiros estágios.

### Sequência de Boot

```
kernel_main(BootInfo)
  ├─ vga_buffer::init(offset)
  ├─ interrupts::init_idt()
  ├─ memory::init_memory(offset)          → OffsetPageTable
  ├─ BootInfoFrameAllocator::init(map)    → FrameAllocator
  ├─ allocator::init_heap(mapper, alloc)  → LockedHeap global
  ├─ TEST: Box::new(41)                  → "Box value: 41"
  ├─ TEST: Vec::push(10, 20, 30)         → "Vec: [10, 20, 30]"
  └─ loop
```

## Consequences

**Positive:**
- `extern crate alloc` funcional — `Box`, `Vec`, `String`, `Arc` disponíveis
- Heap extensível: basta mapear mais páginas e chamar `ALLOCATOR.lock().extend()`
- `BootInfoFrameAllocator` garante uso exclusivo de frames não alocados pelo bootloader
- Código reutiliza tipos seguros da crate `x86_64`

**Negative:**
- `BootInfoFrameAllocator` avança sequencialmente sem rastrear devolução — frames alocados por page tables intermediárias não são reciclados (não crítico para boot, mas precisa de slab allocator no futuro)
- `spin::Mutex` no LockedHeap — deadlock se um handler de exceção tentar alocar
- 100 KB fixo — tamanho arbitrário, precisa ser ajustado conforme demanda

**Risks:**
- `step_by(4096)` no frame allocator: range de endereços pode ser grande, mas o iterador é lazy, só calcula quando `nth()` é chamado
- `Cr3::read()` lê o registrador CR3 da CPU atual — seguro pois estamos em single-core

## Alternatives Considered

1. **`linked_list_allocator` vs `buddy_system_allocator`** — Buddy system é mais eficiente para fragmentação, mas linked_list_allocator é mais simples e testado na blog_os.
2. **Recursive Page Table** — Alternativa ao `OffsetPageTable`, mas a feature `recursive_page_table` do bootloader é conflitante com `map_physical_memory`.
3. **`static mut` heap buffer** — Menos flexível e sem suporte a extensão.
4. **TLSF allocator** — Two-Level Segregated Fit, ideal para sistemas de tempo real, mas complexidade desnecessária neste estágio.

## References

- Blog_OS: "Paging Implementation" — https://os.phil-opp.com/paging-implementation/
- Blog_OS: "Heap Allocation" — https://os.phil-opp.com/heap-allocation/
- `linked_list_allocator` crate: https://crates.io/crates/linked_list_allocator
- Intel SDM Vol. 3, Chapter 4: Paging
