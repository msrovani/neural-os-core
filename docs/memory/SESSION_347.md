# SESSION_347 — Onda 6 code-complete (ADR-0102 HITL wire)

**Sprint:** v1.9.99-s347 · **Bloco:** Ring3 / Onda 6 · **Data:** 2026-09-15

## Goal

Fechar gaps de **código** da Onda 6 (ADR-0102): stash→promote HITL, `/ring3`, honesty T-052/053 AWAITING_HW. **Não** declarar aceite metal sem Alienware.

## Feito (código)

| Item | Onde | Status |
|------|------|--------|
| H1–H3 | feature + demos P6 + predicados | ✅ já no tree (s302/s305) |
| T-051 `#GP` class | `k_nano::ring3::gp_fault_class` | ✅ wired |
| T-056 opcode gate | `verify_blob_no_simd` + call em `ring3_run_native_blob` | ✅ |
| T-057 CapGate deny DMA/FB a CPL=3 | `paging` sandbox_syscalls path | ✅ |
| N4 mailbox / GS zero / teardown mid-fail | `paging.rs` | ✅ (+ cleanup code_leaf fail) |
| Stash runner no boot | `isolation_ring::init_connectors` → `stash_native_runner` | ✅ |
| Promote sob predicados | `app_factory::promote_native_ring_if_ready` | ✅ |
| HITL `/ring3 status\|approve` | hermes Command + Approve `ring3_register` | ✅ |
| Boot slog honesty | `slog_onda6_boot_status` | ✅ |

## Honesty (não mentir)

| ID | Código | Aceite HW |
|----|--------|-----------|
| T-051 | wired | QEMU/WHPX flaky — fora do caminho crítico |
| T-052 | `can_iretq` self-test | **AWAITING_HW** (notebook; depende Onda 2 SMP metal) |
| T-053 | `ring3_mark_hw_gate_passed` via `/ring3 approve`→`/approve` | **AWAITING_HW** checklist 0077 §6 |
| T-054/055 | stash + promote | só após T-053 marked **e** metal (hv=None) |
| T-056/057 | wired | exercício real só com sandbox vivo em metal |

TCG/WHPX **nunca** `register_native_ring` automático (`ring3_can_register_native` exige `HypervisorKind::None`).

## Como o operador usa

```
/ring3 status
/ring3 approve          → HITL Escalate #N
/approve N              → mark T-053 + promote_native_ring_if_ready()
```

Em QEMU: approve marca o gate, mas `can_register` continua false (hv≠None) → wasmi (A) permanece default. Correto.

## Aceite desta sessão

- [x] `cargo check -p k-nano -p hermes -p neural-kernel --release` 0 erros
- [x] Docs: TODO + ADR-0102 checklist honesty + STATE + SESSION_INDEX
- [ ] Metal T-052/053 PASS — **fora** (Alienware + Onda 2)

## Relação com P0 BOOT.LOG

Freeze antigo (SESSION_314) pediu não Onda 6 até BOOT.LOG PASS. Maintainer pediu Onda 6 explicitamente → código fechado; **produto stick** (ADR-0103 S1) continua AWAITING_OPERATOR.
