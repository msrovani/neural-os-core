# ADR-PRE-V2: Resíduos Pós-v1.9 — Itens Não Implementados Transportados das ADRs

**Data:** 2026-07-26
**Status:** Proposed — coleção de itens residual de ADRs auditadas que não foram implementados ou estão incompletos, transferidos aqui para planejamento pós-v2.0.0 gate.
**Origem:** Auditoria ADR decrescente SESSION_220 (0075 → 0001)

---

## 1. ADR-0064 — VectorStore TF-IDF (RAG in-kernel) — ❌ REJEITADO

**Fonte:** `docs/architecture/0064-rag-db-in-kernel.md`
**Lifecycle original:** `descartada`

Item rejeitado. A crate `crates/vector-db` foi criada (F1) mas **jamais integrada** — deletada em 2026-07-26. O SGDB real (`k_ai::sgdb`, ADR-0063) cobre o caso via MemoryDoc/ART/BQ/TickvLite + embedding BGE (`k_ai::memory_systems`). TF-IDF lexical separado não é necessário.

| Subitem | Descrição | Destino |
|---------|-----------|---------|
| F1 | crate `vector-db` | Foi criada, nunca integrada → **deletada** |
| F2–F5 | Integração | Nunca feita — SGDB real (`k_ai::sgdb`) cobre |
| F6 | Embeddings neurais | Já existe em `k_ai::memory_systems` (BGE) |

**Alternativa real:** `k_ai::sgdb::layers::rag_context()` + BQ L4 hybrid + `bge_embed()`. ADR-0064 descartada, manter como referência histórica.

---

## 2. ADR-0060 A.3 — Dynamic MoE (Birth/Merge/Split)

**Fonte:** `docs/architecture/0060-bitnet-cognitivo-bei.md` §A.3
**Lifecycle original:** `fazendo`

O Trinity MoE atual (`cortex::moe`, `trinity.rs`) é estático — 3 experts fixos (hw_control, generator, etc.) com router_weight treinável. Dynamic MoE exigiria:

| Subitem | Descrição | Esforço |
|---------|-----------|---------|
| Birth | Criar novo expert em runtime baseado em necessidade detectada | ~500 LOC |
| Merge | Fundir dois experts similares (similaridade > threshold) | ~400 LOC |
| Split | Dividir expert sobrecarregado em dois especialistas | ~400 LOC |
| Lifecycle | Registrar/monitorar birth/death de experts, budget-aware | ~300 LOC |

**Total estimado:** ~1.600 LOC

---

## 3. ADR-0048/0049/0050 — GPU Compute Golden HW

**Fonte:** `docs/architecture/0048-nvidia-compute-multigeracao.md`, `0049-amd-compute-multigeracao.md`, `0050-intel-compute-multigeracao.md`
**Lifecycle original:** `fazendo` (todos)

Código dos 3 vendors implementado (ACR/NKP/Discovery/GuC) mas **não validado em HW real**:

| Vendor | Status | Pendente |
|--------|--------|----------|
| NVIDIA Pascal | ACR ✅, D2-D4 ✅, dispatch CUBIN ✅ | Validação HW real GTX 1050 |
| AMD RDNA | Discovery ✅, PSP ✅, KIQ/MES ✅ | Validação HW real gfx1030+ |
| Intel Gen9/Arc | Ring alive ✅, GuC boot ✅, COMPUTE_WALKER ✅ | Validação HW real iGPU/dGPU |

**Bloqueio:** AWAITING_HW — todos dependem de silício real para canário final.

---

## 4. ADR-0041 §Ring3/SFI — Isolamento de Produção

**Fonte:** `docs/architecture/0041-k2chj-capability-rings.md` §P09
**Lifecycle original:** `fazendo`

Ring3/SFI PoC existe (P09) mas não é isolamento de produção. Necessário para B/C do ADR-0059 (Cranelift JIT, Rust-subset nativo).

| Subitem | Descrição |
|---------|-----------|
| Ring3 iretq | syscall handler com stack switch |
| Paging UVM | User page tables + #PF demand-page |
| CapGate HW | MMIO/DMA bloqueado em Ring3 |

**Bloqueio:** Pós-v2.0.0 — item mais complexo (est. ~3.000 LOC).

---

## 5. ADR-0057 Layer S/HW — NPU + GPU Full Dispatch

