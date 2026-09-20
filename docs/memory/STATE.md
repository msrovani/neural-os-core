# STATE — neural-os-core v1.9.99-s368 — Fechamento 1.x (Trilho A0/B1/B3.1)

#   PISTA ATIVA: s368 — plano `docs/plans/2026-09-19-fechamento-1x.md` aprovado
#   B1: workspace host 0 fail (fixture GGUF + locks flaky + apply_one_layer poison)
#   B3.1: mixer `compute_mixer_want` free+TSC + 5 host tests
#   PISTA ANTERIOR: s367 LLM response gate; s366 ADR-0106; s360 mesh honesty
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
| Freeze `@ network_agent` | ABERTO — bisector FB pronto |
| xHCI MSC PP pós-HCRST | 🟡 wired; validar metal |

## Trilho B (produto 1.x) — progresso s368

| ID | Item | Estado |
|----|------|--------|
| B1.2 | `cargo test --workspace --exclude nk,boot --no-fail-fast` | ✅ 0 fail (s368) |
| B1.3 | Fixture `target/test_tq2_0.gguf` | ✅ `gen_test_gguf.py` |
| B3.1 | Playback pacing free+TSC | ✅ código+testes; aceite QEMU drops=0 residual |
| B2 | Mesh peer B / relaunch | AWAITING operador |
| Track C | Ring3/B/C/GPU/WiFi… | defer (ADR-0100 §5) |
