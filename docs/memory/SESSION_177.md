# SESSION_177 — F-series ROI (pós-v1.9.9)

**Data:** 2026-07-23  
**Ordem:** ROI × viabilidade bare-metal (skip 7–9)

## Feito
| # | Item | Evidência |
|---|------|-----------|
| 1 | Smoke e2e L1→ckpt→remount→get | `k_ai::sgdb::memory_checkpoint_e2e_smoke` + boot log |
| 2 | SleepCycle 1 impl | `hermes::agents::SleepCycleAgent`; bin `pub use` |
| 3 | Pseudo-emb sem BGE | `embed_or_pseudo`; log `emb=pseudo` |
| 4 | Rescore FP32 top-k BQ | `recall_semantic` path `bq+fp32` |
| 5 | Anotar a491cea sem force | CHANGELOG + esta SESSION; mensagem consciente no commit F |
| 6 | Tirar plan do tree | `git rm` `.cursor/plans/hw_boot_reboot_fix_*.plan.md` + ignore |
| 10 | GitHub Release v1.9.9 | notas em `docs/releases/v1.9.9.md`; **`gh` ausente neste host** — criar com `gh release create` após instalar CLI |

## Skip (honesty)
7 page-hash wear TicKV · 8 port crates tickv/noproto · 9 HNSW/DoD 10M

## Gate
`cargo check --release -p k_ai -p hermes -p neural-kernel --features fat-boot-log` = 0

## Commit
`2282e15` — F-series ROI (main ahead 1 vs origin; push + `gh release` pendente no maintainer)