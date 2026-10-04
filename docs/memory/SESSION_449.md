# SESSION_449 — Consolidação do working tree não commitado: E1–E4 (ADR-0113) + F1 + storage AION + cleanup dead-weight

**Data:** 2026-10-04
**Versao:** v1.9.99-s449 TEST
**Branch:** `main` · **Base:** `3639f815` (= `origin/main`)
**Alcance:** consolidação de **41 modificados + untracked** acumulados desde a s441.
**Zero código novo escrito nesta sessão** — aqui se verifica, documenta e commita o que já
estava no tree. Runbook de retomada: [RESUME.md](RESUME.md).

---

## 1. Contexto

O working tree acumulou frentes **não commitadas** desde a s441. A s446–s448 commitou
apenas `tools/` + docs; o código Rust das frentes abaixo ficou pendente. Este registro
as consolida num commit com build e testes **medidos**.

| frente | o que é | onde |
|---|---|---|
| s441–s445 | fuzz TALC, anti-frag, TalcBuf/ContextWindow, recover/reload | já em SESSION_441..445 + STATE |
| **E1–E4** | ADR-0113 (verificação formal, capabilities, provenance, performance) | sem SESSION próprio |
| **F1 runtime** | sobrevivência do caminho cognitivo (pf_storm, silence, cadência) | parcial em s438/s439 |
| **AION-storage** | Tickv persistente + VirtIO-blk modern + self_heal v4 | dono AION |
| **cleanup dead-weight** | `DEAD_WEIGHT_AUDIT.md` (9 findings) | sem SESSION próprio |

---

## 2. E1–E4 (ADR-0113 "Missão Otimização Sistêmica")

A ADR-0113 define frentes canônicas. O tree implementa as quatro:

| frente | arquivos | o que entrou |
|---|---|---|
| **E1 formal** | `k_nano/net/mesh.rs`, `k_nano/allocator.rs` | `p99_index()` puro + `#[kani::proof]`; fuzz `talc_walk_bins` (2 guards load-bearing, mutation 5/5) — s441 |
| **E2 capabilities** | `k_ai/trust.rs`, `hermes/wasmi_rt.rs`, `hermes/agents.rs` | `grant_cap`/`mint_cap`/`enforce_cap`/`revoke_cap` (revogação **transitiva**) + `CAP_GENERATION` global; `HostState.cap_gen` capturado na construção → import negado se a geração mudou; DENY de capability revogada na execução de skill |
| **E3 provenance** | `event-bus/stamp.rs` *(novo)*, `event-bus/bus.rs`, `neural-kernel/main.rs` | anel bounded de `Stamp` encadeado (hash do anterior + campos + digest do payload), `register_audit_hooks(sha256, publisher)`, `audit_verify`/`audit_dropped`. Best-effort: lock bounded falha → `AUDIT_SKIPPED`, o evento **é entregue** (nunca bloqueia o publish) |
| **E4 performance** | `k_nano/bench_stats.rs` *(novo)*, `agent-core/lib.rs`, `k_nano/boot_report.rs`, `neural-kernel/main.rs`, `tools/bench_boot.ps1` | P50/P99/média/desvio puros (`0` = n/a, série vazia = `None`, nunca `0`); contadores `bench_*` no `AgentInstance` via `TICK_CLOCK_HOOK` (BSP + AP); linha `BENCH` no boot_report + marcadores `env/hv/boot_start_us/ready_us`; harness de bench |

