# SESSION_334 — Fix heap-wrap 2⁶⁴ + OOM instrumentado p/ teste a quente — 2026-09-13

**Objetivo:** resolver o OOM que abortava o boot QEMU com modelos (`grow wrap 2^64` / `size=8388645916`) e deixar o teste a quente (metal) diagnósticos.

**Fonte:** oracle (root-cause definitiva) + fixer (implementação) + verificação QEMU com disco.

---

## 1. Causa-raiz (oracle — definitiva)

1. **RC1 — janela endereçável ~2 GB vs budget 5376 MB:** `HEAP_BUFFER` é `.kheap` no FIM da imagem (`limine.ld`); `heap_start + offset` cruza 2⁶⁴ em ~2044 MB. O budget (`heap_budget_mb(7168) = 5376 MB`) capava em **MB-de-RAM, não em offset endereçável** → todo grow com `need > ~2 GB` andava até o wrap (`grow wrap 2^64 - abort` em `allocator.rs:119-124`) → OOM → hlt.
2. **RC2 — "Heap:128MB" era métrica mentirosa:** o `MemoryAgent` no branch sem-modelo **hardcodava** `heap_target_mb: 128` (o heap real era o piso 512 MB). Três números de "heap" inconsistentes (128 fake / 2048 SKU fake / 5376 budget real / 512 piso real) = a armadilha de debug que fez o **1º fix (HEAP_EXT_BASE) falhar**.
3. **RC3 — LLM ABSENT é por design:** o gate de sandbox (`is_sandbox()`) pula o load de modelos FAT no QEMU — causa separada, não do heap.
4. **RC4 — o alloc de 8,39 GB tem contagem ÍMPAR** (2.097.168.567 elems, align=4) → **length corrompido/miscomputado** (não é shape legítimo). Suspeitos: `parallel_matmul.rs:54` (`Vec::with_capacity(m*n)` via shape lixo, disparado quando os APs ficam pollable) ou length clobberado por DMA (residual SESSION_252).

## 2. Fix (implementado)

- **A (`allocator.rs`):** `BUMP_MAX_OFFSET = usize::MAX - heap_start - 4096`; `grow_bump_auto` faz `want = min(need, budget, window)` e **recusa honestamente ANTES de mapear** (`HEAP fail refuse need=XMB window=~2034MB (agente=<nome>)`); `set_heap_budget_mb` clampa à janela. Sem walks de wrap.
- **B (telemetria honesta):** `memory_agent.rs` no-model branch reporta o `CURRENT_HEAP_MB`/`heap_used_bytes()` REAL (`[MEM] Heap(atual):512MB`); `inventory.rs` deriva `heap_size_mb` do `CURRENT_HEAP_MB` (não mais a tabela SKU 2048).
- **C (OOM handler instrumentado):** o OOM handler agora carimba o **nome do agente** (`tick_in_progress()`, lock-free) + size no VGA/serial **e no canal FB** (`exception_fb_stamp`) — porque **allocs grandes bypassam o grow** (TALC direto: 8,39 GB num heap de 512 MB falha sem invocar grow).

## 3. Verificação QEMU (com disco, pós-fix)

- `grow wrap 2^64`: **0 ocorrências** ✅ (o wrap acabou)
- `[MEM] Heap(atual):512MB` ✅ (telemetria honesta)
- `hermes ARCH heap=512MB` ✅ (real, não SKU)
- **OOM ainda ocorre e para o sistema** (`[OOM/TALC] size=8388746268`, contagem ímpar) — mas agora **com o nome do agente carimbado no FB** (`OOM agente=<nome> size=…`) → o teste a quente no metal nomeia o culpado.
- O alloc de 8,39 GB **bypassa o grow** (TALC direto) — por isso o `HEAP refuse` não aparece; a instrumentação foi movida para o OOM handler.

## 4. Estado

- k-nano **192 pass / 0 fail**; jarbas 89/0; `cargo nk`/`cargo check --release` = 0 erros.
- Imagem `target/usb_hw.img` regenerada com a instrumentação — **pronta para o teste a quente no Alienware**.
- **Pendências:** nomear o requester dos 8,39 GB (o teste a quente no metal dá o nome via `OOM agente=`), #1 USB/mouse, #4 licença, stash leftover.

## 5. Lições (→ AGENTS.md)

- Budget em MB-de-RAM não previne wrap — clamp ao window e recusar antes de mapear.
- Métrica mentirosa (3 números de heap) = a armadilha que fez o 1º fix falhar.
- Allocs grandes bypassam o grow (TALC direto) — instrumentar o OOM handler, não só o grow.
