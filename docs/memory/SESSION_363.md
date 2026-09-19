# SESSION_363 — QEMU 8c hang + jarvis_voice freeze + timer_alive falso DEAD

**Sprint:** v1.9.99-s363 TEST  
**Data:** 2026-09-19  
**Objetivo:** Validar lab WHPX 8c/6G com NKP/Falcon3; destravar boot (Falcon3 bench/IPI/BPE); destravar UI pós-desktop (`jarvis_voice`); corrigir Hub `lapic DEAD` falso.

## Contexto

Continuação do lab GPU/NKP (s361) + boot WHPX. Desktop subia mas UI congelava com stamp `jarvis_voice` / `SPEAKING` / `DSP_TICK 1`. Depois: Hub `FAIL S3` + `60Hz lapic DEAD` + heartbeat `T=00000000` perto do tick ~550.

## Causas e fixes

### 1. Hang boot (pré-Runtime)
| Causa | Fix |
|--------|-----|
| Microbench Falcon3 sync no boot QEMU (~9s/tok + IPI storm) | Skip generate/bench; lab = `measure-falcon3-toks.ps1` |
| `ipi_reschedule_handler` `puts` em toda IPI | Só atomic + EOI |
| BPE FAT PIO no sandbox WHPX | Skip como BGE |

### 2. Freeze UI em `jarvis_voice` (pós-desktop)
| Causa | Fix |
|--------|-----|
| `enable_open_mic` no claim com greeting ainda no `PLAYBACK_RING` | Mic só após ring quieto |
| VAD start + playback → barge-in sem AEC (SESSION_352) | Ignorar VAD start em SPEAKING se ainda não Listening |
| Greeting Piper no WHPX (109k–168k frames) | Formant se `hypervisor().is_sandbox()` |
| Drains EventBus sem teto | Budget VAD/frames/hermes; mixer chunk 4k |
| Mixer urgency < voice | mixer 215 > voice 200 |

Evidência pós-fix: `open_mic ON` → `SPEAKING→SLEEPING` → `DSP_TICK` 500+ → `SCHED tick=480+`.

### 3. Hub `timer DEAD` / FAIL S3 (~tick 550)
| Causa | Fix |
|--------|-----|
| `timer_alive()` spin **2 ms** vs rail **60 Hz (~16.7 ms)** → quase sempre false | Alive via `LAST_TIMER_TSC` (janela ~3 períodos) |
| `input` HID deferred QEMU EnableSlot ~1.2s ×2 (tick 50+120) | Skip deferred HID em sandbox |
| Hermes `Metricas criticas` + `write_vfs` **todo tick** | Throttle 1×/64 ticks |

Pós-fix: Hub `WARN S3` + `60Hz lapic` (amarelo = `trusted=false` WHPX, **não** DEAD); UI viva T545+.

## Honesty residual
- USB `no msc` QEMU — esperado
- Heap bump 84% vs RAM 18% — métrica de orçamento, não OOM
- Frame jitter ppm alto no WHPX
- NKP Ready / W2A8 device = metal (s361)
- BEI `ProceedWithBudget` ainda spam serial (follow-up)

## Arquivos-chave
- `k_nano/interrupts.rs` — `timer_alive`
- `jarbas/audio/{voice,jarvis,mixer,wakeword}.rs`, `display/agent.rs`
- `hermes/agents.rs` — HID sandbox + métricas
- `neural-kernel/main.rs` — bench skip + urgencies

## Aceite
`.\run-qemu-whpx.ps1 -Smp 8 -RamGB 6 -Window` → Runtime + desktop + SCHED past tick 550 + Hub timer ≠ DEAD.
