# SESSION_277 — Integração branches restantes (net + Jarbas + silicon wire)

**Sprint:** v1.9.99-s277 TEST  
**Data:** 2026-08-21  
**Escopo:** Plano “branches restantes” — merge/port seletivo + limpeza de remotes. Tip pré-docs: `ebd262f`.

## Fases

### Fase 1 — `cursor/cherry-net-apic-models` → main

| Commit | Fix |
|--------|-----|
| `6845204` | APIC INIT deassert IPI — `level=1`+`assert=0` (era re-assert) |
| `ac9b0cf` | Gateway SLIRP `10.0.2.1`→`10.0.2.2` + e1000 RDT off-by-one |
| `b8048f7` | `kick_rx_lite` raw ARP/DNS + RDT poke em `nic_recv` |
| `29d80da` | mkfat32: `D:\modelos` + BGE_M3 / hw_expert_v6 |

Smoke: SCHED tick≈192 (net path vivo).

### Fase 2 — SESSION_276 (port seletivo `ac4e853`)

- Tip branch `c234138` **descartado** (stack/mesh/LAPIC — main já tinha SESSION_275).
- Port: compositor/overlay/`CARD_ACTION`, HDA `k_nano`+facade `k_hal`, `infer_in_flight`.
- Commit: `55a776b`. Doc: `SESSION_276.md` (não sobrescrever SESSION_275 mesh).

### Fase 3.1 — Silicon wire (TSC / CachePadded / ReBAR report)

- `k_nano::tsc` calibrado (HPET→PIT→CPUID) → `busy_wait_us` / SMP sleep real (fim do `us*40` fixo).
- `k_nano::sync::CachePadded` wired.
- Boot: `calibrate_tsc()` + `pcie_bypass_report` (ReBAR/PCIe config report).
- Specs `cpu-silicon-directive.md` / `gpu-bare-metal-directive.md` honestas (AMX / work_queue overclaims corrigidos).

### Fase 3.2 — Fat32Io absorb

- `Fat32Io` + `format_fat32_bps` no canônico `k_nano/src/fat32.rs`.
- `BlockDevice::sector_size`.
- Órfão `neural_fs/fat32` **não** wired (evitar dual-module).

### Fase 4 — Remotes

| Remote branch | Destino |
|---------------|---------|
| cherry-net-apic-models | apagado no origin (integrado) |
| aios-chj / jarbas-honest-s276 | apagado (port 276) |
| silicon-wire-tsc-pad-rebar | apagado (tip = main `ebd262f`) |
| **`origin/cursor/silicon-gpu-directives-wip`** | **KEEP_WIP** (`1528b21`) — AMX, bitplanes DROP, lazy_fpu, pcid, trinity_inject, micro_hooks, prefetch, tsc_deadline |

## Tip / refs

- main tip (código): `ebd262f`
- KEEP_WIP: `origin/cursor/silicon-gpu-directives-wip` @ `1528b21`
- Locais ainda apontando remote gone (não podados nesta sessão): `cursor/cherry-net-apic-models`, `cursor/jarbas-honest-s276`, `cursor/silicon-wire-tsc-pad-rebar`

## Verificação / fix residual no check

- `cargo check --release` OPT_LEVEL=1, `CARGO_TARGET_DIR=target/check-s277`.
- Residual no wire: `tsc.rs` chamava `acpi::hpet_base_phys` inexistente → implementado (walk RSDT/XSDT → HPET GAS MMIO). Sem isso o tip `ebd262f` não fechava 0 erros.
- Não commitados: `models/*.v6`, `.freebuff/`

## Lições (resumo)

1. Colisão de número de sessão (mesh 275 ≠ jarbas) → renumerar port para 276.
2. Branch 10 commits atrás → port seletivo, não merge/rebase cego.
3. Specs órfãs overclaimam → corrigir honesty no wire.
4. `busy_wait_us` fixo → TSC calibrado.
5. Fat32Io no canônico, não dual-module.
