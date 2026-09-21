# STATE — neural-os-core v1.9.99-s386 — W2A8/KernelPack honesty

#   PISTA ATIVA: s386 — bughunt W2A8 + KernelPack (H1–H5 + M1–M6 + L1–L4)
#   PISTA ANTERIOR: s385 SGDB/Tickv/NSGDB honesty
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
| s386 | W2A8/KernelPack honesty High→Low | ✅ |
| s385 | SGDB/Tickv/NSGDB bughunt High→Low | ✅ |
| s384 | LazyLock + smoltcp 0.14 + wasmi 2.0 | ✅ |
| s383 | #600/#562/#602 + spin 0.12 | ✅ |
| IDEA #485 | F4 CPU W2A8 ladder | ✅ s386 (device = ADR-0105 AWAITING) |
| IDEA #550 | NKP B0–B3 | 🟡 B1.2/B4 AWAITING_HW |
| IDEA #536 | KernelPack host producers | ⏳ Layer S |
| IDEA #597 | spin+smoltcp ✅; x86_64 0.15 ⛔ nightly 1.98 | parcial fechado |
| IDEA #599 | wasmi 2.0 | ✅ |
| IDEA #537 | put_doc sync OK; cutover 75 callers | 🟡 residual Fase 3 |
| #558b/#559b/#561b | aceite voz metal | residual paralelo |
| B2 | Mesh peer B | AWAITING operador |
| Track C | Ring3/B/C/GPU/WiFi… | defer |
