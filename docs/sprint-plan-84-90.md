# Sprint Plan 84-90 — neural-os-core v0.84.x-0.90.x
# 🔴 ARQUIVO LEGADO — VEJA sprint-plan-84-95.md PARA VERSÃO COMPLETA COM TODOS OS ITENS DO IDEA_BANK

**Data:** 2026-07-05  
**Contexto:** Bloco 21a/21b/21e completos (SMP Foundation, Work-Stealing, Polimento). Próximos blocos focados em GPU Foundations, GPU Decode, JARVIS Persona, Security e AIOS Evolution.

---

## Sprint 84 — Bloco 21c: GPU Foundations

**Objetivo:** GPUs NVIDIA, AMD e Intel como devices de compute. Firmware disponível em linux-firmware e documentação pública (GPUOpen, Intel Gen, nouveau).

**Itens:**
- GPU BAR0/BAR1 mapping UC (~300 LOC)
  - Mapear BARs como uncacheable para MMIO (genérico: NVIDIA, AMD, Intel)
  - Dependência: NVMe (✅)
- ACR secure boot / PSP init / GuC loading (~600 LOC)
  - NVIDIA: FECS/GPCCS signed firmware via ACR
  - AMD: PSP firmware init para RDNA (licença MIT)
  - Intel: GuC/HuC firmware loading para Gen9+
  - Dependência: BAR0 mapping
- PCIe doorbell register (~100 LOC)
  - Setup de doorbell para submissão de jobs (genérico: qualquer GPU)
  - Dependência: BAR0 mapping
- GPU SPSC job ring (~300 LOC)
  - CPU enfileira, GPU consome (padrão comum a NVIDIA/AMD/Intel)
  - Dependência: Doorbell
- VRAM buddy allocator (~400 LOC)
  - Gerenciar VRAM: GDDR (NVIDIA/AMD) + DRAM carveout (Intel)
  - Dependência: BAR1 mapping

**Total:** ~1700 LOC

**Risco:** Secure boot varia por vendor — NVIDIA ACR requer WPR + signature patching, AMD PSP tem firmware MIT, Intel GuC é aberto mas requires minidump. Documentação: nouveau + amdgpu + i915 Linux drivers como referência. Reclocking NÃO é necessário para compute funcional (roda em clock padrão).

**Status:** 🟡 Agendado

---

## Sprint 85 — Bloco 21d: GPU Decode (BitNet offload)

**Objetivo:** Decode do BitNet roda na GPU. Prefill fica na CPU.

**Itens:**
- Agent.xpu prefill/decode split (~400 LOC)
  - CPU faz prefill, GPU faz decode
  - Dependência: GPU job ring
- GPU matmul kernel (ternário) (~300 LOC)
  - Matmul BitNet na GPU via shader (NVIDIA CUDA-style, AMD AQL, Intel GEN)
  - Dependência: GPU ring
- CPU→GPU KV cache DMA (~200 LOC)
  - Transferir KV cache por DMA
  - Dependência: GPU DMA
- XQueue (preemptível) (~600 LOC)
  - Fila de comandos GPU com preempção
  - Dependência: GPU ring

**Total:** ~1500 LOC

**Status:** 🟡 Agendado

---

## Sprint 86 — Bloco 30: JARVIS Persona + Cognitive

**Objetivo:** JARVIS ganha com paralelismo SMP. 17 itens de cognitive/persona.

**Itens:**
- SOUL.md Personality Engine (~300 LOC)
  - Dependência: SMP base
- IPW Monitor (RAPL MSR 0x610) (~150 LOC)
  - Dependência: PerCpu
- Session Compression (~200 LOC)
  - Dependência: Nenhuma
- Notification Gate (~200 LOC)
  - Dependência: Nenhuma
- Sessionless Thread (~100 LOC)
  - Dependência: Nenhuma
- Emotion Analysis (BitNet 7 emoções) (~250 LOC)
  - Dependência: BitNet (✅)
- Capability Contracts + Consent Gates (~200 LOC)
  - Dependência: Nenhuma
- Skill Discovery (DSPy/ACE) (~300 LOC)
  - Dependência: SkillIndex (✅)
- ADE Pipeline (~200 LOC)
  - Dependência: Nenhuma
