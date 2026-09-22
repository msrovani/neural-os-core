# Canvas — Onda 2: §2 Kernel Core (s394)

> Premissas AIOS: fail-closed, honestidade (sem stub-teatro), no_std soft-float sem XMM, QEMU máscara vs HW real.
> Nota: mcpsidebase (`slog_bin`) = instrumento FB/serial canônico; TicketLock **não** reentrante (S316).

## Achados — ordem de aplicação
### HIGH (lane sys: k_nano)
- **H1** `interrupts.rs:908-913` loop 34..=255 apaga mouse(44)/HDA(0x30) → adicionar à skip-list.
- **H2** `interrupts.rs:544` #PF nunca cura → encadear `try_fault_in_heap`+`try_handle_fault` antes do return/halt.
- **H5** `work_stealing.rs` Chase-Lev quebrado (underflow vazio, drop silencioso, Task sem arg, steal pré-CAS) → guard+bool+`Task{f,arg}`+ler pós-CAS.
- **H11** `smp/mod.rs:39,75` AP faz `sti` com spinlock segurado → drop antes / IrqSafeLock.
- **H12** `interrupts.rs:283` 4 IRQs (timer/teclado/mouse/HDA) no mesmo IST → ISTs distintos.
- **H13** `memory.rs:225` `allocate_contiguous` não incrementa `allocated_count`.
- **H14** `sync/mpmc.rs:53` try_send/try_recv com spin infinito → bound + rollback; Drop por try_recv.
### HIGH (lane kernel-bin)
- **H3** `k_nano/paging.rs:101` `clone_current` alias raso → renomear/documentar `shallow_shared` (CoW = residual ADR-0077).
- **H4** `neural-kernel/virtio_vring.rs` packed u64/u32 com `&mut` desalinhado → `read/write_unaligned` (classe i225).
- **H7** `jarbas_fb.rs:126` FB MMIO WB → `NO_CACHE|WRITE_THROUGH`.
- **H8** `k_ia_dma.rs:46` pin sem UC → `map_page_uc` no pin, `set_page_wb` no unpin.
- **H9** `elf_loader.rs` sem W^X/stack exec + `p_vaddr` HHDM sem gate + `p_memsz` sem teto → 3 guards.
- **H10** `isolation_ring.rs:31` ELF ignora caps → `ENTER_USER` gate.
### MED (sys: runqueue victim, decode u16→u32+len==cols, mpmc bound; kernel: mesh 0xFF spin, infer_queue CAS)
### MED (assinadas jarbas/hermes) — HC1 NTP storm (gravar LAST_ATTEMPT no início + deletar chamada inline cron) · HC5 ACPI loop-infinito/checksum/is_page_present · HC7 async TimerFuture slot vazado
### Docs (TECNOLOGIAS §2)
- **HD1** §2.23 S3 `suspend_resume.rs` **não existe** (stub-teatro) → status parse-only.
- **HC6** §2.16 vconsole F1–F6 sem wiring (0 callers) → status + InputAgent skip.
- M3 CFS contabilidade morta → EEVDF ou marcar residual (upstream).
### LOW · simplificações
- L17 Box/Vec ponytail (feito) · deletar `fw_cfg::write_file` (0 callers, spec sem guest-write) · MPMC/CFS prune · `Ncounters`· etc. (ver exp-2a/2b).

## Upstream (lib-3) — sem bump pendente
- spin 0.12.3 teto; migrar lazy_static→Lazy/LazyLock remove spin 0.9.9 transitivo (wasmi mantém).
- talc 5.1.1 (fix chunk >2^56) — DEFER p/ sprint de heap (breaking base fixa).
- Chase-Lev/CFS: EEVDF (virtual deadline) como ideia para agentes interativos.
- event-bus bounded aggiornata: anel fixo/seq (política, não crate). seL4 16 ativo; ideia mint/rights.
- **x86_64 0.15.5** exige nightly ≥2026-07-10 (pin=07-05): par cargo-toolchain futuro; uart 0.8 junto.
## Verificação: `cargo check --release` 0 erros + post-tarefas s394.
