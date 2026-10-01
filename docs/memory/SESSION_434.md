# SESSION_434 — Instrumentação + diagnóstico do overflow TALC (idea #630): 2 gaps de realloc + fail-closed de classe

## Origem
- #630 (3 sessões vendo `OOM/TALC ... agente=infer_worker` com span de 6911MB + stall
  silencioso pós-OOM). Instrumentação virou descoberta: o OOM nunca passou pelo path
  instrumentado — eram **2 gaps de realloc**, não falta de memória.

## Instrumentação (s434a — tudo zero-alloc, atomic, chamável do path de morte)
- `allocator.rs`: `TALC_PF_OUTSIDE_SPAN` (cr2 de fault no range TALC acima do span
  demand-pagado), `TALC_NULL_BINS_NONEMPTY`/`_EMPTY` (fragmentação vs span nunca
  materializado), snapshot one-shot `availability_low/high` dos bins do talc (layout
  confirmado no fonte 4.4.3: low@0 high@8 bins@16, ZST handler no fim),
  `talc_pf_outside_span_cr2()`/`talc_null_avail_snapshot()`.
- `memory.rs`: `pmm_free_frames()`/`pmm_allocated_count()`/`pmm_total_frames()`.
- `try_fault_in_heap`: registra cr2 fora do span ANTES de recusar (candidata 2).
- `map_page_direct`: `PF_DIAG_PT_ALLOC_FAIL` (alloc_pt_frame 0 em qualquer nível —
  antes virava MAP_FAIL, causa real invisível).
- `oom()`: linha `[OOM-DIAG] pmm free/alloc/total | talc nulls bins_avail req pf_out`.
- Claim: stop-the-world spin bounded 2s (candidata 2 race — uso pós-diagnóstico).
- **Fail-closed de classe:** `oom()` saiu do `loop { hlt() }` (WHPX: sem wake = stall
  silencioso; E o core pode estar segurando lock de sistema → congela TODOS os cores
  que pedirem o mesmo lock) → spin com heartbeat de serial a cada 10s
  (`[OOM-HALT] agente=... size=... tick=...`).

## Descobertas (o instrumento contou a história)
- **s434a (log 140959):** heartbeat quebrou o stall — padrão `size=146`/`size=10624`
  alternando ~140 ticks = **N cores parkados no OOM** (não 1). MAS `[OOM-DIAG]` veio
  com `nulls=0 bins=0/0 req=0/0` = defaults → a morte NÃO passou pelo `GlobalAlloc::alloc`
  do híbrido (único path instrumentado). `pmm free=1.7M` (6.5GB) → PMM descartado.
- **s434b:** o `Talck::realloc` (talc 4.4.3) chama o **malloc INTERNO** — NULL não passa
  pelo nosso `alloc`. Gap 1 fixado: realloc de chunk TALC-residente com falha → snapshot +
  counter + `oom()` (antes: null propagado ao `__rust_realloc` builtin → handler sem diag).
  **Log 142925 ainda mostrou `nulls=0`** → havia OUTRO path.
- **s434c = causa-raiz:** realloc de chunk **bump-residente** (`HybridAllocator::realloc`
  → default `GlobalAlloc::realloc(&BUMP_ALLOC)` = alloc novo + copy + dealloc) — o alloc
  novo era do **bump puro, SEM overflow**: com a janela 2030MB cheia, retornava NULL
  **sem nunca tocar o TALC** → `alloc_error_handler` (nosso `oom()`), sem counter/snapshot
  e **sem usar os 6911MB de TALC disponíveis**. Qualquer `Vec`/`String` nascido no bump
  crescendo com bump cheio = morte cega. Fix: realloc bump→aloca pelo **híbrido**
  (TALC dá o espaço novo), copy manual, dado velho segue no bump (sem free — sempre foi).

## Validação (QEMU 8GB/8c, log `boot_whpx_20261001_144435.txt`)
- **0× `OOM/TALC` em ~16min de runtime (T+57771)** — recorde de uptime; as 3 sessões
  anteriores morriam em T+34k. Bump no teto (62.553× "budget cap — recusa grow") com
  sistema VIVO: scheduler ativo (tick lento honesto de audio_input/safety), 395 matmuls,
  HubTriage em cadência + dedupe cooldown, LLM timeout→fallback→HITL→Jarbas→MoE→LLM
  (T+32694..33408). O stall silencioso pós-OOM também não ocorreu (não há mais OOM).
- Gates: k-nano 244/244 (-t1; o paralelo pega o flaky compact_batch_guard conhecido),
  hermes 273/273 (-t1), `cargo nk` 0 erros, build boot + build_image, ELF e uefi.img
  provados (`bins_avail`, `OOM-DIAG`, `OOM-HALT`).
- Nota de método: prova de string no uefi.img falhou 2× para comentários (o literal só
  existe no fonte) — provar format string de CÓDIGO ATIVO (`bins_avail`), não comentário.

## Lições
- **Um counter no path errado é pior que nenhum:** `TALC_OVERFLOW_NULL` dava a falsa
  impressão de cobertura; `nulls=0` no diag foi o que delatou os gaps (instrumento
  honesto = counter que ZERA quando o path não passa).
- **`GlobalAlloc::realloc` default = armadilha em allocator híbrido:** "alloc novo +
  copy" com alloc de UMA camada perde o overflow da outra (o bump cheio matava com
  TALC 6911MB livre). Realloc precisa de política explícita por residência do chunk.
- **Talck::realloc/alloc_zeroed não passam pelo GlobalAlloc::alloc do usuário** —
  qualquer counter/snapshot de OOM tem que viver nos 3 métodos do wrapper.
- **`loop { hlt() }` no handler de OOM = stall silencioso em 2 mecanismos** (WHPX sem
  wake + lock do core morto). Spin + heartbeat de serial converte silêncio em evidência
  (o padrão 146/10624 a cada 140 ticks = N cores parkados ficou visível).

## Residuais → ideias
- `alloc_zeroed` bump-residente (mesmo gap do realloc? verificar — zeroed do bump é
  inline, mas o default `alloc_zeroed` do GlobalAlloc chama `alloc` — coberto pelo
  híbrido; confirmar no próximo bughunt).
- Fragmentação de longo prazo do TALC (agora que serve os spills: monitorar
  bins_avail ao longo de horas de runtime).
- #627 (seam per-CPU infer-stamp) segue aberto.
