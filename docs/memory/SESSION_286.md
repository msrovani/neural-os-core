# SESSION_286 — Emagrecer neural-kernel C1-C5: bin vira facade (hermes/jarbas/k_nano/k_hal)

**Data:** 2026-08-23 00:30
**Commits:** 3ae31b5, a7440eb (C1 197 + C2 1941 + C4 2714 + C5 1360 + C3 2200 = ~9k LOC movidos, 19.3k→13k)
**Objetivo:** Premissa `neural-emagrecer-bin.mdc` — bin só `pub use/wire`, lógica nas crates, `0088 Observe→Plan→Act`.

## O que foi feito

1. **C1 Delete dead 197 LOC** `tracer.rs 73 + verify.rs 124` (`@dead` nunca instanciado, zero callers). `labor_smokes` mantido (10 callers vivos). `cargo check` 0 erros, `check_duplication` 53→51.
2. **C2 Cap/Paging 1941 LOC** `capability_gate/syscall/address_space/isolation_ring/exec_arena/user_mode → k_nano::paging (R0) + k_hal::cap_gate (R1)` — `Cap` canônico, `AddressSpace CoW`, `W^X arena`, `Ring3 iretq/TSS.RSP0`, `int 0x90` gated `soft-float`. Bin vira `pub use` + `register_native_ring`. `Cap::ENTER_USER` removido. `cargo build bare-metal 13s OK`.
3. **C4 Net 2714 LOC** `net/netstack/network_agent/netdiag/proto/slip/e1000/rtl8139/i225/virtio_net → k_nano::net (R0 E1000 0x3800) + hermes::netstack (R3 smoltcp 915L, P2P 42069, Checksum Both, DNS raw)`. Triplicata zerada, `bootstrap_early` → `k_ai::boot_bind`. Bin facades.
4. **C5 TLS+FB 1360 LOC** `tls_trust/client → hermes::tls` (`embedded-tls 0.19` soft) + `vga_buffer 66→4L facade pub use k_nano+jarbas`, `fb_print/_print` movido para `jarbas::display::fb` (`GRAPHICS_OWNED` guard), macros `print!` hoisted para `main.rs`. `check_duplication` DUP vga/tls 0.
5. **C3 Agents 2200 LOC** `agents.rs 2544 bin → hermes canônico 2857L`, bin vira 30× `pub use` facade (130762B→912B). Merge `sysinfo/mouse/log_analyst`, `Cap` gate `k_hal`, `boot_mode` Install/Live. `AgentFleet 147` unificado. DUP `agents` zerado.

## Verificação

- `cargo check --release` 0 erros (30 warnings known)
- `cargo check -p neural-kernel --target x86_64-unknown-none` 0 erros
- `cargo build -p neural-kernel --target x86_64-unknown-none` 0 erros
- `cargo test -p k_nano --features ring3` 143 passed
- `python tools/check_duplication.py` DUP 52→42 (fora escopo C3)
- `qemu TCG 1c` antes loop GDT em `04dae06` (Selectors::call_once) — fix `cd5d9ed` já no branch, loop ausente em `b11b1d6` base

## Lições (Aprenda)

- **Gate soft-float é por target, não por feature:** `#[target_feature]` não re-legaliza split LLVM com `-sse` no nível target (`pmaddubsw 256`); gate `#[cfg(all(x86_64, not(target_os="none")))]` + stub escalar é o único que evita `STATUS_ILLEGAL_INSTRUCTION` no `cargo build bare-metal`.
- **Facade não é cópia:** `pub use + lógica mantida` = duplicação labelada `role_diff`; emagrecer só sai movendo autoridade, não arquivo. `check_duplication.py` mede LOC, mas `crate::` paths provam drift.
- **Tracer morto polui `allow(dead_code)`:** `@dead` 73+124 LOC mascaram dead-code real e impedem `deny(warnings)` futuro — `rm` é ponytail rung 1.
- **Net gate canônico documentado:** `e1000 0x3800/0x3818`, `Checksum Both`, `wall_pause_us` via `tsc::sleep_us` — triplicata só zera com R0 transporte + R3 policy split.

## Próximo

- `C3` pendente já era o último — plano `C1-C5` completo `~9k` (piso `13k`). Próximo `C6` seria `fs/*` restante (6 arquivos DUP) se necessário, mas `0075` declara `13k` como novo normal.
