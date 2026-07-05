# Integration: ADRs + IDEA_BANK = Sprints + TODO

**Data:** 2026-07-05  
**Objetivo:** Visão unificada de como ADRs, IDEA_BANK, Sprints e TODO se integram para guiar o desenvolvimento do neural-os-core.

---

## Fluxo de Integração

```
┌─────────────────┐
│   ADRs          │ → Decisões arquiteturais → Diretrizes técnicas
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│   IDEA_BANK     │ → Inventário de ideias → Status de cada ideia
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│   Sprints       │ → Plano de execução → Blocos por dependência
└────────┬────────┘
         │
         ↓
┌─────────────────┐
│   TODO          │ → Tarefas concretas → Itens por prioridade
└─────────────────┘
```

---

## ADRs Relevantes (Decisões Arquiteturais)

### ADR-0037: SMP + GPU Architecture
**Status:** ✅ v4 (multiplataforma genérica)  
**Decisões:**
- Suporte para qualquer processador (x86-64, ARM64, RISC-V)
- Suporte para qualquer GPU (NVIDIA, AMD, Intel, Apple Silicon)
- Suporte para qualquer NPU (AMD XDNA, Intel NPU, Apple ANE)
- HW real como critério de performance (não QEMU/VBox)
- Plano de 9 passos: SPSC ring → IPI → PerCpu → Work-stealing → GPU BAR → GPU job ring → XSched → burn-flex

**Impacto nas Sprints:**
- Bloco 21a (SMP Foundation) — SPSC ring, IPI, PerCpu
- Bloco 21b (Work-Stealing + Matmul) — Chase-Lev, parallel-for, AgentScheduler multicore
- Bloco 21c (GPU Foundations) — GPU BAR mapping, ACR secure boot, job ring, VRAM allocator
- Bloco 21d (GPU Decode) — Agent.xpu split, GPU matmul, KV cache DMA, XQueue

### ADR-0030: Disk Intelligence Agent
**Status:** ✅ Implementado  
**Decisões:**
- StorageController trait com 6 implementações
- FilesystemProbe registry com 10+ probes
- ARC cache 1MB DRAM + tier migration MHI

**Impacto nas Sprints:**
- Bloco 19 (Disk Intelligence) — Sprint 75

### ADR-0018: Security Pipeline
**Status:** ✅ Implementado  
**Decisões:**
- TrustCache (allow/deny/TTL/denylist)
- Ed25519 kernel signing
- 5 detectores (PortScan, ArpSpoof, etc)
- Graduated Enforcement (Observe→Warn→Contain→Enforce)

**Impacto nas Sprints:**
- Bloco 18 (Security) — Sprint 74

---

## IDEA_BANK (Inventário de Ideias)

### Seção 1.2: SMP / APIC / Multicore
**Status:** ✅ Bloco 21a/21b completos

| # | Item | Destino | Sprint |
|---|---|---|---|
| 18 | x2APIC mode (MSR-based) | ✅ Bloco 21a | 81 |
| 24 | PerCpu struct | ✅ Bloco 21a | 81 |
| 25 | GS.base segment register | ✅ Bloco 21a | 81 |
| 26 | INIT-SIPI-SIPI via LAPIC ICR | ✅ Bloco 21a | 81 |
| 36 | SPSC ring lockless (bbqueue) | ✅ Bloco 21a | 81 |
| 37 | `#[repr(align(64))]` cross-core | ✅ Bloco 21a | 81 |
| 38 | IPI handler registrável | ✅ Bloco 21a | 81 |
| 39 | Work-stealing Chase-Lev | ✅ Bloco 21b | 82 |
| 40 | Parallel-for AVX2 matmul | ✅ Bloco 21b | 82 |
| 41 | AgentScheduler multicore | ✅ Bloco 21b | 82 |
| 42 | Per-CPU slab allocator | ✅ Bloco 21b | 82 |

### Seção 1.5: Memory Hierarchy Index (MHI)
**Status:** 🟡 Agendado

| # | Item | Destino | Sprint |
|---|---|---|---|
| 67 | `AllocTier::Vram` → alocar no BAR da GPU | 🟡 Bloco 21c | 84 |
| 68 | `AllocTier::Nvme` → alocar no NVMe via SFS | ⏳ Pós-MVP | 24+ |
| 69 | `AllocTier::Hdd` → cold storage | ⏳ Pós-MVP | 24+ |

---

## Sprints (Plano de Execução)

### Sprint 84 — Bloco 21c: GPU Foundations
**Origem:** ADR-0037 (item 5-9 do plano de 9 passos)  
**IDEA_BANK:** #67 (AllocTier::Vram)  
**TODO:** B-03 (NVIDIA PFIFO), B-04 (AMD PM4)

