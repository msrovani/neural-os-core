# SESSION_278 — Ring3 TCG aceite parcial (ADR-0077 §6 iretq + falta)

**Sprint:** v1.9.99 TEST  
**Data:** 2026-08-21  
**Branch:** `cursor/ring3-tcg-accept-s278`  
**Escopo:** Campanha QEMU TCG `-NoDisk` para fechar bloqueadores mensuráveis de **ADR-0077 §6** (iretq CPL=3, AS sandbox, contenção de falta, boot vivo). **Não** liberar B/C / `register_native_ring`. *(Número histórico no branch: ADR-0060 — canônico hoje = 0077.)*

## Evidência QEMU TCG

```
SUCCESS iretq+CPL3 marker=... Cap::ENTER_USER
Ring3 user-mode demo OK
BOOT: P6 Ring3 OK
SUCCESS fault-containment sandbox_dead kernel_alive (P6: fault during Ring3)
Ring3 fault-containment OK
BOOT: P6 Ring3 fault-containment OK
```

Serial: `logs/ring3_tcg.txt`. Accel TCG, 2c/6G, sem 2º disco, OVMF edk2.

## Fixes (3 ciclos)

| Ciclo | Sintoma | Causa | Fix |
|-------|---------|-------|-----|
| 1 | `#PF` CR2=HHDM stack pós `cr3→user`, pré-iretq; spin `SAVED_RIP=0` | `create_sandbox_as` só P4[511] — stack Limine em HHDM | Copiar P4[HHDM] supervisor-only no sandbox AS |
| 2 | `#GP(0x20)` no path iretq | Seletores Ring3 vinham de GDT **fantasma** em `interrupts_ext` (nunca `lgdt`) — índice colidia com TSS da GDT `k_nano` | User CS/DS na GDT **carregada** `k_nano::interrupts` (layout 0x08/0x10/0x18\|3/0x20\|3/TSS) |
| 3 | `#PF` em `int 0x90` com RIP user | `TSS.RSP0=0` | Inicializar `privilege_stack_table[0]` no TSS BSP |

## Checklist ADR-0077 §6 (honesto)

| Critério | Estado |
|----------|--------|
| iretq estável + retorno kernel | PASS (TCG) |
| AS isolado (sandbox + kernel supervisor) | PASS parcial (P4[511]+HHDM; HHDM supervisor — CPL=3 não USER) |
| Contenção de falta | PASS (`demo_ring3_fault_containment`) |
| Syscall gate Cap | PASS parcial (deny Cap vazio + EXIT via int 0x90) |
| DMA/MMIO negados CapGate Ring3 | PASS (TCG: PIN_DMA=1 MAP_FB=1) |
| Soft-float / SSE trap | PASS (TCG: CR0.EM + xorps → #UD contained) |
| Gate HV (`ring3_is_safe`) | Mantido: só KVM; TCG/WHPX = false → **sem** `register_native_ring` |
| Boot sem reboot loop | PASS |

## Invariante porto seguro

- `TRY_ENTER_RING3=true` nesta branch (demo boot).
- `ring3_is_safe()` / `init_connectors()` **não** registram ring nativo no host Windows/TCG.
- Caminhos B/C continuam `AWAITING_ISOLATION`; apps IA = wasmi A.

## Residual / próximo

- Expandir `ring3_is_safe(Tcg)` só após evidência WHPX/HW (CapGate+soft-float já PASS em TCG).
- WHPX: campanha própria (MSR SYSCALL já gated).
- PCID / lazy FPU: residual ADR-0082, não bloqueador deste aceite.
