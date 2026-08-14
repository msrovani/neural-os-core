# SESSION_262 — Early BOOT.LOG no pendrive live USB (feature wire + safe overwrite)

**Data:** 2026-08-14  
**Foco:** Log de boot gravado o mais cedo possível na partição FAT do stick (HW real sem COM).

## Diagnóstico

1. **Bug raiz (feature gate):** `fat-boot-log` existia só em `neural-kernel`. O
   `persist_now` real em `k_nano::boot_logger` estava atrás de
   `#[cfg(feature = "fat-boot-log")]`, mas **k_nano não tinha a feature** → o
   stub `persist_now → false` era o que sempre compilava. `init_after_usb` /
   SysInfo chamavam `flush()` em vão. QEMU mascarava (COM1 → `logs/boot.txt`).
2. **Timing:** USB-MSC só subia depois de NIC/net/ATA/HDA/AHCI — hang nesses
   caminhos = zero log no stick.
3. **SESSION_260:** reescrever o dir cluster a cada flush + crash = FAT rasgado.
4. **SysInfo retry:** só `flush()` sem re-probe MSC quando `USB_MSC=None`.
5. **serial.rs journal:** path ATA-only — no live USB nunca gravava.

## Fix

| Item | Mudança |
|------|---------|
| Wire | `k_nano` feature `fat-boot-log`; bin `fat-boot-log = ["k-nano/fat-boot-log"]` |
| Early | Após `init_platform_sync`: `init_xhci` + MSC + `init_after_usb` (ckpt K18) |
| Idempotente | `init_xhci` early-return se já up; DriverInit reusa MSC (sem re-Address) |
| Safe write | `overwrite_boot_log` data-only; dirent só se size < 512 (1 setor) |
| Retry | `try_ensure_usb_msc` + `ensure_persisted`; SysInfoAgent usa isso |
| Serial HW | `write_to_disk_journal` → `boot_logger::append_raw` |

## Verificação

- `cargo check -p k-nano --features fat-boot-log`
- `cargo check -p neural-kernel --features fat-boot-log`
- Host: assert simbólico de que `persist_now` não é stub sob a feature
- HW: rebuild `usb_hw.img` → Rufus DD → ler `BOOT.LOG` no volume de dados

## Lições

- Feature do **bin** não propaga para crates — sempre espelhar `foo = ["dep/foo"]`.
- Live USB: canal de verdade = FAT no stick; serial é só QEMU/bancada.
- Dir FAT não-atômico + crash = corrupção agravada; data-only após 1º dirent.
