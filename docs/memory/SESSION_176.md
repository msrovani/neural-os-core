# SESSION_176 — SGDB Memory Quality Jump (E1–E5)

**Data:** 2026-07-23  
**Plano:** `sgdb_memory_quality` (não editar o plan file)  
**Pesquisa:** Tock TicKV, NoProto, ART (Leis), Elastic/Qdrant BQ+SIMD

## Diagnóstico pré-série
D-series tinha despacho/ckpt/bench; memória AIOS ainda frágil: SleepCycle ignorava SGDB; L4 BQ só demo; L1 RAM sem flush.

## E1 SleepCycle ↔ SGDB
- `checkpoint_working()` / `prune_working_ram()`
- CONSOLIDATE → flush L0/L1 + compact; PRUNE → limpa arena RAM
- Wire em `hermes` + `neural-kernel` SleepCycleAgent

## E2 Hermes recall
- `recall_semantic` / `remember_semantic` / `remember_fact`
- Prompt: TF-IDF + BQ L4 hybrid; log `[sgdb] recall=tfidf|bq|hybrid|…`
- `memory_store::remember` → L3 `ts/…` + HANR

## E3 TickvLite Valid-flag
- Magic `TKL` + byte3 `V`/0 (invalidate in-place)
- `invalidate_key` antes de overwrite; recover pula V=0
- Honesty: ≠ TicKV page-fit 2037B

## E4 Índices
- ART Node16: `_mm_cmpeq_epi8` se `allow_avx2`
- Hamming AVX-512: VPOPCNTDQ se CPUID ECX.14; senão XOR+ZMM

## E5 NMD1 patterns + docs
- `MemoryDoc::patch_payload` + `sortable_ts_key`
- ADR Visão vs Ship + Pesquisa→aplicação; TECNOLOGIAS; IDEA

## Gate
`cargo check --release -p k-nano -p k_ai -p hermes -p neural-kernel --features fat-boot-log` = 0

## Residual
crates tickv/noproto; HNSW; SQL; AEAD; DoD 10M/100k; kill-9 HW

## Pós-release — commit intermediário (não perder)

**Fato:** `main` `a491cea` (tag `v1.9.9`) **não** foi um “agente SGDB” commitando de propósito.
Foi **auto-checkpoint do Cursor** ao checkout da branch `cursor/compute-dispatch-smp-gpu-npu-ff0d` (ADR-0059).

| Efeito | Detalhe |
|--------|---------|
| Engoliu | Todo D/E-series ainda uncommitted (conteúdo certo) |
| Engoliu também | `.cursor/plans/hw_boot_reboot_fix_7972b37b.plan.md` (plano HW soft-reboot — outro tema) |
| Mensagem | `checkpoint before checking out …` (ruim; author = git local) |
| Tag | `v1.9.9` → `a491cea` (após retarget; conteúdo SGDB ok) |

**Utilidade desta nota:** evita panic (“perdi o trabalho”), evita force-rewrite de `main`, e ensina o ritual antes de trocar de branch/agente.

**Ritual anti-perda (fazer sempre):**
1. `git status` — se dirty, **commit nomeado** ou stash **antes** de checkout de outra branch/agente.
2. Não deixar série grande só em working tree + “commit depois”.
3. Após checkpoint espúrio: conferir `git show HEAD --stat`; se conteúdo ok, só documentar / commit de anotação; **não** `reset --hard` sem backup.
4. Tag: `git show vX.Y.Z` deve apontar pro commit da feature, não pro tip de outra branch.
5. Plans em `.cursor/plans/`: ou commit consciente, ou `.gitignore` — senão o checkpoint mistura temas.