**Fonte:** `docs/architecture/0057-compute-dispatch-smp-gpu-npu.md` §WS-D/WS-E
**Lifecycle original:** `completa` (gated honestamente)

GPU dispatch existe mas é **gated** — só executa se canário Ready PASS. NPU é detection-only.

| Subitem | Descrição |
|---------|-----------|
| GPU full | GPU como backend primário de matmul (não fallback) |
| NPU XDNA/Intel | Driver + dispatch real (não só detection) |

**Bloqueio:** HW real (GPU Ready + NPU detection).

---

## 6. ADR-0033 — On-Device Micro-Learning (Dynamic MoE)

**Fonte:** `docs/architecture/0033-on-device-micro-learning.md`
**Lifecycle original:** `modernizada`

AutoLearnAgent existe (SESSION_108) e faz fine-tuning on-device. Mas falta o **MoE dinâmico propriamente dito** — birth/merge/split de especialistas em runtime (mesmo que item 2 acima).

---

## 7. ADR-0054 — Perci/Bitwork Integration

**Fonte:** `docs/architecture/0054-perci-bitwork-integration.md`
**Lifecycle original:** `pesquisa` (adiada)

Perci Bitwork é arquitetura associativa binária (não transformer). Viabilidade depende de OK do maintainer pós-Net RX estável.

---

## Resumo