**Itens:**
- GPU BAR0/BAR1 mapping UC (~300 LOC)
- ACR secure boot (~600 LOC)
- PCIe doorbell register (~100 LOC)
- GPU SPSC job ring (~300 LOC)
- VRAM buddy allocator (~400 LOC)

**Total:** ~1700 LOC

### Sprint 85 — Bloco 21d: GPU Decode
**Origem:** ADR-0037 (item 6-9 do plano de 9 passos)  
**IDEA_BANK:** N/A (nova funcionalidade)  
**TODO:** N/A (novo bloco)

**Itens:**
- Agent.xpu prefill/decode split (~400 LOC)
- GPU matmul kernel (ternário) (~300 LOC)
- CPU→GPU KV cache DMA (~200 LOC)
- XQueue (preemptível) (~600 LOC)

**Total:** ~1500 LOC

### Sprint 86 — Bloco 30: JARVIS Persona + Cognitive
**Origem:** ADR-0034 (JARVIS Unified Interaction Layer)  
**IDEA_BANK:** N/A (nova funcionalidade)  
**TODO:** N/A (novo bloco)

**Itens:**
- SOUL.md Personality Engine (~300 LOC)
- IPW Monitor (RAPL MSR 0x610) (~150 LOC)
- Session Compression (~200 LOC)
- Notification Gate (~200 LOC)
- Sessionless Thread (~100 LOC)
- Emotion Analysis (BitNet 7 emoções) (~250 LOC)
- Capability Contracts + Consent Gates (~200 LOC)
- Skill Discovery (DSPy/ACE) (~300 LOC)
- ADE Pipeline (~200 LOC)
- Semantic Cache (5-tier) (~150 LOC)
- Dreaming/Consolidation (~200 LOC)
- Ego Layer (~250 LOC)
- Proactive Heartbeats (~100 LOC)
- Tool-State Save Game (~100 LOC)
- Auto-Skill Generation (~150 LOC)
- Babel-Index (~100 LOC)
- SleepCycle Agent (~780 LOC)

**Total:** ~3280 LOC

### Sprint 87 — Bloco 31: JARVIS Security + AHCI
**Origem:** ADR-0018 (Security Pipeline) + ADR-0030 (Disk Intelligence)  
**IDEA_BANK:** N/A (nova funcionalidade)  
**TODO:** N/A (novo bloco)

**Itens:**
- Fail-Closed Safety Invariant (~200 LOC)
- Merkle Audit Trail (~200 LOC)
- Fluid Persona (~100 LOC)
- AHCI driver (SATA 6G NCQ) (~700 LOC)

**Total:** ~1200 LOC

### Sprint 88+ — Bloco 32+: AIOS Evolution
**Origem:** ADR-0031 (AIOS Self-Update WASM JARVIS)  
**IDEA_BANK:** N/A (nova funcionalidade)  
**TODO:** B-01 (DHCP/DNS/HTTP), B-11 (WWW Infrastructure), B-12 (Browser Agent), B-13 (MCP TCP), B-17 (WWW Agents restantes), B-27 (Plugin Hub)

**Itens:**
- B-01 RX fix (RTL8139 DHCP/RX) (~500 LOC) 🔴 bloqueador
- WWW Agents (~2600 LOC) 🔴 depende de B-01
- Self-Update Agent (~800 LOC) 🔴 depende de B-01
- Plugin Hub + Marketplace (~400 LOC) 🔴 depende de B-01
- Voice Pipeline (~1600 LOC) 🔴 depende de B-01
- Multi-device sync (~300 LOC) 🔴 depende de B-01
- SKYNET Mesh (~300 LOC) 🔴 depende de B-01
- WiFi (~1000 LOC) 🔴 depende de B-01

**Total:** ~7500 LOC

---

## TODO (Tarefas Concretas)

### 🔴 Bloqueantes (3 itens)

**B-01: DHCP/DNS/HTTP — Rede funcional**
- **Origem:** ADR-0016 (Network Strategy)
- **IDEA_BANK:** #250-255 (Arquitetura Neural de Rede)
- **Bloqueia:** B-11, B-12, B-13, B-17, B-27 (toda cadeia WWW)
- **Esforço:** 🔴 3-7 dias (incerto — depende do diagnóstico)

**B-03: NVIDIA PFIFO PUSH_BUFFER + FALCON firmware**
- **Origem:** ADR-0029 (GPU Architecture)
- **IDEA_BANK:** N/A (nova funcionalidade)
- **Bloqueia:** Nenhum (folha na DAG)
- **Esforço:** 🔴 ~1500 LOC, 3-6 semanas

