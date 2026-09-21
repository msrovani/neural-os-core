# SESSION_374 — k_ai bughunt AIOS (honesty R2 residual pós-s356)

**Foco:** Premissas ADR-0088 AIOS · SelfHeal · Trust · SGDB · Agency · Safety I1–I4 · ADR-0092 slog.

**Canvas:** [k-ai-bughunt-s374](file:///C:/Users/msrov/.cursor/projects/c-DEV-neural-os-core-latest/canvases/k-ai-bughunt-s374.canvas.tsx)

## Achados → fixes (ordem aplicada)

| Sev | Achado | Fix |
|-----|--------|-----|
| H1+H9 | `status=executed` + notify com bridge off | `ingest_recovery_to_nsgdb(..., status)`; executed só se `push_respawn`/restore OK |
| H6 | slog sev `hw`/`FS`/`BQ`/`update_with_replay` TRACE | `ok`/`warn`/`fail` canónicos |
| H5 | I3 proxy `Pass` | k_ai→`Warning`; hermes sobrescreve com `TRUST_CACHE.entry_count`; `all_pass` só Violation |
| H4 | `check_path` sem regra = allow | Contain/Enforce → deny sem PathRule |
| H2+H3 | ReAct/MCP/Candle/Spawn fingem runtime | DEGRADED/not_wired; `spawn`→`None`; connect false |
| H7 | token budget teatro | `record_tokens` no InferQueue submit OK |
| H8 | Trainer residual FFN+Attn perdido | `dx += residual` pós-rms_ffn e pós-rms_attn |
| H10 | `self_heal_disk` órfão | `pub mod self_heal_disk` |
| M1 | restore bool ignorado | AI/execute_recovery checam `restore_checkpoint()` |
| M4 | `put_hanr` put_doc Err silencioso | propaga Err |
| M5 | dual-write layer=3 hardcoded | `layer_from_key(md/L*|hanr/)` |
| M6 | BQ dim pad silent | `insert`/`insert_f32` → `bool` refuse mismatch |
| M11 | `skill-registry` unused | removido do Cargo.toml |
| M12/M13 | SELF warn sucesso; InferenceFS fake | sev ok; label synthetic/DEGRADED |
| L1/L2 | Ring 1 no lib; orphans overclaim | Ring 2; codemap honesty |

## Não tocado (já honest s356)

- `push_respawn` AtomicPtr; restore bitmap len≠ refuse; put_kv volatile warn
- put_skill_blob Err sem Tickv; inventory MADT; remember_semantic exige emb
- migrate `TargetFoundNotMigrated`; mhi_scheduler no-op

## Upstream / deps

| Crate | Ação |
|-------|------|
| skill-registry | removido (0 use) |
| neural-sgdb | path v1.1.20 — manter |
| x86_64 0.15 | adiar |
| spin/libm/lazy_static | em uso — manter |

## Testes

- `cargo test -p k_ai --lib`: **46 pass / 1 fail flaky** (`ingest_bootlog_cross_boot_order_and_cap` — statics; **PASS isolado**)
- `trainer_self_test_passes` OK pós-residual
- `cargo check -p hermes`: 0 erros

## Residual (não bloqueante)

- ART `.unwrap` em take/child (paths já Some)
- orphans disk: `agency_importer`, `skill_snapshot`, `workflow_learner`, `fine_tuning_pipeline` (docs honesty; não wire stubs)
- `posture_check` sempre true (documentado; gate rede em NetAgent)
