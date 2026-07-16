# SESSION 114 — ADR-0042 N4 CLOSED

**Data:** 2026-07-16  
**Versão:** v1.7.6  
**Pista:** ADR-0042 N4 (hermes = orquestra) → **✅ CLOSED**; próximo **N5** (jarbas ego)

## Objetivo

Fechar N4 sob critérios ADR: intent routing, ReAct/skills wiring honesto, orquestração cortex (`generate_via_model`), telemetria `[N4-HERMES]`, EventBus intent flow, IPC→jarbas mirror. Sem claim `v2.0.0`. Voz → Sprint Sound.

## Entregas

| Item | Status | Nota |
|------|--------|------|
| Gate serial `[N4-HERMES]` | ✅ | `n4_hermes_gate()` após N3 gate |
| N4.1 intent routing | ✅ | HermesAgent `intent_router` + `USER_INTENT`/`HERMES_RESPONSE` |
| N4.2 ReAct + skills + WASM SFI | ✅ | ReAct 7 fases; `SKILL_REGISTRY`; WASM hub 7 builtins; CapGate P3 |
| N4.3 cortex orchestrate | ✅ | `global_arena` pending route → `generate_via_model` |
| N4.4 EventBus intent e2e | ✅ | boot `intent_e2e=GATED`; prior L5 `USER_INTENT` + GEN |
| N4.5 IPC→jarbas | ✅ | `jarbas_bridge::topics_in_sync()`; full wire BLOCKED (allocator) |
| N4.6 crate `hermes` | ⏳ | monólito espelha `neural-kernel/src/hermes.rs` |
| `cargo nk` | ✅ 0 erros | soft-float alias |
| Docs | ✅ | ADR-0042 checklist N4, STATE, CHANGELOG 1.7.6, IDEA #437 |

## Evidência QEMU (WHPX short)

**Log canônico:** `logs/boot_n4_20260716_144651.txt`

```text
[N4-HERMES] intent_router=REGISTERED topics=USER_INTENT/HERMES_RESPONSE react=7phase
[N4-HERMES] skills=N wasm_sfi=7 CapGate allow=2 deny=2
[N4-HERMES] cortex_orchestrate=OK route=global_arena pending→generate_via_model
[N4-HERMES] intent_e2e=GATED boot default (feature=weather-e2e; prior L5 evidence OK)
[N4-HERMES] IPC→jarbas topics_mirror=OK full_wire=BLOCKED(N4.6 allocator)
[N4-HERMES] gate complete n4.1=OK n4.2=OK n4.3=OK n4.4=OK n4.5=OK criteria=MET (N4.6 crate hermes link deferred)
```

**N4.4 prior HIT (weather-e2e):** `logs/boot_whpx_20260716_110041.txt` — EventBus `USER_INTENT` + `[GEN] decoded_len=12`.

**Ops:** `cargo clean -p neural-kernel && cargo nk` → `bootloader_linker -u -o target build …/neural-kernel` → `run-qemu-whpx.ps1 -Smp 2 -NoSerialBridge` → kill após gate (~30s).

## Decisão N4.6 / Sound

- **N4.6:** não bloquear N5 por link crate `hermes` (allocator clash k_nano, padrão N2.5/N3.5).
- **Sound:** STT retrain, Piper VITS pleno, Mic→Wake — não critérios N4.
- **Não** declarar `v2.0.0` até N5.

## Próximo

- ADR-0042 **N5** (jarbas ego / compositor / persona)
- Sprint Sound = voz only
- Sem push
