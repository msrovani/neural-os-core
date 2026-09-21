# SESSION_372 — k_nano bughunt AIOS (honesty + trim)

**Foco:** Premissas ADR-0088 / 0092 / 0103 — Ring 0 `k_nano` (HAL, drivers, slog, storage).

**Canvas:** [k-nano-bughunt-s372](file:///C:/Users/msrov/.cursor/projects/c-DEV-neural-os-core-latest/canvases/k-nano-bughunt-s372.canvas.tsx)

## Achados → fixes (ordem aplicada)

| Sev | Achado | Fix |
|-----|--------|-----|
| HIGH | i225 CTRL_RST stuck continuava init | TSC 2s / spin → `false` + `warn` (paridade e1000) |
| HIGH | `storage::nvme` ASQ/ACQ @ `0x10000000` | `init()` → `Err` deny; produção = `disk_agent::nvme` |
| HIGH | sev=`e1000`/`ahci`/… → TRACE mudo (Reset OK invisível) | `Sev::from_sub` aliases → Ok; `err`→Fail |
| MED | ArcCache evict dirty sev=`info` | `warn` (DATA LOSS RISK) |
| MED | `disk_power.rs` órfão (ADR-0103 delete) | ficheiro apagado |
| LOW | ruvix-* + linked_list_allocator + spinning_top 0 callers | removidos do `Cargo.toml` |

## Já ok (não tocado)

tpm sha256 FIPS; e1000 TDBAL@0x3800 + reset timeout; AHCI `wait_ci_clear`→false; xHCI IR0+0x20; virtio_blk `DESC_F_NEXT`; ATA `in ax`; NVMe PRP em `disk_agent`.

## Upstream

- smoltcp 0.13 / virtio-drivers 0.13 / spin 0.9 — manter
- x86_64 0.15 / uart_16550 0.6 / talc 5 — **adiar** (API break boot)

## Verify

- `cargo test -p k-nano --lib` → **193 pass**
- `cargo check -p k-nano --release` → 0 erros

## Residual pós-auditoria ([Deep k_nano honesty](42dc8599-2390-41b2-98ed-ed1f6c144fc1) + [deps](67ad30bd-e4fd-4806-8617-83ac632792bd))

| Sev | Achado | Fix |
|-----|--------|-----|
| HIGH | `sync_cache` default `true` | default `false`; ATA `flush_cache` 0xE7 wired |
| HIGH | FAT mount bps≤4096 mas write `[u8;512]` | mount recusa `bps != 512` |
| MED | NVMe disable sem fail se RDY stuck | TSC/spin + `None` |
| MED | AHCI reprograma CLB com CR stuck | skip porta + warn |
| LOW | DiskIntelligenceAgent.cache morto | campo + tick flush removidos |

Deps: ruvix/linked_list/spinning_top já trimmed; bumps major (x86_64/talc/uart/smoltcp) adiados.
