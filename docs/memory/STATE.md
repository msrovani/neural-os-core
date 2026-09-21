# STATE — neural-os-core v1.9.99-s378 — ticket-lock honesty

#   PISTA ATIVA: s378 — ticket-lock (Sync/try_lock/IrqSafe wrap/docs)
#   PISTA ANTERIOR: s377 skill-registry; s376 neural-kernel; s375 event-bus
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
| s374–s377 | k_ai / event-bus / neural-kernel / skill-registry | ✅ |
| s378 | ticket-lock honesty | ✅ |
| B2 | Mesh peer B / relaunch | AWAITING operador |
| Track C | Ring3/B/C/GPU/WiFi… | defer |
| IDEA #562 | AUDIO_FRAME → ring SPSC | residual |
| IDEA #597 | bump smoltcp/spin/x86_64 | residual |
| IDEA #599–#600 | wasmi / CapGate DynSkill | residual |
| IDEA #602 | spin vs ticket-lock policy | residual |
