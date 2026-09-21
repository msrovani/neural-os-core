# SESSION_389 — Logging QEMU + HW bughunt (ADR-0092 honesty)

**Data:** 2026-09-21  
**Premissas:** ADR-0092 canal A (COM1+BOOT.LOG+ramlog) · ADR-0088 evidência · SESSION_264 fat-boot-log · SESSION_360 DENY/fail sev

## Achados → fixes (High→Low)

| ID | Sev | Fix |
|----|-----|-----|
| H1 | HIGH | `serial::dispatch_bytes`: `to_file` → `append_raw` sempre (antes só sem COM1) |
| H2 | HIGH | `boot_logger::log` só slog (sem log_quiet duplicata) |
| H3 | HIGH | UPDATE/Market fetch FAIL → sev `fail`; OK → `ok` |
| M1 | MED | CapGate MAP_BAR/FE DENY esperado → sev `ok` |
| M2 | MED | NetAgent tick → `trace`; Continuous announce → `ok` |
| M3 | MED | boot_logger init/SKIP sev canónico ok/warn |
| M4 | MED | shutdown: flush FAT BOOT.LOG + ring ATA legado honesto |
| L1 | LOW | mojibake comments boot_logger |
| L2 | LOW | hotspots info→ok/warn (FS, Market scan) |
| L3 | LOW | teste `file_gate_matches_adr0092`; uart_16550 idea-only |

## Upstream (idea-only)

- uart_16550 estável — sem bump
- Limine logwriter-efi = seal NEURLOG! (já wired SESSION_369)

## Residuals

- Migrar slog `info` restante nos drivers (alias→Ok mas não canónico)
- Aceite metal `E:\BOOT.LOG` (operador)
- CI `parse_boot_score.py` no boot log QEMU

## Gates

- `cargo test -p k-nano --lib slog` → **5/5**
- `cargo test -p k-nano --features fat-boot-log --lib boot_logger` → **9/9**
- `cargo check -p k-nano --features fat-boot-log --release` → 0 erros
- `cargo check -p hermes|neural-kernel --features fat-boot-log --release` → 0 erros

## Canvas

`logging-bughunt-s389.canvas.tsx` — 10/10 aplicados
