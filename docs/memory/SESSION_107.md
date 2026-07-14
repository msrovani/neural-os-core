# SESSION 107 — Boot A/B → ADR-0041 Capability Ladder (P0–P9)

**Data:** 2026-07-14  
**Sprint:** 107 (capability ladder + Voice I/O em paralelo no roadmap)  
**Status:** Pacotes A+B + MVP C + P3–P9 ✅ PoC (commits `9bb1382`…`49c4301`)

---

## Fluxo da sessão

1. **Boot audit** → STI/PIC, stack ≥2MB, fases/`BOOT_PHASE`.
2. **Pacote A** — `init_phase` RR, DiagnosticSkill, docs heap.
3. **Pacote B** — `init_platform_sync` antes de drivers; Agency → EventDriven.
4. **MVP C (P2)** — 2 AS + CR3 + SharedSpscRing + Cap + `int 0x90` (non-fatal).
5. **P3** CapGate Hermes · **P4** JARBAS FB · **P5** DMA pin + Cortex mmap.
6. **P6** Ring3 `iretq`/stub/`ENTER_USER` · **P7** demand-page #PF.
7. **P8** VirtIO vring layout sobre pin · **P9** GGUF/FAT mmap pré-fill + lazy.

## Descobertas

- Syscall soft = `int 0x90` (0x80–0x82 = IPI). Demos capability **sempre non-fatal**.
- Shallow-copy L4 ≠ isolamento forte contra o kernel.
- Ring3 é PoC de código — **não tratar como usermode estável** até prova QEMU UEFI.
- #PF cura só PRESENT (frames pré-preenchidos); I/O no fault = fora de escopo.
- P8 não mexe no NIC live; QUEUE_NOTIFY = follow-up.
- Agency EventDriven sem eventos fica ociosa (esperado).

## Próximo

- Voice I/O pipeline (foco produto Sprint 107).
- Capability: SFI WASM (#426) · ELF usermode · QUEUE_NOTIFY · streaming/on-fault seguro.
