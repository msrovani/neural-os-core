# ADR-0023: Memory Systems & Second Brain Analysis (Tier 3)

**Status:** Draft  
**Date:** 2026-06-25  
**Author:** IDA IA + Dev  

## Context

After analyzing the Crom ecosystem (Tier 0, ADR-0020 — 75 repos, 12 ideas), Life OS / Personal OS (Tier 1, ADR-0021 — 20 repos, 13 ideas), and Personal AI Assistants (Tier 2, ADR-0022 — 21 repos, 15 ideas), we expand the analysis to **Tier 3: Memory Systems & Second Brain**. The goal is to extract ideas implementable in `no_std` bare-metal x86-64 for neural-os-core's Hermes Cortex and TrustCache subsystems.

## Repos Analisados

| # | Repo | Stars | Stack | Função Central |
|---|---|---|---|---|
| 1 | [rohitg00/agentmemory](https://github.com/rohitg00/agentmemory) | ~24,000 | TypeScript + iii-engine (Rust) | #1 persistent memory for AI coding agents; 53 MCP tools, 12 auto hooks, 4-tier consolidation |
| 2 | [novyxlabs/novyx-vault](https://github.com/novyxlabs/novyx-vault) | 3,900 | TypeScript (Next.js + Tauri) | Open-source second brain with AI memory, rollback, audit, knowledge graph |
| 3 | [novyxlabs/novyx-mcp](https://github.com/novyxlabs/novyx-mcp) | — | TypeScript | 120 MCP tools: policy-as-code, approval workflows, governance, threat intel |
| 4 | [eugeniughelbur/obsidian-second-brain](https://github.com/eugeniughelbur/obsidian-second-brain) | 2,600 | Python/Claude Code | Cross-CLI skill: 32 commands, vault-first research, scheduled agents, bi-temporal facts |
| 5 | [wazionapps/nexo](https://github.com/wazionapps/nexo) | 22 | TypeScript/Python | Atkinson-Shiffrin cognitive memory: 150+ MCP tools, Ebbinghaus decay, trust scoring, LoCoMo F1 0.588 |
| 6 | [Per0x1de-1337/MemoryOS](https://github.com/Per0x1de-1337/MemoryOS) | 13 | Python (85.8%) + HTML | Temporal knowledge graph + hybrid vector retrieval + Ebbinghaus decay; 78ms queries |
| 7 | [sachitrafa/cognitive-ai-memory](https://github.com/sachitrafa/cognitive-ai-memory) | — | Python | YourMemory: Ebbinghaus decay formula, LoCoMo R@5 34% vs Mem0 18% |
| 8 | [IAAR-Shanghai/Awesome-AI-Memory](https://github.com/IAAR-Shanghai/Awesome-AI-Memory) | — | Reference | Curated repository of LLM memory research papers, frameworks, tools |
| 9 | [ballred/obsidian-claude-pkm](https://github.com/ballred/obsidian-claude-pkm) | 1,500 | TypeScript | Claude Code MCP integration for Obsidian vault |
| 10 | [henrydaum/second-brain](https://github.com/henrydaum/second-brain) | — | — | Microkernel AI runtime for second brain |
| 11 | [sopaco/cortex-mem](https://github.com/sopaco/cortex-mem) | 287 | Rust (100%) | Memory layer for AI agents in Rust |
| 12 | TyGu888/PersonalAssistant | — | — | Agent skill collection with code review, project management |
| 13-14 | COG-second-brain, IncubativeSecondBrain | <530 | Various | Obsidian-focused second brain starters (Claude Code plugin/vanilla) |

### Descoberta Crítica: Memory Systems Ecosystem Landscape

O ecossistema de memória para agentes em 2026 se divide em dois paradigmas:

1. **Coding Agent Memory (agentmemory, YourMemory, MemoryOS):** Foco em capturar automaticamente tool use, sessões de coding, e decisões de engenharia. agentmemory domina este espaço com 24k ★ e 95.2% R@5.

2. **Second Brain / PKM (novyx-vault, obsidian-second-brain):** Foco em notas pessoais, knowledge graphs, e integração com Obsidian. Novyx Labs (3.9k ★) é a força dominante com 120 MCP tools + Tauri desktop.

3. **Cognitive Memory (nexo):** Abordagem mais academicamente rigorosa — aplica Atkinson-Shiffrin (1968) + Ebbinghaus decay diretamente em agentes. 150+ MCP tools, LoCoMo F1 0.588.

O padrão emergente: **memória de agente está convergindo para MCP como protocolo universal.** Todos os 3 principais projetos (agentmemory, nexo, novyx) expõem MCP servers.

## Deep-Dive: agentmemory (rohitg00/agentmemory)

### Overview

| Metric | Value |
|---|---|
| Stars | ~24,000 (crescimento viral Q1-Q2 2026) |
| Language | TypeScript (Node.js) + iii-engine (Rust runtime) |
| License | Apache-2.0 |
| Install | `npm install -g @agentmemory/agentmemory` |
| Version | 0.9.27 |
| Tests | 1,423+ passing |
| Docs | README.md + 12 traduções + DESIGN.md + ROADMAP.md |

### Source Code Structure (60+ files in `src/`)

```
src/
├── cli/           # CLI commands (connect, demo, doctor, upgrade, import-jsonl)
├── eval/          # Benchmark adapters (LongMemEval, coding-agent-life)
├── functions/     # Core memory operations (40+ files)
│   ├── access-tracker.ts    # Track memory access frequency
│   ├── actions.ts           # Action management (todo-like tasks)
│   ├── audit.ts             # Cryptographic audit trail
│   ├── auto-forget.ts       # TTL-based automatic forgetting
│   ├── branch-aware.ts      # Memory branching (feature branches for memory)
│   ├── cascade.ts           # Cascade delete/update operations
│   ├── checkpoints.ts       # CI/approval checkpoints for actions
│   ├── claude-bridge.ts     # Bi-directional sync with MEMORY.md
│   ├── compress.ts          # LLM compression of observations
│   ├── compress-file.ts     # File-level compression
│   ├── compress-synthetic.ts # Synthetic data generation
│   ├── consolidate.ts       # Memory consolidation (tier promotion)
│   ├── consolidation-pipeline.ts # Full pipeline orchestrator
│   ├── context.ts           # Context injection for agent sessions
│   ├── crystallize.ts       # Crystallize patterns into reusable skills
│   ├── dedup.ts             # SHA-256 dedup (5min window)
│   ├── diagnostics.ts       # System health checks
│   ├── disk-size-manager.ts # Disk quota management
│   ├── enrich.ts            # Entity resolution + enrichment
│   ├── evict.ts             # Cache eviction (importance-based)
│   ├── export-import.ts     # Data portability
│   ├── facets.ts            # Faceted search dimensions
│   ├── file-index.ts        # File-level indexing
│   ├── flow-compress.ts     # Streaming compression
│   ├── frontier.ts          # BFS frontier for graph traversal
│   ├── governance.ts        # Memory governance (delete policies)
│   ├── graph.ts             # Knowledge graph entity/edge CRUD
│   ├── graph-retrieval.ts   # Graph-based memory retrieval
│   ├── image-quota-cleanup.ts
│   ├── image-refs.ts
│   ├── leases.ts            # Action leasing (multi-agent coordination)
│   ├── lessons.ts           # Lesson extraction + reinforcement
│   ├── mesh.ts              # Multi-instance mesh sync
│   ├── migrate.ts           # Schema migration
│   ├── migrate-vector-index.ts
│   ├── observe.ts           # Core observation storage
│   ├── obsidian-export.ts   # Export to Obsidian format
│   ├── patterns.ts          # Pattern detection
│   ├── privacy.ts           # Privacy filter (strip secrets, API keys)
│   ├── profile.ts           # Project profile (summarize project state)
│   ├── query-expansion.ts   # Multi-query expansion for search
│   ├── recent-searches-sweep.ts
│   ├── reflect.ts           # Memory reflection (DMN-like)
│   ├── relations.ts         # Memory relation management
│   ├── remember.ts          # Structured memory (patterns/preferences)
│   ├── replay.ts            # Session replay
│   ├── retention.ts         # Retention scoring (decay + reinforcement)
│   ├── routines.ts          # Reusable routine/workflow definitions
│   ├── search.ts            # BM25 + Vector + Graph hybrid search
│   ├── sentinels.ts         # Watchdog sentinels (webhook/timer/threshold)
│   ├── signals.ts           # Inter-agent signaling
│   ├── sketches.ts          # Lightweight idea sketches
│   ├── skill-extract.ts     # Extract reusable skills from patterns
│   ├── sliding-window.ts    # Sliding window for context budget
│   ├── slots.ts             # Memory slots (Letta-like core memory)
│   ├── smart-search.ts      # Combined triple-stream search
│   ├── snapshot.ts          # Git-like memory snapshots
│   ├── summarize.ts         # Session summarization
│   ├── team.ts              # Team memory (shared + private namespaces)
│   ├── temporal-graph.ts    # Bi-temporal knowledge graph
│   ├── timeline.ts          # Temporal ordering of observations
│   ├── verify.ts            # Verification/validation
│   ├── vision-search.ts     # Image-based search
│   └── working-memory.ts    # Working memory management
├── health/        # Health monitoring
├── hooks/         # Agent lifecycle hooks (12 hooks)
│   ├── session-start.ts
│   ├── prompt-submit.ts
│   ├── pre-tool-use.ts
│   ├── post-tool-use.ts
│   ├── post-tool-failure.ts
│   ├── pre-compact.ts
│   ├── subagent-start.ts
│   ├── subagent-stop.ts
│   ├── notification.ts
│   ├── task-completed.ts
│   ├── stop.ts
│   └── session-end.ts
├── mcp/           # MCP server tools (53 tools)
├── providers/     # LLM providers (anthropic, openai, gemini, openrouter, minimax)
│   ├── anthropic.ts
│   ├── openai.ts
│   ├── circuit-breaker.ts
│   ├── fallback-chain.ts
│   ├── embedding/   # Embedding providers
│   └── resilient.ts
├── replay/        # Session replay engine
├── state/         # State management
├── telemetry/     # Telemetry
├── triggers/      # Trigger system (cron, webhook)
├── utils/         # Utilities
├── viewer/        # Real-time web viewer (port 3113, HTML + SVG)
├── auth.ts        # HMAC authentication
├── config.ts      # Configuration management
├── types.ts       # TypeScript type definitions (~700 lines)
└── index.ts       # Entry point
```

### Type System (Core Types from `types.ts`)

| Type | Purpose |
|---|---|
| `RawObservation` | Raw hook payload (sessionId, toolName, toolInput/Output, userPrompt, modality) |
| `CompressedObservation` | LLM-compressed: facts[], concepts[], files[], importance, confidence |
| `Memory` | Structured memory: patterns, preferences, architecture, bugs, workflows |
| `Session` | Session metadata (project, model, status, observationCount, commitShas) |
| `SessionSummary` | Per-session: keyDecisions, filesModified, concepts |
| `MemorySlot` | Letta-like core memory slots (content, sizeLimit, pinned, readOnly, scope) |
| `MemoryRelation` | Relation types: supersedes, extends, derives, contradicts, related |
| `SemanticMemory` | Extracted facts with confidence, accessCount, strength |
| `ProceduralMemory` | Reusable workflows: steps[], triggerCondition, frequency |
| `HybridSearchResult` | bm25Score + vectorScore + graphScore + combinedScore |
| `TripleStreamResult` | BM25 + Vector + Graph fusion |
| `TemporalState` | Entity state over time (currentEdges, historicalEdges, timeline) |
| `RetentionScore` | score = salience × temporalDecay × reinforcementBoost |
| `GraphNode/GraphEdge` | Knowledge graph with bi-temporal timestamps (tcommit, tvalid, tvalidEnd) |
| `GraphSnapshot` | Precomputed top-degree subgraph for performance |
| `Action` | Actionable items with priority, assignee, dependencies |
| `Lease` | Multi-agent lease system (acquire/renew/expire) |
| `Routine/RoutineRun` | Reusable workflows with step dependencies |
| `Signal` | Inter-agent communication (info, request, response, alert, handoff) |
| `Checkpoint` | CI/approval gates for actions |
| `Sketch` | Lightweight idea sketches (promote→action or discard) |
| `Sentinel` | Watchdog: webhook/timer/threshold triggers |
| `Crystal` | Crystallized session outcomes (narrative, keyOutcomes, lessons) |
| `Lesson` | Reinforcement learning: content, confidence, reinforcements, decayRate |
| `Insight` | Higher-order patterns: sourceConceptCluster, sourceCrystals |
| `Facet` | Multi-dimensional tags for search |
| `ProviderConfig` | LLM provider: type, model, maxTokens, baseURL |
| `CircuitBreakerState` | closed/open/half-open with failure count |
| `DecayConfig` | lambda, sigma, tierThresholds (hot/warm/cold) |

### Memory Pipeline

```
PostToolUse hook fires
  → SHA-256 dedup (5min window)         [dedup.ts]
  → Privacy filter (strip secrets)      [privacy.ts]
  → Store raw observation               [observe.ts]
  → LLM compress structured facts       [compress.ts]
  → Vector embedding                    [embedding/]
  → Index in BM25 + vector + graph      [search.ts, graph.ts]

Stop / SessionEnd hooks fire
  → Summarize session                   [summarize.ts]
  → Knowledge graph extraction          [graph.ts]
  → Slot reflection + DMN               [reflect.ts, slots.ts]

SessionStart hook fires
  → Load project profile                [profile.ts]
  → Hybrid search (BM25 + vector + graph, RRF fusion) [smart-search.ts]
  → Token budget enforcement (2000 tok) [context.ts]
  → Inject into conversation
```

### 4-Tier Consolidation

| Tier | Data Structure | Function | Analogy |
|---|---|---|---|
| Working | `RawObservation` | `observe.ts` | Short-term memory (raw tool use) |
| Episodic | `SessionSummary` | `summarize.ts` | "What happened" (session-level) |
| Semantic | `SemanticMemory` | `consolidate.ts` | "What I know" (facts) |
| Procedural | `ProceduralMemory` | `crystallize.ts` | "How to do it" (workflows) |

Consolidation pipeline (`consolidation-pipeline.ts`) runs periodically:
1. Working → Episodic: on SessionEnd/Stop
2. Episodic → Semantic: on configurable interval (default: every 10 sessions)
3. Semantic → Procedural: when pattern frequency exceeds threshold

### Multi-Agent Coordination

agentmemory supports advanced multi-agent primitives rarely seen in memory systems:

| Primitive | File | Description |
|---|---|---|
| Leases | `leases.ts` | Acquire/renew/release exclusive action ownership (prevents 2 agents from doing the same work) |
| Signals | `signals.ts` | Info/request/response/alert/handoff messages between agents |
| Routines | `routines.ts` | Reusable multi-step workflows with step dependencies |
| Checkpoints | `checkpoints.ts` | CI/approval gates before actions resolve |
| Sentinels | `sentinels.ts` | Watchdog timers/webhooks/thresholds that trigger on conditions |
| Mesh | `mesh.ts` | Cross-instance synchronization (multi-machine memory mesh) |
| Team | `team.ts` | Shared + private namespaced memory within a team |
| Slots | `slots.ts` | Letta-like core memory slots (pinned, scoped, size-limited) |

## Deep-Dive: nexo (wazionapps/nexo) — Cognitive Memory

### Atkinson-Shiffrin Memory Model

nexo implements the Atkinson-Shiffrin (1968) multi-store memory model, the dominant cognitive psychology framework:

```
Sensory Register (48h retention)
  ↓  (attention + relevance filter)
Short-Term Memory (7d half-life)
  ↓  (rehearsal + frequent access promotes)
Long-Term Memory (60d half-life, semantic vectors)
```

**Key formulas:**
- Memory strength at time t: `S(t) = S₀ × e^(-λt)` where λ = decay rate
- Promotion threshold: access frequency > 3x within STM half-life
- Ebbinghaus curve: retention = e^(-t/τ) where τ = half-life / ln(2)

### Features

| Feature | Description |
|---|---|
| Semantic RAG | fastembed (768-dim), BM25 + cross-encoder reranking |
| Trust Score | 0-100 alignment index; corrections lower it, successes raise it |
| Metacognitive Guard | Before editing code, injects known errors, real schemas, blocking rules |
| Cognitive Dissonance | Detect when new instruction contradicts strong memory → ask instead of overwrite |
| 100% Local | SQLite + ONNX Runtime, zero cloud dependencies |
| Knowledge Graph | Bi-temporal, BFS traversal, intelligent dedup with event sourcing, D3.js viz |
| LoCoMo F1 | 0.588 (+55% vs GPT-4) |
| 150+ MCP Tools | Sessions, memory, learning, credentials, guard, preferences, entities, diary |

## Deep-Dive: novyx-vault (novyxlabs/novyx-vault)

### Key Innovations

| Innovation | Description | Portability |
|---|---|---|
| Ghost Connections | Sidebar shows semantically related memories even without shared keywords | Média — requires vector similarity |
| Draft→Review→Merge | Memory changes staged as drafts, reviewed, then merged (like Git PRs) | Baixa — state machine + approval |
| Magic Rollback | Rollback memory to any point in time ("2 hours ago") | Média — requires checkpointing |
| Cryptographic Audit Trails | Every memory mutation logged with integrity proofs | Alta — Ed25519 + Merkle |
| Memory Health | Health score (0-100), stale memory count, contradiction count | Baixa — scoring formula |
| Policy-as-Code | Governance policies written as code, enforced at runtime | Média — rule engine |
| Context Spaces | Multi-agent coordination with isolated context namespaces | Média — namespace isolation |

## Patterns Portáveis para neural-os-core

### Classificação por Complexidade

#### Imediatas / Simples (Sprint 23) — 3 ideias

| # | Ideia | Origem | Descrição | Esforço |
|---|---|---|---|---|
| 214 | SHA-256 Memory Dedup | agentmemory `dedup.ts` | Port SHA-256 dedup (5min sliding window) to no_std; prevent duplicate EventBus messages and TrustCache entries | ~100 LOC |
| 215 | Privacy Filter for Memory | agentmemory `privacy.ts` | Strip API keys, secrets, `<private>` tagged content before memory storage; regex + pattern matching | ~80 LOC |
| 216 | Memory TTL/Eviction | agentmemory `evict.ts`, `auto-forget.ts` | Auto-evict stale memory entries based on configurable TTL; EvictionPolicy enum (TimeToLive, ImportanceRank, AccessFrequency) | ~150 LOC |

#### Complexidade Baixa (Sprint 24) — 4 ideias

| # | Ideia | Origem | Descrição | Esforço |
|---|---|---|---|---|
| 217 | Hybrid Search (BM25 + MLP) | agentmemory `search.ts`, `smart-search.ts` | RRF fusion for Hermes intent routing; combine MLP intent classifier with BM25 keyword fallback; Reciprocal Rank Fusion with k=60 | ~200 LOC |
| 218 | 4-Tier Memory Consolidation | agentmemory `consolidation-pipeline.ts` | Working→Episodic→Semantic→Procedural pipeline in Hermes daemon; EventBus topics for each tier transition | ~400 LOC |
| 219 | Ebbinghaus Decay for TrustCache | nexo cognitive model, YourMemory formula | `strength = importance × e^(-λ_eff × days) × (1 + recall_count × 0.2)`; λ_eff = 0.16 × (1 - importance × 0.8) | ~120 LOC |
| 220 | Session Replay | agentmemory `replay.ts` | Record Hermes daemon turns (intent→skill→response) as atomic events; replay with speed control for debugging | ~200 LOC |

#### Complexidade Média (Sprint 25) — 3 ideias

| # | Ideia | Origem | Descrição | Esforço |
|---|---|---|---|---|
| 221 | Knowledge Graph on MHI | agentmemory `graph.ts`, `temporal-graph.ts` | Entity extraction + BFS traversal over semantic file system; GraphNode (file, concept, decision, error, pattern) + GraphEdge (uses, modifies, depends_on, fixes) with bi-temporal timestamps | ~500 LOC |
| 222 | Metacognitive Guard | nexo guard system | Before each Hermes skill execution, check TrustCache for "have I made this mistake before?"; inject known error patterns, schemas, blocking rules | ~300 LOC |
| 223 | Draft→Review→Merge Memory | novyx-vault workflow | Memory changes staged as drafts on EventBus; Hermes daemon reviews before merge; approval/rejection workflow | ~350 LOC |

#### Complexidade Alta (Sprint 26+) — 2 ideias

| # | Ideia | Origem | Descrição | Esforço |
|---|---|---|---|---|
| 224 | Atkinson-Shiffrin Cognitive Memory | nexo brain architecture | Full 3-tier memory: Sensory Register (48h, ring buffer) → STM (7d, priority queue) → LTM (permanent, semantic-indexed); promotion on access frequency > threshold | ~800 LOC |
| 225 | Bi-temporal Knowledge Graph for MHI | MemoryOS, agentmemory `temporal-graph.ts` | Track MHI tiers across time: (valid_from, valid_to) per allocation; append-only, old facts superseded never deleted; "what did MHI look like at time T?" | ~600 LOC |

#### Futuras (Pós-MVP) — 2 ideias

| # | Ideia | Origem | Descrição | Esforço |
|---|---|---|---|---|
| 226 | Team/Shared Memory | agentmemory `team.ts` | Namespaced memory across neural-os-core instances; shared + private isolation per agent ring | ~400 LOC |
| 227 | Memory Git Snapshots | agentmemory `snapshot.ts` | Version, rollback, diff memory state; SHA-256 commit chain for TrustCache and Hermes memory | ~500 LOC |

#### Descartadas — 2 ideias

| # | Ideia | Motivo |
|---|---|---|
| — | LLM-based memory compression | Requer LLM externo (Anthropic/OpenAI), incompatível com no_std bare-metal. Nossa compressão será baseada em token counting + sliding window |
| — | Vector embeddings (ONNX) | ONNX Runtime + sentence-transformers requerem alocador dinâmico e sistema de arquivos; incompatível com kernel bare-metal |

### Total: 14 ideias (3 imediatas, 4 baixa, 3 média, 2 alta, 2 futuras, 2 descartadas)

## Mapeamento para Sprint

```
Sprint 23 (Network Sprint)
  ├── #214 SHA-256 Memory Dedup (imediata)
  ├── #215 Privacy Filter for Memory (imediata)
  └── #216 Memory TTL/Eviction (imediata)

Sprint 24 (Memory Systems Sprint)
  ├── #217 Hybrid Search BM25 + MLP (baixa)
  ├── #218 4-Tier Memory Consolidation (baixa)
  ├── #219 Ebbinghaus Decay for TrustCache (baixa)
  └── #220 Session Replay (baixa)

Sprint 25 (Knowledge Sprint)
  ├── #221 Knowledge Graph on MHI (média)
  ├── #222 Metacognitive Guard (média)
  └── #223 Draft→Review→Merge Memory (média)

Sprint 26+ (Cognitive Sprint)
  ├── #224 Atkinson-Shiffrin Cognitive Memory (alta)
  └── #225 Bi-temporal Knowledge Graph (alta)

Pós-MVP
  ├── #226 Team/Shared Memory (futura)
  └── #227 Memory Git Snapshots (futura)
```

## Lições Aprendidas

1. **MCP is the universal protocol for agent memory.** All 3 major projects (agentmemory, nexo, novyx) expose MCP servers. neural-os-core's EventBus IPC is architecturally equivalent to MCP — both are capability-based pub/sub with tool exposure.

2. **Memory deduplication is essential.** agentmemory's SHA-256 dedup (5min window) is the single most portable pattern. Without dedup, raw observation capture generates O(n²) memory chaos.

3. **Privacy-by-design is table stakes.** Every memory system (agentmemory, nexo, novyx) includes a privacy layer. Our TrustCache already has basic access control; adding a privacy filter for captured memory content is mandatory.

4. **Cognitive psychology models outperform flat RAG.** nexo (Atkinson-Shiffrin + Ebbinghaus) achieves LoCoMo F1 0.588 vs Mem0's 0.18. The Ebbinghaus decay formula is ~20 LOC of no_std Rust — enormous leverage for minimal code.

5. **4-tier consolidation is the emerging standard.** Working→Episodic→Semantic→Procedural is agentmemory's architecture and mirrors human memory consolidation. Our Hermes daemon already has the EventBus infrastructure to support this.

6. **Belongs in IDEA_BANK.md as items #214-227.** Total IDEA_BANK count goes from 213 to 227.

## References

- ADR-0020: Crom Ecosystem Rust Viability Analysis (Tier 0)
- ADR-0021: Life OS Ecosystem Analysis (Tier 1)
- ADR-0022: Personal AI Assistant Ecosystem Analysis (Tier 2)
- [agentmemory](https://github.com/rohitg00/agentmemory) — 24k ★ memory engine
- [novyx-vault](https://github.com/novyxlabs/novyx-vault) — 3.9k ★ second brain
- [nexo](https://github.com/wazionapps/nexo) — Cognitive memory (Atkinson-Shiffrin)
- [MemoryOS](https://github.com/Per0x1de-1337/MemoryOS) — Temporal knowledge graph
- [obsidian-second-brain](https://github.com/eugeniughelbur/obsidian-second-brain) — 2.6k ★
- Atkinson, R. C., & Shiffrin, R. M. (1968). Human memory: A proposed system and its control processes.
- Ebbinghaus, H. (1885). Über das Gedächtnis.
- LongMemEval (ICLR 2025) — agentmemory R@5 95.2%
- LoCoMo Benchmark — nexo F1 0.588
