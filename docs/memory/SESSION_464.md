# SESSION_464 — Auditoria USB BOOT.LOG: verificação estática + 9 fixes + readback + regressão host

**Data:** 2026-10-11 · **Branch:** main (working tree) · **Escopo:** persistência do log em USB (Caminho A kernel + Caminho B logwriter-efi + ordem de shutdown)

## Contexto

Auditoria externa do fluxo de persistência (`boot_logger.rs`, `boot_ramlog.rs`, `usb_msc.rs`, `xhci/mod.rs`, `hub_msc.rs`, `logwriter-efi`, `shutdown.rs`). Achados verificados por inspeção estática contra o código; causa elétrica/firmware do notebook (CCS=0) permanece UNKNOWN sem HW. Nenhum reset xHCI alterado por conjectura.

## Verificação (todos confirmados, exceto HW)

- **P0 `bulk_transfer` deadlock:** guard `XHCI_STATE` vivo da linha 1747 até fim da função; `lock()` no poll (1798) = spin eterno na 1ª BOT. BOT nunca funcionou por este caminho (QEMU usa virtio-blk).
- **P0/P1 `written > 0` aceito:** `overwrite_boot_log:548` retornava Ok em cadeia curta.
- **P1 `sync_cache()` ignorado:** USB + virtio-blk (`persist_now`).
- **P1 BOT:** CSW sem tag/resíduo; DMA sem free (~2 MiB/256 KiB); `write_sector` sem guarda 4Kn.
- **P0/P1 EFI `try_write_regular`:** `set_position` + `flush` ignorados. (`write` uefi 0.35 já é todo-ou-erro `Result<(), usize>` — sem loop; quantidade provada via `get_position`.)
- **P0 shutdown:** S5 incondicional com `try_flush_ramlog()` descartado.
- **P1 `flags`:** `init_from_phys` preservava `POFF` ao consumir selo.
- **§1 revisão:** `try_flush_ramlog()==true` = backend gravou+verificou em não-volátil (NÃO "RAM selada") → renomeado `persisted_to_storage`.

## Fixes

1. `xhci/mod.rs::bulk_transfer` — lock escopado só ao doorbell; poll re-locka limpo.
2. `boot_logger.rs::overwrite_boot_log` — exige `written == write_len`.
3. `boot_logger.rs::persist_now` — `ok = msc.sync_cache()` / `ok = vb.sync_cache()`.
4. `boot_logger.rs::verify_written_chain` (NOVO) — readback pela cadeia FAT via `read_sectors` existente; falha = IoFail (tenta ESP). Chamado no ramo de sucesso.
5. `usb_msc.rs` — guarda 4Kn no write; CSW tag+resíduo 0; `bot_transfer` splitado com `free_dma` sempre.
6. `logwriter-efi/main.rs::try_write_regular` — `set_position` + `write` + `get_position()==len` + `flush` conferidos.
7. `shutdown.rs` — `persisted_to_storage` (dump || try_flush); S5/0x604 só se true, senão warm-reset p/ captura.
8. `boot_ramlog.rs::init_from_phys` — `flags=0` ao consumir `NEURDONE`/`NEURLOG!`.

## Validação host

- `cargo check --release` → 0 errors (inclui logwriter-efi via bindeps).
- `cargo test -p k-nano --lib -- --test-threads=1` → 381 passed, 0 failed.
- `boot_logger` 13/13: `MemDisk` + FAT32 mínima (MBR 0x0C, BPB spc=1, FAT, root+BOOT.LOG) — parcial rejeitada; full aceita + bytes no LBA; corrupção de 1 B reprovada; tamanhos 0/511/512/513 (513 exige 2 clusters); cadeia truncada reprovada; erro de leitura no verify reprovado.
- Paralelo dá o crash pré-existente `STATUS_PRIVILEGED_INSTRUCTION` de statics (classe SESSION_418) — serializado é verde.

## Lições

1. **Guard de `spin::Mutex` vive até fim do escopo (NLL não antecipa Drop)** — re-lock do mesmo mutex = deadlock determinístico; escopar o lock ao trecho mínimo.
2. **Contador de progresso ≠ prova de persistência** — `written` prova que o laço andou; só readback prova que o meio gravou; só flush confirmado prova durabilidade.
3. **Booleano ambíguo reintroduz bug de ordem** — `ramlog_flushed` sugeria "RAM preparada" quando significava "gravado em disco"; nomear `persisted_to_storage`.
4. **Contrato de retorno de `write` varia por crate** — uefi 0.35 `Result<(), usize>` (todo-ou-erro) vs `Result<usize>`; provar quantidade via `get_position` independe do contrato.
5. **FAT32 mínima mockável em ~40 linhas** — MBR+BPB+FAT+root bastam p/ exercitar write/verify sem HW.

## Pendentes (HW)

- Reset xHCI e `CCS=0` em `00:14.0`: intocados até evidência pré-HCRST.
- Encerramento: gravação conhecida + readback + flush + recovery pós-reset + origem registrada (direta vs EFI).