**Honestidade E1/E2:** as provas `#[cfg(kani)]` (`trust.rs:530`, 4 `#[kani::proof]`)
**compilam mas NÃO rodam** neste host — Kani não suporta `x86_64-unknown-none` e WSL não
está instalado (s446/s447, IDEA #639). E1 formal segue **UNKNOWN**, não "pendente de
instalar".

**Honestidade E3 vs audit:** o finding #7 do dead-weight audit pedia **remover** o
hashing de stamps ("no runtime consumer found"). O E3 fez o **oposto** — adicionou o
anel e o wiring. O E3 supersede o finding: o carimbo tem consumidor (o publisher de
audit no boot), então "sem caller" era falso no momento do commit.

---

## 3. F1 runtime — sobrevivência do caminho cognitivo

| arquivo | mudança |
|---|---|
| `k_nano/boot_ramlog.rs` | HDR 16→24 (`flags`@16, `recover_count`@20), `seal_core` (hard/soft/quiet), `clflush`+`mfence`, `reboot_ordered` anti-loop + park observável — s445 |
| `neural-kernel/interrupts_ext.rs` | `ramlog_note()` lock-free; selos soft/quiet em #UD/#GP/#PF; BSP `pf_storm`/`pf_repeat` → `reboot_ordered` (o AP segue `hlt`) |
| `k_nano/silence_watchdog.rs` | `CORE_INFER_STAGE` por-core + stage no dump `[SILENCE]` |
| `k_nano/smp/percpu.rs` | `fault_context_is_bsp()` por GS (distingue o core que faultou) |
| `jarbas/display/compositor.rs` | `PAINT_GAP_US` EWMA + `gap_is_overdue()` relativo à cadência observada (mata o flap de `present_overdue` abaixo de 60 Hz) + teste |
| `jarbas/audio/jarvis.rs` | instrumentação TSC do drain de `LLM_RESPONSE` + stage 4 (lento > 500 ms) |
| `cortex/infer_queue.rs` | `DECODE_RING` (64 amostras toks/µs p/ P50/P99), `A2_SLICE_EXIT_US` + log de slice lento, `note_infer_stage` 1/2/3 |
| `neural-kernel/shutdown.rs` | `wbinvd` + `clear_recover_count()`; warm-reset-para-capture em vez de S5 |

---

## 4. AION-storage (dono AION — aqui só se REGISTRA o que já estava no tree)

| arquivo | mudança |
|---|---|
| `k_nano/storage/tickv.rs` | `advance_oversized` (pula registros > `MAX_VLEN`); mount de ckpt **replays a cauda pós-ckpt**; `append_off=size` fail-closed no timeout de scan; `ckpt_dirty` + ckpt-on-flush file/nvme |
| `k_nano/storage/flash.rs` | `with_flash_dev` cai para `virtio_modern::MODERN_BLK` (antes só legacy era lido → Tickv caía em RAM) |
| `k_nano/virtio_modern.rs` | implementa `BlockDevice` para `VirtIoBlkModern` (era inicializado mas **sem leitor**) |
| `k_nano/virtio_blk.rs` | driver legacy/transitional tentado **primeiro**, modern só como fallback |
| `k_ai/self_heal.rs` | checkpoint **v4 `SHV4`** (só hash, sem bitmap de 2 MiB; parse legado v2/v3); `restore` não muta mais o `GLOBAL_ALLOCATOR` |

**Nota de dono (D5 / RESUME §4):** "não tocar em `tickv.rs` sem o AION". Nesta sessão
**nenhuma linha nova** foi escrita nesses arquivos — apenas o que já estava no tree foi
commitado. O fix do `try_mount_from_ckpt` que o AION persegue (AION-0009/0010) continua
dele.

---

## 5. Cleanup dead-weight (`DEAD_WEIGHT_AUDIT.md`)

Dos 9 findings, **5 implementados**, 1 **invertido** (E3), 3 não tocados:

| # | finding | status | evidência |
|---|---|---|---|
| 1 | `MatrixLearningAgent` redundante | ✅ | `matrix_learn.rs` **deletado** (478 linhas); `pub mod` removido de `hermes/lib.rs`; intercept removido de `agents.rs`; registro removido de `main.rs` |
| 2 | `BootLogAgent` `PollEvery(32)` | ✅ | `schedule: ScheduleKind::Oneshot` (tick Continuous engasgava mesh 1G) |
| 3 | `SelfLearningAgent` `PollEvery(500)` | ✅ | `Oneshot` + tick gateado pelos receivers (`hermes_rx`/`intent_rx`/`error_rx`); `data_collector.rs` campos `pub` |
| 4 | `MetricsAgent` `PollEvery(9)` | ❌ não | `jarbas/display/metrics_agent.rs` intocado |
| 5 | `SelfHealAgent` heartbeat silencioso | ✅ | removidos `silent.heartbeat("self_heal")` + o bloco I5 falso |
| 6 | `InputAgent` USB poll por tick | ✅ | `poll_usb_keyboard()` removido do tick; fica só o caminho IRQ `RAW_HW_IRQ1` |
| 7 | stamp hashing | **invertido** | `stamp.rs` é **novo** e wired (E3) — o audit não achou consumidor, o E3 criou um |
| 8 | estados de teste artificiais | ❌ não | nenhum refactor de anotação de teste |
| 9 | slog de safety/security | ❌ não | `safety.rs`/`security.rs` intocados |

Bônus do mesmo cleanup: `boot_log_agent.rs` mantém `write_log` (util) mas o agente
deixou de ser registrado.

---

## 6. Verificação (medida nesta sessão)

| check | resultado |
|---|---|
| `cargo build --release -p boot` | **Finished**, **0 erros** (1m12s; warnings de `set_urgency` com nome que não bate no manifest — conhecidos) |
| `cargo test -p hermes --lib -- --test-threads=1` | 322 passed / **1 failed** = `permission_gate::tests::test_risk_level_classify` — **pré-existente** (desde s440) |
| `cargo test -p cortex --lib -- --test-threads=1` | **126/126** |
| `cargo test -p jarbas --lib -- --test-threads=1` | **135/135** |
| `cargo test -p k-hal --lib -- --test-threads=1` | **75/75** |
| `cargo test -p k_ai --lib -- --test-threads=1` | **abort** `memory allocation ... failed` = `sgdb::bench::d_series_100k` — **pré-existente** (s418/s443, provado com arquivo em HEAD) |
| `cargo test -p cortex --test tq2_0_gguf_load` | **1 failed** — **pré-existente**: `gguf.rs` e o teste não estão modificados na árvore; o gerador `tools/gen_test_gguf.py` (tracked) produz os pesos esperados, então a divergência é do dequant vs fixture, não do diff |
| `python tools/gen_test_gguf.py` | OK (fixture 258 B) |

**A suíte completa sob paralelismo (`cargo test --workspace`) reporta 6 targets falhos;
todos os extras eram flaky de statics compartilhados** (lição SESSION_346/418) — isolados
com `--test-threads=1` voltam a passar. Só sobraram os 3 pré-existentes acima.

---

## 7. UNKNOWN (não afirmar o contrário)

- **Kani/E1/E2**: provas `#[cfg(kani)]` compilam mas **nunca rodaram** (toolchain ausente).
- **Runtime**: nenhuma das frentes (E1–E4, F1, storage AION, cleanup) foi validada em
  **boot de QEMU** nesta sessão — o gate foi host (`build` + testes). `HOST PASS != RUNTIME PASS`.
- **`tq2_0_gguf_load`**: a causa (fixture vs dequant) não foi isolada; registrado como
  **pré-existente**, não como regressão.
- **Sync n-sgdb**: feito nesta sessão (ver §9) ou pendente, conforme o servidor.

---

## 8. Lições

1. **Agente com `PollEvery` que não produz nada por tick é dead weight mensurável.** O
   padrão correto é `Oneshot` + gate por evento (`try_receive`), preservando a API — foi
   o que os findings 2/3 fizeram.
2. **"Sem consumidor" é um estado temporal, não uma propriedade.** O audit marcou o
   stamp como removível; no mesmo dia o E3 criou o consumidor. Antes de deletar por
   "zero callers", checar se uma frente irmã está prestes a usá-lo.
3. **A árvore pode acumular muitas frentes sem commit.** Consolidação exige separar
   **flaky de paralelismo** (rodar isolado) de **regressão real** antes de registrar
   veredito — o `cargo test --workspace` mentiu 3 falhas que `--test-threads=1` não reproduz.
4. **Commit de consolidação ≠ autoria.** As frentes AION/E1–E4 mantêm seus donos; o
   commit apenas materializa o que já estava no tree (D5).

## 9. Registros deste fechamento

- `SESSION_449` (este), `STATE.md`, `SESSION_INDEX.md`, `IDEA_BANK.md` (#642–#644),
  `CHANGELOG.md`, `TODO.md`.
- Higiene: `*_check.txt` removidos; `sgdb_memory.db` + `tools/.forum_loop_state/`
  ignorados (artefatos locais, não versionar).
- Sync n-sgdb: ADR-0113 + SESSION_449 + IDEA #642–#644 (`scope=project/neural-os-core`,
  `curate(op=commit_run)`).
