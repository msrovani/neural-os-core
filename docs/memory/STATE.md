# STATE — neural-os-core v1.9.99-s377 — skill-registry honesty

#   PISTA ATIVA: s377 — skill-registry (WASM fail-closed / Trust / cache / trim)
#   PISTA ANTERIOR: s376 neural-kernel; s375 event-bus; s374 k_ai; s373 k_hal
#   Não declarar v2.0.0

## Gate ADR-0100 (Trilho A) — checklist vivo

| Onda | Item | Estado |
|------|------|--------|
| 0 | Honesty BOOT_AI T-001–T-006 | ✅ |
| 1 | I/O + HardwareInfo (mín. T-011) | ✅ |
| 2 | SMP metal K23 `online==madt-1` | ▶️ AWAITING_HW / operador |
| 3 | OTA A2 **ou** A3 | `[ ]` lab A2 próximo |
| Review | ADR formal + OK humano | `[ ]` |

## Aceite metal quente (bloqueia A)

| Item | Estado |
|------|--------|
| `E:\BOOT.LOG` real | ▶️ AWAITING_OPERATOR |
| Freeze `@ network_agent` | ABERTO |
| xHCI MSC PP pós-HCRST | 🟡 wired; validar metal |

## Trilho B (produto 1.x)

| ID | Item | Estado |
|----|------|--------|
| B1.2 | workspace host 0 fail | ✅ s368 |
| B3.1 | Playback pacing free+TSC | ✅ s368 |
| s374–s376 | k_ai / event-bus / neural-kernel honesty | ✅ |
| s377 | skill-registry honesty | ✅ |
| B2 | Mesh peer B / relaunch | AWAITING operador |
| Track C | Ring3/B/C/GPU/WiFi… | defer |
| IDEA #562 | AUDIO_FRAME → ring SPSC | residual |
| IDEA #597 | bump smoltcp/spin/x86_64 | residual |
| IDEA #599 | wasmi 2.x hermes | residual |
| IDEA #600 | CapGate tokens DynSkill | residual |
