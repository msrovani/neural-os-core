# SESSION_435 — Telemetria de uso REAL do TALC (HUB HEALTH + hub_triage)

**Data:** 2026-10-01 · **Idea:** #630 (residual: monitorar fragmentação em runtime de horas) · **Sprint:** v1.9.99-s435

## Objetivo

O residual do #630 (s434): o TALC de 6911MB tinha telemetria only-at-death
(`talc_null_avail_snapshot` one-shot) e o HUB contava o span INTEIRO como
headroom (estimativa generosa s430b). Para monitorar fragmentação em runtime
de horas, era preciso medir occupied vs free REAIS e expor no HUB HEALTH +
hub_triage.

## Layout do talc 4.4.3 (confirmado no fonte `~/.cargo/registry/src/*/talc-4.4.3/src/talc.rs`)

- `Talc<O>` sem feature counters: `availability_low @0`, `availability_high @8`, `bins: *mut Bin @16`, `oom_handler` (ZST) no fim.
- `bins` = array de **128 sentinelas** `Option<NonNull<LlistNode>>` (BIN_COUNT = 2×64); cada sentinela termina em `None` (lista NÃO-circular nos gaps — `register_gap` faz `insert(node, bin_ptr, *bin_ptr)`, o 1º nó tem `next = old head` que pode ser `None`).
- **Gap-node:** `next @0` (1 ptr), `size @16` (`GAP_LOW_SIZE_OFFSET = NODE_SIZE = 2 ptr`), `high_size @8` do acme. MIN_CHUNK_SIZE = 24B.
- `Tag` NUNCA aparece nos bins (só chunks alocados) — o walk dos bins vê apenas gaps.

## Implementação

### `crates/k_nano/src/allocator.rs`
- `TalcUsage { free_bytes, used_bytes, largest_free, gaps, partial }` + statics `TALC_USAGE` (cache sob Mutex) + `TALC_USAGE_SAMPLES` (n/a ≠ 0).
- `talc_walk_bins(talc, span)` — percorre os 128 bins lendo os gap-nodes via `read_volatile` (escritor = talc sob o MESMO lock do Talck); `used = span − free`; CAP `TALC_WALK_MAX_GAPS=4096` + bounds-check (`node` e `node+size` dentro do span) → `partial=1` honesto em vez de loop infinito/dado falso. Zero alloc.
- `talc_refresh_usage()` (2 Hz no HUD, 1/min no triage) / `talc_usage()` (cache) / `talc_usage_samples()`.
- **Headroom honesto:** `heap_headroom_bytes()`/`heap_observe()` somam `talc_usage().free_bytes` (clampado à capacidade), NÃO o span inteiro. `HeapObserve` +5 campos (`talc_used_mb/talc_free_mb/talc_largest_mb/talc_gaps/talc_partial`).
- Seed pós-claim em `talc_init_post_memory` (stop-the-world ainda ativo — exclusivo; pós-boot o walk é sempre do HUD sob lock).

### `crates/jarbas/src/display/gauges.rs`
- `HUB_ROWS` 20→21; nova linha **`talc`** (row 20): `u{used}M/{span}M lg{largest}M g{gaps}`; Warn quando `free≥256MB && largest×4 < free` (fragmentação), `partial g{n}` em walk abortado, `n/a` sem amostra (honestidade n/a ≠ 0). Fora do cálculo de worst quando Na.

### `crates/hermes/src/hub_triage.rs`
- `HubTriageInputs` + 5 campos do TALC (via `heap_observe()`); snapshot JSON estendido (`talc_used/talc_free/talc_largest/talc_gaps/talc_partial` no objeto `heap`).
- Vereditos novos (observe-only, lição s429-lab): `talc fragmentado (largest/free baixo)` (free≥256MB e largest×4 < free) e `talc metadata parcial (walk abortado)`.
- Linha `ok` do slog carrega a evidência: `ok talc u{}M f{}M lg{}M g{}` (regra 419 — dado novo só está feito com slog em runtime).

## Validação

