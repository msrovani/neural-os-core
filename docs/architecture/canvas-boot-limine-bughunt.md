# Canvas — Bughunt Boot/Limine AIOS (High → Low + Simplificações + Upstream)

> Premissa máxima (ADR-0088): somos o PRIMEIRO AIOS — IA desde o boot, HITL, auto-tudo, nada bypassado, +10% contínuo. Boot = Observe→Plan→Act→Verify→Remember. QEMU = dev/debug; aceite = HW real. UEFI-only; Limine = handoff canônico.

## 1. Premissas / mandatórias (contratos a preservar)
- P1 AIOS-desde-boot; P2 DeviceTree→plano+Trust→execução só do visto (NVMe>AHCI>USB>ATA); P3 8 fases + BOOT_PHASE; P4 HW Real First (não confiar só em OVMF p/ USB); P5 no_std/no_main, anéis = lógica, isolamento = wasmi+Ring3 gated; P6 Limine canônico (bootloader 0.11 removido S232); P7 UEFI-only + SB OFF + Rufus DD; P8 contratos: kernel.elf / ESP 0xEF / UPDATE.CFG com URL (IP só em config); P9 stack Limine viva sem `address` no protocolo — derivar via RSP; P10 ESP FAT32 + GUID LE + nunca formatar volume existente-sem-mount; P11 governança IDEA→ADR→TODO→STATE→SESSION→check; P12 gate v2.0.0 = checklist+review+OK humano.
- Mandatórias de uso: OVMF pflash dual-file (CODE ro + VARS rw); limine.conf `boot():/kernel.elf`; ESP tree (kernel.elf + BOOTX64=logwriter + EFI/neural/limine.efi + boot/limine.conf + UPDATE.CFG); FAT32-only ≥65525 clusters; GUID ESP sempre `uuid.bytes_le`; pendrive removable MBR slot0 0x0C + slot1 0xEF SEM 0x80 (0xEE protetor só p/ fixed disk); StackSizeRequest 8MB sem ler `response.address`; reserva PMM (kernel + heap + stack-via-RSP 8M + ramlog); limine.ld `.requests` primeiro, `.kheap` NOLOAD no fim; KernelAddressRequest sempre; build.rs `rerun-if-changed` completo; `cargo clean -p neural-kernel + check --release` 0 erros, targets sob `target/`; pipeline build-boot→limine-esp→uefi→usb_hw com `PACK_LLM=2b`; BOOT.LOG 256KB BOM UTF-8 + retry SysInfoAgent; QEMU teto 6G, e1000+user gate, SLIP frozen.

## 2. Achados ordenados (causa → fix mínimo → simplificação)
### HIGH
- **H1** `tools/build_usb_unified.py:39` GUID ESP em BE (ordem textual) — deveria ser `.bytes_le` como `mk_esp_fat.py:107`. Risco: copiar partição errada como ESP via fallback. Fix: 1 linha `uuid.UUID(...).bytes_le`. Simplificação: eliminar constante duplicada, fonte única.
- **H2** `neural-kernel/src/main.rs:1527-32` `stack_base = (rsp_phys & !2M-1) - 4M` com `-` puro + sem gate `pm_offset==0` → wrap/hang. Fix: `saturating_sub` + gate + log. Simplificação: extrair `reserve_limine_stack()` (~10 linhas), remover comentário stale "2MB" (request real 8MB).
- **H3** `run-qemu-whpx.ps1:302` + `run-qemu-uefi.ps1:205` OVMF single-file pflash → `#GP PlatformPei`/sem NVRAM. Fix: CODE ro + VARS rw + checar existência. Simplificação: unificar `Build-QemuArgs` num helper único.
- **H4** Filtro de modelo divergente (`run-qemu-uefi.ps1:185` descarta >70MB vs whpx carrega tudo >10KB) → boot "sem LLM" vs OOM. Fix: mesmo predicado. Simplificação: `Get-ModelFiles` compartilhado.
- **H5** `crates/boot/build.rs:9-19` não observa `limine.conf`/`BOOTX64.EFI`/`mk_esp_fat.py` → `uefi.img` stale. Fix: +3 `rerun-if-changed`. Sem simplificação (adição mínima).
### MED
- **M1** `tools/limine/limine.conf:10-11` `path`+`kernel_path` duplicados, kernel no root (não /boot). Fix: manter só `path:` + comentário. Simplificação: deletar linha.
- **M2** `elf_loader.rs:95` `checked_mul().unwrap_or(0)` → overflow lê header 0. Fix: `?`/early-return com erro. Simplificação: sim.
- **M3** `k_nano/src/fat32.rs:1562,1619-23` setor `[0u8;512]` fixo ignora `bps` (4Kn) → truncamento. Fix: buffer por `bps`. Correção-de-correção, sem simplificação.
- **M4** `tools/build_image.py:149-51` default `PACK_LLM=falcon3` baixa GB inline (hang). Fix: default `none` + opt-in explícito. Simplificação: menos trabalho default (YAGNI).
- **M5** `tools/verify_usb_esp.py:33,118-39` gate só kernel+BOOTX64, 4 entries → Limine-only com gate verde. Fix: checar `limine.efi`/`limine.conf`. Simplificação: estender `find_file`.
### LOW
- **L1** `mk_esp_fat.py:284-304` `alloc_chain` sem ENOSPC → corrompe FAT em silêncio. Fix: `raise SystemExit("ESP cheia")`. Sem simplificação.
- **L2** `k_nano/src/memory.rs:629-32` `is_page_present` retorna false mudo com `pm==0`. Fix: warn/debug_assert. Simplificação: só telemetria.
- **L3** `k_nano/src/allocator.rs:377-80` comentário contradiz `grow_bump_auto`. Fix: reescrever 1 linha. Só doc.
- **Descartados (já OK):** `map_page_uc` vs `set_page_uc`, scans com PRESENT, `StackSizeResponse.address`, heap wrap 2⁶⁴, FAT short-write principal, `fat-boot-log`, KERNEL_PHYS/VIRT, FB bpp dinâmico.

