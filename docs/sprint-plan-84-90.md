# Sprint Plan 84-90 — neural-os-core v0.84.x-0.90.x

**Data:** 2026-07-05  
**Contexto:** Bloco 21a/21b/21e completos (SMP Foundation, Work-Stealing, Polimento). Próximos blocos focados em GPU Foundations, GPU Decode, JARVIS Persona, Security e AIOS Evolution.

---

## Sprint 84 — Bloco 21c: GPU Foundations

**Objetivo:** RTX 1050 (GP107 Pascal) como device de compute. Firmware disponível em linux-firmware desde 2017.

**Itens:**
- GPU BAR0/BAR1 mapping UC (~300 LOC)
  - Mapear BARs como uncacheable para MMIO
  - Dependência: NVMe (✅)
- ACR secure boot (~600 LOC)
  - Carregar firmware signed FECS/GPCCS (disponível)
  - Dependência: BAR0 mapping
- PCIe doorbell register (~100 LOC)
  - Setup de doorbell para submissão de jobs
  - Dependência: BAR0 mapping
- GPU SPSC job ring (~300 LOC)
  - CPU enfileira, GPU consome
  - Dependência: Doorbell
- VRAM buddy allocator (~400 LOC)
  - Gerenciar 4GB GDDR5
  - Dependência: BAR1 mapping

**Total:** ~1700 LOC

**Risco:** ACR secure boot requer WPR setup + signature patching. Seguir nouveau + pascal-egpu. Reclocking NÃO é necessário para compute funcional (roda em clock padrão).

**Status:** 🟡 Agendado

---

## Sprint 85 — Bloco 21d: GPU Decode (BitNet offload)

**Objetivo:** Decode do BitNet roda na GPU. Prefill fica na CPU.

**Itens:**
- Agent.xpu prefill/decode split (~400 LOC)
  - CPU faz prefill, GPU faz decode
  - Dependência: GPU job ring
- GPU matmul kernel (ternário) (~300 LOC)
  - Matmul BitNet na GPU via shader
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
  - Bloqueador: 🔴 QEMU SLiRP
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

**Status:** 🔴 Bloqueado (B-01)

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

Após pesquisa expandida (ADR-0037 v4), confirmado para **nosso i5-6400 + RTX 1050 (GP107)**:
- **CPU**: AVX2 + FMA é o teto (sem AMX, AVX-512, APX, NPU)
- **GPU**: firmware NVIDIA Pascal **disponível** em linux-firmware (FECS/GPCCS signed)
- **NPU**: irrelevante (sem HW, firmware fechado)
- **Foco**: SMP 4 cores primeiro (garantido), GPU segundo (viável, complexo)

---

## Dependências

- Bloco 21c depende de NVMe (✅)
- Bloco 21d depende de Bloco 21c
- Bloco 30 depende de SMP base (✅)
- Bloco 31 depende de Nenhuma
- Bloco 32+ depende de B-01 (🔴 bloqueado)
