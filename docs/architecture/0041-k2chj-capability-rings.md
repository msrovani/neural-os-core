# ADR-0041: K²CHJ Capability-Based Rings + SFI

**Data:** 2026-07-14  
**Status:** Accepted — P0–P9 PoC complete (monólito; non-fatal demos)  
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
- Hermes WASM SFI completo; VirtIO ring DMA real (QUEUE_NOTIFY); streaming GGUF >RAM
- Reescrever Agency / drivers / Pacotes A+B

**Nota P6:** user-mode Ring 3 via `iretq` + stub + `SYS_EXIT_USER` é PoC boot non-fatal (não scheduler multi-task).  
**Nota P7:** demand-paging #PF cura lazy weights (frames pré-alocados); GGUF/FAT = **P9**.  
**Nota P8:** VirtIO vring layout-compatible sobre DMA pin; NIC live observe-only (path paralelo).  
**Nota P9:** GGUF/FAT file-backed mmap — pré-fill frames no register; #PF só PRESENT (sem I/O no fault).

---

## 3. Gap analysis (visão × realidade)

| Claim visão | Status | Evidência | Esforço | Risco boot |
|-------------|--------|-----------|---------|------------|
| K-Nano Ring 0 exclusivo CR3/GDT/IDT | **Parcial** | `memory.rs` CR3 único; `interrupts.rs` GDT/IDT globais | G | Alto se CR3 errar |
| Slab / lock-free scheduling, sem heap no path crítico | **Parcial** | `slab.rs`, `agent-core` RR; heap ainda no boot path | M | Médio |
| K-IA Ring 3 + MMIO / VirtIO / DMA pin | **Parcial** (P5+P8 PoC) | `k_ia_dma` + `virtio_vring` + Cap PIN/MAP_DMA/VRING_SETUP; live NIC untouched | G | Médio |
| Cortex Ring 3 + mmap pesos | **Parcial** (P5+P7+P9) | `cortex_mmap` + `demand_page` + `gguf_mmap`; Cap MAP_WEIGHTS/DEMAND_PAGE/MAP_FILE; FAT pré-fill | G | Baixo |
| Hermes WASM SFI + host caps | **Parcial** | `wasm*.rs`, `trust::check_syscall` — sem AS separado | M | Baixo |
| JARBAS Ring 3 + FB MMIO + VSync | **Parcial** (P4 PoC Ring0+AS) | `jarbas_fb.rs` + Cap MAP/WRITE_FB | G | Médio |
| IPC só ring lock-free entre AS | **Fictício** → **MVP C parcial** | EventBus in-process; MVP C: SPSC shared pages | M | Baixo se isolado |
| Capability autoritativa por operação | **Parcial** | EventBus `CapabilityToken`; MVP C: `Cap` bitflags + syscall | P | Baixo |
| Dois address spaces + CR3 switch | **Fictício** → **MVP C** | Novo: `address_space.rs` | M | Médio (mitigado: non-fatal) |
| Ring3 CPL=3 real (`iretq`) | **Fictício** → **P6 PoC** | GDT user + TSS.RSP0 + `user_mode.rs` | G | Médio (non-fatal + fault abort) |

---

## 4. Prioridades P0 → P9

| Pri | Item | Status pós-MVP C |
|-----|------|------------------|
| **P0** | Gap documentado (esta ADR) | ✅ |
| **P1** | ADR curto + non-goals | ✅ |
| **P2** | **MVP C:** 2 AS + CR3 switch + ring SPSC shared + Cap + trap `int 0x90` + demo boot non-fatal | ✅ PoC |
| **P3** | Hermes WASM host-functions por Cap (sem AS full) | ✅ CapGate + SEND_TCP/WRITE_RING |
| **P4** | JARBAS FB MMIO capability + double-buffer contract | ✅ PoC |
| **P5** | K-IA DMA pin + Cortex mmap pesos (AS dedicado) | ✅ PoC |
| **P6** | Ring3 user-mode real (`iretq` + stub USER + Cap::ENTER_USER + return) | ✅ PoC |
| **P7** | Demand-paging via #PF (lazy Cortex weights) | ✅ PoC |
| **P8** | VirtIO vring wiring sobre DMA pin | ✅ PoC |
| **P9** | GGUF/FAT file-backed mmap sobre demand-paging | ✅ PoC |

