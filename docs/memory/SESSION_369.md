# SESSION_369 — Boot bughunt + Trilha A wire (logwriter / init_from_phys)

**Data:** 2026-09-20/21  
**Premissas:** ADR-0088 Observe→Plan→Act · ADR-0042 telemetria · ADR-0092 sev · Trilha A BOOT.LOG

## Delivered

### Bughunt → fix (12 achados)
| ID | Fix |
|----|-----|
| C1/C2 | `boot_ramlog::init_from_phys()` wired pós-`PHYS_MEM_OFFSET`; `append` zera `NEURDONE`; sev `warn` em `NEURLOG!` |
| H2/H3 | `emit_phase_banner` sev=status; `flush()` slog `warn` se `!ok` |
| H1/L1 | PHASE banners **após** SystemBringup / HardwareDiscovery / Diagnostics / DriverInit |
| H4 | `crates/boot/build.rs`: mk_esp fail → **panic**; prune `limine-esp-tree` |
| M2/M3 | `StorageKind::VirtioBlk` no plano; classify RAID/SCSI → `None` (não Ata) |
| M1/M4 | `SESSION_BODY` e soft-reboot API/feature **removidos** |
| M5 | Report BOOT.LOG rev. 3 |
| Upstream | Limine vendor **12.9.0** `BOOTX64.EFI`; uefi-rs **0.35** no logwriter |

### Trilha A produto
- `crates/logwriter-efi` (BOOTX64 stage-0 → SFS `BOOT.LOG` → chainload `\EFI\neural\limine.efi`)
- Seal em panic / orderly reboot (`shutdown.rs`)
- PMM reserve ramlog (já existia; report atualizado)
- CRC=0 no logwriter = fail-closed

### Honesty / UI adjacente (working tree)
- jarbas display + gauges; `run-qemu-whpx.ps1`; mkfat32/populate placeholder honesto; pci/gpu detect

## Verificação
- `cargo check -p k-nano --features fat-boot-log` OK
- `boot_bind` tests 5/5 OK
- `logwriter-efi` `--target x86_64-unknown-uefi` OK
- `cargo check --release -p neural-kernel --features fat-boot-log` OK

## Aceite residual (operador)
- Stick com BOOTX64=logwriter + Limine 12.9
- Reboot **ordenado** UI/CAD (não power button)
- `E:\BOOT.LOG` com linhas `[T+]`
- uefi 0.35→0.40 residual

## Files (principais)
- `crates/logwriter-efi/**`, `crates/boot/build.rs`, `crates/k_nano/src/boot_*.rs`, `storage_probe.rs`, `boot_bind.rs`
- `crates/k_ai/src/boot_observe.rs`, `crates/neural-kernel/src/main.rs`, `shutdown.rs`
- `tools/limine/vendor/BOOTX64.EFI`, `VERSION.txt`
- `docs/reports/2026-09-20-*.md`, SESSION/STATE/CHANGELOG

## IDEA
- #591 Trilha A logwriter + consume cross-boot
