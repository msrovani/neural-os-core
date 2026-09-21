# STATE — neural-os-core v1.9.99-s376 — neural-kernel honesty

#   PISTA ATIVA: s376 — neural-kernel (shutdown/BootLog/slog/Cargo trim)
#   PISTA ANTERIOR: s375 event-bus; s374 k_ai; s373 k_hal; s372 k_nano; s371 cortex
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
| s374 | k_ai honesty residual | ✅ |
| s375 | event-bus bounded + honesty | ✅ |
| s376 | neural-kernel wire honesty | ✅ |
| B2 | Mesh peer B / relaunch | AWAITING operador |
| Track C | Ring3/B/C/GPU/WiFi… | defer |
| IDEA #562 | AUDIO_FRAME → ring SPSC | residual (cap 8 mitiga) |
| IDEA #597 | bump smoltcp 0.14 / spin 0.12 / x86_64 0.15 | residual soft-float |