**B-04: AMD PM4 ring buffer real**
- **Origem:** ADR-0029 (GPU Architecture)
- **IDEA_BANK:** N/A (nova funcionalidade)
- **Bloqueia:** Nenhum (folha na DAG)
- **Esforço:** 🔴 ~500 LOC, 2-4 semanas

### 🟠 Alta (4 itens)

**B-11: Network Infrastructure (WWW 63.1)**
- **Origem:** ADR-0016 (Network Strategy)
- **IDEA_BANK:** #117-123 (Rede/Network Stack)
- **Bloqueia:** B-12, B-13, B-17, B-27
- **Esforço:** 🔴 ~400 LOC, 1-2 semanas

**B-12: Browser Agent (WWW 63.2)**
- **Origem:** ADR-0016 (Network Strategy)
- **IDEA_BANK:** N/A (nova funcionalidade)
- **Bloqueia:** Nenhuma (depende de B-11)
- **Esforço:** 🔴 ~500 LOC, 1-2 semanas

**B-13: MCP Agent — TCP listener**
- **Origem:** ADR-0032 (WASM Agent Apps)
- **IDEA_BANK:** N/A (nova funcionalidade)
- **Bloqueia:** Nenhuma (depende de B-01)
- **Esforço:** 🟡 ~200 LOC, 3-5 dias

**B-17: WWW Agents restantes (63.3-63.7)**
- **Origem:** ADR-0016 (Network Strategy)
- **IDEA_BANK:** N/A (nova funcionalidade)
- **Bloqueia:** Nenhuma (depende de B-11)
- **Esforço:** 🔴 ~1700 LOC total, 4-8 semanas

---

## Matriz de Rastreabilidade

| ADR | IDEA_BANK | Sprint | TODO |
|-----|-----------|--------|------|
| ADR- #18 (SMP/APIC) | #16-42 | 81-82 | N/A (completo) |
| ADR- #19 (Disk Intelligence) | #63-74 | 75 | N/A (completo) |
| ADR- #18 (Security Pipeline) | #256-267 | 74 | N/A (completo) |
| ADR- #37 (SMP+GPU) | #67 (Vram) | 84-85 | B-03, B-04 |
| ADR- #34 (JARVIS) | N/A | 86 | N/A (novo bloco) |
| ADR- #18 (Security) + #30 (Disk) | N/A | 87 | N/A (novo bloco) |
| ADR- #31 (AIOS Evolution) | N/A | 88+ | B-01, B-11, B-12, B-13, B-17, B-27 |
| ADR- #16 (Network Strategy) | #117-123, #250-255 | 88+ | B-01, B-11, B-12, B-13, B-17 |

---

## Resumo de Esforço

| Categoria | LOC | Status |
|-----------|-----|--------|
| Bloco 21a/21b/21e (completos) | ~3.860 | ✅ |
| Bloco 21c (GPU Foundations) | ~1.700 | 🟡 Agendado |
| Bloco 21d (GPU Decode) | ~1.500 | 🟡 Agendado |
| Bloco 30 (JARVIS Persona) | ~3.280 | 🟡 Agendado |
| Bloco 31 (JARVIS Security) | ~1.200 | 🟡 Agendado |
| Bloco 32+ (AIOS Evolution) | ~7.500 | 🔴 Bloqueado |
| **Total** | **~19.040** | |

---

## Próximos Passos

1. **Sprint 84 (Bloco 21c):** GPU Foundations — RTX 1050 (GP107 Pascal)
   - Começar com GPU BAR0/BAR1 mapping UC
   - Depois ACR secure boot
   - Depois PCIe doorbell register
   - Depois GPU SPSC job ring
   - Finalmente VRAM buddy allocator

2. **Sprint 85 (Bloco 21d):** GPU Decode (BitNet offload)
   - Agent.xpu prefill/decode split
   - GPU matmul kernel ternário
   - CPU→GPU KV cache DMA
   - XQueue preemptível

3. **Sprint 86 (Bloco 30):** JARVIS Persona + Cognitive
   - 17 itens de cognitive/persona
   - SOUL.md, IPW Monitor, Session Compression, etc.

4. **Sprint 87 (Bloco 31):** JARVIS Security + AHCI
   - Fail-Closed Safety Invariant
   - Merkle Audit Trail
   - Fluid Persona
   - AHCI driver

5. **Sprint 88+ (Bloco 32+):** AIOS Evolution
   - Depende de B-01 (DHCP/DNS/HTTP)
   - WWW Agents, Self-Update, Voice Pipeline, WiFi
