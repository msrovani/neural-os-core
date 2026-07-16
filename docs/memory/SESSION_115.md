# SESSION 115 — ADR-0042 N5 CLOSED

**Data:** 2026-07-16  
**Versão:** v1.7.7  
**Pista:** ADR-0042 N5 (jarbas = ego/UI) → **✅ CLOSED**; N1–N5 funcionais completos

## Objetivo

Fechar N5 sob critérios ADR: compositor vivo, persona/humor via Hermes, voz como expressão (agents + TTS+FB path), display integration, IPC←hermes mirror, telemetria `[N5-JARBAS]`. Sem claim automático `v2.0.0`. Voz quality → Sprint Sound.

## Entregas

| Item | Status | Nota |
|------|--------|------|
| Gate serial `[N5-JARBAS]` | ✅ | `n5_jarbas_gate()` após N4 gate |
| N5.1 compositor | ✅ | DisplayAgent + GPU FB + P4 jarbas_fb present |
| N5.2 persona | ✅ | JarvisAgent + `persona_pipeline` 16-stage + SoulProfile |
| N5.3 voice agents | ✅ | `jarvis_voice` + `wakeword` + `audio_mixer`; Hermes-only |
| N5.4 FB/display | ✅ | `paint_tts_response` + boot splash |
| N5.5 voice e2e | ✅ | boot `voice_e2e=GATED`; prior Sprint107 TTS+FB |
| N5.6 IPC←hermes | ✅ | `jarbas_bridge::topics_in_sync()`; full wire BLOCKED |
| N5.7 crate `jarbas` | ⏳ | monólito espelha `neural-kernel/src/{display,audio,jarvis}/*` |
| `cargo nk` | ✅ 0 erros | soft-float alias |
| Docs | ✅ | ADR-0042 checklist N5, STATE, CHANGELOG 1.7.7, IDEA #438 |

## Evidência QEMU (WHPX short)

**Log canônico:** `logs/boot_n5_20260716_145943.txt`

```text
[N5-JARBAS] compositor=REGISTERED display=OK gpu=ABSENT p4_present=CAP-OK apps=HermesChat+Settings+Power
[N5-JARBAS] persona=REGISTERED jarvis=OK pipeline=16stage [PERSONA] mode=...
[N5-JARBAS] voice_e2e=GATED boot default (feature=weather-e2e; prior Sprint107 TTS+FB OK)
[N5-JARBAS] voice_agents jarvis_voice=OK wakeword=OK mixer=OK hermes_only=OK (no direct ATA/PCI)
[N5-JARBAS] IPC←hermes topics_mirror=OK full_wire=BLOCKED(N5.7 allocator)
[N5-JARBAS] gate complete n5.1=OK n5.2=OK n5.3=OK n5.4=OK n5.5=OK n5.6=OK criteria=MET (N5.7 crate jarbas link deferred)
```

**N5.5 prior HIT:** `logs/boot_whpx_20260716_110041.txt` — TTS `pcm_samples=15428` + FB paint.

**Ops:** `cargo clean -p neural-kernel && cargo nk` → `run-qemu-whpx.ps1 -Smp 2 -NoSerialBridge` → kill após gate (~30s).

## Decisão N5.7 / Sound / v2.0

- **N5.7:** não bloqueia N5 funcional (allocator clash k_nano, padrão N2.5/N3.5/N4.6).
- **Sound:** STT retrain, Piper VITS pleno, Mic→Wake runtime — não critérios N5.
- **v2.0.0:** N1–N5 funcionais ✅ — gate **pode ser discutido**; não declarar sem review qualidade ADR.

## Próximo

- Sprint Sound = voz only
- Wire crates N2.5–N5.7 (allocator refactor)
- Sprint 108 self-evolving
- Sem push
