# SESSION_163 — Emagrecer neural-kernel (cutover seguro, zero perda)

**Data:** 2026-07-19  
**Escopo:** Ondas 0–6 do plano Emagrecer — inventário + stubs cirúrgicos + promotes bin→crate.

## Onda 0

- Script: `tools/diff_bin_crate.py` (`--markdown`, `--onda N`, `--strict`)
- Tabela: `docs/memory/BIN_CRATE_DIFF.md`
- Regra: `bin_ahead` → promover antes de stub; nunca `pub use` de cópia mais velha

## Onda 1 (k_nano baixo risco)

Stubs: `sync`, `hw_rng`, `tpm`, `slip`, `dma`, `slab`, `io_scheduler`.

(gpt/exfat/ahci/rtl8139 adiados até Onda 3–4 por `PciDevice`/`BlockDevice`.)

## Onda 2 (k_ai thin)

Stubs: `conversation`, `chunker`, `usage`, `profile`, `cognitive`, `training_agent`.  
Split: `shutdown` (HW no bin).  
Adiados: `boot_log_agent`, `memory_agent` (VRAM), `gguf` (K-quants+FAT).

## Onda 3 (pci / USB)

- `pci` stub + `read_config_word` → `pub` em k_nano  
- Stubs: `simd`, `xhci`, `rtl8139`, `ahci`, `hw_agents`  
- Adiados: `serial`/`vga`/`usb_msc`/`virtio_net` (bin ahead / k_hal layering)

## Onda 4 (disco)

- Promovidos bin→k_nano: `fat32`, `ata` (probe exFAT), `e1000` (`prove_rx`, `read32` pub)  
- Stubs: `fat32`, `ata`, `e1000`, `gpt`, `exfat`, `exfat_write`, `fs_driver`, `storage_manager`  
- `block_dev`: hybrid — trait k_nano + `UsbMassStorage` impl no bin  
- **Um `ATA_DRIVER`**: `pub use k_nano::ATA_DRIVER` (mirror removido)  
- `neural_fs` agent: permanece no bin (USB_TRUST/USB_MSC)

## Onda 5 (plataforma)

- `acpi` stub (S5 no k_nano)  
- `TIMER_TICKS` + `MOUSE_ABS_*` canônicos k_nano; IRQ mouse atualiza abs  
- `apic` stub (mouse_gsi); `LAPIC_VIRT_BASE`/`set_page_uc` → `pub`  
- `inventory` + `context_window` stubs  
- Adiados: full `interrupts` IDT cutover, `smp`, `boot_logger` promote

## Onda 6 (residuals)

- `global_arena` promovido (pending_route) → cortex; stub no bin  
- Removido alias morto `cortex_model_hub`  
- **Mantidos no bin (honesto):** `cortex.rs`, `bpe`, `model_hub`, `agents`, `net*` + `net_bridge`, `audio/*`

## Gate

`cargo nk` (target `target/check-emagrecer`) = **0 erros** pós-ondas.

## Checklist residual (próximas sessões)

1. Promote `boot_logger` / `serial` / `vga` (heap_ready)  
2. Merge `model_hub` + cutover `bpe`/`cortex.rs`/`agents`  
3. Audio cutover ADR-0045  
4. `smp` unificar trampoline
