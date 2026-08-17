# SESSION_270 — BOOT.LOG só DEV/TEST; padrão com timestamp

**Data:** 2026-08-17  
**Foco:** Correção de contrato de log pós-SESSION_269.

## Decisão

| Canal | Quando | Nome |
|-------|--------|------|
| **DEV/TEST** | `fat-boot-log` + Live/Install/early | `BOOT.LOG` fixo (overwrite 8.3) |
| **Produto** | `BootMode::Installed` | `/logs/boot_<tick7hex>.log` (timestamp) |
| **Server telemetria** | POST `/api/logs` | `neural-<YYYYMMDD-HHMMSS>-<seq>.log` (ADR-0086) |

`BOOT.LOG` **não** é o padrão de produto — martelar ATA interno sem esse arquivo era o spam da s269.

## Fix

- `boot_mode::peek()` — lê cache sem re-lock ATA
- `fixed_boot_log_dev_only()` — false quando Installed
- `timestamped_session_name()` → `boot_{:07X}.log`
- `persist_timestamped_vfs` → `/logs/...` quando não-DEV
- mkfat32: placeholder BOOT.LOG marcado DEV/TEST only

## Verificação

- `cargo test -p k-nano --features fat-boot-log boot_logger` → 8/8 PASS
- `cargo test -p k-nano boot_mode` → 2/2 PASS
- `cargo check -p neural-kernel --features fat-boot-log` → 0 erros
