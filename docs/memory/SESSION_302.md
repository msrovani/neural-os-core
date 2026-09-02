# SESSION_302 — Ring3 Onda 6: isolamento CPL=3 (ADR-0102)

**Data:** 2026-09-01 | **Sprint:** v1.9.99-s302 | **Status:** 🟡 Código Onda 6 wired; aceite HW T-052/053 pendente

---

## Objetivo

Implementar o isolamento Ring3 conforme **ADR-0102** (H1–H3, gaps N1–N7, T-056, wiring ELF + `register_native_ring` gated).

## Entregas (código)

| Item | Onde | Estado |
|---|---|---|
| H1 feature `ring3` bin→k_nano | `neural-kernel/Cargo.toml` | ✅ |
| H2 demos P6 reais (`iretq`, fault, CapGate, SSE) | `k_nano::paging` | ✅ |
| H3 `ring3_can_iretq` + `ring3_can_register_native` separados | `k_nano::ring3` | ✅ |
| N1 `MAX_SANDBOXES=1` + `SANDBOX_BUSY` | `paging::enter_user_mode` | ✅ |
| N2 GS.base zerado pré-iretq | `paging` | ✅ |
| N3 teardown L4+frames | `ring3_run_native_blob`, demos | ✅ |
| N4 mailbox USER `{nr,arg0,arg1,cap,result,status}` | `k_nano::ring3` + handler bin | ✅ |
| N5 HHDM supervisor-only no sandbox | `create_sandbox_as` | ✅ |
| N6 IF=0 documentado (DoS aceito) | `enter_user_mode` | ✅ |
| T-056 verificador opcode SSE/AVX | `ring3::verify_blob_no_simd` | ✅ |
| RSP0 no TSS carregado | `interrupts::set_bsp_rsp0` | ✅ |
| ELF + `load_and_spawn` + `run_process` | `isolation_ring`, `elf_loader` | ✅ |
| `fault_abort` → `HEALTH_ISSUE` | `ring3::publish_sandbox_fault` | ✅ |
| Delete espelho `smp/percpu.rs` bin | removido | ✅ |
| `app_factory` reativado (seam register) | `hermes/lib.rs` | ✅ |

## Gate produção (ainda pendente)

- **T-052/053:** aceite em notebook real (BOOT.LOG + demos P6).
- **T-054/055:** `ring3_mark_hw_gate_passed()` + HITL maintainer → `register_native_ring`.
- **T-051:** separar `#GP` OVMF vs kernel (não bloqueia dev).
- **T-057:** pin DMA pós-registro (deny CapGate já wired).

## Validação

```text
cargo check --release -p neural-kernel  → 0 erros
cargo test -p k-nano ring3            → 3 passed
```

## Lições

1. **`app_factory` comentado em hermes = seam morto** — `register_native_ring` não compila se o módulo estiver `DEAD CODE`; religar ao wirear `isolation_ring`.
2. **Mailbox N4 ≠ marker u32** — handler deve ler `SyscallMailbox` em VA user; inferir `cap` quando campo zero (demos CapGate).
3. **`ring3_can_register_native` ≠ `ring3_is_safe`** — metal (`None`) vs KVM são predicados distintos; TCG/WHPX nunca registram ring.
4. **`set_rsp0` em `interrupts_ext`** escrevia TSS fantasma — usar `k_nano::interrupts::set_bsp_rsp0` no TSS que `lgdt` carrega.

## Próximo

- Boot QEMU: validar log P6 (`iretq`, fault-containment, CapGate deny).
- HW: T-053 checklist 0077 §6 → `ring3_mark_hw_gate_passed()` → segunda `init_connectors`.
