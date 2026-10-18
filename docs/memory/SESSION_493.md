# SESSION_493 — AIOS NeuralFS substrato (não “trocar mkfat32”)

**Sprint:** v1.9.99-s493 TEST  
**ADR/docs:** NeuralFS.md §14; INDEX; ADR-0079/0088/0113  
**Data:** 2026-10-18

## Goal

Fechar o gap de auto-adaptação: data volume = NeuralFS canónico; FAT data = DEGRADED; ESP FAT + BOOT.LOG DEV; seed host `mkneuralfs` ≡ layout SysInstaller; Observe→Plan→Act→Verify→Remember.

## Feito

### F0 — Doutrina + postura
- `docs/architecture/NeuralFS.md` §14 + INDEX.
- `k_nano::storage_posture` (`DataFs` / `BootLogBackend` / `slog_posture_once` / `observe_fat_fallback_once`).
- Taxonomia: FAT=DEGRADED; NRFS=canónico; BOOT.LOG=ESP.

### F1 — Streaming
- `NeuralVolume::write_file` → `write_file_stream` (anti materializar multi-GB).
- Teste host `write_file_stream_roundtrip` PASS.
- Hub LOO NeuralFS: `read_range` + `validate_for_slot` + `neural_paths_for`.

### F2 — Seed lab + ESP bootlog
- `tools/mkneuralfs.py` → `cargo test export_lab_seed --ignored` (MBR 0x7F @2048, `ecosystem/models/`).
- `build_image.py --fs neuralfs` → `disk_*.neural.raw`.
- `build_usb_unified.py --fs neuralfs` (MBR type 0x7F, sem BPB FAT).
- `boot_logger` prioridade ESP quando `bootlog_prefer_esp()`.
- `tools/extract_esp_bootlog.py` + HOWTO.

### F3 — Sense→Plan→Act
- NeuralFsAgent: `note_nrfs_mounted` / `observe_fat_degraded` / posture slog.
- Hub: NeuralFS paths first; FAT fallback **warn** + `observe_fat_fallback_once`.

### F4 — Convergência installer
- `sys_installer`: `create_dir` `/boot` + `ecosystem/models` + `firmware` + `logs` (mesmo contrato do seed).
- Migrate HITL FAT→NRFS = residual (IMPLEMENTED path dirs; metal VERIFIED à parte).

## Prova (host)

| Claim | Estado |
|---|---|
| `storage_posture` tests | VERIFIED 2/2 |
| `write_file_stream_roundtrip` | VERIFIED |
| `cargo check -p k-nano/cortex --release` | 0 erros |
| Seed magic NEURALFS (smoke) | VERIFIED 64MB |
| QEMU WHPX 2 boots: ATA prefer 0x7F + mount | VERIFIED (`ATA mount LBA=2048`, `data_fs=neuralfs nrfs_mounted=true`) |
| Hub Active blob via NRFS | UNKNOWN (smoke sem Active; LOO fat_fallback hw_identify esperado) |
| Metal `usb_hw.img` NRFS + LOO | UNKNOWN |
| PackageHub ecosystem real | IMPLEMENTED paths; VERIFIED runtime UNKNOWN |

### QEMU fix s493 (runtime)
1. `ata.rs`: preferir disco MBR `0x7F` sobre FAT/ESP.
2. `neural_fs_agent`: se ATA já é NRFS, **não** skip por live-USB.
Logs: `logs/boot_nrfs_s493_fix3_*.txt` (+ 2º boot).

## Remember

`storage_verdict`: default lab ainda pode ser FAT até `--fs neuralfs` VERIFIED em QEMU; anti-padrão = pack FAT como “nativo PackageHub”.

## Pendentes

1. 2 boots WHPX com `disk_qemu.neural.raw` + Active + slog `data_fs=neuralfs`.
2. `usb_hw.img --fs neuralfs` metal LOO mid-niche.
3. Migrate path FAT→NRFS HITL no PartitionPlanner.
4. Espelho best-effort `/logs/` NRFS (produto) — canal DEV permanece ESP.
