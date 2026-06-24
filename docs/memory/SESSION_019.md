# Sessão 019 — SMP Multi-Core Boot Fix (14/14 Sprint 19)

**Data:** 2026-06-23
**Duração:** Sessão de depuração do SMP multi-core
**Versão:** v0.14.1

## Contexto

O Sprint 19 (Block 2: SMP + Slab + Heap 4 MB) foi implementado e passava `cargo check --release`, mas o SMP multi-core não funcionava: todas as APs travavam ao iniciar. O sintoma original era `[SMP] APs acordados: 0` no serial, e a QEMU trace mostrava `check_exception` com #PF (page fault) + triple fault nos APs.

## Dificuldades e Barreiras

### 1. Diagnóstico sem GDB
- Sem GDB disponível (Windows toolchain), usamos `qemu-system-x86_64.exe -d int,cpu_reset,guest_errors -D qemu_trace.log`
- `qemu_trace.log` mostrava AP iniciar (CS=0xF000, EIP=0xFFF0), seguir o SIPI (CS=0x1000, EIP=0x0000), executar trampoline 16→32→PAE, ativar paging, e então #PF imediata
- A pista crítica: `check_exception` mostrava #PF nos APs com CR2=0x400A4 (exatamente a primeira instrução do trampoline_64)

### 2. Red Herring: dump da trampoline mostrava zeros
- Inicialmente suspeitamos que o dump da trampoline exibir zeros significava que a identidade-map não estava funcionando
- Corrigimos isso acreditando que era um problema de patching (RBX relativo)
- Na verdade, o dump de 40 bytes **antes** de `init_trampoline()` rodar mostrava zeros porque o código identidade-map ainda **não tinha sido copiado** para o frame físico — a ordem das operações está correta, o dump só estava mal posicionado para diagnóstico

### 3. Red Herring: `trampoline_64` off by 2
- Suspeitamos que `trampoline_64` estava 2 bytes antes do início do código 64-bit
- Após análise detalhada do hex dump do `trampoline_blob`, `trampoline_64` em 0x2060ae é **exatamente** o primeiro byte de `mov ecx, 0xC0000080` (imediatamente após `retf` em 0x2060ad)
- A confusão veio de ler o dump errado — estávamos vendo offsets decimais (hex dump do patch header) misturados com bytes

### 4. Red Herring: identidade-map de 4 páginas
- Suspeitamos que o AP precisava de identidade-map de 4 páginas (0x0, 0x10000, 0x20000, 0x40000)
- Tentativa: map_identity_region() e depois restore_identity_map() — código complexo e frágil
- Na verdade, o AP só precisa da **página 0x40000** mapeada para VA física = PA

### 5. Root Cause Real
- O bootloader (v0.9.34, `map_physical_memory`) identity-mapa páginas baixas 0-7 na tabela de páginas em 0x4000
- PD[0] = 0x4023 → PT base = 0x4000 (presente, escrita, usuário)
- PT[0..7] = 0x00003, 0x01003, ..., 0x07003 (páginas 0..7 mapeadas identity)
- **PT[64] (VA 0x40000) = 0x0000000000000000** (não presente)
- Quando o AP ativa paging e tenta buscar em 0x400A4 (retf para código 64-bit), o MMU gera #PF → sem handler AP → triple fault → reset

### 6. Fix
- Escrevemos PTE `0x40000 | 0x003` (Present|Write) em `phys_offset + 0x4200` (endereço de PT[64] no mapeamento virtual)
- Uma única instrução `write_volatile` resolve o problema

### 7. Race Condition no CPU_COUNT
- `CPU_COUNT: AtomicU8` com `fetch_add` deveria funcionar, mas QEMU TCG não garante atomicidade cross-vCPU em software
- Todos os APs liam o mesmo valor antes do incremento
- Fix: `spin::Mutex` em torno de `CPU_COUNT.fetch_add(1, Ordering::SeqCst)` — o spinlock garante exclusão mútua entre APs mesmo sem atomics cross-vCPU confiáveis
- 50ms busy-wait após segundo SIPI necessário para que todos os APs completem o trampoline antes do BSP ler o contador

### 8. `native_memcpy` vs `asm!` memcpy
- Tivemos que implementar `memcpy` com `asm!` para o patching do trampoline header porque `core::intrinsics::copy_nonoverlapping` ocasionalmente gerava código que chamava `native_memcpy` (não disponível em `no_std`)
- `asm!("rep movsb")` é 100% bare-metal e não depende de std/libc

### 9. Slab Allocator Memory Corrupt
- O Slab Allocator corrompia a free list porque `align_of::<*mut u8>()` (8) forçava alinhamento de 8 bytes, mas os buckets de 32/64/128 etc bytes criavam gaps
- Fix: `SLAB_CHUNK_SIZE` = bucket_size (não alinhado para 8), e a free list aponta para o chunk **depois** de armazenar o `next_ptr`
- `ptr.write::<*mut u8>(next)` seguido de `ptr.add(bucket_size)` — o bucket_size já inclui o espaço do next pointer

## Soluções Adotadas

1. **Identity-map PTE via write_volatile**: Em vez de `mapper.identity_map()` (que exigiria `FrameAllocator` ativo), escrevemos diretamente no page frame físico 0x4200 (endereço virtual via `phys_offset + 0x4200`)
2. **spin::Mutex no CPU_COUNT**: Protege o contador de APs contra race condition cross-vCPU no QEMU TCG
3. **50ms busy-wait**: Garante que todos os APs terminaram o trampoline antes do BSP ler o contador
4. **asm! memcpy**: Substitui `copy_nonoverlapping` para evitar dependência de `native_memcpy`

## Lições Aprendidas

1. **FTrace > GDB**: `-d int,cpu_reset,guest_errors -D qemu_trace.log` é mais eficiente que GDB para debug de boot de APs
2. **Bootloader identity-maps ONLY pages 0-7**: PD[0] existe (0x4023) mas PT[64..] é zero. O AP precisa de mapeamento para 0x40000
3. **QEMU TCG não garante atomicidade cross-vCPU**: Sempre proteger atomics compartilhados com spinlock
4. **Não confiar em pistas visuais de dump**: O dump do trampoline antes do init parecia "zero" mas era apenas a ordem das operações
5. **50ms é tempo suficiente** para QEMU TCG APs completarem trampoline (todos os 3 APs em -smp 4 bootam em < 20ms)

## Status Final

- `-smp 1`: ✅ Boot normal (single core)
- `-smp 2`: ✅ BSP + AP 1 = `APs acordados: 1`
- `-smp 4`: ✅ BSP + AP 1, 2, 3 = `APs acordados: 3`
- `qemu_trace.log`: zero `check_exception` — nenhum #UD, #PF, #GP
- `cargo check --release`: 0 erros, 0 warnings
- `cargo bootimage --release`: 0 erros
