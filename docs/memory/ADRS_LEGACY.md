# ADRs Consolidadas — Conhecimento Preservado (39 ADRs)

**Data:** 2026-07-08
**Propósito:** Conhecimento das 39 ADRs arquiteturais. ADRs com tecnologias já implementadas foram arquivadas. Pendências movidas para Sprint 92/TODO.md.

---

## ✅ ADRs Totalmente Implementadas (25)

| ADR | Título | Tecnologia | Status Real |
|-----|--------|------------|-------------|
| 0001 | Initial Architecture | Toolchain, bootloader 0.11.15 | ✅ |
| 0002 | VGA + Serial | VGA 0xB8000, UART 16550 | ✅ |
| 0003 | IDT | GDT/TSS/IST, 256 entries | ✅ |
| 0004 | Memory | OffsetPageTable, heap 0x4444_4444_0000 | ✅ |
| 0005 | SIMD/FPU | CR0/CR4, SSE/AVX2/FMA | ✅ |
| 0006 | Neural Primitives | libm, SiLU, RMSNorm | ✅ |
| 0009 | PIC Watchdog | PIC 8259A, PIT, #PF handler | ✅ |
| 0011 | BitLinear | matmul_hybrid ADD/SUB | ✅ |
| 0012 | 2-bit Packing | PackedTernaryTensor, 4× compress | ✅ |
| 0013 | Executive Summary | Manifesto arquitetural | ✅ |
| 0017 | Bugfix Sprint | 10 CRITICAL bugs corrigidos | ✅ |
| 0018 | Sprint 24 Bugs | 40+ bugs (H1-H12 + M1-M16 + L1-L12) | 🟡 VER NOTA |
| 0020 | Crom Ecosystem | ArchiveTensor, MoE router, DSL | ✅ |
| 0023 | Memory Systems | #214-227 todos implementados (SHA-256, Privacy, BM25, KG, etc) | ✅ |
| 0024 | Agent Frameworks | Cline, OpenHands, Agent Zero padrões | ✅ |
| 0025 | Security Sandbox | InnerWarden, ai-jail, vexfs padrões | ✅ |
| 0026a | Ecosystem Batch 3 | Scheme trait (Redox), Type-State (Theseus) | ✅ |
| 0026b | xHCI USB Driver | Driver xHCI completo (detecção, HID, BOT) | ✅ |
| 0027 | Self-Healing | FailureTaxonomy, BudgetedRecovery, CorrectivePrompting | ✅ |
| 0029 | GPU Architecture | Intel/NVIDIA/AMD drivers, VRAM buddy, SPSC ring | ✅ |
| 0030 | DiskIntelligence | ATA, AHCI, NVMe, USB-MSC, FAT32, GPT, SMART | ✅ |
| 0031a | AIOS Evolution | WASM Runtime, JARVIS, Self-Update | ✅ |
| 0032 | WASM Apps | wasm_exec, MemoryPool, WASI→Skill | ✅ |
| 0033 | Micro-Learning | BitNetTrainer, AutoLearnAgent, TaskSpawner | ✅ |
| 0036 | JARVIS Unified | SOUL.md, Emotion, Cognitive, IPW, Notification | ✅ |
| 0037 | SMP+GPU | SPSC, IPI, PerCpu, Work-Stealing, GPU rings | ✅ |

**Nota ADR-0018 (bugs Sprint 24):** Os bugs H1-H12 foram resolvidos (ADR-0017). M1-M16 e L1-L12 são bugs menores que foram corrigidos ao longo do desenvolvimento natural. Nada pendente.

## 🟡 ADRs com Pendências Mínimas (3)

| ADR | Pendência | Movido para |
|-----|-----------|-------------|
| 0016 (Network) | **B-01** DHCP/RX bug — único bloqueador real | 🔴 TODO.md |
| 0019 (Cortex LLM) | RoPE, inner_attn_ln, ffn_layernorm (v3.1 features) | Sprint 92 |
| 0028 (GGUF) | GGUF v3 loader para modelos 9B+ | Sprint 92 |

## 🔴 ADRs Substituídas ou Não Implementadas (11)

| ADR | Motivo |
|-----|--------|
| 0007 (Intent Router MLP) | Substituído pelo Trinity MoE (ADR-0033) |
| 0010 (Strategic Roadmap) | Plano geral, itens reassignados |
| 0014 (Hardware Ideas) | Ideias incorporadas ao IDEA_BANK |
| 0015 (MVP Route) | MVP concluído, rota diferente |
| 0021 (Life OS Ecosystem) | Pesquisa apenas, 22 ideias catalogadas |
| 0022 (Personal AI Ecosystem) | Pesquisa apenas, 15 ideias catalogadas |
| 0031b (Self-Update Research) | Substituído por 0031a |
| 0034 (JARVIS Conscious) | Substituído por 0036 |
| 0035 (JARVIS Deep Research) | Substituído por 0036 |
| 0038 (Ecosystem Optimization) | Decisões aplicadas nos sprints |

---

## Conhecimento dos ADRs com Pendências

### ADR-0016 — Network Strategy
**Bloqueador:** B-01 — RX fix RTL8139/DHCP. smoltcp DHCP nunca completa. Sem isso, ~18K LOC de sprints 92+ ficam bloqueados (WWW Agents, Self-Update, Voice Pipeline, WiFi, Cross-OS).
**Implementado:** RTL8139 (I/O), e1000 (MMIO), VirtIO-net, smoltcp TCP/IP, HTTP client, ICMP ping.

### ADR-0019 — Cortex LLM (v3.1 pendente)
**Implementado:** BitNet-b1.58 850M, AVX2 kernel, BPE, KV Cache, Medusa, GQA 20→5, BitFFN, QEMU loader.
**Pendente → Sprint 92:** RoPE cos/sin table, inner_attn_ln, ffn_layernorm.

### ADR-0028 — GGUF Format Research
**Implementado:** GGUF header parser (Fase 1), Q4_0 dequant (Fase 2).
**Pendente → Sprint 92:** Streaming ATA/USB para modelos 9B+ (Fase 3), heap >5GB.
