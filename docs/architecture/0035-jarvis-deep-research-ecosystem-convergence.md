# ADR-0035: J.A.R.V.I.S. Deep Research — Convergência de Ecossistema, Academia & Sprint Plan

> **SUPERSEDED:** pesquisa preservada como histórico. A decisão unificada e canônica é a [ADR-0036](0036-jarvis-unified-interaction-layer.md).

**Data:** 2026-07-04
**Status:** Superseded
**Sprint Target:** 77-80 (JARVIS Sprint)
**Depende de:** ADR-0034 (JARVIS Conscious Interaction Layer), ADR-0031 (AIOS Evolution), ADR-0032 (WASM Agent Apps), ADR-0033 (On-Device Micro-Learning)

---

## 1. Escopo da Pesquisa

Pesquisa exaustiva em **6 repositórios próprios**, **27 projetos open-source** (11 JARVIS-like + 16 Batch 2: voice assistants, Rust agents, mega-projects), **20+ papers arXiv 2026**, e **fóruns de desenvolvedores** (GitHub Topics, arXiv, dev.to). Objetivo: identificar **todas** as features viáveis e compatíveis com o neural-os-core, encadear ideias aproveitáveis para JARVIS/Hermes/Cortex/Kernel, e definir sprint plan priorizado.

---

## 2. Ecossistema Próprio — 6 Repositórios Analisados

### 2.1 msrovani/JARVIS (C#, .NET MAUI, 2 commits)

**O que é:** Assistente virtual multiplataforma (Windows/Android/iOS/MacOS) com análise de personalidade, detecção emocional, voz, Semantic Kernel, Gemini API, embeddings vetoriais, SQLite.

| Feature | Portável para AIOS? | Como |
|---|---|---|
| **Personality Analysis** (adaptação ao estilo do usuário) | ✅ Sim | SOUL.md + adaptive tone no HermesAgent |
| **Emotional Analysis** (detecção de sarcasmo, emoções) | ✅ Sim | BitNet classifier (~50KB) no Trinity MoE |
| **Semantic Kernel Plugin Pattern** | ✅ Sim | Nosso MCP Layer já é análogo |
| **Vector Embedding Storage** (SQLite + similaridade) | ✅ Sim | KnowledgeGraph + HybridSearch já existem |
| **Voice I/O** (TTS + STT) | 🟡 Futuro | Piper TTS + Vosk STT (pós B-01) |
| **Multiplataforma** (.NET MAUI) | ❌ Não | Somos bare-metal |
| **Gemini API** | ❌ Não | Cortex BitNet local-first |

**Insight chave:** O padrão `EmotionalAnalysisService → preprocess → detect_emotion → adjust_response` é diretamente portável como skill do HermesAgent.

### 2.2 msrovani/Jotape (Kotlin/Compose, 4 commits)

**O que é:** Assistente virtual Android com STT, RAG, LLM, TTS, verificação de voz, Supabase (pgvector), gamificação, monetização freemium.

