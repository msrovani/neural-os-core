# SESSION INDEX — neural-os-core (42 sessões)

**Propósito:** Catálogo de todas as sessões de desenvolvimento. Cada sessão documenta o que foi feito, descobertas, decisões e bloqueios.  
**Uso:** Consulte este índice antes de iniciar qualquer trabalho para saber se um caminho já foi trilhado.  
**Formato:** `SESSION_NNN.md — Título | Sprint | Principais descobertas | Lições aprendidas`

---

## Sessões Recentes (Sprints 79-83)

| Sessão | Sprint | Bloco | Título | Principais Descobertas |
|--------|--------|-------|--------|----------------------|
| 080 | 80 | 24 | AVX2 Debug + WHPX Detection | WHPX emula VEX como VM exits (2x+ lento). `has_avx2()` detecta hypervisor via CPUID. Row buffer substitui unpack_all (17MB→6.9KB). FFN gate+up = 52% do tempo. |
| 082 | 97 | — | RustCoder Expert + Trinity MoE | Expert Rust treinado (hidden=128, 6L, 1.6M params) com 41.200 amostras na GTX 1050 (loss 0.34). RUSTCODER_MODEL global. Fast-path no HermesAgent. Loading da FAT32. |
| 083 | 86-87 | 30-31 | JARVIS Persona + Security | SOUL.md FAT32, 4 compressoes, Notification 4 urgencias, SlabBuddy. I1-I4 completos, AUDIT_TRAIL global, AHCI instanciado. |
| 089 | 88-89 | 32-33 | JARVIS Emotion + Cache + SleepCycle+mbedding | ADE real, Pipeline 16 stages wireados, edge-dhcp fix. SleepCycle 5 fases, KG bitemporal, BGE semantic_search, burn-flex. |
| 079 | 79 | 23 | LLM Infrastructure + Xuvisco | BitNet-b1.58 850M integrado. BPE tokenizer. QEMU loader a 4GB. Xuvisco fix: VGA sequencer I/O (0x3C4/0x3C5) ao invés de write a 0xB8000. **ATENÇÃO:** 0xB8000 não mapeado pelo bootloader UEFI/OVMF. |

## Sessões SMP + GPU Research (Sprint 81-83)

| Sessão | Sprint | Bloco | Título | Principais Descobertas |
|--------|--------|-------|--------|----------------------|
| (ADR-0037) | 81-83 | 21a/b/e | SMP Foundation + Work-Stealing + Polimento | SPSC ring (bbqueue), IPI vetorizado, PerCpu, Chase-Lev work-stealing, parallel-for AVX2, AgentScheduler multicore, per-CPU slab. ✅ Completos. |

## Sessões Anteriores (Sprints 1-78)

| Sessão | Sprint | Título | Tópicos Chave |
|--------|--------|--------|---------------|
| 001 | 1 | Initial Setup | Toolchain, bootloader, primeira compilação |
| 002 | 2 | VGA + Serial | VGA text mode, serial logging |
| 003 | 3 | IDT Setup | Interrupt Descriptor Table, handlers |
| 004 | 4 | Memory + Paging | Page tables, OffsetPageTable, heap init |
| 005 | 5 | SIMD + FPU | SSE/AVX enablement, CR0/CR4 |
| 006 | 6 | Neural Primitives | Tensor, matmul, libm |
| 007 | 7 | Intent Router MLP | MLP 16→8→3, classification |
| 008 | 8 | PIC + Watchdog | PIC remap, watchdog timer |
| 009 | 9 | Page Fault Handler | Page fault recovery, self-heal |
| 010 | 10 | SMP + APIC | APIC init, SMP bootstrap |
| 011 | 11 | BitLinear | BitLinear layer, ternary weights |
| 012 | 12 | 2-bit Packing | quantize_to_packed, packing format |
| 013 | 13 | Executive Summary | Estado da arte 2026, roadmap |
| 019 | 19 | PCI + ACPI + SMP | PCI scan, MADT parser, SMP boot |
| 021 | 21 | ATA + NVMe | ATA PIO, NVMe driver |
| 022 | 22 | FAT12 + USB | FAT12 filesystem, USB xHCI |
| 023 | 23 | RTL8139 + Network | RTL8139 driver, smoltcp integração |
| 024 | 24 | Network Fixes | e1000 TDT fix, DHCP, ARP |
| 025 | 25 | Neural Cortex | Cortex::think(), 12 intenções |
| 026 | 26 | Transformer Engine | Attention, BitNet 4 layers |
| 027 | 27 | Cortex LLM Daemon | LLM_REQUEST/LLM_RESPONSE, 9600+ ticks |
| 028 | 28 | HW-Aware LLM | HwIdentifySkill, 23K PCI IDs |
| 029 | 29 | USB xHCI Driver | xHCI port scan, HID boot |
| 030 | 30 | USB HID | Keyboard via xHCI |
| 031 | 31 | Hardware Capabilities | 25 pares capability→class→driver |
| 032 | 32 | Self-Healing Init | FailureClass, SelfHeal::analyze() |
| 033 | 33 | Exception Handlers | Page Fault, Double Fault, GPF com SelfHeal |
| 034 | 34 | Respawn Queue | RESPAWN_QUEUE, corrective prompting |
| 035 | 35 | Feedback Loop | lessons, already_tried(), alternativas |
| 036 | 36 | Boot Refactor | 5 mini-sprints em 1 bloco |
| 037 | 37 | Agent/Skill-First | Paradigma: tudo é agente ou skill |
| 038 | 38 | Migration | 8 tasks → 8 agents |
| 056 | 56 | FAT32 + HW Real | Boot em notebook físico, FAT32 |
| 059 | 59 | Bootloader 0.11 | UEFI framebuffer 1280×720 |
| 061 | 61 | Desktop | DisplayAgent, framebuffer console |
| 062 | 62 | Filesystem | VFS Layer, MHI ARC bridge |
| 065 | 65 | Network Evolution | DHCP, ARP, VirtIO-net |
| 066 | 66 | GPU Architecture | GPU detection, VRAM tier, Intel ring |
| 067 | 67 | Auto-Skills | Skill generation, TaskPattern |
| 068 | 68 | USB Mass Storage | USB-MSC bulk, BOT protocol |
| 079 | 79 | LLM Infrastructure | BitNet-b1.58, BPE, AVX2, Trinity MoE |
| 080 | 80 | AVX2 Debug | WHPX detection, KV Cache, timing |

