# ADR-0041: K²CHJ Capability-Based Rings + SFI

**Data:** 2026-07-14  
**Status:** Accepted (direção) / In Progress (MVP C)  
**Sprint:** 107+ (capability microkernel)  
**Propósito:** Formalizar a visão de anéis por capability (K-Nano / K-IA / Cortex / Hermes / JARBAS), o estado real (monólito Ring 0) e o roadmap incremental sem desfazer Pacotes A/B.

---

## 1. Contexto

A visão-alvo é um **capability microkernel** onde:

| Anel lógico | Privilege | Contrato |
|-------------|-----------|----------|
| **K-Nano** | Ring 0 único | CR3/GDT/IDT exclusivos; slab/lock-free no scheduling; sem heap dinâmico no path crítico |
| **K-IA** | Ring 3 + MMIO mapeado | PCI, VirtIO rings, zero-copy DMA pinning |
| **Cortex** | Ring 3 | mmap de pesos, AVX/AMX, MoE |
| **Hermes** | Ring 3 (WASM SFI) | Host functions gated por capability |
| **JARBAS** | Ring 3 + FB MMIO | Double-buffer / VSync |
| **IPC** | Cross-AS | Só ring buffers lock-free (sem sockets internos) |

**Realidade atual (2026-07-14):** boot = monólito `neural-kernel` em Ring 0 único. Crates K²CHJ existem no workspace, mas o binário de boot não depende delas para isolamento. Pacotes A+B (STI/PIC, stack 2MB, init_phase RR, BOOT_PHASE, DiagnosticSkill, `init_platform_sync` antes de drivers, Agency EventDriven) **permanecem** — não desfazer.

---

## 2. Decisão

1. Tratar K²CHJ crates como **fronteiras lógicas** até haver address spaces + IPC real.
2. Evoluir o monólito com provas de conceito **não-fatais** no boot (falha → warn, continua).
3. Adotar **capability tokens de operação** (bitflags) além do `CapabilityToken` do EventBus (legado/Ed25519).
4. IPC interno futuro = **SPSC/MPMC ring buffers** em páginas compartilhadas mapeadas nos address spaces envolvidos.
5. Syscall mínimo = trap software (`int 0x90`; 0x80–0x82 reservados para IPI SMP). Ring 3 completo é fase seguinte.

### Non-goals desta sprint / MVP C

- Separar binários por crate K²CHJ
- User-mode Ring 3 estável (jump `iret`/sysret) — bônus; stub Ring0↔Ring0 + CR3 OK
- Hermes WASM SFI completo, JARBAS FB isolado, K-IA DMA pinning, Cortex mmap de pesos
- Reescrever Agency / drivers / Pacotes A+B

---

## 3. Gap analysis (visão × realidade)

| Claim visão | Status | Evidência | Esforço | Risco boot |
|-------------|--------|-----------|---------|------------|
| K-Nano Ring 0 exclusivo CR3/GDT/IDT | **Parcial** | `memory.rs` CR3 único; `interrupts.rs` GDT/IDT globais | G | Alto se CR3 errar |
| Slab / lock-free scheduling, sem heap no path crítico | **Parcial** | `slab.rs`, `agent-core` RR; heap ainda no boot path | M | Médio |
| K-IA Ring 3 + MMIO / VirtIO / DMA pin | **Fictício** (drivers Ring 0) | `pci.rs`, `virtio_*`, `dma.rs` no monólito | G | Alto |
| Cortex Ring 3 + mmap pesos | **Fictício** | `cortex.rs`, `arena.rs` — mesmo AS | G | Médio |
| Hermes WASM SFI + host caps | **Parcial** | `wasm*.rs`, `trust::check_syscall` — sem AS separado | M | Baixo |
| JARBAS Ring 3 + FB MMIO + VSync | **Fictício** | `display/`, compositor Ring 0 | G | Médio |
| IPC só ring lock-free entre AS | **Fictício** → **MVP C parcial** | EventBus in-process; MVP C: SPSC shared pages | M | Baixo se isolado |
| Capability autoritativa por operação | **Parcial** | EventBus `CapabilityToken`; MVP C: `Cap` bitflags + syscall | P | Baixo |
| Dois address spaces + CR3 switch | **Fictício** → **MVP C** | Novo: `address_space.rs` | M | Médio (mitigado: non-fatal) |

---

## 4. Prioridades P0 → P5

| Pri | Item | Status pós-MVP C |
|-----|------|------------------|
| **P0** | Gap documentado (esta ADR) | ✅ |
| **P1** | ADR curto + non-goals | ✅ |
| **P2** | **MVP C:** 2 AS + CR3 switch + ring SPSC shared + Cap + trap `int 0x90` + demo boot non-fatal | ✅ PoC |
| **P3** | Hermes WASM host-functions por Cap (sem AS full) | ⏳ |
| **P4** | JARBAS FB MMIO capability + double-buffer contract | ⏳ |
| **P5** | K-IA DMA pin + Cortex mmap pesos (AS dedicado) | ⏳ |

Roadmap explícito: **MVP C → Hermes/JARBAS → K-IA → Cortex mmap**.

---

## 5. MVP C — aceite

- Dois `AddressSpace` (L4 próprio, shallow-copy do kernel + mapas privados).
- `Cr3::write` A → B → kernel, com interrupções mascaradas na janela crítica.
- Página compartilhada com `SpscRing`; escrita num AS, leitura no outro.
- `Cap::{PING, WRITE_RING, READ_RING}` + `syscall::dispatch` via `int 0x90` (ABI staging via atomics).
- Demo após DriverInit; erro → serial WARN, boot segue.
- **TODO Ring3:** entrada user-mode real (stub code page + `iretq`) quando estável no QEMU UEFI.

---

## 6. Consequências

- Positivo: prova hardware-level de isolamento de página + IPC shared-memory sem reinventar drivers.
- Negativo: shallow-copy L4 compartilha PageTables inferiores do kernel — AS ainda não é isolamento forte contra o kernel (intencional no PoC).
- EventBus continua pub/sub in-process até migração gradual para rings cross-AS.