| Feature | Portável? | Como |
|---|---|---|
| **RAG Pipeline** (STT → RAG → LLM → TTS) | ✅ Sim | Pipeline Hermes: input → MemoryTree → Cortex → output |
| **Voice Verification** (autenticação por voz) | 🟡 Futuro | TrustAgent + biometria vocal |
| **Gamification** (recompensas, streaks) | ✅ Sim | OptimizerAgent + CronAgent |
| **Feedback Loop** (fine-tuning via feedback) | ✅ Sim | TrainingAgent (IDEA #312) |
| **Monetization Strategy** (freemium) | ❌ Não | Bare-metal OS |
| **Ethical Control & Sanctions** | ✅ Sim | SafetyAgent já tem Leis de Asimov |
| **pgvector** (vetores no Postgres) | ✅ Já temos | KnowledgeGraph + HNSW |

**Insight chave:** O pipeline `docs/05-security-privacy.md` com RLS + moderação de conteúdo mapeia para nosso SecurityAgent + SafetyAgent.

### 2.3 msrovani/SKYNET (TypeScript/Rust→WASM, 43 commits, 570 testes)

**O que é:** DePIN super app — inferência de IA distribuída em malha P2P. Speculative decoding distribuído, thermal scheduling, circadian-aware scheduling, evolução genética, federated learning, x402 micropagamentos.

| Feature | Portável? | Como |
|---|---|---|
| **Distributed Speculative Decoding** (DSD) | ✅ Sim | Cortex Medusa já faz speculative decode; DSD estende para multi-device |
| **Thermal Management** (30 testes, zone/trend/cooldown) | ✅ Sim | MemoryAgent + RAPL MSR |
| **Circadian-Aware Scheduling** (12 testes) | ✅ Sim | CronAgent + timezone-aware scheduling |
| **Genetic Evolution** (pop=20, crossover 70%, mutation 15%) | ✅ Sim | OptimizerAgent + TrainingAgent |
| **Federated Learning** (FedYogi + Secure Aggregation MPC) | ✅ Sim | TrainingAgent + SKYNET mesh |
| **Activation Checkpoints** (preemption recovery) | ✅ Sim | AgentScheduler + checkpoint/restore |
| **Segment Means Compression** | ✅ Sim | Session compression do Hermes buffer |
| **Pipeline Parallelism** (layer partition por capacidade) | ✅ Sim | Trinity MoE routing |
| **x402 Micropayments** | 🟡 Futuro | B-01 + blockchain integration |
| **CRDT Sync** (Automerge v2) | 🟡 Futuro | Multi-device sync |
| **Plugin System** (16 testes, schema + registry + loader) | ✅ Sim | WASM Agent Apps (ADR-0032) |
| **TEE Attestation** (SGX/SEV/CCA, 24 testes) | ✅ Sim | SecurityAgent + TEE |

**Insight chave:** SKYNET é o **backend distribuído** do JARVIS. Quando B-01 (networking) estiver pronto, o neural-os-core pode participar da malha SKYNET como nó L1 (PC) ou L2 (workstation), usando speculative decoding para acelerar inferência do Cortex.

### 2.4 msrovani/BeFree (TypeScript, 37 commits, 1★)

**O que é:** Rede social P2P com IA pessoal embarcada "JARBAS", economia tokenizada (BFR), identidade soberana (DID), reputação, governança, simulação comunitária.

| Feature | Portável? | Como |
|---|---|---|
| **JARBAS Persona** (prompt operacional canônico, memória evolutiva) | ✅ Sim | Template para SOUL.md do JARVIS |
| **Reputation System** (decaimento exponencial, ranking) | ✅ Sim | TrustAgent + reputation scoring |
| **Governance** (propostas, votação ponderada, quórum) | 🟡 Futuro | Multi-user AIOS |
| **DID (Ed25519)** | ✅ Já temos | BootTrustAgent + Ed25519 keys |
| **P2P Messaging** (libp2p + IPFS + GunDB) | 🟡 Futuro | B-01 networking |
| **Analytics Digest** (tendências, pulsações, intenções) | ✅ Sim | OptimizerAgent + Dashboard |
| **Automation Engine** (tarefas reativas, jobs recorrentes) | ✅ Sim | CronAgent + automation tasks |
| **Simulation** (cenários roteirizados, presets) | ✅ Sim | QEMU testing + simulation harness |
| **Sensory Engine** (trilha ambiente, TTS, vibração) | 🟡 Futuro | DisplayAgent + audio |

**Insight chave:** O `jarbasPersona` com memória evolutiva + validadores de saída + helpers de atualização é o **template perfeito** para o SOUL.md do JARVIS.

### 2.5 msrovani/Android_AI_Car (Kotlin, 24 commits)

**O que é:** Assistente de IA para Android Auto — voz, Gemini, navegação, controle de música.

| Feature | Portável? | Como |
|---|---|---|
| **Car Mode** (interface simplificada para direção) | 🟡 Futuro | DisplayAgent + car profile |
| **Voice Commands** (STT → action) | ✅ Sim | Voice pipeline (pós B-01) |
| **Navigation Integration** | 🟡 Futuro | Skill: `/navigate` |
| **Music Control** | 🟡 Futuro | Skill: `/play` |

**Insight chave:** O padrão `ConversationManager → detect_intent → execute_action` é idêntico ao nosso HermesAgent intent routing.

### 2.6 msrovani/mem0-supabase (Python, 3 commits, 1★)

**O que é:** 12-layer cognitive memory architecture para AI agents. Supabase-native com pgvector, knowledge graph, ego layer, security (8 threats), MCP integration.

| Feature | Portável? | Como |
|---|---|---|
| **12-Layer Memory** (perception → ego) | ✅ Parcial | Já temos: Working (MemoryTree), Episodic (EventLog), Lifecycle (Ebbinghaus), Semantic (KG), Graphic (KG). Faltam: Reflexive, Consolidation/Dreaming, Ego |
| **Reflexive Layer** (real-time synaptic broadcasting) | ✅ Sim | EventBus já faz broadcast; falta priorização reflexiva |
| **Consolidation/Dreaming** (synthetic memory generation) | ✅ Sim | CronAgent + background consolidation |
| **Ego Layer** (meta-cognitive identity synthesis) | ✅ Sim | SOUL.md + self-model no CortexAgent |
| **Context Paging** (token-efficient background context) | ✅ Sim | Session compression |
| **Tool-State Save Game** (snapshot de ferramentas) | ✅ Sim | Agent checkpoint/restore |
| **Memory Bridge** (multi-agent private/shared sync) | ✅ Sim | EventBus + capability tokens |
| **Hybrid Search RRF** (vector + full-text fusion) | ✅ Já temos | HybridSearch no event-bus |
| **Semantic Compression** (merge similar facts) | ✅ Já temos | Dedup no event-bus |
| **Context Firewall** (PII redaction) | ✅ Sim | SecurityAgent + mask_secrets |
| **Heartbeats** (proactive autonomous turns) | ✅ Sim | CronAgent + proactive triggers |
| **MCP Integration** | ✅ Já temos | SkillRegistry + McpManifest |

**Insight chave:** As 3 layers que faltam (Reflexive, Dreaming, Ego) são as mais impactantes para JARVIS:
- **Reflexive:** EventBus com prioridade — eventos críticos (DISK_HEALTH, SECURITY_ALERT) bypassam o Hermes e geram notificação imediata
- **Dreaming:** CronAgent noturno que consolida memórias do dia, gera insights sintéticos
- **Ego:** SOUL.md + auto-modelo — JARVIS sabe quem é, o que sabe, o que não sabe

---

## 3. Pesquisa Acadêmica — arXiv 2026 (20+ Papers)

### 3.1 Agent Operating Systems (6 papers)

| Paper | Insight Chave | Portável? |
|---|---|---|
| **AgenticOS** (arXiv:2606.21129) | OS como "intent filter" — agentes declaram intent, OS sintetiza least-privilege. 4-layer: Ghost Kernel + Logic Shutter + Agent Capsule + Semantic Boundary Gateway | ✅ Nosso SafetyAgent + SkillRegistry já fazem parte disso |
| **TopoClaw** (arXiv:2605.15556) | Agent OS topology-aware: cross-device action placement, cross-user identity attribution, cross-context authority governance | ✅ Multi-device JARVIS (futuro) |
| **AgentOS KDD** (arXiv:2603.08938) | Personal Agent OS: NUI portal, Agent Kernel (intent mining), Skills-as-Modules, personal KG | ✅ Idêntico à nossa arquitetura! Valida Hermes+SkillRegistry+KG |
| **Governed MCP** (arXiv:2604.16870) | Kernel-resident MCP governance: 6-layer pipeline (schema, trust, rate, adversarial, ProbeLogits, constitutional). Anima OS = bare-metal Rust x86_64 ~86K LOC | ✅ **Diretamente aplicável!** Anima OS é análogo ao neural-os-core |
| **ActPlane** (arXiv:2606.25189) | OS-level policy enforcement via eBPF. IFC DSL for cross-event policies. 1.9-8.4% overhead | 🟡 eBPF não aplicável em bare-metal, mas o padrão de policy DSL é portável |
| **Unfireable Safety Kernel** (arXiv:2606.26057) | Rust safety kernel com SMT proof (Z3) + bounded model checking (Kani). 4 properties: process separation, pre-action enforcement, fail-closed, signed evidence | ✅ SafetyAgent + SecurityAgent devem adotar fail-closed invariant |

### 3.2 Personal AI Assistants (8 papers)

| Paper | Insight Chave | Portável? |
|---|---|---|
| **Fluid Personality Framework** (arXiv:2607.01034) | Adaptação conjunta de persona metafórica (coach/tutor/tool) + intensidade de personalidade (low/medium/high) baseado em contexto, urgência, traits do usuário | ✅ SOUL.md dinâmico — persona muda por contexto |
| **Cognitive Digital Twins** (arXiv:2606.23094) | CDT = modelo computacional da cognição de uma pessoa. 5A governance: Authority, Autonomy, Access, Accountability, Availability | ✅ JARVIS como CDT do usuário |
| **Twin Agents** (arXiv:2605.19838) | Digital twin que representa conhecimento/perspectiva/estilo do usuário quando indisponível. 3 failure modes: schema gap, epistemic gap, model artifact | ✅ HermesAgent como twin do usuário |
| **SocialMemBench** (arXiv:2605.17789) | Benchmark para memória social em grupos. Mem0/LangMem/Graphiti/Cognee avaliado. 5 failure modes identificados | ✅ Benchmarks para nosso MemoryTree + KG |
| **Seneca Planner** (arXiv:2604.19425) | Conversational planner com persistent DB + goal tracking + behavioral patterns. Scaffolds reflection | ✅ CronAgent + planning skill |
| **LiveClawBench** (arXiv:2604.13072) | Benchmark para OpenClaw-style assistants. Triple-Axis Complexity Framework + reproducible full-stack mocks | ✅ Benchmarks para HermesAgent |
| **AgentDisCo** (arXiv:2605.11732) | Disentangled critic+generator for deep research. Policy bank + meta-optimization harness | ✅ CortexAgent research mode |
| **GTA-2** (arXiv:2604.15715) | General Tool Agents benchmark. Checkpoint-based evaluation. Top models: 14.39% success on workflows | ✅ Evaluation framework para skills |

### 3.3 Security & Governance (4 papers)

| Paper | Insight Chave | Portável? |
|---|---|---|
| **Dynamic Malicious Skills** (arXiv:2606.16287) | Ataque via SKILL.md com instruções maliciosas. Defesa: kernel-enforced read-only mounts | ✅ SecurityAgent + skill immutability |
| **Right to History** (arXiv:2602.20214) | PunkGo: Rust sovereignty kernel com Merkle tree audit logs, capability isolation, energy governance | ✅ BootLogAgent + Merkle audit trail |
| **Aegis Architecture** (arXiv:2603.16938) | Cryptographic runtime governance: IEPL + EVA + EKM + ILK. Immutable ethics policy layer | ✅ SafetyAgent + Ed25519 signed policies |
| **Qualixar OS** (arXiv:2604.06392) | Universal agent orchestration: 12 topologies, Forge team design, 3-layer model routing, consensus judge | ✅ AgentScheduler + Trinity MoE routing |

### 3.4 Memory & Learning (2 papers)

| Paper | Insight Chave | Portável? |
|---|---|---|
| **SuperMemory-VQA** (arXiv:2606.00825) | Egocentric VQA for long-horizon memory. 52.9h video, 4853 QA pairs. AI glasses as memory assistant | ✅ MemoryTree + episodic memory |
| **Contextual Multi-Objective** (arXiv:2605.03900) | AI behavior as context-dependent choice over objectives. Hierarchical constraints + deliberative policy | ✅ SafetyAgent + multi-objective routing |

---

## 4. Feature Convergence Matrix — O que já temos vs. o que falta

### 4.1 Features que JÁ TEMOS (validadas pela pesquisa)

| # | Feature | Nosso Componente | Validação Externa |
|---|---|---|---|
| 1 | Intent routing | HermesAgent | AgentOS KDD paper |
| 2 | Multi-agent orchestration | AgentScheduler + 173 agents | SynkraAI (12 agents), Qualixar OS (12 topologies) |
| 3 | Safety interceptor (Asimov) | SafetyAgent | TabNews artigo, Aegis paper |
| 4 | Model orchestration (MoE) | Trinity MoE | TabNews ("modelos pequenos > um grande") |
| 5 | Knowledge Graph | event-bus KG | mem0-supabase Graphic Layer |
| 6 | Hybrid search (vector + text) | event-bus HybridSearch | mem0-supabase RRF Fusion |
| 7 | Semantic dedup | event-bus Dedup | mem0-supabase Semantic Compression |
| 8 | Privacy/PII masking | SecurityAgent mask_secrets | mem0-supabase Context Firewall |
| 9 | Ebbinghaus forgetting | event-bus Lifecycle | mem0-supabase Lifecycle Layer |
| 10 | Ed25519 trust | BootTrustAgent | BeFree DID, Aegis IEPL |
| 11 | Self-healing | SelfHealAgent | SKYNET Genetic Evolution, ARYA Unfireable Kernel |
| 12 | Speculative decoding | Cortex Medusa | SKYNET DSD |
| 13 | WASM sandbox | ADR-0032 | OpenJarvis WASM runner |
| 14 | On-device training | ADR-0033 | OpenJarvis GRPO/SFT |
| 15 | MCP Layer | SkillRegistry | Governed MCP paper, mem0 MCP |
| 16 | Cron scheduling | CronAgent | BeFree Automation Engine |
| 17 | Dashboard | agent-core Dashboard | Qualixar OS 24-tab dashboard |

### 4.2 Features para ADOTAR (novas, priorizadas)

| # | Feature | Fonte | LOC | Sprint | Prioridade |
|---|---|---|---|---|---|
| 1 | **SOUL.md Personality Engine** | JARVIS C# + BeFree JARBAS + Fluid Personality paper | ~300 | 77 | 🔴 Crítica |
| 2 | **IPW Monitoring** (tokens/watt via RAPL) | OpenJarvis Stanford + SKYNET Thermal | ~150 | 77 | 🟡 Alta |
| 3 | **Session Compression** | OpenJarvis + SKYNET Segment Means + mem0 Context Paging | ~200 | 77 | 🟡 Alta |
| 4 | **Notification Gate** | JARVIS C# + BeFree Automation | ~200 | 77 | 🟡 Alta |
| 5 | **Emotion Analysis** | JARVIS C# EmotionalAnalysisService | ~250 | 78 | 🟡 Alta |
| 6 | **Capability Contract** | terminal-jarvis 9-cap + Governed MCP | ~100 | 78 | 🟡 Alta |
| 7 | **Skill Discovery (DSPy/ACE)** | OpenJarvis + SynkraAI ADE | ~300 | 78 | 🟡 Média |
| 8 | **Dreaming/Consolidation** | mem0-supabase Layer 6 | ~200 | 79 | 🟡 Média |
| 9 | **Ego Layer** (self-model) | mem0-supabase Layer 12 + Twin Agents paper | ~250 | 79 | 🟡 Média |
| 10 | **Reflexive Broadcast** | mem0-supabase Layer 2 | ~100 | 79 | 🟢 Baixa |
| 11 | **Merkle Audit Trail** | Right to History (PunkGo) | ~200 | 80 | 🟢 Baixa |
| 12 | **Fail-Closed Safety Kernel** | Unfireable Safety Kernel + Aegis | ~300 | 80 | 🟡 Média |
| 13 | **Fluid Persona** (context-adaptive) | Fluid Personality Framework paper | ~150 | 80 | 🟢 Baixa |
| 14 | **Voice TTS** (Piper/espeak) | Priler/jarvis + JARVIS C# | ~200 | N+1 | 🔴 Pós B-01 |
| 15 | **Voice STT** (Vosk) | Priler/jarvis | ~400 | N+1 | 🔴 Pós B-01 |
| 16 | **Wake Word** (Rustpotter) | Priler/jarvis | ~100 | N+1 | 🔴 Pós B-01 |
| 17 | **SKYNET Mesh Node** | SKYNET DSD + P2P | ~500 | N+2 | 🔴 Pós B-01 |
| 18 | **Gamification** | Jotape feedback + metrics | ~200 | N+1 | 🟢 Baixa |
| 19 | **Tool-State Save Game** | mem0-supabase Layer 9 | ~150 | 79 | 🟢 Baixa |
| 20 | **Proactive Heartbeats** | mem0-supabase Layer 12 | ~100 | 79 | 🟡 Média |

### 4.3 Features REJEITADAS (incompatíveis)

| Feature | Motivo |
|---|---|
| Tauri desktop | Somos bare-metal, não desktop app |
| Node.js/Python runtime | no_std incompatível |
| LuaJIT scripting | no_std incompatível, WASM é melhor |
| YOLO mode (bypass segurança) | Contradiz SafetyAgent |
| Multiplayer presence | Irrelevante para OS kernel |
| Google OAuth | Bare-metal sem browser |
| n8n/OpenClaw workflow | Ecossistema Node.js |
| Cloud-first routing | Somos local-first |
| C++ orchestrator | Rust-only |
| TensorFlow dependency | BitNet local é suficiente |

---

## 5. Cadeia de Ideias Encadeadas — JARVIS → Hermes → Cortex → Kernel

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        CADEIA DE CONVERGÊNCIA                            │
│                                                                           │
│  JARVIS (C#)          BeFree (TS)         mem0-supabase (Python)         │
│  ├─ Personality       ├─ JARBAS persona   ├─ 12-layer memory             │
│  ├─ Emotion           ├─ Reputation       ├─ Dreaming engine             │
│  ├─ Voice I/O         ├─ Governance       ├─ Ego layer                   │
│  └─ Semantic Kernel   └─ Automation       └─ Tool-state save             │
│         │                    │                    │                       │
│         ▼                    ▼                    ▼                       │
│  ┌──────────────────────────────────────────────────────────┐            │
│  │              J.A.R.V.I.S. LAYER (Sprint 77-80)            │            │
│  │  SOUL.md + Emotion + Session Comp + IPW + Notifications  │            │
│  │  + Dreaming + Ego + Reflexive + Capability Contract      │            │
│  └──────────────────────────┬───────────────────────────────┘            │
│                              │                                            │
│  SKYNET (TS/Rust)   Jotape (Kotlin)    arXiv papers                     │
│  ├─ DSD             ├─ RAG pipeline    ├─ AgenticOS intent filter       │
│  ├─ Thermal mgmt    ├─ Voice verify    ├─ Governed MCP 6-layer          │
│  ├─ Circadian       ├─ Gamification    ├─ Unfireable Safety Kernel      │
│  ├─ Genetic evo     ├─ Ethical ctrl    ├─ Fluid Personality             │
│  └─ FL (FedYogi)    └─ Monetization    └─ Right to History (Merkle)     │
│         │                    │                    │                       │
│         ▼                    ▼                    ▼                       │
│  ┌──────────────────────────────────────────────────────────┐            │
│  │                    HERMES (Sprint 77+)                     │            │
│  │  Intent routing + ReAct + Council + Handoff               │            │
│  │  + ADE pipeline + Fluid persona + Proactive heartbeats    │            │
│  └──────────────────────────┬───────────────────────────────┘            │
│                              │                                            │
│  ┌──────────────────────────▼───────────────────────────────┐            │
│  │                    CORTEX (Sprint 77+)                     │            │
│  │  BitNet LLM + Trinity MoE + Medusa speculative            │            │
│  │  + IPW-aware routing + Emotion classifier expert           │            │
│  └──────────────────────────┬───────────────────────────────┘            │
│                              │                                            │
│  ┌──────────────────────────▼───────────────────────────────┐            │
│  │                    KERNEL (Sprint 77+)                     │            │
│  │  173+ agents + SkillRegistry + SafetyAgent                │            │
│  │  + RAPL MSR + Merkle audit + Fail-closed invariant        │            │
│  │  + SKYNET mesh node (pós B-01)                            │            │
│  └──────────────────────────────────────────────────────────┘            │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 6. Sprint Plan — JARVIS Sprints (77-80)

### Sprint 77 — JARVIS Persona + IPW + Session Compression (~950 LOC)

| # | Item | LOC | Fonte | Dependências |
|---|---|---|---|---|
| 77.1 | SOUL.md parser + personality engine | ~300 | JARVIS C# + BeFree JARBAS + Fluid Personality | — |
| 77.2 | IPW Monitor (RAPL MSR 0x610) | ~150 | OpenJarvis + SKYNET Thermal | MemoryAgent |
| 77.3 | Session Compression | ~200 | OpenJarvis + SKYNET Segment Means + mem0 Paging | HermesAgent.buffer |
| 77.4 | Notification Gate | ~200 | JARVIS C# + BeFree Automation | EventBus |
| 77.5 | Sessionless Thread mode | ~100 | Residuum | EventLog |

**Entregável:** JARVIS tem personalidade, mede eficiência energética, compacta conversas longas, notifica proativamente, e mantém thread contínuo sem perda de contexto.

### Sprint 78 — Emotion + Discovery + Contracts + Cache (~1200 LOC)

| # | Item | LOC | Fonte | Dependências |
|---|---|---|---|---|
| 78.1 | Emotion Analysis (BitNet classifier) | ~250 | JARVIS C# EmotionalAnalysis | Trinity MoE |
| 78.2 | Capability Contract + Consent Gates | ~200 | terminal-jarvis + Governed MCP + Moltis | SkillRegistry + SafetyAgent |
| 78.3 | Skill Discovery (DSPy/ACE) | ~300 | OpenJarvis + SynkraAI ADE | SkillObserver |
| 78.4 | ADE Pipeline (Spec→Execute→Review→Recover) | ~200 | SynkraAI | AgentScheduler |
| 78.5 | Semantic Cache (5-tier routing) | ~150 | NabaOS | KnowledgeGraph |
| 78.6 | Persona Pipeline Stages | ~100 | OVOS 16-stage pipeline | HermesAgent |

**Entregável:** JARVIS detecta emoções, descobre skills automaticamente, valida capacidades com consent gates, cache semântico reduz inferências redundantes em 97.5%.

### Sprint 79 — Dreaming + Ego + Heartbeats + Auto-Skills (~900 LOC)

| # | Item | LOC | Fonte | Dependências |
|---|---|---|---|---|
| 79.1 | Dreaming/Consolidation (CronAgent noturno) | ~200 | mem0-supabase Layer 6 | CronAgent |
| 79.2 | Ego Layer (self-model + identity synthesis) | ~250 | mem0-supabase Layer 12 + Twin Agents | SOUL.md |
| 79.3 | Proactive Heartbeats | ~100 | mem0-supabase Layer 12 | CronAgent |
| 79.4 | Tool-State Save Game | ~100 | mem0-supabase Layer 9 | AgentScheduler |
| 79.5 | Auto-Skill Generation | ~150 | Cratos | SkillObserver + TrainingAgent |
| 79.6 | Babel-Index (entropy monitoring) | ~100 | NEOTH | MemoryTree |

**Entregável:** JARVIS "sonha" (consolida memórias), tem auto-consciência (sabe o que sabe), age proativamente, gera skills automaticamente, e monitora entropia da memória.

### Sprint 80 — Security Hardening + Audit (~500 LOC)

| # | Item | LOC | Fonte | Dependências |
|---|---|---|---|---|
| 80.1 | Fail-Closed Safety Invariant | ~200 | Unfireable Safety Kernel + Aegis | SafetyAgent |
| 80.2 | Merkle Audit Trail | ~200 | Right to History (PunkGo) | BootLogAgent |
| 80.3 | Fluid Persona (context-adaptive) | ~100 | Fluid Personality Framework | SOUL.md |

**Entregável:** JARVIS é seguro por arquitetura (fail-closed), auditável (Merkle), e adapta personalidade ao contexto.

### Sprint N+1 — Voice + Cross-Device (pós B-01, ~1600 LOC)

| # | Item | LOC | Fonte |
|---|---|---|---|
| N1.1 | Piper TTS Integration (C++ binary) | ~100 | Piper 11.2K★ |
| N1.2 | STT (Vosk/Whisper.cpp) | ~400 | Priler/jarvis + Rhasspy3 |
| N1.3 | Wake Word (Rustpotter) | ~100 | Priler/jarvis |
| N1.4 | Wyoming Protocol IPC | ~300 | Rhasspy3 |
| N1.5 | Multi-device sync (CRDT) | ~300 | SKYNET + BeFree |
| N1.6 | Gamification | ~200 | Jotape |
| N1.7 | Voice pipeline (8-domain) | ~200 | Rhasspy3 pipeline |

### Sprint N+2 — SKYNET Mesh Integration (pós B-01, ~500 LOC)

| # | Item | LOC | Fonte |
|---|---|---|---|
| N2.1 | SKYNET mesh node (L1/L2) | ~300 | SKYNET |
| N2.2 | Distributed speculative decoding | ~200 | SKYNET DSD |

---

## 7. Total LOC Estimado

| Sprint | LOC | Foco |
|---|---|---|
| **77** | ~950 | Persona + IPW + Compression + Notifications + Sessionless |
| **78** | ~1200 | Emotion + Discovery + Contracts + Cache + Persona Pipeline |
| **79** | ~900 | Dreaming + Ego + Heartbeats + Auto-Skills + Babel-Index |
| **80** | ~500 | Security + Audit + Fluid Persona |
| **N+1** | ~1600 | Voice (Piper+Vosk+Wyoming) + Cross-Device + Gamification |
| **N+2** | ~500 | SKYNET Mesh |
| **TOTAL** | **~5650** | JARVIS completo |

---

## 8. Referências

### Repositórios Próprios
- [msrovani/JARVIS](https://github.com/msrovani/JARVIS) — C#/.NET MAUI personal assistant
- [msrovani/Jotape](https://github.com/msrovani/Jotape) — Kotlin Android assistant
- [msrovani/SKYNET](https://github.com/msrovani/SKYNET) — DePIN distributed AI inference
- [msrovani/BeFree](https://github.com/msrovani/BeFree) — P2P social network + JARBAS AI
- [msrovani/Android_AI_Car](https://github.com/msrovani/Android_AI_Car) — Android Auto AI assistant
- [msrovani/mem0-supabase](https://github.com/msrovani/mem0-supabase) — 12-layer cognitive memory

### Papers arXiv (2026)
- AgenticOS (2606.21129) — Intent-oriented secure OS
- TopoClaw (2605.15556) — Topology-aware Agent OS
- AgentOS KDD (2603.08938) — NL-driven data ecosystem
- Governed MCP (2604.16870) — Kernel-level MCP governance (Anima OS, bare-metal Rust)
- ActPlane (2606.25189) — OS-level policy enforcement
- Unfireable Safety Kernel (2606.26057) — Rust + Z3 + Kani proof
- Fluid Personality (2607.01034) — Adaptive persona + personality intensity
- Cognitive Digital Twins (2606.23094) — 5A governance framework
- Twin Agents (2605.19838) — Trust calibration for digital twins
- SocialMemBench (2605.17789) — Social group memory benchmark
- Seneca (2604.19425) — Personalized conversational planner
- LiveClawBench (2604.13072) — Real-world assistant benchmark
- AgentDisCo (2605.11732) — Deep research agents
- GTA-2 (2604.15715) — General tool agents benchmark
- Dynamic Malicious Skills (2606.16287) — SKILL.md injection attack
- Right to History (2602.20214) — PunkGo sovereignty kernel
- Aegis (2603.16938) — Cryptographic runtime governance
- Qualixar OS (2604.06392) — Universal agent orchestration
- Contextual Multi-Objective (2605.03900) — Context-dependent objective selection
- SuperMemory-VQA (2606.00825) — Egocentric long-horizon memory

### Projetos Open-Source (pesquisa anterior)
- OpenJarvis Stanford (7.3K★) — IPW, session compression, skill discovery
- Priler/jarvis (2.9K★) — Offline-first, Vosk STT, wake word
- SynkraAI/aiox (3K★) — CLI First, 12 agents, ADE pipeline
- terminal-jarvis — Capability contract, security check
- dyoburon-jarvis — WebGPU, plugin IPC bridge

### Batch 2 — Pesquisa Profunda (2026-07-04)

#### Assistentes de Voz Open-Source (Legado + Ativos)

| # | Projeto | Stars | Status | Insight Portável |
|---|---|---|---|---|
| 12 | **MycroftAI/mycroft-core** | 6.6K★ | Archived (2024) | Skill system + messagebus architecture. Pioneiro. Sucessores: OVOS + Neon |
| 13 | **rhasspy/rhasspy3** | 382★ | Archived (2025) | **Wyoming protocol** — IPC padronizado para componentes de voz. Pipeline de 8 domínios: mic→wake→asr→vad→intent→handle→tts→snd |
| 14 | **OpenVoiceOS/ovos-core** | 278★ | Ativo (270 releases) | **Persona pipeline 16 stages**: stop_high→converse→ocp_high→padatious_high→adapt_high→persona→...→fallback_low. LLM fallback via ovos-persona |
| 15 | **NeonGeckoCom/NeonCore** | 205★ | Ativo (418 releases) | Multi-user, Docker modular, signal manager, skill hot-reload |
| 16 | **rhasspy/piper** | 11.2K★ | Migrado→OHF-Voice | **TTS neural local em C++** — rápido, offline, 100+ vozes. Portável como binary externo |

#### Projetos Rust-Based (Novos, Alta Relevância)

| # | Projeto | Stars | Insight Portável |
|---|---|---|---|
| 17 | **moltis-org/moltis** | 2.8K★ | **Single binary Rust** agent server. Sandboxed execution, consent-gated tools, multi-provider LLM routing, voice, memory, Telegram/WhatsApp/Discord/Teams, MCP tools. **Valida nossa arquitetura!** |
| 18 | **opencrust-org/opencrust** | 137★ | Multi-agent Rust, **self-learning skills**, RAG, MCP, agentskills.io marketplace compat |
| 19 | **first-fluke/cratos** | 4★ | **Auto-skill generation** from workflow observation + **model routing to cheapest LLM** + event sourcing + replay every execution |
| 20 | **nabaos/nabaos** | 5★ | **OS for autonomous AI agents** em Rust. **5-tier cache-first routing** (97.5% cost reduction), Ed25519 constitution enforcement, 130 agents, 106 plugins, WASM. **Quase idêntico ao neural-os-core!** |
| 21 | **The-Geek-Freaks/NEOTH** | 1★ | Local-first daemon, **5-tier memory**, consent-gated tools, WASM plugin sandbox, multi-provider LLM routing, **signed audit logs**, **Babel-Index** (collapse prediction on own runtime) |
| 22 | **mrdushidush/claudette** | 11★ | **Air-gapped** personal AI. REPL + TUI + CLI + Telegram. Local model via Ollama. Zero cloud, zero API key, zero telemetry |
| 23 | **Grizzly-Endeavors/residuum** | 8★ | **Sessionless** personal AI — one continuous thread, no context loss, MCP, Discord/Telegram |
| 24 | **bmtowfiq2026-hue/Nexus** | 2★ | Rust+Go. OpenClaw breadth + Hermes learning loop + **graph memory** + **DID identity** + privacy-as-config |

#### Projetos Grandes (Multi-language)

| # | Projeto | Stars | Insight Portável |
|---|---|---|---|
| 25 | **Fosowl/agenticSeek** | 26.6K★ | Fully local Manus AI. Autonomous agent + voice + web browsing + coding. **Valida local-first approach** |
| 26 | **leon-ai/leon** | 17.3K★ | Open-source personal assistant. Node.js + Python. Offline-first. Skill packages |
| 27 | **pipecat-ai/pipecat** | 13.2K★ | Voice/multimodal conversational AI framework. Real-time pipeline architecture |

---

## 9. Novos Padrões Portáveis (Batch 2)

### 9.1 Wyoming Protocol IPC (Rhasspy3)
Protocolo padronizado para comunicação entre componentes de voz (mic, wake, asr, vad, intent, handle, tts, snd). Cada componente é um programa independente que fala Wyoming via stdin/stdout ou WebSocket.

**Portável:** Definir um protocolo binário leve entre JARVIS components (futuro voice pipeline). Nosso EventBus já faz IPC entre agents; Wyoming seria o protocolo para módulos de voz externos.

### 9.2 Persona Pipeline 16 Stages (OVOS)
OVOS usa um pipeline de intent resolution com 16 stages ordenadas por prioridade:
```
stop_high → converse → ocp_high → padatious_high → adapt_high →
ovos-persona-pipeline-high → ocp_medium → fallback_high → stop_medium →
adapt_medium → padatious_medium → adapt_low → common_qa →
fallback_medium → ovos-persona-pipeline-low → fallback_low
```

**Portável:** Nosso HermesAgent pode adotar pipeline similar:
```
SafetyCheck → StopHandler → ConverseHandler → SkillHighPriority →
PersonaHandler (LLM fallback) → SkillMedium → CommonQA → FallbackLow
```

### 9.3 5-Tier Cache-First Routing (NabaOS)
NabaOS implementa 5 tiers de cache para routing de LLM requests:
1. Exact match (hash)
2. Semantic match (embedding similarity > 0.95)
3. Pattern match (intent + entities)
4. Provider fallback (round-robin)
5. Cold start (first available)

Resultado: **97.5% cost reduction** vs. always-fresh LLM calls.

**Portável:** Nosso CortexAgent + Trinity MoE já faz routing; adicionar semantic cache tier (KnowledgeGraph já tem embeddings!) reduziria chamadas redundantes ao LLM.

### 9.4 Auto-Skill Generation (Cratos)
Cratos observa workflows repetitivos do usuário e **auto-gera skills** a partir dos padrões detectados. Cada execução é gravada via event sourcing e pode ser replayed.

**Portável:** Nosso SkillObserver (já existe!) + TrainingAgent podem implementar isso. Padrão:
```
observe_workflow() → detect_pattern() → generate_skill() → validate() → register()
```

### 9.5 Consent-Gated Tools (Moltis/NEOTH)
Antes de executar qualquer tool com side effects, o sistema pede consentimento explícito do usuário. Tools são classificados por risk level:
- **Safe:** read-only, sem side effects (auto-aprovado)
- **Moderate:** write local, sem network (prompt confirmation)
- **Dangerous:** network, filesystem write, system changes (explicit approval + audit log)

**Portável:** Nosso SecurityAgent + CapabilityContract (Sprint 78) podem implementar consent gates. Mapeia diretamente para o SafetyAgent.

### 9.6 Babel-Index / Collapse Prediction (NEOTH)
NEOTH implementa um "Babel-Index" que **prevê quando sua própria memória vai colapsar** (perder coerência). Monitora:
- Entropy score (diversidade de topics vs. capacidade)
- Contradiction rate (memórias conflitantes)
- Staleness index (memórias antigas não-validadas)

Quando o score ultrapassa threshold, dispara consolidação automática.

**Portável:** Nosso MemoryTree + Ebbinghaus decay podem ser estendidos com entropy monitoring. O Dreaming/Consolidation (Sprint 79) já faz parte disso.

### 9.7 Sessionless Continuous Thread (Residuum)
Em vez de sessões discretas, um único thread contínuo que nunca perde contexto. Cada interação é append-only ao thread.

**Portável:** Nosso Session Compression (Sprint 77) + EventLog podem implementar modo "continuous thread" onde o Hermes nunca "esquece" — apenas compacta.

### 9.8 Signed Audit Logs (NEOTH/PunkGo)
Cada ação do agente é assinada criptograficamente (Ed25519) e armazenada em Merkle tree. Permite verificação posterior de "o que o agente fez" sem possibilidade de tampering.

**Portável:** BootLogAgent + Ed25519 (já temos!) + Merkle audit trail (Sprint 80).

---

## 10. Features Adicionais do Batch 2

| # | Feature | Fonte | LOC | Sprint | Prioridade |
|---|---|---|---|---|---|
| 21 | **Semantic Cache** (5-tier routing) | NabaOS | ~150 | 78 | 🟡 Alta |
| 22 | **Auto-Skill Generation** | Cratos | ~250 | 79 | 🟡 Média |
| 23 | **Consent-Gated Tools** | Moltis/NEOTH | ~150 | 78 | 🟡 Alta |
| 24 | **Babel-Index** (entropy monitoring) | NEOTH | ~100 | 79 | 🟢 Baixa |
| 25 | **Sessionless Thread** | Residuum | ~100 | 77 | 🟢 Baixa |
| 26 | **Persona Pipeline Stages** | OVOS | ~200 | 78 | 🟡 Alta |
| 27 | **Wyoming Protocol** (voice IPC) | Rhasspy3 | ~300 | N+1 | 🔴 Pós B-01 |
| 28 | **Piper TTS Integration** | Piper 11.2K★ | ~100 | N+1 | 🔴 Pós B-01 |

---

## 11. Decisão

**ADOTAR** as 28 features (20 originais + 8 do Batch 2), implementadas em 4 sprints (77-80) totalizando ~3550 LOC de kernel + ~2100 LOC pós-B-01.

**REJEITAR** as features da Seção 4.3 (Tauri, Node.js, Python, LuaJIT, YOLO, multiplayer, cloud-first).

**INTEGRAR** SKYNET como backend distribuído do JARVIS quando B-01 estiver pronto (Sprint N+2).

**VALIDAR** com benchmarks da academia: LiveClawBench, GTA-2, SocialMemBench.

**PRIORIZAR** os padrões do Batch 2:
- **Semantic Cache** (NabaOS) — reduz custo de inferência em 97.5%
- **Consent-Gated Tools** (Moltis/NEOTH) — segurança by design
- **Persona Pipeline Stages** (OVOS) — estrutura o Hermes intent resolution
- **Auto-Skill Generation** (Cratos) — skill discovery automático

**REFERÊNCIA CRUZADA:** NabaOS (5★, Rust) é o projeto mais similar ao neural-os-core encontrado na pesquisa — OS for autonomous AI agents, Ed25519 constitution, 130 agents, 106 plugins, WASM, 5-tier cache routing. Valida nossa arquitetura independentemente.