Roadmap explícito: **MVP C → … → Ring3 → demand-paging → VirtIO vring → GGUF/FAT mmap** — P0–P9 concluídos (PoC).  
Próximo: SFI pleno Hermes / ELF usermode / QUEUE_NOTIFY real.

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
- Stub VirtIO (pré-P8): phys addr logado como “buffer pinned ready”; ring/vring wiring = **P8**.
- `cortex_mmap.rs`: aloca N páginas peso simuladas, mapeia em `CORTEX_WEIGHT_VA` (eager); Cap `MAP_WEIGHTS`.
- Demand-paging (#PF first touch) = **P7**; mmap GGUF/FAT = TODO; PoC = memória simulada.
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

### P7 — aceite (demand-paging via #PF)

- `demand_page.rs`: registry global (IrqSafeLock) de VAs lazy; frames **pré-alocados** no register (path #PF sem `GLOBAL_ALLOCATOR`).
- `AddressSpace::reserve_page`: caminho PT CoW + leaf NOT PRESENT; `install_present_leaf_current` só instala leaf no CR3 atual.
- `cortex_mmap::mmap_weights_lazy` + Cap `MAP_WEIGHTS|DEMAND_PAGE` / `SYS_DEMAND_PAGE`; deny sem Cap.
- `#PF` handler: se CR2 em range → map PRESENT + return (retry); senão comportamento anterior (count/warn/hlt).
- Demo boot non-fatal pós-P6: lazy 4 pages → switch CR3 → first-touch R/W → verify magic → restore; falha → WARN.
- Limitação: PoC simulado (não GGUF/FAT); cure usa try_lock (se falhar, não cura); USER leaf opcional no registry.

### P8 — aceite (VirtIO vring + DMA pin)

- `virtio_vring.rs`: Virtqueue layout-compatible (`Desc`+`AvailRing`+`UsedRing` espelhando `virtio_net`); Cap `VRING_SETUP` / `SYS_VRING_SETUP`.
- Backing: `k_ia_dma::pin_frames` (4 pages: desc|avail|used|payload); `Desc.addr` = phys pinnado (zero-copy claim).
- Path paralelo: se `VIRTIO_DEV` presente, loga `rx/tx_queue_phys` **sem mutar** filas live (NIC intacto).
- Sem device VirtIO: PoC layout-only ainda = SUCCESS documentado.
- Demo boot non-fatal pós-P7: deny Cap → pin+setup SUCCESS → log phys/indices; falha frame → Cap-only / WARN.

### P9 — aceite (GGUF/FAT file-backed mmap)

- `gguf_mmap.rs`: localiza blob no FAT (`BITNET.BIN`/`HWEXPRT.BIN`/…); pré-lê 1–4 páginas via `read_file_range` em frames alocados **antes** do #PF.
- Cap `MAP_FILE` (+ `MAP_WEIGHTS|DEMAND_PAGE`) / `SYS_MAP_FILE`; deny sem Cap; CapGate `aios_map_file`.
- Integra `demand_page::register_lazy` em `FILE_WEIGHT_VA` — leaf NOT PRESENT; first-touch só instala PRESENT (sem I/O no fault path).
- Fallback documentado se arquivo ausente: stub magic `NFIL` + WARN (non-fatal) ou Cap-only SUCCESS.
- Demo boot non-fatal pós-P8: deny → mmap → touch → verify magic GGUF/`0xBE11BE11`/fallback → restore CR3.
- Limitação: PoC = prefixo do arquivo (não streaming 8GB); parser GGUF completo permanece em `gguf.rs`.

---

## 6. Consequências

- Positivo: prova hardware-level de isolamento de página + IPC shared-memory sem reinventar drivers.
- Negativo: shallow-copy L4 compartilha PageTables inferiores do kernel — AS ainda não é isolamento forte contra o kernel (intencional no PoC).
- EventBus continua pub/sub in-process até migração gradual para rings cross-AS.

---

## 7. Real vs stub (checklist operacional)

| Peça | Real | Stub / limite |
|------|------|----------------|
| Pacotes A+B boot | ✅ | — |
| P0–P2 AS/CR3/SPSC/Cap/int 0x90 | ✅ | Shallow L4 |
| P3 CapGate | ✅ | SFI/AS WASM pleno = #426 |
| P4 JARBAS FB | ✅ | VSync stub; bootloader FB |
| P5 DMA + mmap | ✅ | Pesos eager simulados |
| P6 Ring3 iretq | ✅ código | Untested QEMU estável; sem ELF/preempt |
| P7 demand-page #PF | ✅ | Sem I/O no fault |
| P8 VirtIO vring | ✅ layout+pin | Sem QUEUE_NOTIFY; NIC untouched |
| P9 GGUF/FAT mmap | ✅ pré-fill | Prefixo 1–4 pág.; sem streaming |

**Checklist P0–P9:** todos ✅ PoC.

## 8. Próximos

1. Validar Ring3 em QEMU UEFI (TRY_ENTER_RING3).
2. SFI WASM + Cap contract (#426).
3. QUEUE_NOTIFY VirtIO real (path paralelo seguro).
4. On-fault I/O seguro / streaming GGUF > prefixo.
5. ELF usermode / preempt (após Ring3 estável).
