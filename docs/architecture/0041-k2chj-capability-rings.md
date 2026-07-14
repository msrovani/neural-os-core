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
- Hermes WASM SFI completo; VirtIO ring DMA real; GGUF/FAT mmap + demand-paging
- Reescrever Agency / drivers / Pacotes A+B

**Nota P6:** user-mode Ring 3 via `iretq` + stub + `SYS_EXIT_USER` é PoC boot non-fatal (não scheduler multi-task).

---

## 3. Gap analysis (visão × realidade)

| Claim visão | Status | Evidência | Esforço | Risco boot |
|-------------|--------|-----------|---------|------------|
| K-Nano Ring 0 exclusivo CR3/GDT/IDT | **Parcial** | `memory.rs` CR3 único; `interrupts.rs` GDT/IDT globais | G | Alto se CR3 errar |
| Slab / lock-free scheduling, sem heap no path crítico | **Parcial** | `slab.rs`, `agent-core` RR; heap ainda no boot path | M | Médio |
| K-IA Ring 3 + MMIO / VirtIO / DMA pin | **Parcial** (P5 PoC pin+map) | `k_ia_dma.rs` + Cap PIN/MAP_DMA; VirtIO ring = stub | G | Médio |
| Cortex Ring 3 + mmap pesos | **Parcial** (P5 PoC eager mmap) | `cortex_mmap.rs` + Cap MAP_WEIGHTS; GGUF/FAT = TODO | G | Baixo |
| Hermes WASM SFI + host caps | **Parcial** | `wasm*.rs`, `trust::check_syscall` — sem AS separado | M | Baixo |
| JARBAS Ring 3 + FB MMIO + VSync | **Parcial** (P4 PoC Ring0+AS) | `jarbas_fb.rs` + Cap MAP/WRITE_FB | G | Médio |
| IPC só ring lock-free entre AS | **Fictício** → **MVP C parcial** | EventBus in-process; MVP C: SPSC shared pages | M | Baixo se isolado |
| Capability autoritativa por operação | **Parcial** | EventBus `CapabilityToken`; MVP C: `Cap` bitflags + syscall | P | Baixo |
| Dois address spaces + CR3 switch | **Fictício** → **MVP C** | Novo: `address_space.rs` | M | Médio (mitigado: non-fatal) |
| Ring3 CPL=3 real (`iretq`) | **Fictício** → **P6 PoC** | GDT user + TSS.RSP0 + `user_mode.rs` | G | Médio (non-fatal + fault abort) |

---

## 4. Prioridades P0 → P6

| Pri | Item | Status pós-MVP C |
|-----|------|------------------|
| **P0** | Gap documentado (esta ADR) | ✅ |
| **P1** | ADR curto + non-goals | ✅ |
| **P2** | **MVP C:** 2 AS + CR3 switch + ring SPSC shared + Cap + trap `int 0x90` + demo boot non-fatal | ✅ PoC |
| **P3** | Hermes WASM host-functions por Cap (sem AS full) | ✅ CapGate + SEND_TCP/WRITE_RING |
| **P4** | JARBAS FB MMIO capability + double-buffer contract | ✅ PoC |
| **P5** | K-IA DMA pin + Cortex mmap pesos (AS dedicado) | ✅ PoC |
| **P6** | Ring3 user-mode real (`iretq` + stub USER + Cap::ENTER_USER + return) | ✅ PoC |

Roadmap explícito: **MVP C → Hermes/JARBAS → K-IA → Cortex mmap → Ring3** — P0–P6 concluídos (PoC).  
Próximos: demand-paging #PF; VirtIO vring; GGUF/FAT mmap.

---

## 5. MVP C — aceite

- Dois `AddressSpace` (L4 próprio, shallow-copy do kernel + mapas privados).
- `Cr3::write` A → B → kernel, com interrupções mascaradas na janela crítica.
- Página compartilhada com `SpscRing`; escrita num AS, leitura no outro.
- `Cap::{PING, WRITE_RING, READ_RING}` + `syscall::dispatch` via `int 0x90` (ABI staging via atomics).
- Demo após DriverInit; erro → serial WARN, boot segue.
- **P6 Ring3:** ver aceite abaixo (`user_mode.rs`).

