# SESSION_139 — HW USB boot: BOOT.LOG + console FB legível

**Data:** 2026-07-17  
**Foco:** Notebooks sem serial — diagnóstico no stick FAT32 e texto legível no framebuffer.

## Problema

1. HW real sem COM → sem log serial.
2. Windows não montava volume de dados do `usb_hw.img` (layout MBR/GPT).
3. Bootloader panica em Intel HD 620 (`BltOnly` GOP).
4. Hang/alloc antes do heap (`disable_vga_plane` + journal).
5. Tela ilegível: TRACE do bootloader + `boot_ckpt` + `fb_print` sobrescrevendo sem limpar faixa.

## Feito

### USB / Windows mount
- `tools/build_usb_unified.py`: MBR slot0 = FAT32 dados (`0x0C`), slot1 = ESP (`0xEF`) + GPT UEFI.
- Evitar 0xEE-first (Windows monta lixo) e protective-only 0xEE em removable (sem letra).
- `tools/mkfat32.py`: BPB jmp/OEM + seed `BOOT.LOG`; `tools/inspect_usb_layout.py` valida.

### BOOT.LOG (fat-boot-log)
- Feature ligada em `crates/boot/Cargo.toml`.
- `boot_logger.rs`: 8.3 `BOOT.LOG`, overwrite via BlockDevice (USB-MSC→ATA), flush rate-limited.
- `heap_ready()` / `mark_heap_ready()` — **proibido alloc/journal antes do heap**.
- `init_after_usb()` após probe USB-MSC; serial sem COM → append + FB.

### Bootloader BltOnly
- `vendor/bootloader` + `vendor/bootloader-x86_64-uefi`: `SetMode` Rgb/Bgr; senão boot sem FB (sem panic).
- Workspace `[patch.crates-io] bootloader = { path = "vendor/bootloader" }`.
- Ver `vendor/README.md`. **Não** versionar `vendor/bootloader/target/`.

### Console FB legível
- `jarbas/display/fb.rs`: `console_clear` / `console_print` / `boot_ckpt` / `boot_splash` com cursor atômico; limpa faixa por linha; wrap limpa tela.
- Probe UEFI: `console_clear()` antes de K0 (remove TRACE).
- `neural-kernel/vga_buffer::fb_print` → `console_print` (sem ghost).

### Checkpoints boot
- `main.rs`: K0–K17 (probe→heap→xHCI→USB-MSC→flush BOOT.LOG).

## Verificação
- `cargo check -p jarbas --release` (target/check-fb-console) → 0 errors
- `cargo check -p neural-kernel --release` → 0 errors
- Validação final = foto HW + `E:\BOOT.LOG` após rebuild USB

## Evidência HW (ainda aberta)
- `BOOT.LOG` placeholder = kernel não chegou ao flush USB.
- Último progresso observado: APIC/timer/x2APIC; sem `PLATFORM sync OK` → próximo: STI pós-APIC / SMP / PCI.

## Lições (NÃO REPETIR)
1. Removable Windows: MBR FAT32 dados **antes** de ESP; não confiar só em GPT.
2. Sem serial: um console FB com clear-por-linha; nunca empilhar texto sem limpar banda.
3. Journal/FAT log só **depois** do heap.
4. GOP `BltOnly` → SetMode Rgb/Bgr ou boot headless; não chamar `frame_buffer()` cego.
5. `disable_vga_plane` no early boot: sem `println!`/alloc.

## Rebuild USB
```powershell
cargo build --release -p boot
python tools/build_usb_unified.py --size 2048 --fat32 --build-boot --output target/usb_hw.img
```
Rufus DD no stick; ler `BOOT.LOG` no volume de dados.
