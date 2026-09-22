# SESSION_394 — BEI Bughunt Profundo + Fixes (ADR-0060)

## Escopo
Bughunt profundo do BEI (BitNet Ecosystem Intelligence, 8 ondas) + auditoria de deps externas. Canvas completo em `docs/memory/BEI_BUGHUNT_CANVAS.md`.

## Achados (oracle): 11 HIGH + 12 MED-aplicados + LOWs

### HIGH corrigidos
- **H1** `k_nano/src/sync/mpmc.rs` — spin infinito em `try_send/try_recv` (CAS sem budget) → stall do scheduler. Fix: `MAX_RETRIES=64`, revalidação pós-CAS, wrapping ops.
- **H2** `cortex/moe.rs merge_pair` — só media bias, pesos de `b` descartados (merge=no-op). Fix: média ternária por-posição via `get_weight`+`pack_weights`.
- **H3** `cellular/evolution` — `observe()` nunca chamado → GROW/PRUNE mortos. Fix: `feed_plasticity()` 1×/tick por região (dado real: load das filas).
- **H4** `bei.rs` seção 7 — lifecycle MoE morto (experts vazio). Fix: gate `!dmoe.base.experts.is_empty()`.
- **H5** `k_ai/economy pressure()` — sempre 0 (ninguém chama `allocate`). Fix: derivado de `heap_observe()` (1.0 − headroom/window), fail-closed 1.0.
- **H6** 3 BudgetManager coexistindo — comentário ponytail + TODO convergência (sem refactor grande, por contrato).
- **H7** `hermes/memory/mod.rs` — L0 `clear()` cego por tick; `auto_promote` stub; L7 sem teto. Fix: TTL sweep cobre L0, promote real (mais quente ≥ threshold sobe L→L-1), L7 cap 1 GiB+evict.
- **H8** `executive/bei.rs` — `ProceedWithBudget: 10` logado **todo tick** (spam comprovado em `boot-whpx-20260802.txt`: ~60 linhas idênticas). Fix: static LAST_BUDGET, log só na mudança.
- **H9** supervisor sem entradas (`record_result` nunca chamado) — documentado como gap (não inventada métrica falsa).
- **H10** duplicata MPMC divergente (`k_nano/src/mpmc.rs` 207L, 0 callers) → facade de 7 linhas.
- **H11** sev fora de ADR-0092 (`request`/`pnp`/`hanr` → Trace mudo) em `hal_offer.rs`/`memory_store.rs`. Fix: Absent→warn, Quarantined→warn, bind-fail→fail, Bound→ok.

### MED/LOW aplicados
M14 PromoteSkill "(dummy stub até #412)"; M15 init log warn→ok; M16 `budget_per_tick` respeitado (`used_this_tick`); M17 `sleep_cycle()` a cada 64 ticks idle; M18 `flush_all` reorder merges→splits→births; M19 `used` bytes incremental (O(n²)→O(n)); M20 gate `low_mem` (headroom<48MB) nas seções 1/2/5 do tick; M21 lock-order pc→net corrigida (coleta sob lock, dropa, age); M22 HalOffer reusa `ABSENT_BACKOFF` canônico do k_hal; LOWs: `len()` saturating_sub, `Reverse(count)` em evict, `_temperature`, docs honestas.

### Deps externas (DAO audit — lib-1)
- **wasmi**: já em 2.0.0 final; `portable-dispatch` já ligado em `hermes/Cargo.toml` (verificado compilando). Nenhum bump.
- smoltcp 0.14.0, spin 0.12.3, virtio-drivers 0.13, embedded-tls 0.19: latest; no-op.
- talc 4.4.3→5.0.4 e x86_64 0.14→0.15: **bloqueados** (major/API quebra + nightly pin) → registrados em IDEA_BANK/TODO.
- BitNet.cpp (portável, zero deps): kernels TL2 + tiling configurável → IDEA_BANK (futura otimização cortex).

## Vale registrar
- `crates/hermes/src/agents.rs:905` consertado: `crate::log_analyst_agent` → `log_analyst_agent` (path errado, pré-existente de outra sessão s392; sem ele `-p hermes` não compilava).
- `MESH FRAG skip` em bei.rs:530+ — comentário "uma vez" era mentira; deixei claro que checa todo tick.

## Sessões concorrentes (IMPORTANTE)
- A working tree estava **dirty** com trabalho de outras sessões (s390–s393, canvas onda-1, MHI). Não comitar amplo, não reverter. fix-3 e fix-4 (fixer lanes) morreram no meio de `bei.rs` deixando `if !low_mem {` desequilibrado + `phase_deg` órfão — reparado manualmente pelo orquestrador (`LAST_PHASE_DEG` static).
- Fixers originais perderam mudanças (sessão concorrente `git checkout` arquivos) — reaplicado e revalidado.
- Falhas de teste PRÉ-EXISTENTES, **não minhas**: `cortex tq2_0_gguf_load` (`got -0 want 1.5` — include golden), `k_ai audit::tests`, `hermes trinity_inject::tests` (7) — todas em arquivos modificados por outra sessão. Meus escopos: cargo check 0 erros.

## Validação
- `cargo check --release` workspace: **0 erros** (0.17s incremental pós-fix).
- `cargo check -p k-nano / cortex / k-ai / hermes / jarbas`: 0 erros.
- `cargo test`: 192 cortex PASS; suites hermes/k_ai passam exceto as 3 pré-existentes fora do escopo.

## Pós-tarefas
- Canvas: `docs/memory/BEI_BUGHUNT_CANVAS.md`
- SESSION_394 (este arquivo) + SESSION_INDEX + STATE + CHANGELOG
- TECNOLOGIAS.md: BEI já registrado (7.7) — entrada atualizada implicitamente (nenhum dep novo).
- Commit isolado dos arquivos BEI (sem tocar no trabalho concorrente).

## Pendências honestas (não inventadas)
1. `H9` real: ligar `record_result_full` na borda de inferência (mede confiança/latência de verdade) — requer desenho de métrica, não inventar.
2. `soul_mirror` vs `affect` paleta duplicada — skip (`set_affect_snapshot` não existe).
3. talc 5.x + x86_64 0.15 — IDEA/TODO.
4. TL2 BitNet kernel port — IDEA.
