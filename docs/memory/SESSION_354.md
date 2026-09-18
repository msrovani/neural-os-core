# SESSION_354 — NSGDB sync + K33 boot + k_nano bughunt

## Goal
1. Alinhar AIOS ao neural-sgdb mais recente (MCP/host).
2. Desbloquear hang de boot em `K33[28] sgdb` (FileFlash/Tickv).
3. Analisar papel/premissas do `k_nano` e bughunt aprofundado R0.

## Evidência / causa

### NSGDB
- MCP/host em `C:\DEV\neural-sgdb` = **1.1.20**; crate vendored/junction no AIOS estava atrás.
- Sync via junction + `tools/sync-neural-sgdb.ps1`; `k_ai` bridge/comentários de versão alinhados.

### Boot K33[28]
- Hang pós-PHASE 5 em rebuild/`Sgdb::open` / puts Tickv sobre FileFlash ATA.
- Causa profunda: `maybe_gc` → `compact()` (wipe+rewrite PIO) quando `append_off > HIGH_WATER` (256KB) no hot path de put.
- Mitigações: `set_gc_suspended` no mount file/nvme; boot_init LIGHT + ingest/`boot_init_deferred` no Runtime; `maybe_gc` skip auto-compact file/nvme.

### k_nano bughunt
| Sev | Bug | Fix |
|---|---|---|
| P0 | ATA `wait_bsy`/`wait_drq` 10M spin sem wall-clock | TSC budget 5s; `wait_bsy` → bool |
| P0 | Tickv ckpt+recover varrem volume sem teto | deadline 3s compartilhado; recover DEGRADED |
| P0 | AHCI CI timeout retornava **true** (falso sucesso) | `wait_ci_clear` → false + slog |
| P1 | virtio-net reset `while` infinito | cap 1M spins |
| P1 | e1000 slog "Reset OK" com RST stuck | timeout → `return false` |

## Arquivos
- `crates/k_nano/src/{ata,ahci,e1000,virtio_net,storage,storage/tickv}.rs`
- `crates/k_ai/src/sgdb/{store,mod,nsgdb_bridge}.rs` + `Cargo.toml`
- `crates/neural-kernel/src/main.rs` (defer Runtime)
- `tools/sync-neural-sgdb.ps1`
- `Cargo.lock`

## Verify
- `cargo check -p k-nano --release` — **0 erros** (target `target/check-knano`)
- QEMU re-boot pós-fix: **não re-rodado** nesta sessão (AWAITING)

## Lições
1. **Timeout que retorna sucesso é pior que hang** — AHCI/e1000 mentiam "OK".
2. **Spin count ≠ wall-clock** — TCG/WHPX/HW divergem; usar `tsc::now_us`.
3. **Compact no put FileFlash = soft-hang de boot** — GC só explícito/SleepCycle até Runtime.
4. **AIOS deve trackear neural-sgdb por junction/sync**, não cópia stale.

## Aberto
- Re-teste QEMU/WHPX boot completo (PHASE 6/7 + NSGDB Runtime).
- Residuais k_nano: NVMe disable sem check RDY; AHCI stop-port 1000 spins.
