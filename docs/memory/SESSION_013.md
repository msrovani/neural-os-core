# Session 013 — Sprint 19 (Block 2: SMP + Slab + Heap 4 MB)

**Data:** 2026-06-23

## Dificuldades e Decisões

### 1. Trampoline Assembly — O Maior Desafio do Projeto Até Hoje

**Problema:** O trampoline SMP é o código mais complexo já escrito para o neural-os-core. Ele precisa iniciar em 16-bit real mode (onde a CPU acorda após SIPI), transicionar para 32-bit protected mode, ativar PAE, entrar em 64-bit long mode, e finalmente chamar uma função Rust — tudo sem saber em qual endereço físico o código está rodando (já que o endereço é alocado dinamicamente).

**Dificuldade Técnica:** Em 16-bit mode, `cs:[displacement]` usa CS.base como base (SIPI seta CS.base = endereço físico da página). Mas após o far jump para 32-bit, CS.base = 0 (pela GDT flat). Em 64-bit mode, segment overrides CS/DS/ES/SS são ignorados para data — apenas GS e FS funcionam.

**Solução:** Três estágios de acesso a dados:
- 16-bit: usar `cs:` segment override para ler jmp32_val do header (CS.base = phys_addr após SIPI)
- 32-bit: carregar EBX com jmp32_val (preservado pelo retf), subtrair offset do código para obter phys_base, usar `[ebx + offset]` para todos os acessos
- 64-bit: usar RIP-relative `[rip + label - current]` (identidade de mapeamento garante que o endereço virtual = físico)

**Far Jumps:** Não foi possível usar `ljmp $seg, $offset` porque o offset (endereço físico absoluto) não é conhecido em tempo de link. Solução: `push 0x08; push eax; retf` (com 0x66 prefix em 16-bit). Funciona porque retf pop EIP + CS na ordem correta.

**Encoding Manual:** Parte da assembly precisou de bytes manuais (`0x66` para operand size override em 16-bit). O `0x66 0x0F 0x01 0x13` para LGDT em 16-bit com destino EBX foi especialmente crítico.

### 2. Slab Allocator — Send/Sync com Raw Pointers

**Problema:** O SlabAllocator contém `*mut u8` (raw pointers) para a free list. `Mutex<SlabAllocator>` requer `SlabAllocator: Send` para que `Mutex`: Send+Sync.

**Solução:** `unsafe impl Send for SlabAllocator {}` — justificado porque o `Mutex` garante exclusão mútua de acesso.

### 3. Identity Mapping for Trampoline Page

**Problema:** Quando o trampoline ativa paging (PG bit em CR0), ele carrega CR3 com as page tables do BSP. Se a página do trampoline não estiver identity-mapped (virtual = physical), a CPU faz page fault imediatamente.

**Solução:** Antes de enviar SIPI, `init_smp()` verifica se a trampoline page já está mapeada (via `mapper.translate_page()`). Se não, mapeia com `PRESENT | WRITABLE`. O bootloader pode ou não identity-mappar o low 1 MB — essa verificação cobre ambos os casos.

### 4. Aliasing do OffsetPageTable

**Problema:** O `mapper` criado em `kernel_main` via `memory::init_memory()` mantém uma referência `&mut PageTable` ao mesmo hardware. `init_smp()` cria um novo `OffsetPageTable` a partir do mesmo ponteiro — duas referências mutáveis simultâneas = UB em Rust.

**Solução:** Escopar o `mapper` original: `{ let mut mapper = ...; init_heap(&mut mapper, ...); }`. O mapper é dropado após o heap init, liberando a referência. `PHYS_MEM_OFFSET` (AtomicU64) é preservado independentemente.

## Modulações

- **Slab não é o global_allocator** — O SlabAllocator é um alocador de pools fixos para objetos do kernel, não substitui o LockedHeap como global. Ambos coexistem no heap de 4 MB: Slab nos primeiros 512 KB, LockedHeap nos 3.5 MB restantes.
- **Trampoline com endereço dinâmico** — Ao invés de fixar em 0x8000 (como Linux faz), alocamos via `allocate_below_1mb()` que retorna qualquer frame livre < 1 MB. O assembly é patchado em runtime com o endereço real.
- **PerCpu simplificado** — 64 bytes (um cache line), apenas campos essenciais. Ring-level e tipo de core são campos, não tipos separados.
- **AP entry point** — O AP entra em 64-bit e imediatamente faz EOI (se houver pending), incrementa contador e hlt-loops. Futuro: integrar com o escalonador de tasks.

## Erros Corrigidos a Quente

1. `memory.rs` — `OffsetPageTable` acidentalmente renomeado para `OffsetTable` durante edição. Corrigido.
2. `smp/percpu.rs` — padding do BSP_PCPU const com 48 bytes, struct com 43. Corrigido para 43.
3. `main.rs` — `mapper` scope introduzido para evitar aliasing de referência mutável à page table.
4. `smp/mod.rs` — `crate::percpu` corrigido para `percpu` (mesmo módulo).

## Pendências Técnicas

1. **Verificação de compilação** — Cargo não está no PATH desta máquina. Necessário executar `cargo check --release` na primeira oportunidade para validar a assembly do trampoline.
2. **QEMU SMP test** — `qemu-system-x86_64 -smp 2 -m 512M` para validar INIT-SIPI-SIPI e AP boot.
3. **Message Type 5 (x2APIC)** — Não implementado. APs com x2APIC não serão detectados.
4. **PerCpu para APs** — `BSP_PCPU` é usado como exemplo pela trampoline. APs precisarão de PerCpu individuais em uma futura iteração.

## Decisões Arquiteturais

1. **Slab como alocador auxiliar, não global** — O LockedHeap continua sendo o global_allocator. Slab é para objetos de tamanho fixo (page table entries, task structs, etc.).
2. **PHYS_MEM_OFFSET global** — AtomicU64 acessível de qualquer módulo. Necessário para trampoline, identity mapping, e futuras alocações que precisam traduzir phys→virt.
3. **Trampoline header 48 bytes** — 6 campos u64 patchables. Compacto e previsível.
4. **Identity mapping condicional** — Não assumir que o bootloader mapeou o low 1 MB identity. Verificar e mapear se necessário.

## Total de Linhas

- `src/slab.rs`: 152
- `src/smp/mod.rs`: 112
- `src/smp/percpu.rs`: 74
- `src/smp/trampoline.rs`: 187
- Modificações em `memory.rs`, `allocator.rs`, `apic.rs`, `main.rs`: ~+90

**Total: ~615 linhas novas/modificadas.**