| Prioridade | Item | Fonte | Esforço est. | Bloqueio |
|-----------|------|-------|-------------|----------|
| 🔴 Alta | VectorStore TF-IDF | ADR-0064 | ~1.750 LOC | Nenhum (puro algoritmo) |
| 🔴 Alta | Dynamic MoE birth/merge/split | ADR-0060 A.3 | ~1.600 LOC | Arquitetural |
| 🔴 Alta | Self-Optimization (#157-#163) | IDEA_BANK §1.22 | ~1.250 LOC | Nenhum |
| 🔴 Alta | Success Engine (#149-#152) | IDEA_BANK §1.20 | ~800 LOC | Nenhum |
| 🔴 Alta | Security Pipeline (#260-#264) | IDEA_BANK §1.20 | ~1.080 LOC | Nenhum |
| 🔴 Alta | Self-Learning OS (#313) | IDEA_BANK §1.28 | ~800 LOC | Nenhum |
| 🟡 Média | GPU golden HW validation | ADR-0048/49/50 | — | AWAITING_HW |
| 🟡 Média | JARVIS Features (#315.x) | IDEA_BANK §1.31 | ~1.450 LOC | Pós-features core |
| 🟡 Média | Agents Evolution (A-xxx) | IDEA_BANK §1.28 | ~1.800 LOC | Pós-scheduler maduro |
| 🟡 Média | Cross-OS Compat (#306a-d) | IDEA_BANK §1.25 + ADR-0062 | ~3.300 LOC | Port código ClaudioOS |
| 🟡 Média | Developer Tooling (#394,395,172,236) | IDEA_BANK | ~1.700 LOC | Pós-Marketplace |
| 🟡 Média | Ring3/SFI produção | ADR-0041 P09 | ~3.000 LOC | Pós-v2.0.0 |
| 🟢 Baixa | GPU Foundations (#326-#332) | IDEA_BANK Bloco 21c/21d | ~2.600 LOC | GPU golden HW |
| 🟢 Baixa | NPU driver full | ADR-0057 WS-E | — | HW real |
| 🟢 Baixa | Perci/Bitwork | ADR-0054 | pesquisa | Maintainer OK |

**Total estimado:** ~22.000 LOC · **Dias:** 80-180

---

## 8. Self-Optimization / Workflow Learning (IDEA #157–#163)

**Fonte:** `docs/memory/IDEA_BANK.md §1.22`
**Valor:** 🎯 Core — scheduler adaptativo que melhora com uso

| Subitem | Descrição | Esforço |
|---------|-----------|---------|
| #157 | Usage Pattern Analyzer — LLM detecta workflow do usuário | ~250 LOC |
| #158 | Workflow Predictor — pré-carrega recursos por hora/padrão | ~200 LOC |
| #160 | Dynamic Resource Scaling — MHI tiers auto-ajuste por uso real | ~200 LOC |
| #161 | Self-Optimizing Scheduler — prioriza agents por workflow | ~300 LOC |
| #162 | Workflow Profile — perfil exportável ("dev"/"escritório") | ~150 LOC |
| #163 | Hardware Config Learning — SystemArchitecture evolui com feedback | ~150 LOC |

**Total estimado:** ~1.250 LOC · **Dias:** 6-12

---

## 9. Self-Learning OS (IDEA #313)

**Fonte:** `docs/memory/IDEA_BANK.md §1.28`
**Valor:** 🎯 Core — sistema que aprende dos próprios dados

| Subitem | Descrição | Esforço |
|---------|-----------|---------|
| #313a | DataCollector — coleta EventBus/logs/SMART como dataset de treino | ~300 LOC |
| #313b | Pipeline LogAgent→DataCollector→TrainingAgent→.bitnet→Trinity Hub | ~200 LOC |
| #313c | Melhoria contínua — cada boot usa modelo treinado no boot anterior | ~300 LOC |

**Total estimado:** ~800 LOC · **Dias:** 4-8

---

## 10. Success Engine — Feedback Loop (IDEA #149–#152)

**Fonte:** `docs/memory/IDEA_BANK.md §1.20`
**Valor:** 🎯 Core — qualidade de resposta que melhora com uso

| Subitem | Descrição | Esforço |
|---------|-----------|---------|
| #149 | Feedback loop — 👍/👎 nas respostas do Hermes, ajusta geração | ~200 LOC |
| #150 | Ternary weight update on-device — {-1,0,+1} via probabilidade | ~300 LOC |
| #151 | Experience replay buffer — últimas N interações p/ SleepCycle | ~50 LOC |
| #152 | Weight consolidation — export modelo atualizado para .bitnet | ~250 LOC |

**Total estimado:** ~800 LOC · **Dias:** 4-10

---

## 11. Security Pipeline (IDEA #260–#264)

**Fonte:** `docs/memory/IDEA_BANK.md §1.20 (Tier 3 Security)`
**Valor:** 🎯 Core — detecção de intrusão em kernel bare-metal

| Subitem | Descrição | Esforço |
|---------|-----------|---------|
| #260 | Event→Detector→Response Pipeline — 5 detectores iniciais (PortScan, ArpSpoof, PingFlood, DhcpStarvation, TimerAnomaly) | ~200 LOC |
| #261 | Decision Review + Human Escalation — timeout auto-resolve, high severity nunca auto-resolve | ~120 LOC |
| #262 | Hash Chain Audit Trail — SHA-256 chain de eventos, verify_chain() | ~60 LOC |
| #263 | Knowledge Graph p/ eventos de segurança — 6 node types, ~20 relations | ~400 LOC |
| #264 | Cross-Layer Correlation Rules — 5 regras multi-estágio iniciais | ~300 LOC |

**Total estimado:** ~1.080 LOC · **Dias:** 6-12

---

## 12. GPU Foundations (IDEA #326–#332)

**Fonte:** `docs/memory/IDEA_BANK.md § (Sprint Planning Blocos 21c/21d)`
**Valor:** 🧩 Extensão — compute GPU real (pós golden HW validation)

| Subitem | Descrição | Esforço | Bloqueio |
|---------|-----------|---------|----------|
| #326 | GPU BAR0/BAR1 mapping UC — MMIO direto | ~300 LOC | GPU golden HW |
| #327 | GPU doorbell + SPSC job ring — submissão de jobs | ~400 LOC | #326 |
| #328 | VRAM buddy allocator — gerenciamento VRAM | ~400 LOC | #327 |
| #329 | Agent.xpu prefill/decode split — CPU prefill, GPU decode | ~400 LOC | #327 |
| #330 | GPU matmul kernel ternário — PTX/AQL/GEN assembly | ~300 LOC | HW real |
| #331 | CPU→GPU KV cache DMA — swap KV entre RAM e VRAM | ~200 LOC | #330 |
| #332 | XQueue preemptível (XSched) — 3 níveis de preempção | ~600 LOC | #327 |

**Total estimado:** ~2.600 LOC · **Dias:** 15-30 · **Bloqueio:** GPU golden HW (§3)

---

## 13. JARVIS Features (IDEA #315.x)

**Fonte:** `docs/memory/IDEA_BANK.md §1.31` + ADR-0036
**Valor:** 🎯 Core — persona, memória e proatividade do JARVIS

| Subitem | Descrição | Esforço |
|---------|-----------|---------|
| #315.1 | SOUL.md Personality Engine — parser + persona + adaptive tone | ~300 LOC |
| #315.4 | Notification Gate — 4 urgências, rate limiting, dedup | ~200 LOC |
| #315.6 | Emotion Analysis — BitNet classifier 7 emoções + adjust_tone | ~250 LOC |
| #315.11 | Persona Pipeline — 16 stages (OVOS-inspired) | ~100 LOC |
| #315.14 | Proactive Heartbeats — JARVIS inicia conversa | ~100 LOC |
| #315.15 | Tool-State Save Game — snapshot + rollback de skills | ~100 LOC |
| #315.18 | Fail-Closed Safety Invariant — 4 invariantes SMT-proof | ~200 LOC |
| #315.19 | Merkle Audit Trail — Ed25519 chain, ring buffer 4096 | ~200 LOC |

**Total estimado:** ~1.450 LOC · **Dias:** 6-15

---

## 14. Agents Evolution (IDEA A-001–A-020)

**Fonte:** `docs/memory/IDEA_BANK.md §1.28`
**Valor:** 🎯 Core — arquitetura agent/skill-first madura

| Subitem | Descrição | Esforço |
|---------|-----------|---------|
| A-003 | AgentScheduler multicore (CFS) — fairness entre agents | ~500 LOC |
| A-007 | Capability-Based Routing — EventBus roteia por capability declarada | ~300 LOC |
| A-014 | Agent Budget + Watchdog — tick_budget por ciclo, watchdog pausa | ~200 LOC |
| A-015 | Agent Hooks — Pre/Post tick hooks, HookRegistry | ~300 LOC |
| A-016 | Multi-Agent Orchestration — graph-based: sequential, concurrent, handoff | ~500 LOC |

**Total estimado:** ~1.800 LOC · **Dias:** 10-20

---

## 15. Cross-OS Compatibility (IDEA #306a-d)

**Fonte:** `docs/memory/IDEA_BANK.md §1.25` + ADR-0062 (ClaudioOS)
**Valor:** 🧩 Extensão — compatibilidade binária Windows/Linux via código existente do ClaudioOS

| Subitem | Descrição | Esforço | Fonte |
|---------|-----------|---------|-------|
| #306a | PE32+ loader + Win32 compat (kernel32/user32/gdi32/ntdll + D2D/WASAPI/XInput/WIC) | ~1.500 LOC | ClaudioOS tem compilável |
| #306b | ELF loader + Linux syscall translation (open/read/write/mmap/clone → skills) | ~1.000 LOC | ClaudioOS tem parcial |
| #306c | Mach-O loader (macOS) — stub investigativo | ~300 LOC | Investigação |
| #307 | Syscall-to-Skill Translation Layer — unifica NT/Linux/XNU | ~500 LOC | Requer #306a/b |

**Total estimado:** ~3.300 LOC · **Dias:** 20-40 · **Nota:** Viabilidade 🟡 porque ClaudioOS tem código de referência compilável

---

## 16. Developer Tooling (IDEA #394, #395, #172, #236)

**Fonte:** `docs/memory/IDEA_BANK.md` (múltiplas seções)
**Valor:** 🧩 Extensão — ferramentas para o ecossistema dev

| Subitem | Descrição | Esforço |
|---------|-----------|---------|
| #394 | BitNet IDE — IDE on-device com Cortex-assisted code generation | ~500 LOC |
| #395 | Marketplace/App Store agent — HTTP search, install, Ed25519 verify | ~400 LOC |
| #172 | MCP Server support — EventBus + SkillRegistry via JSON-RPC 2.0 | ~400 LOC |
| #236 | Plugin Hub / MCP Index — AI security scan, catálogo de plugins | ~400 LOC |

**Total estimado:** ~1.700 LOC · **Dias:** 10-25

---

- SESSION_220 — auditoria ADR decrescente
- Commit `5ea319a` (BitNet recommendations)
- Commit `7a5e0a7` (UI WM fixes)
- Commit `0fdf20e` (ADR-0065 FASE 2.2+3.2)
- Commit `289339c` (ADR-0065 FASE 1.1-3.1)
