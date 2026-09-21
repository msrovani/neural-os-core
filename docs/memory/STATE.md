# STATE — neural-os-core v1.9.99-s384 — smoltcp 0.14 + wasmi 2.0

#   PISTA ATIVA: s384 — ordem residual LazyLock + smoltcp + wasmi
#   PISTA ANTERIOR: s383 CapGate DynSkill + MicFrameRing
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
| s383 | #600/#562/#602 + spin 0.12 | ✅ |
| s384 | LazyLock + smoltcp 0.14 + wasmi 2.0 | ✅ |
| IDEA #597 | spin+smoltcp ✅; x86_64 0.15 ⛔ nightly 1.98 | parcial fechado |
| IDEA #599 | wasmi 2.0 | ✅ |
| #558b/#559b/#561b | aceite voz metal | residual paralelo |
| B2 | Mesh peer B | AWAITING operador |
| Track C | Ring3/B/C/GPU/WiFi… | defer |