- Semantic Cache (5-tier) (~150 LOC)
  - Dependência: Nenhuma
- Dreaming/Consolidation (~200 LOC)
  - Dependência: CronAgent (✅)
- Ego Layer (~250 LOC)
  - Dependência: BitNet (✅)
- Proactive Heartbeats (~100 LOC)
  - Dependência: Nenhuma
- Tool-State Save Game (~100 LOC)
  - Dependência: Nenhuma
- Auto-Skill Generation (~150 LOC)
  - Dependência: Nenhuma
- Babel-Index (~100 LOC)
  - Dependência: Nenhuma
- SleepCycle Agent (~780 LOC)
  - Dependência: CronAgent (✅)

**Total:** ~3280 LOC

**Status:** 🟡 Agendado

---

## Sprint 87 — Bloco 31: JARVIS Security + AHCI

**Objetivo:** Security avançado + AHCI driver para SATA 6G NCQ.

**Itens:**
- Fail-Closed Safety Invariant (~200 LOC)
- Merkle Audit Trail (~200 LOC)
- Fluid Persona (~100 LOC)
- AHCI driver (SATA 6G NCQ) (~700 LOC)

**Total:** ~1200 LOC

**Status:** 🟡 Agendado

---

## Sprint 88+ — Bloco 32+: AIOS Evolution

**Objetivo:** Tudo que depende de rede (LAN). B-01 é o gatekeeper.

**Itens:**
- B-01 RX fix (RTL8139 DHCP/RX) (~500 LOC)
  - 🔴 Buscar na internet: smoltcp DHCP debug, RTL8139 RX datasheet, testes em HW real com roteador físico
- WWW Agents (~2600 LOC)
  - Bloqueador: 🔴 B-01
- Self-Update Agent (~800 LOC)
  - Bloqueador: 🔴 B-01
- Plugin Hub + Marketplace (~400 LOC)
  - Bloqueador: 🔴 B-01
- Voice Pipeline (~1600 LOC)
  - Bloqueador: 🔴 B-01
- Multi-device sync (~300 LOC)
  - Bloqueador: 🔴 B-01
- SKYNET Mesh (~300 LOC)
  - Bloqueador: 🔴 B-01
- WiFi (~1000 LOC)
  - Bloqueador: 🔴 B-01

**Total:** ~7500 LOC

**Status:** 🔴 Bloqueado — primeiro passo: **buscar na internet** diagnósticos, patches e soluções para DHCP+RTL8139+smoltcp em bare-metal e QEMU

---

## Resumo de Esforço

| Sprint | Bloco | LOC | Status |
|---|---|---|---|
| 84 | 21c (GPU Foundations) | ~1700 | 🟡 Agendado |
| 85 | 21d (GPU Decode) | ~1500 | 🟡 Agendado |
| 86 | 30 (JARVIS Persona) | ~3280 | 🟡 Agendado |
| 87 | 31 (JARVIS Security) | ~1200 | 🟡 Agendado |
| 88+ | 32+ (AIOS Evolution) | ~7500 | 🔴 Bloqueado |
| **Total** | | **~15.180** | |

---

## Nota de HW Real

O sistema visa hardware real, geral e moderno:
- **CPU**: Qualquer x86-64 (AVX2, AVX-512) ou ARM64. Nossa máquina de teste atual tem i5-6400 (AVX2 + FMA, sem AMX/APX/NPU).
- **GPU**: NVIDIA (qualquer modelo com BAR0/BAR1 + firmware), AMD (RDNA+ com PM4), Intel (Gen6+ com ring buffer). Firmware disponível em linux-firmware desde 2017.
- **NPU**: Detecção ACPI/PCI futura para AMD XDNA, Intel NPU, Apple ANE.
- **AVX-512**: Suportado quando disponível (Ice Lake+), com fallpath AVX2 para CPUs sem.
- **Foco**: SMP primeiro (garantido em qualquer multi-core), GPU segundo (viável, complexo), NPU terceiro (futuro).

---

## Dependências

- Bloco 21c depende de NVMe (✅)
- Bloco 21d depende de Bloco 21c
- Bloco 30 depende de SMP base (✅)
- Bloco 31 depende de Nenhuma
- Bloco 32+ depende de B-01 (🔴 bloqueado — buscar solução na internet)
