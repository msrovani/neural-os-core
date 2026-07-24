# tools/limine — path canônico Limine (ADR-0065 Labor 17 cutover)

**Default documentado (L17):** Limine ESP + `run-qemu-limine.ps1`.  
**Legado opt-in:** bootloader 0.11 via `run-qemu-whpx.ps1` / crate `boot` (não removido).

## Build kernel higher-half

```powershell
cargo build --release -p neural-kernel --target x86_64-unknown-none `
  --features limine-boot,fat-boot-log --target-dir target/limine
```

## ESP + QEMU

ESP = **somente FAT32 + LFN** (`mk_esp_fat.py`). Sem FAT16.

```powershell
.\tools\limine\build_esp.ps1
python tools\limine\mk_esp_fat.py --esp-dir tools\limine\esp --output target\limine-esp.img --size-mb 128
.\run-qemu-limine.ps1
```

`build_esp.ps1` baixa o zip **binary** do Limine (GitHub releases) para `vendor/BOOTX64.EFI` se ausente.

## Honesty

- Limine SMP / modules BitNet = residual
- Sem `BOOTX64.EFI`: smoke QEMU aborta; `cargo check` default continua verde
- Path 0.11 = legado para debug WHPX rápido
