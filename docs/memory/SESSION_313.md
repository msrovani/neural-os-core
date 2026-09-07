# SESSION 313 — hermes + jarbas Unification (FASE 1-7)

**Sprint:** v1.9.99-s319
**Date:** 2026-09-07
**Status:** ✅ COMPLETE

## Problema

Análise profunda de jarbas (65 módulos) + hermes (55 módulos) revelou:
- Emotion Analysis duplicada (hermes keyword-based vs jarbas 16-feature classifier)
- Soul/Personality duplicada (hermes SoulEngine vs jarbas SoulProfile)
- Voice emotion não alimenta affect pipeline
- LoopPhase não afeta display rendering
- Session HNSW não populado no boot

## Dependência Chain

```
k_nano (R0) → k_hal (R1) → cortex (R2) → k_ai (R2) → hermes (R3) → jarbas (R3)
```

**hermes é a camada INFERIOR. jarbas depende de hermes.**

## Plano: 7 FASEs

### FASE 1: Verify Build ✅
- Build passa após revert de mudanças circulares

### FASE 2: Unify Emotion ✅

| Task | Change | File |
|------|--------|------|
| hermes canonical | Adicionado `Sarcasm` variant | `hermes/src/emotion.rs` |
| jarbas delegation | `jarbas::audio::ser/context/jarvis` usam `hermes::emotion::Emotion` | `jarbas/src/audio/*.rs` |
| Remove duplicate | `jarbas::jarvis::EmotionAnalysis` removido | `jarbas/src/jarvis.rs` |

### FASE 3: Unify Soul ✅

| Task | Change | File |
|------|--------|------|
| delegation | `SoulProfile::from_hermes()` construtor | `jarbas/src/jarvis.rs` |
| default | `default_jarbas()` delega para `hermes::soul::SoulEngine::default()` | `jarbas/src/jarvis.rs` |

### FASE 4: Wire SER → Affect ✅

| Task | Change | File |
|------|--------|------|
| publish | `VOICE_EMOTION` event com valence | `jarbas/src/audio/voice.rs` |
| consume | Subscreve `VOICE_EMOTION`, atualiza `AffectRegulator` | `hermes/src/agents.rs` |

### FASE 5: Wire LoopPhase → Display ✅

| Task | Change | File |
|------|--------|------|
| subscribe | `LOOP_PHASE` receiver em DisplayAgent | `jarbas/src/display/agent.rs` |
| static | `COGNITIVE_PHASE` para renderização adaptativa | `jarbas/src/display/agent.rs` |

### FASE 6: Remove Dead Code ⏭️
- `uvc_driver` e `gpu` são usados extensivamente — mantidos

### FASE 7: HNSW Boot Populate ✅

| Task | Change | File |
|------|--------|------|
| populate | `session_load()` popula HNSW no boot | `hermes/src/cognitive_bridge.rs` |

## Arquivos Modificados

| Arquivo | LOC± | Mudança |
|---------|------|---------|
| `hermes/src/emotion.rs` | +2 | Sarcasm variant |
| `hermes/src/agents.rs` | +18 | VOICE_EMOTION consumer |
| `hermes/src/cognitive_bridge.rs` | +11 | HNSW boot populate |
| `jarbas/src/jarvis.rs` | +14 | SoulProfile delegation, Emotion removed |
| `jarbas/src/audio/ser.rs` | +2 | hermes::emotion import |
| `jarbas/src/audio/context.rs` | +2 | hermes::emotion import |
| `jarbas/src/audio/jarvis.rs` | +7 | hermes::emotion::EmotionAnalyzer |
| `jarbas/src/audio/voice.rs` | +19 | VOICE_EMOTION publish |
| `jarbas/src/display/agent.rs` | +7 | LOOP_PHASE subscription |

## Build

```
cargo clean -p neural-kernel && cargo check --release — 0 errors
```

## Commits

| Commit | Description |
|--------|-------------|
| `038eb9ba` | FASE 1-7: Unify Emotion, Soul, wire SER→Affect, LoopPhase→Display, HNSW boot |
