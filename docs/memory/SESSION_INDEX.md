# SESSION INDEX — neural-os-core

**Propósito:** Catálogo de sessões. Sessions de Sprints 1-68 foram arquivadas (conhecimento consolidado no código e neste índice). Sessions de Sprint 79+ mantidas individualmente.

---

## Sessões Mantidas (Sprint 79+)

| Sessão | Sprint | Bloco | Título | Principais Descobertas |
|--------|--------|-------|--------|----------------------|
| 079 | 79 | 23 | LLM Infrastructure | BitNet-b1.58 850M, BPE, Trinity MoE, QEMU loader, Xuvisco fix |
| 080 | 80 | 24 | AVX2 Debug + WHPX | WHPX emula VEX como VM exits. has_avx2() detecta hypervisor via CPUID. KV Cache 200× speedup |
| 081 | 81-83 | 21a/b/e | SMP + GPU Research | SPSC ring, IPI, PerCpu, Work-Stealing, GPU architecture |
| 082 | 97 | — | RustCoder Expert | Expert Rust (1.6M params) treinado com 41.2K amostras |
| 083 | 86-87 | 30-31 | JARVIS Persona+Security | SOUL.md, I1-I4, AUDIT_TRAIL, AHCI |
| 089 | 88-89 | 32-33 | JARVIS Emotion+Cache+SleepCycle | ADE, Pipeline 16 stages, KG bitemporal, BGE |
| 093 | 93+ | — | SDIO Pipeline | 45 packs, 95.812 entradas, loss 0.38 |

## Sessões Anteriores (Sprints 1-68 — Arquivadas)

Conhecimento consolidado no código-fonte e no `SESSION_INDEX.md` abaixo. Arquivos individuais removidos.

| Sprints | Tópicos Chave |
|---------|---------------|
| 1-13 | Toolchain, VGA, IDT, Memory, SIMD, Tensor, BitLinear, 2-bit Packing, PIC, SMP |
| 19-38 | PCI, ACPI, ATA, FAT12, RTL8139, e1000, Neural Cortex, Transformer, xHCI, Self-Healing |
| 56-68 | FAT32, Bootloader 0.11, DisplayAgent, VFS, GPU Architecture, Auto-Skills, USB-MSC |

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
