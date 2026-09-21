# SESSION_389b — Logging honesty follow-up (fat-boot-log + SCORE + phase)

**Data:** 2026-09-21  
**Continua:** SESSION_389 / tag `v1.9.99-s389`  
**Premissas:** ADR-0092 · SESSION_264 feature propagate · SESSION_346 static race

## Achados subagent → fixes

| ID | Sev | Fix |
|----|-----|-----|
| H4 | HIGH | `neural-kernel` default + `cargo nk` → `fat-boot-log` |
| H5 | HIGH | `PHASE_RANK` + `phase_0_7_label()` no BOOT SCORE |
| H6 | HIGH | `note_phase_status` re-emite upgrade ok→warn→fail |
| H7 | HIGH | `ensure_persisted` retorna `flush()` (não true mentiroso) |
| M5 | MED | BootLog ring `[t=N]` ticks (não wall-clock ms falso) |
| M6 | MED | `dump_into` wrap-safe; shutdown sem LBA 2048 |
| M7 | MED | `flush_bootlog_after_greeting` honesto se flush fail |
| L4 | LOW | session header `fat-boot-log={0\|1}` via cfg |
| L5 | LOW | `LOG_SECTOR` deprecated (bin+hermes); CapGate SUCCESS→ok |
| L6 | LOW | HDA/TPM/MEM/PCI/SMART sev canónico |

## Simplificação / upstream

- uart_16550 / Limine logwriter: idea-only (já wired)
- Sem syslog POSIX — canal A é a redução correta AIOS

## Gates

- `cargo test -p k-nano --lib boot_report` → **7/7** (TEST_LOCK)
- `cargo check -p neural-kernel --release` → 0 erros

## Canvas

`logging-bughunt-s389b.canvas.tsx`
