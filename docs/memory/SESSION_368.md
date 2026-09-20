# SESSION_368 — Fechamento 1.x: B1 CI verde + B3.1 pacing + gate checklist

**Data:** 2026-09-19  
**Plano:** `docs/plans/2026-09-19-fechamento-1x.md` (aprovado maintainer)

## Delivered

### B1 — Host workspace 0 fail
- Fixture: `python tools/gen_test_gguf.py` → `target/test_tq2_0.gguf`
- Flakes parallel:
  - `difficulty_gate::TEST_LOCK` + `*_inner` (soft_stride)
  - `boot_observe` INGEST_TEST_LOCK
  - `mhi` RATE_TEST_LOCK + `migration_rate_reset_for_test`
- **Bug real:** `apply_one_layer` refuse não invalidava `x` → `forward_with_kv` continuava, pad soft_stride + `advance` → KV desalinhado (`layer 0 len=0`). Fix: poison `*x = Tensor::zero((0,0))` nos refuses pré-append.
- Aceite: `cargo test --workspace --exclude neural-kernel --exclude boot --no-fail-fast` → **0 fail** (`logs/host_workspace_b1f.txt`)

### B3.1 — Playback pacing (ADR-0045 V1)
- `jarbas::audio::mixer::compute_mixer_want(free, ring, max, dt_us, rate)` — free HDA ∩ TSC Δt×16 kHz ∩ 320/tick
- 5 testes host; METRIC `PLAY_SAMPLES_DROPPED` quando sobe
- Aceite QEMU drops==0 em /tick 30/60/120 = residual lab

### A0.1 — STATE gate checklist
- `docs/memory/STATE.md` s368 com tabela Onda 0–3 + metal quente + Trilho B

## Não feito (próximo)
- A2/A3 metal operador + Onda 3 OTA lab
- B3.2 duplex / STT_UNCERTAIN
- Mesh peer B relaunch

## Files
- `crates/cortex/src/{difficulty_gate,bughunt_s353,cortex}.rs`
- `crates/k_ai/src/boot_observe.rs`
- `crates/k_nano/src/mhi.rs`
- `crates/jarbas/src/audio/mixer.rs`
- `docs/memory/STATE.md`
- `docs/plans/2026-09-19-fechamento-1x.md`
