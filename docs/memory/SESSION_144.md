# SESSION_144 — Onda 3: exFAT write + FS agents fecho

**Data:** 2026-07-18  
**Pista:** residuals ADR-0040 · Onda 3  
**Check:** `cargo check --release` = 0 erros

## PreFlight

```
python tools/preflight_wave.py --wave 3
```

- **#417** PARTIAL → código write+smoke opt-in (evidência runtime = `EXFAT_WRITE=1` no QEMU)
- **#418** BLOCKED — fila Onda 7 (`depends_on: lan` / `force_blocked`)
- **#419** PARTIAL/✅ CLI — `storage_report`; UI App defer

Correção PreFlight: `VERDICT=AWAITING_REAL_HW` global não contamina mais itens sem `awaiting: [...]`.

## #417 exFAT write (opt-in)

| Peça | Path |
|------|------|
| Write bitmap+FAT+dir | `crates/neural-kernel/src/exfat_write.rs` (+ espelho `k_nano`) |
| Discover 0x81 no mount | `exfat.rs` `discover_bitmap` |
| Smoke | `smoke_write_roundtrip` → `EXFATWR.TXT` |
| Gate | `CONFIG.TXT` → `EXFAT_WRITE=1` (`mkexfat` default `=0`) |
| Hook boot | `NeuralFsAgent::try_exfat_write_smoke` |

**Limites MVP:** create-only no root; max 64 KiB; sem extend de diretório; NTFS/EXT write permanece ⏳.

**Risco:** write em mídia real pode corromper — só com flag explícita (mesmo padrão `NEURALFS_USB_FORMAT`).

## #282e–h

| ID | Fecho |
|----|-------|
| 282e InferenceFsAgent | ✅ já em `fs/` + `register_fs_agent` |
| 282f HermesFsAgent | ✅ `/chat/` wired |
| 282g RamFsAgent | ✅ `/mnt/ram/` wired |
| 282h Auto tier MHI | ⏳ defer — sem `MhiScheduler` auto-migrate |

## #419

- ✅ `storage_manager::storage_report` (CLI)
- ⏳ App UI jarbas — fora do gate Onda 3 (cauda opcional)

## Próximo

Onda 4 (Sound residuals) ou QEMU com `EXFAT_WRITE=1` para promover #417 PARTIAL→evidência boot.