## 3. Upstream (repo vs ponta; só migrar com spike + `cargo check --release`)
| Item | Repo | Upstream | Decisão |
|---|---|---|---|
| Limine binário | 12.9.0 (2026-09-20, à frente das distros) | v12.x tip | manter; ideia: importar `limine deploy` p/ BIOS fallback sem mudar path UEFI |
| limine-protocol | header à mão, sem crate | spec standalone (trunk 08/2026), crate `limine` 0.6.5 (base rev 6); StackSize só `{revision}`, HHDM restritivo rev≥3, FB WC no PAT5 | ideia: sync header / pedir Executable Address + EFI mmap (elimina 2 hardcodes) |
| x86_64 | req 0.14.11, lock 0.14.13 (teto da linha) | 0.15.5 (exige nightly ≥07-10, posterior ao pin 07-05) | ficar no 0.14 (correto) |
| bootloader 0.11 | removido (tabela AGENTS stale 0.11.15) | 0.11.17 | só corrigir doc |
| linked_list_allocator | removido (menção 0.9 stale) | 0.10.6 | manter fora (talc venceu) |
| talc | 4.4.3 | 5.1.1 (breaking: TalcCell/TalcLock, base fixa, lock_api, counters) | **spike isolado 4→5 + counters** (telemetria heap); validar no_std+soft-float+`.bss.heap` |
| uart_16550 | req 0.2, lock 0.2.19 | 0.8.1 (rewrite: Uart16550/Config, MMIO+FIFO+IRQ) | **spike isolado** mantendo fachada `puts/puthex` lock-free |
| pic8259 | removido (menção 0.10 stale) | 0.11.0 | manter fora; documentar contrato `0xF8` (+`0xEF` slave IRQ12) |
| spin / lazy_static | 0.12.3 / 1.5.0 (tetos) | tetos | nenhuma; longo prazo: `LazyLock`/`spin::Lazy` (mata `&TRINITY` vs `&*TRINITY`) |
| uefi (logwriter-efi) | 0.35.0 | 0.40.0 (breaking 0.36: edition 2024, IpAddress→uefi-raw) | **spike isolado** no binário autônomo; ideia: `proto::dma::iommu` como ref p/ flush (F16) |
| Hygiene | AGENTS.md Active Dependencies stale (bootloader/spin/linked_list/pic) | Cargo.lock canônico | sincronizar tabela com lock |

## 4. Ordem de aplicação (obrigatória)
H1 → H2 → H3 → H4 → H5 → M1 → M2 → M3 → M4 → M5 → L1 → L2 → L3 → hygiene docs → spikes (talc → uart → uefi → limine-header) — cada passo com `cargo check --release` / teste do artefato, sem quebrar build.

## 5. Pós-tarefas
`cargo clean -p neural-kernel + cargo check --release` (0 erros), `cargo test --workspace --exclude neural-kernel --exclude boot --no-fail-fast`, `tools/check_duplication.py`, `tools/update_tecnologias.py`, STATE+SESSION+TECNOLOGIAS+IDEA_BANK, commit+tag.
