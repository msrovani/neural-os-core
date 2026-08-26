# SESSION_292 — Instalador pendrive→HD externo + NeuralFS opt-in FAT32

**Objetivo:** tornar o cenário "boot por pendrive → instalar no HD (interno ou externo USB)" funcional em HW real, e permitir NeuralFS persistente em stick/HD externo sem debug build.

## Problema 1 — Instalador exigia ATA como source

`AutoInstallerAgent::run_install_from_bus` (`crates/k_nano/src/installer_agent.rs`) hardcodava
`ATA_DRIVER` como boot device. Bootando por pendrive/HD externo USB em HW real, o boot device é
`USB_MSC` → erro constante `"sem ATA (boot device ausente)"`. Funcionava só em QEMU (disco IDE).

### Fix
- `read_kernel_from_boot()`: tenta `ATA_DRIVER` → `USB_MSC` (`read_kernel_elf(dev: &mut dyn BlockDevice)`).
- `install_on_disk()`: source com o mesmo fallback (escape por raw ptr, padrão existente).
- **Guarda nova:** target ≠ source comparado **por endereço** (`core::ptr::eq` sobre `*const u8`)
  antes de `SysInstaller::install`. Sem isso, com source USB o auto-pick (AHCI→NVMe→USB) podia
  escolher o próprio boot device e reformatá-lo (perda do meio de boot em runtime).

## Problema 2 — `NEURALFS_USB_FORMAT=1` era flag morta na imagem unified

`peek_config_txt` (`crates/k_nano/src/neural_fs/neural_fs_agent.rs`) lia CONFIG.TXT **só em exFAT**
(VBR `EXFAT   `). A imagem unified (`build_image.py --hw --unified`) tem dados em FAT32 `0x0C`
(SESSION_258/260) → a flag nunca era lida → `usb_format_allowed()` false no release → NeuralFS USB
preso em RAM 4MB.

### Fix
- Ramo FAT32 (`0x0B|0x0C|0x1C|0xEF`) em `peek_config_txt`, cap 4096B igual ao ramo exFAT.
- Beneficia todas as flags do mesmo caminho: `NEURALFS_USB_FORMAT`, `EXFAT_WRITE`, `USB_TRUST_ENFORCE`.

## Suporte — `fat32::read_root_file_dev()` (novo)

`Fat32Reader`/`read_mbr` são tipados em `&AtaDriver` (I/O `&self`). Generalizar o reader tocava
45 call-sites em 7 crates (cortex, hermes, k_hal, jarbas, k_ai, bin). Solção mínima: função livre
`read_root_file_dev(dev: &mut dyn BlockDevice, part, name)` em `fat32.rs`, espelhando
`Fat32Reader::read_file` (gate de type 0x0B/0x0C/0x1C/0x73/0xEF, validação BPB anti-OOB,
MAX_ROOT_DIR_CLUSTERS=256, teto MAX_INLINE 256MB, bounds de chain, FAT 28-bit) + gate extra
`data.len() < file_size → None` (chain truncada não retorna dados parciais).

Callers novos: `installer_agent::read_kernel_elf` e `peek_config_txt` (ramo FAT32).

## Tool — `tools/write_usb_hd.ps1`

Grava `target/usb_hw.img` direto num HD externo USB (raw write .NET, buffer 4MB, progresso):
admin-check, lista discos USB/removível, recusa disco não-USB, recusa tamanho < imagem,
**recusa enclosure 4K-native** (`LogicalSectorSize != 512` — GPT escrita não seria achada pelo
firmware), `Clear-Disk` antes do raw write (solta volume cache), confirmação digitada SIM.
Alternativa zero-código: Rufus → Advanced → "List USB Hard Drives" → modo Imagem DD.

## Verificação

- `cargo check --release -p k-nano --target-dir target/check-instfix`: 0 erros (5 warnings conhecidos).
- `cargo check --release` (workspace completo): 0 erros.
- `cargo test --workspace --exclude neural-kernel --exclude boot`: 166/168 PASS.
  2 falhas pré-existentes em `hermes::wasm_build` (op-IR WASM v2.0, outra sessão; arquivo não
  modificado nesta sessão).

## Limitações honestas / residuais

1. **`USB_MSC` é single-device** (`globals.rs:7`): pendrive E HD externo plugados juntos = só um
   enumera. Instalar pendrive→HD externo por USB requer que o HD seja o MSC presente. Multi-LUN/MSC
   múltiplos = trabalho maior (fora de escopo).
2. **Índices da UI vs `device_for_index` divergem sem ATA:** `scan_disks` numera bus entries a partir
   de `disks.len()` (=0 sem ATA) enquanto `device_for_index(0)` é sempre ATA. Com DISK_SELECTION=0
   sem ATA, o clique cai no auto (1..3). Auto mode funciona; seleção explícita de card com índice
   deslocado é residual.
3. Aceite final = HW real (pendrive boot → install → reboot pelo HD).

## Nota de processo

Sessão concorrente ativa na árvore durante o trabalho (boot_observe.rs, boot_ramlog.rs, main.rs,
neural-sgdb/, refactor canônico `read_mbr_dev`+`parse_mbr_with` em fat32.rs — inclusive um
duplicado transitório de `read_mbr_dev` que apareceu e foi removido pela outra sessão). Commit
desta sessão é **cirúrgico**: apenas os arquivos acima; hunks da outra sessão em fat32.rs ficam
não-staged.
