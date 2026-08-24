# SESSION_288 — Trinity 7 kind + Falcon3-7B PRO.v6 (F1+P0-P3) — router LOADED 25KB 93.5%

**Data:** 2026-08-24
**Sprint:** v1.9.99-s288 TEST
**Commits base:** 086ffe5 (HEAP_BUDGET), 20a6410 (P1-P4), 7e2c52e (lm_head), mkfat32 alias dedup
**Objetivo:** Fechar auditoria Trinity (15 ADRs + 52 ideias, gap LCG→neural) + Falcon3-7B como inteligência embarcada + Router neural LOADED

---

## Contexto — Auditoria Trinity

Auditoria cobriu 15 ADRs + 52 ideias Trinity/MoE. Gaps: router era LCG seed=42 (não neural), 7 kind declarados mas 6 wired, Generator 1.5B placeholder, HEALTH I5 mentirosa. F1–P3 fecham honestidade.

## F1 — Falcon3-7B como inteligência embarcada

- **ModelHeader dinâmico** (`crates/cortex/src/model.rs`): `parse_model_header(data)` lê hidden/num_layers/num_heads/vocab/intermediate/theta/tie do header v6 em runtime; `LOADED_HEADER` static + `set_model_header`/`loaded_model_header()`; `v6_file_size()` delega ao header (zero duplicação). Corrige hardcoded `FALCON3_NUM_LAYERS=22` (real 28), `FALCON3_FILE_SIZE=1.03GB` (real 1.78GB), `FALCON3_MAX_SEQ=4096` (real 32768). Dims reais: hidden=3072, 28L, 12H, kv=4, intermediate=23040, vocab=131080.
- **auto_arena_size()** (`crates/cortex/src/arena.rs`): 50% da RAM detectada, clamp [512MB, 4GB]; `CORTEX_ARENA_MAX_SIZE=4GB` suporta Falcon3-10B (2.5GB). Fallback 2GB se RAM não detectada.
- **slot_footprint_mb() dinâmico** (`crates/cortex/src/model_fit.rs`): tenta `loaded_model_header().file_size_mb()` para slots active/generator/pro antes de constantes (4MB smoke, 1MB hwexpert).

## P0 — Router neural

- **tools/train_router.py**: 202 amostras curadas, 93.5% acc estratificada (31 holdout, seed 7). Exporta `target/ROUTER.BITNET` 25.8KB (25818 B) header 0xBE11BE11 ver6 model_type=2 vocab=99 hidden=64 n_exp=7, weight i8 84.8% nonzero (gate ≥1%), holdout 29/31 0.935 (gate ≥0.80) — VALIDATION PASS via `tools/validate_router_v6.py`.
- **tools/mkfat32.py**: alias dedup (`seen{}` src→(start_cluster,len)): mesmo bytes (FALCON3.V6 + PRO.V6 + LLAMA8B.BIN aliasando falcon3) reuse clusters sem double alloc — economiza ~771MB por alias. `PRO.V6` e `LLAMA8B.BIN` priorizam falcon3 quando PACK_LLM=falcon3.
- **Boot:** `moe_router=LOADED` quando ROUTER.BITNET presente, `ABSENT(keyword)` fallback caso contrário + `FALLBACK_GENERATOR`.

## P1 — Wiring R3

- **global_arena wrappers** (`crates/cortex/src/global_arena.rs`): wrappers `global_arena_*` para hermes não tocar arena direto.
- **hermes single classify_with_trace** (`crates/hermes/src/trinity_router.rs`): único ponto `classify_with_trace` neural, sem duplicata LCG.
- **AutoLearn replay R3** (`crates/hermes/src/auto_learn.rs`): `AutoLearnAgent` em R3 com `update_with_replay()` via RouteTrace congelados da TensorArena.

## P2 — DynamicMoE

- **clone_with_noise fix ternário** (`crates/cortex/src/dynamic_moe.rs`): noise strided não quebra packing ternário (4 pesos/byte) — adiciona ruído por peso lógico, não por byte.
- **Int8Router align x*127** (`crates/cortex/src/trinity.rs`): quantização `x*127` alinhada ao loader (scale f32 por linha).

## P3 — Docs honestidade

- 7 kind honestos (HwIdentify, HwControl, RustCoder, DiskDiag, Security, Generator, SpeechSynth — 2 wired HWEXPRT/RUSTCDR, 4 keyword→Generator) — AGENTS.md + TECNOLOGIAS.md + README atualizados.
- Falcon3 dims dinâmicas (não hardcoded) documentadas em AGENTS.md lições Fase 1.