- Testes: **hermes 275/275** (-t1; 2 novos: `talc_fragmentado_e_observe_only`, `talc_walk_parcial_nunca_propoe`), **k-nano 244/244** (-t1), **jarbas 134/134** (-t1).
- `cargo nk` 0 erros (20s); `cargo build --release -p boot` + `build_image.py`; strings de código ativo provadas no uefi.img (`ok talc u`, `partial g`, `talc fragmentado`).
- **QEMU 8GB/8c (log `boot_whpx_20261001_182016`):** boot limpo, Tier 1 ready 6911MB, triagens a 1/min com `ok talc u0M f6911M lg6911M g1` (span fresco, gap único, telemetria real), zero OOM, sistema vivo (matmuls 8 workers, LayerDiag fluindo).

## Lições

1. **A lista de gaps do talc NÃO é circular no sentido usual dos algoritmos de teste** — o iterador do crate usa o sentinela como head; direto no bin, a terminação é `None` (1º nó insere com `next = old head = None`). Walk host-side é trivial uma vez que o layout é lido do FONTE, não de analogia.
2. **Dado medido sob lock do próprio escritor = consistência sem barreira** — o HUD lê os bins sob o lock do Talck; `read_volatile` cobre reordenação do compilador; nenhum core escreve os bins fora do lock.
3. **Headroom generoso aposentado com telemetria real** — o estimado (span inteiro) virou medido (gaps); os gates 64/128MB continuam válidos como piso do bump.

## Residuais

- Fragmentação real só aparece com o bump derramando para o TALC e frees criando buracos — monitorar em runtime de horas (objetivo do #630 residual); a regra largest×4 < free é o primeiro detector.
- `talc_capacity_bytes()` duplica a fórmula de `talc_capacity_mb()` (MB × ~1MB) — consolidação futura se uma 3ª cópia aparecer.

## Addendum — Lab longo pós-s434 (2 boots 8GB/8c, 30+ min)

**Resultado da telemetria (o que o lab foi fazer):** linha `talc` estável a 1/min
nos 2 boots, `u0M f6911M lg6911M g1` durante toda a vida útil (gap único, sem
spill materializado), **zero OOM/TALC/OOM-HALT** — estabilidade pós-s434
confirmada até o freeze.

**Stall silencioso DETERMINÍSTICO descoberto (novo residual, s436):**
- 2/2 boots com o MESMO padrão: log congela em **T+23334** (boot 183000) e
  **T+23437** (boot 185325), QEMU vivo com ~1 core em spin (CPU delta ≈1.0×
  parede), **SEM nenhum OOM** (o heartbeat `[OOM-HALT]` não dispara = não é
  `oom()`; sem #PF novo).
- Contexto comum aos 2 boots: (1) bump acabou de chegar ao **teto 2030MB**
  (grow T+23058/T+23148); (2) job `a2_proof` id=2 completa: prefill ~57s,
  slow_slice n=22, **1º decode OK (tok=2305, out_len=4)**, `a2_proof done`,
  `done id=2 len=4 in_flight=0`; (3) ÚLTIMA linha do log em ambos:
  `[Log] [ok] - [JARBAS] JARBAS:  and` (echo do texto da resposta);
  (4) HubTriage com telemetria ok até T+20898-23334.
- Interpretação honesta: correlação temporal forte (teto do bump + cleanup
  pós-a2_proof + eco do Jarbas), causa NÃO provada — suspeito principal é o
  path de eco `[JARBAS]` do LogAgent (log/serial lock segurado ou spin no
  String do eco), mas pode ser o AP do infer no cleanup pós-done. Carimbar
  enter/exit de JarbasAgent::tick + slog no path do eco é o próximo passo
  (s436).
- Nota de transparência: o lab de 30+ min NÃO completou o tempo cheio nos 2
  boots (freeze em ~T+23.4k = ~6,5min de runtime após warm-up). A telemetria
  s435 está provada; a estabilidade de longo prazo segue bloqueada pelo stall
  (3ª forma do residual silencioso: 1º OOM sem diag (s433), OOM com diag
  (s434, resolvido), agora spin SEM OOM pós-job).

**Lições:** (1) "QEMU vivo + log congelado" tem 3 classes distintas (OOM cego,
OOM com heartbeat, spin sem OOM) — o OOM-HALT da s434 provou ser o filtro
perfeito: sem heartbeat = não é oom(); (2) determinismo (2/2 no mesmo T e no
mesmo job) é pista forte de path lógico (echo/cleanup), não race de boot.
