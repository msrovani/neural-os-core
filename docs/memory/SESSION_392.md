# SESSION_392 — Boot/Limine bughunt AIOS (H1–H5, M1–M5, L1–L3) + canvas

**Sprint:** v1.9.99-s392 — Boot/Limine bughunt: GUID ESP, stack RSP, OVMF dual-file, ELF overflow, FAT 4Kn
**Premissa:** ADR-0088 AIOS-desde-boot; Limine = handoff canônico (S232); UEFI-only; HW Real First.
**Canvas:** `docs/architecture/canvas-boot-limine-bughunt.md` (premissas P1–P12, mandatórias M1–M16, contratos C1–C8, achados, upstream).

## Premissas Limine/boot re-lidas (fonte: exp-1)
- P1–P3: AIOS-desde-boot, Observe→Plan→Act→Verify→Remember, 8 fases + BOOT_PHASE.
- P6/P7: Limine canônico (bootloader 0.11 removido S232), UEFI-only, BIOS = triple-fault.
- P8: `kernel.elf` / ESP 0xEF / UPDATE.CFG = contratos; IP só em config.
- P9: `StackSizeResponse` = só `{ revision }` — **sem `address`** (SESSION_254 fantasma); reserva via RSP.
- P10: ESP FAT32 + GUID sempre `uuid.bytes_le` + nunca formatar volume existente-sem-mount.
- Mandatórias: OVMF pflash dual-file; `boot():/kernel.elf`; MBR pendrive slot0 0x0C + slot1 0xEF SEM 0x80;
  StackSizeRequest 8MB; reserva PMM (kernel+heap+stack-via-RSP 8M+ramlog); `rerun-if-changed` completo;
  QEMU teto 6G; `PACK_LLM=2b` p/ não baixar Falcon3 inline.

## Bughunt — achados e fixes aplicados (fonte: exp-2; lanes fix-1/fix-2/fix-3)
| ID | Arquivo | Achado → Fix |
|---|---|---|
| H1 | `tools/build_usb_unified.py:39` | GUID ESP/BASIC em BE textual → `uuid.UUID(...).bytes_le` (fonte única c/ mk_esp_fat). Bônus: BASIC campo2 tb estava BE. |
| H2 | `neural-kernel/src/main.rs` | `(rsp & !2M-1) - 4M` com `-` puro → helper `reserve_limine_stack` (`saturating_sub` + gate `pm_offset==0` + log fail). |
| H3 | `run-qemu-whpx.ps1` / `run-qemu-uefi.ps1` | OVMF pflash single-file → CODE ro + VARS rw + checagem dos 2 arquivos. |
| H4 | mesmos | filtro modelo divergente (>70MB vs tudo) → mesmo predicado `>10KB` nos dois. |
| H5 | `crates/boot/build.rs` | sem observar limine.conf/BOOTX64.EFI/mk_esp_fat → +3 `rerun-if-changed` (fim da classe stale S233/295). |
| M1 | `tools/limine/limine.conf` | `path`+`kernel_path` duplicados → só `path:` + comentário root-da-ESP. |
| M2 | `neural-kernel/src/elf_loader.rs:95` | `checked_mul().unwrap_or(0)` → early-return `Err("ELF: program header offset overflow")`. |
| M3 | `k_nano/src/fat32.rs` | `append_file` setor `[0u8;512]` fixo → buffer fatiado por `bps` (4Kn). Residual: `update_file_size` ainda usa `*512` (fora do escopo). |
| M4 | `tools/build_image.py` | default `PACK_LLM=falcon3` (hang multi-GB) → default `none` + opt-in. |
| M5 | `tools/verify_usb_esp.py` | gate só kernel+BOOTX64 → `find_file` recursivo + checa `limine.efi`/`limine.conf`. |
| L1 | `tools/limine/mk_esp_fat.py` | `alloc_chain` sem ENOSPC → `SystemExit("ESP cheia")`. |
| L2 | `k_nano/src/memory.rs` | `is_page_present` false mudo c/ pm==0 → `warn`. |
| L3 | `k_nano/src/allocator.rs` | comentário contradizia `grow_bump_auto` → reescrito (HEAP_SIZE=piso, limite=janela+budget). |

## Upstream (fonte: lib-1; lane fix-4 — docs, sem bump sem check verde)
- Limine 12.9.0 local à frente das distros → manter; ideia: `limine deploy` p/ fallback BIOS.
- `x86_64` 0.14.13 = teto da linha (0.15.x exige nightly pós-pin) → ficar. `spin` 0.12.3 / `lazy_static` 1.5.0 = tetos.
- **DEFER c/ motivo:** talc 4.4.3→5.x (breaking TalcCell/base fixa; heap `.bss.heap`/soft-float atritam);
  uart 0.2.19→0.8 (rewrite Uart16550/Config; quebra serial de boot); uefi 0.35→0.40 (só logwriter-efi, spike isolado futuro);
  header limine manual preservado (no_std mínimo; ideia: Executable Address + EFI mmap).
- Hygiene aplicada: AGENTS.md Active Dependencies sincronizada c/ Cargo.lock (removidos bootloader/linked_list/pic8259 stale);
  contrato máscara PIC `0xF8` (+slave `0xEF` IRQ12) documentado.
- `tools/update_tecnologias.py` **não existe** no repo (só `update_neuralfs_md.py`) — passo N/A.

## Verificação
- `cargo check --release` → **0 errors** (1m20s; 34 warnings pré-existentes, nenhum nas linhas tocadas).
- `py_compile` nos 4 .py + `[Parser]::ParseFile` nos 2 .ps1 → OK. `__pycache__` removido.
- `tools/check_duplication.py` → só DUPs pré-existentes (facades); nenhum novo.
- Descartados verificados (já OK): map_page_uc/set_page_uc, scans PRESENT, StackSize address, heap wrap 2⁶⁴, FAT short-write principal, fat-boot-log, KERNEL_PHYS/VIRT, FB bpp.

## Pós-tarefas
STATE s391→s392, AGENTS.md sprint, índice, commit + tag `v1.9.99-s392`.