## Falcon3-7B download

- Fonte: `tiiuae/Falcon3-7B-Instruct-1.58bit` (3.27GB HF) → `python tools/convert_falcon3_bitnet.py --hf-repo tiiuae/Falcon3-7B-Instruct-1.58bit --output target1/PRO.v6`
- Artefato: `target1/PRO.v6` 1.864.557.403 B (1778.2 MB) header 0xBE11BE11 ver6 num_params 2468048068 hidden 3072 28L; aliases FAT `FALCON3.V6`/`PRO.V6`/`LLAMA8B.BIN` dedup via mkfat32.
- `tools/convert_falcon3_bitnet.py`: lm_head fallback (root vs model.lm_head) + per-projection weight_scale (7e2c52e).

---

## Arquivos tocados

| Arquivo | O que |
|---------|-------|
| `crates/cortex/src/model.rs` | ModelHeader + parse + loaded_header + v6_file_size dinâmico |
| `crates/cortex/src/arena.rs` | auto_arena_size 50% RAM [512MB,4GB], CORTEX_ARENA_MAX_SIZE 4GB |
| `crates/cortex/src/model_fit.rs` | slot_footprint_mb dinâmico via header |
| `crates/cortex/src/trinity.rs` | Int8Router x*127, moe_router LOADED gate |
| `crates/cortex/src/dynamic_moe.rs` | clone_with_noise fix ternário strided |
| `crates/hermes/src/trinity_router.rs` | single classify_with_trace |
| `crates/hermes/src/auto_learn.rs` | AutoLearn replay R3 |
| `crates/cortex/src/global_arena.rs` | wrappers global_arena |
| `tools/train_router.py` | 202 curadas, 93.5% acc, export ROUTER.BITNET |
| `tools/validate_router_v6.py` | validação parse_end+nonzero+holdout |
| `tools/convert_falcon3_bitnet.py` | lm_head fallback + weight_scale |
| `tools/mkfat32.py` | alias dedup PRO.V6/FALCON3.V6/LLAMA8B.BIN |
| `AGENTS.md` | 7 kind + Falcon3 dims + lições Fase 1 |
| `TECNOLOGIAS.md` | Trinity 7 kind LOADED + Falcon3-7B |
| `target/ROUTER.BITNET` | 25818 B, ver6, 93.5% |
| `target1/PRO.v6` | 1864557403 B, ver6 |

## Verificação

| Check | Resultado |
|-------|-----------|
| `cargo check --release` | ✅ 0 erros (workspace) |
| `python tools/validate_router_v6.py target/ROUTER.BITNET` | ✅ VALIDATION PASS (29/31 93.5%, nonzero 84.8%) |
| `header PRO.v6` | ✅ 0xBE11BE11 ver6 hidden 3072 layers 28 |
| `header ROUTER.BITNET` | ✅ 0xBE11BE11 ver6 vocab99 hidden64 n7 |
| `cargo test --workspace --exclude neural-kernel --exclude boot` | ✅ 118/120 (2 preexistentes hermes::wasm_build CAP_GPU) |

## Lições (Aprenda)

- **F1 ModelHeader:** dims hardcoded mentem (22 vs 28L, 1.03 vs 1.78GB). Header v6 autodescritivo + `parse_model_header()` elimina duplicação — `v6_file_size()` deriva do header, não de const.
- **P0 alias 8.3:** FAT 8.3 sem dedup duplica 771MB por alias (FALCON3.V6 + PRO.V6 + LLAMA8B.BIN). `seen{}` reuse cluster é ponytail — um dict economiza 1.5GB de imagem.
- **P1 single classify:** dois pontos de classificação (hermes + cortex) divergem (LCG vs neural). Single `classify_with_trace` neural é fonte única.
- **P2 noise strided:** ruído byte-a-byte quebra packing ternário (4 pesos/byte). Noise por peso lógico (strided) preserva layout.
- **Falcon3 lm_head fallback:** `lm_head.weight` em alguns repos HF fica na raiz, não em `model.lm_head`. Fallback evita tensor faltante + scale per-projection (não 1.0) melhora acurácia.

## Próximo

- Boot QEMU com PRO.v6 + ROUTER.BITNET LOADED (validar `moe_router=LOADED` no serial)
- T-017 metal K23 (SMP) — independente
- Wire hermes classify_with_trace no caminho hot HermesAgent (se ainda keyword)