---

## Lições Críticas (NÃO REPETIR)

Estes são caminhos já trilhados que terminaram em dead-end ou soluções já encontradas:

1. **Xuvisco no boot (Sprint 71, 79):** UEFI/OVMF não mapeia 0xB8000 → page fault pre-IDT. Solução: VGA sequencer I/O ports (0x3C4/0x3C5) para Screen Off. **Nunca escrever em 0xB8000 antes da IDT.**

2. **AVX2 sob WHPX (Sprint 80):** Instruções VEX/AVX2 causam VM exit (~10k+ ciclos). Scalar é 2x+ rápido. `has_avx2()` deve detectar hypervisor via CPUID 0x40000000 e retornar false.

3. **e1000 TDT bug (Sprint 23-24):** `send()` escrevia REG_TDT = idx (== TDH) → hardware via ring vazio. **TDT = (idx+1) % NUM_DESC.** NUM_DESC RX mínimo = 48 para 82540EM.

4. **Ramdisk via bootloader (Sprint 79):** FAT partition autosized ~64MB insuficiente para modelos >100MB. **Usar QEMU loader (`-device loader,addr=0x100000000`) para dev, NVMe/FAT32 para HW real.**

5. **QEMU loader com -m 2G (Sprint 79):** Modelo em 512MB conflita com frame allocator do bootloader. **Usar -m 4G+ e addr=0x100000000** (acima de 4GB).

6. **VirtIO-GPU GET_DISPLAY_INFO (Sprint 45):** Resposta 0x0 no QEMU TCG. Bug de emulação. **Framebuffer UEFI é mais confiável.**

7. **FAT12 removido (Sprint 75):** FAT32-only. 102 LOC eliminados. **Novos FS devem ser FAT32+.**

8. **Partition mask 0x1C (Sprint 74):** Hidden FAT32 LBA. Bootloader aceita (mbr_nostd mapeia 0x1C→Fat32). SO não monta. **Usar 0x0C para compatibilidade com outros OS.**

9. **TPM fallback (Sprint 74):** TPM ausente → 0xFFFF FFFF no probe. **Fallback silencioso.** Ed25519 é enforcement primário.

10. **Hermes event-driven (Sprint 76):** ReAct cycle só avança com entrada real. **84 linhas/seg → 0 quando ocioso.**

---

## MAPA DE MEMÓRIA (MemPalace + Docs)

| Domínio | Onde encontrar | Propósito |
|---------|---------------|-----------|
| Estado atual | `docs/memory/STATE.md` | Versão, sprint atual, arquitetura, pendências |
| Ideias | `docs/memory/IDEA_BANK.md` | 354 ideias catalogadas com status e sprint |
| Decisões | `docs/architecture/*.md` | 38 ADRs (ADR-0001 a ADR-0037) |
| Plano | `docs/sprint-plan-84-95.md` | 9 sprints (84-95) com items do IDEA_BANK |
| Checklist | `docs/TODO.md` | Checklist mestre com sub-itens, goals, dificuldades |
| Sessões (aprendizado) | `docs/memory/SESSION_*.md` | 42 sessões com descobertas e correções |
| Este índice | `docs/memory/SESSION_INDEX.md` | Catálogo de sessões + lições críticas |
| Código fonte | `crates/neural-kernel/src/` | Kernel bare-metal (135+ arquivos Rust) |
| Workspace crates | `crates/agent-core/`, `crates/skill-registry/`, `crates/event-bus/`, `crates/ticket-lock/` | Suporte ao kernel |
| Config VM | `tools/` | Scripts de build, QEMU launch, image creation |
