# BEI BUGHUNT CANVAS — ADR-0060 (SESSION 394)

Escopo: BitNet Ecosystem Intelligence (8 ondas, ~2.9K LOC) + dependências/ideias portadas.
Critério: High = quebra/risco real de boot/scheduler/honestidade; Med = funcionalidade morta; Low = cosmética.

## HIGH (11)

| # | Arquivo | Problema | Fix aplicado |
|---|---|---|---|
| H1 | `k_nano/src/sync/mpmc.rs:64,83` | spin infinito em `try_send/try_recv` (CAS sem budget) — stall do scheduler | MAX_RETRIES=64 + revalidação pós-CAS + wrapping ops |
| H2 | `cortex/src/moe.rs:446-462` | `merge_pair` só media bias; pesos de `b` ignorados (merge = no-op) | média por-posição via `get_weight`/`pack_weights` |
| H3 | `cellular.rs`/`evolution.rs` | `observe()` nunca chamado → GROW/PRUNE mortos | `feed_plasticity()` 1x/tick por região com dado real |
| H4 | `hermes/src/bei.rs:344+` | lifecycle MoE lifecycle morto (experts vazio) | gate `!dmoe.base.experts.is_empty()` |
| H5 | `k_ai/src/economy.rs` | `pressure()` sempre 0 (ninguém chama `allocate`) | derivado de `heap_observe()` (1.0 − headroom/window), fail-closed 1.0 |
| H6 | economy/agent-core | 3 BudgetManager coexistindo sem fio | comentário ponytail + TODO convergência (sem refactor) |
| H7 | `hermes/src/memory/mod.rs` | L0 `clear()` todo tick; `auto_promote` stub; L7 sem teto | TTL sweep cobre L0; auto_promote real; L7 cap 1 GiB+evict |
| H8 | `executive.rs`/`bei.rs:293` | `ProceedWithBudget: 10` logado **todo tick** (spam serial) | static LAST_BUDGET — log só se muda |
| H9 | `bei.rs:255+` | supervisor sem entradas (`record_result` nunca chamado) | gap documentado (sem inventar métrica) |
| H10 | `k_nano/src/mpmc.rs` | duplicata MPMC divergente (207L, 0 callers) | virou facade `pub use sync::mpmc::*` |
| H11 | `hal_offer.rs`/`memory_store.rs` | sev fora do contrato ADR-0092 (Trace mudo p/ Absent/Quarantined) | Absent→warn, Quarantined→warn, bind-fail→fail, Bound→ok; "hanr"→ok/warn |

## MED (12, 22 originais)

- M12 `bei.rs`: separado bloco state-tick (M12 parcial — mesh continua no mesmo hook por design, comentado).
- M13 wave3: log honesto "DEGRADED (zero_weights scaffold)".
- M14 PromoteSkill: string "(dummy stub até #412)".
- M15 init: `ok "BEI 8 waves up (connect PARTIAL — wire no tick)"` (era warn DEGRADED sobre sucesso).
- M16 `cellular.rs`: `budget_per_tick` respeitado (`used_this_tick` cap).
- M17 `sleep_cycle()` chamado a cada 64 ticks ociosos.
- M18 `moe.rs flush_all`: ordem merges→splits→births.
- M19 `memory/mod.rs write`: `used` incremental (O(n²)→O(n)).
- M20 `bei.rs`: early-gate `low_mem` (headroom<48) nas seções 1/2/5.
- M21 `bei.rs`: lock-order pc→net coletado antes de tocar net.
- M22 `hal_offer.rs`: cache de Absent reusa `ABSENT_BACKOFF` canônico do k_hal (sem cache duplicado).

## LOW

- `sync/mpmc.rs:91` len() → `saturating_sub` (evita underflow transiente).
- `memory/mod.rs` evict_one tie-break `Reverse(access_count)` (antes evictava o mais acessado).
- `economy.rs` temperature → `_temperature` (write-only).
- `cellular.rs` doc \"Blocked é scaffold\".
- 7 vs 8 waves ADR: documentado aqui (doc drift, não código).

## Atualizações externas (lib-1)

| Solução | Repo | Upstream | Ação |
|---|---|---|---|
| wasmi 2.0.0 | = | 2.0.0 final | **feito**: feature `portable-dispatch` já estava ligada — verificado em check |
| smoltcp | 0.14.0 | 0.14.0 | no-op; TODO documentado: `poll_delay()` em vez de spin (fora do escopo BEI) |
| spin | 0.12.3 | 0.12.3 | no-op |
| talc | 4.4.3 | 5.0.4 | skip major (API quebra) — TODO IDEA_BANK |
| x86_64 | 0.14.13 | 0.15.x | bloqueado por nightly pin (Step trait) |
| virtio-drivers | 0.13 | 0.13 | no-op |
| BitNet TL2 | idea | kernels 1.15–2.1x | IDEA_BANK (port futuro, sem deps) |

## Não-corrigidos / honestidade

- H9 é gap de design: supervisor consome de fonte inexistente (não inventado dado falso). Caminho real = ligar `record_result_full` nas bordas de inferência (fora do escopo do bughunt).
- `talc` 5.x, `x86_64` 0.15: upgrades major bloqueados por semver/toolchain; registrados.
- Árvore estava dirty de sessão anterior (MHI canvas/SESSION_393 não finalizados) — preservados intocados.
- 2 lanes fixer morreram no meio de `bei.rs` (delimitador quebrado + `phase_deg` órfão) — reparados pelo orquestrador antes da lane final.

## Validação

`cargo check -p hermes -p jarbas` 0 erros; `cargo check --release` (workspace) 0 erros.
Pré-existente fora do escopo corrigido pelo orquestrador: `agents.rs:905` path `crate::log_analyst_agent` → `log_analyst_agent`.
