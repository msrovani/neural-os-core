# SESSION_478 — #PF storm de pressão do heap: try_fault_in_heap sem gate + range do SLAB descoberto

**Data:** 2026-10-16 · **Estado:** v1.9.99-s478 TEST · **Branch:** `main`
**Contexto:** o gate `heap_headroom_critical()` do s476 estava só no `sleep_cycle` (cellular.rs). Um #PF storm
reapareceu no boot QEMU (TCG, smp=4) com `cr2=0x000040000007fff8` — endereço no range do SLAB
(entre `HEAP_START=0x400000000000` e `LARGE_HEAP_START`), que não era coberto por nenhum range do
`try_fault_in_heap` (caía em `no_rng`). O handler de page fault chamava `try_fault_in_heap` diretamente
sem verificar se o heap estava crítico, causando #PF dentro do handler de #PF → storm → park core.

---

## 1. Causa-raiz — três problemas

- **Problema 1 (gate incompleto):** `heap_headroom_critical()` existia em `sleep_cycle` (cellular.rs),
  `moe.rs`, `infer_queue.rs`, etc., mas **não** em `try_fault_in_heap` (allocator.rs:1777). Essa função é
  chamada **diretamente do handler de page fault** (interrupts_ext.rs:203) quando `try_handle_fault` falha.
  Quando o heap está pressionado, `try_fault_in_heap` tenta `alloc_physical_frame()` e pode causar outro
  #PF dentro do handler → storm → park core (fail-closed).
- **Problema 2 (range do SLAB descoberto):** `try_fault_in_heap` só cobria três ranges: TALC
  (`LARGE_HEAP_START` a `TALC_SPAN_END`), bump (`HEAP_BUFFER` no kernel virt) e kernel virt. O range do
  **SLAB** (`HEAP_START=0x400000000000` a `LARGE_HEAP_START`) não era coberto — faults no SLAB caíam em
  `no_rng=1` → `try_fault_in_heap` retornava false → #PF não curado → repetição infinita → storm.
- **Problema 3 (handler sem guard):** O handler de page fault (`interrupts_ext.rs`) chamava
  `try_fault_in_heap` sem verificar `heap_headroom_critical()` primeiro. Mesmo com o gate no
  `try_fault_in_heap`, o handler deveria recusar antes de tentar alocar sob pressão.

## 2. Fix (2 arquivos)

- **crates/k_nano/src/allocator.rs:**
  - Adicionado gate `heap_headroom_critical()` no início do bloco de alocação do heap
    (antes de `alloc_physical_frame()` na linha ~1848): se o heap está crítico, retorna `false`
    honestamente em vez de tentar alocar e causar #PF dentro do handler.
  - Adicionado range `in_slab` para endereços entre `HEAP_START` e `LARGE_HEAP_START`: mapeia via
    identidade HHDM (`cr2 - pmoff`), já que a página física já existe (parte do heap inicial
    `.bss.heap`); só falta a entrada da page table.
- **crates/neural-kernel/src/interrupts_ext.rs:**
  - Adicionado `!crate::allocator::heap_headroom_critical() &&` antes da chamada a
    `try_fault_in_heap` no handler de page fault (linha ~203): se o heap está crítico, não tenta
    alocar — vai direto para o tratamento de falha (fail-closed), evitando #PF dentro do handler.

## 3. Prova de runtime

- Boot QEMU (TCG, smp=4, 4GB, disco_qemu.raw com FALCON3.BIN + BITNET2B.BIN):
  `logs/boot_whpx_20261016_010907.txt`.
- **Resultado:** `pf=0` (zero page faults), `heap=1024/3840M`, `tick=43723+`,
  `aps=3`, `[SILENCE] log parado ha 810s` — sistema estável por 13+ minutos sem nenhum #PF.
- Antes do fix: `#PF storm ip=... streak=3+ park core (fail-closed)` em poucos segundos após o Runtime.
- **Classificação:** fix = runtime-VERIFIED (QEMU TCG, 13+ min estável).

## 4. Lições

- **Gate de heap só no sleep_cycle não é suficiente:** `try_fault_in_heap` é chamado diretamente do
  handler de page fault e precisa do mesmo guard `heap_headroom_critical()`.
- **Range do SLAB não coberto:** faults entre `HEAP_START` e `LARGE_HEAP_START` caíam em `no_rng`
  porque `try_fault_in_heap` só cobria TALC, bump e kernel virt. Adicionar `in_slab` com mapeamento
  HHDM fecha a lacuna.
- **Handler de page fault precisa de guard próprio:** verificar `heap_headroom_critical()` antes de
  chamar `try_fault_in_heap` evita #PF dentro do handler de #PF — padrão fail-closed consistente com
  o s476.

## 5. Pendentes herdados (inalterados)

- mesh re-run com kernel atual (CURAIX-0060)
- F1 1h 8c/8GB (UNKNOWN)
- Teste USB em metal (foto PRE/POST 00:14.0, TODO s467) — agora confirmação, não primeira descoberta