### P3 — aceite (parcial → done mínimo)

- `capability_gate.rs`: `check` / `host_send_tcp` / `host_write_ring` + demo boot non-fatal.
- `Cap::SEND_TCP` + `SYS_SEND_TCP`; `aios_send_tcp` / `aios_write_ring` em `aios_api.rs`.
- Hermes `execute_skill`: skills net/http/tcp passam por CapGate; `wasm_rt::host_call_gated` para imports.
- Ainda sem AS separado para WASM (SFI pleno = #426).

### P4 — aceite (JARBAS FB MMIO + double-buffer)

- `jarbas_fb.rs`: `FbContract` (virt/phys/stride/w/h/bpp) a partir do FB bootloader (`GpuDevice`).
- `Cap::{MAP_FB,WRITE_FB}` + `SYS_MAP_FB` / `SYS_PRESENT_FB`; deny sem Cap + log serial.
- AS JARBAS (`AddressSpace::clone_current`) mapeia `DEMO_MAP_PAGES` do FB em `JARBAS_FB_VA`.
- `JarbasDoubleBuffer` (backheap) + `present` (cópia + stub vsync via `TIMER_TICKS`/`sfence`).
- Demo boot non-fatal após P3; sem FB → Cap-only SUCCESS; falha → WARN, boot segue.
- Path primário = UEFI/bootloader FB (VirtIO-GPU BAR = evolução). Ring3 jump = bônus futuro.

### P5 — aceite (K-IA DMA pin + Cortex weight mmap)

- `k_ia_dma.rs`: `pin_frames` / `map_pinned` / `unpin` opcional; Cap `PIN_DMA`/`MAP_DMA`; AS K-IA em `K_IA_DMA_VA`.
- Stub VirtIO: phys addr logado como “buffer pinned ready”; ring/vring wiring = follow-up.
- `cortex_mmap.rs`: aloca N páginas peso simuladas, mapeia em `CORTEX_WEIGHT_VA` (eager); Cap `MAP_WEIGHTS`.
- Demand-paging (#PF first touch) e mmap GGUF/FAT = TODO documentado; PoC = memória simulada.
- Demo boot non-fatal pós-P4: deny → pin+map DMA → mmap pesos + touch → restore CR3; falha frame alloc → Cap-only SUCCESS / WARN.
- `SYS_PIN_DMA` / `SYS_MAP_DMA` / `SYS_MAP_WEIGHTS` em `syscall.rs`.

### P6 — aceite (Ring3 user-mode real)

- GDT: `kernel_data` + `user_code` + `user_data` (DPL=3); TSS `privilege_stack_table[0]` (RSP0).
- IDT `int 0x90` com DPL=3; `Cap::ENTER_USER` + `SYS_EXIT_USER`.
- `address_space::map_user_page` propaga `USER_ACCESSIBLE` em toda a cadeia PT.
- `user_mode.rs`: stub (marker + `int 0x90`) em páginas USER dedicadas; `enter_user_mode` via `iretq` (IF=0); return salva RIP/RSP e `jmp` kernel; deny sem Cap.
- Demo boot non-fatal pós-P5; #GP/#PF durante demo → WARN + restore (não halt).
- Flag `TRY_ENTER_RING3` para disable se WHPX/QEMU instável (🟡 parcial).
- Limitação: PoC single-threaded; sem ELF loader / preemptive usermode; shallow L4 ainda compartilha PTs do kernel.

---

## 6. Consequências

- Positivo: prova hardware-level de isolamento de página + IPC shared-memory sem reinventar drivers.
- Negativo: shallow-copy L4 compartilha PageTables inferiores do kernel — AS ainda não é isolamento forte contra o kernel (intencional no PoC).
- EventBus continua pub/sub in-process até migração gradual para rings cross-AS.
