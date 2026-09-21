# SESSION_383 — Plano IDEA: #600 CapGate DynSkill + #562 MicFrameRing + #602 + spin 0.12

**Data:** 2026-09-21  
**Sprint:** v1.9.99-s383  
**Pista:** executar plano pós-análise últimas 25 IDEA_BANK

## Entregue

### Fase 0 — Higiene IDEA
- #558/#559/#560/#561 reconciliados (✅ / parcial + sub-IDEAs residuais)
- #542 linha antiga enxuta; #606 residual atualizado

### Fase 1 — #600 CapGate DynSkill ✅
- `skill-registry::DynamicSkill` → `required_tokens: [DYNSKILL_TOKEN=0xD1]`
- `hermes::dynskill::register_dynskill` = registry + `trust_allow(0xD1, name)`
- skill_sync / evolve / skill_opt usam register
- HermesAgent `execute_skill`: remapeia Legacy(1)→0xD1 **só** se trust já concedido
- CapGate: 0xD1 → EMPTY (sandbox); Legacy(1) → RING_OP
- Testes: Legacy(1) deny / 0xD1 allow

### Fase 2 — #562 MicFrameRing ✅
- `jarbas::audio::mic_ring` — SPSC estático CAP=32, drop_oldest + `MIC_OVERRUN`
- Fan-out `VOICE_MIC_RING` + `WAKE_MIC_RING`
- capture deixa de publicar `AUDIO_FRAME` no EventBus
- voice/wakeword drenam rings; VAD_TRANSITION permanece no bus

### Fase 3 — #602 ✅
- Política documentada em `crates/ticket-lock/codemap.md`

### Fase 4 — #597 parcial
- `spin` 0.9 → **0.12** em 7 crates; check OK
- `spin::Lazy` deprecated → `LazyLock` (warnings; fix follow-up)
- **Não** bump x86_64 0.15 / smoltcp 0.14 nesta sessão (blast radius)

### Fase 5 — #599 wasmi 2.x
- Adiada (depende de #597 completo + sprint API)

## Gates
- `cargo test -p skill-registry --lib` → 11/11
- `cargo test -p hermes --lib` → 205/205
- `cargo test -p jarbas --lib` → 102/102 (+2 mic_ring)
- `cargo check -p neural-kernel --release` → 0 erros (pré-spin)
- `cargo check -p hermes -p jarbas -p k-nano --release` pós-spin → 0 erros

## Residuais
- #597: x86_64 0.15, smoltcp 0.14; Lazy→LazyLock
- #599: wasmi 2.x
- #558b/#559b/#561b: aceite metal / AEC / CTC incremental
