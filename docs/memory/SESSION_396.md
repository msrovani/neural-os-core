# SESSION_396 — Onda 2: §2 Kernel Core (loop TECNOLOGIAS)

**Programa:** loop por tecnologia do TECNOLOGIAS.md (premissas → bughunt → upstream → canvas → fixes High→Low → pós-tarefas).
**Canvas:** `docs/architecture/canvas-onda2-kernel-core.md`. Onda 2: §2.2–§2.27 (exceto 2.1 Limine de s392).
> Numeração: s394 (BEI) já ocupada por f8773265; esta sessão usa **s396**.

## Achados (exp-4/exp-5) → fixes (3 lanes)
- **k_nano:** IDT skip/ISTs (H1/H12), #PF cura (H2), Chase-Lev (H5), AP drop lock (H11), allocated_count (H13), MPMC rollback+Drop (H14), ACPI guards (HC5), CPUFreq guard (M7), TimerFuture (HC7), RTC timeout/12h, fw_cfg write_file deletada.
- **kernel-bin/cortex:** paging shallow doc (H3), vring unaligned (H4), FB UC/WT (H7), DMA pin UC (H8), ELF W^X/guards (H9), ring3 ENTER_USER (H10), mesh peer real + yield (M4), decode u32 + mask.len==cols (M5), infer_queue CAS (M6).
- **hermes/event-bus:** NTP honesto (HC1/M5), mailbox teto (M6), IPC smoke (H-lab).

## Upstream (lib-3) — nenhum bump pendente
spin 0.12.3 teto · talc 5.1.1 DEFER (fix chunk >2^56, sprint de heap) · Chase-Lev/CFS→EEVDF ideia · event-bus bounded política · seL4 16 ativo (mint/rights) · **x86_64 0.15.5** exige nightly≥2026-07-10 (pin 07-05) — par toolchain futuro; uart 0.8 junto.

## Pós-tarefas
`cargo check --release` 0 errors; sem DUP novo; commit + tag s396.
