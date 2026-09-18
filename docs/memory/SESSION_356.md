# SESSION_356 — k_ai análise + bughunt profundo (R2 honesty)

## Goal
Mapear papel/premissas do `k_ai` (Ring 2 autonomia) e bughunt amplo: SelfHeal, Trust, Safety, Security, SGDB/NSGDB, Audit, Agency, learn.

## Papel (resumo)
R2: SelfHeal (checkpoint best-effort), TrustCache tipo `(token,agent,skill)`, Agency/seed, SGDB cognitivo (Tickv+ART/BQ+neural-sgdb), boot_observe Escalate≠Auto, Safety I1–I4 + detectores, SelfLearning/cognitive. Deps: `k_nano`+`cortex`+`k_hal`; sem hermes. Wire: hermes possui `TRUST_CACHE`/SecurityPipeline/SafetyAgent; bin registra bridges.

## HIGH corrigidos
| Bug | Fix |
|---|---|
| `push_respawn` `read_volatile` no código da fn | `AtomicPtr<()>` + transmute do endereço |
| `migrate_to_another_disk` → `Ok` sem migrar | `TargetFoundNotMigrated` + warn |
| `semantic_snapshot` `[u8;2MiB]` na stack | `Vec` heap |
| AI JSON `+11/+10` off-by-two | parser quoted string |
| AuditTrail wrap: prev=`last()`, verify ordem Vec | prev cronológico + verify/last_n alinhados |
| PortScan ignora portas; PingFlood msg=0; Arp unbounded | unique ports; count antes zero; dedup |
| restore bitmap curto ainda `true` | refuse se len≠BITMAP_SIZE |
| slog `HEAL`/`n2`/`init`/`recall` → TRACE | `ok`/`warn`/`fail` |
| `posture_check` `crate::net` fantasma | removido |
| I2 `agent_count=0` Violation | Warning até scheduler |
| I3 Trust sempre Pass sem honesty | warn proxy + docs |
| `put_skill_blob` Ok sem Tickv | `Err` + warn |
| `put_kv` RAM silencioso | warn rate-limited; `status` `volatile=` |
| `from_khal` cpu=1 | MADT `BOOT_APIC_IDS` |
| `k_ai::safety_invariants` morto | SafetyAgent hermes chama `check_all` |
| LIGHT defer `HEAVY_DONE=true` + skip ingest | keep deferred; ingest + publish_boot_ai |
| `save_checkpoint` valid sem bitmap | abort + `valid=false` |
| Tickv adapter `Flushed` em RAM | `Buffered` se backend=ram |
| L4 texto → BQ f32 | só payload tipado ≥8 dims |

## Verify
- `cargo check -p k_ai -p hermes` — 0 erros
- `cargo test -p k_ai audit_ --lib` — 2/2 ok

## Aberto (residual)
- TransformerTrainer backward residual skip + unwraps hot path
- BQ Hamming dim mismatch era; ART unwrap → fail-closed
- Token budget teatro (`record_tokens` sem callers)
- Agency::new vazio sem DEGRADED wire seeds
- Trust Observe allow + PathRule default-allow (sem callers)
- Dual-write put_hanr / put_doc sync layer hardcoded

## Lições
1. Bridge de fn: nunca `read_volatile` no endereço do código — transmute o valor da fn.
2. `Ok`/done com índices frios ou migração não executada = honesty falsa (classe timeout=success).
3. slog sub desconhecido (`HEAL`,`n2`) = TRACE mudo — contrato ok\|warn\|fail.
4. Bitmap 2 MiB nunca materializa na stack (SESSION_290 / Checkpoint).
5. `"action":`.len()==9; +11 quebra JSON compacto do LLM.
