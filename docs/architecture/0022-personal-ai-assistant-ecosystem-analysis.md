# ADR-0022: Personal AI Assistant Ecosystem Analysis (Tier 2)

**Status:** Draft  
**Date:** 2026-06-25  
**Author:** IDA IA + Dev  
**PR:** TBD

## Context

Após analisar o ecossistema Crom (Tier 0 — 75 repos, 12 ideias) e Life OS / Personal OS (Tier 1 — 20 repos, 22 ideias), expandimos a análise para **Tier 2: Personal AI Assistant Frameworks**. Foram analisados 21 repositórios cobrindo desde assistentes pessoais virais (OpenClaw, Hermes Agent) até runtimes cognitivos em Rust (Lethe, ZeroClaw, Ironclaw) e frameworks git-native (GitAgent).

O objetivo: extrair ideias implementáveis em `no_std` bare-metal x86-64, classificá-las por viabilidade técnica e complexidade, e mapeá-las contra o roadmap existente.

## Repos Analisados

| # | Repo | Stars | Stack | Função Central |
|---|---|---|---|---|
| 1 | [openclaw/openclaw](https://github.com/openclaw/openclaw) | 380,433 | TypeScript | Self-hosted AI assistant, 50+ integrations |
| 2 | [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent) | 202,916 | Python | Self-improving AI agent, closed learning loop |
| 3 | [zeroclaw-labs/zeroclaw](https://github.com/zeroclaw-labs/zeroclaw) | 32,036 | Rust | Agent OS runtime, single binary, 30+ channels |
| 4 | [HKUDS/nanobot](https://github.com/HKUDS/nanobot) | 44,736 | Python | Lightweight AI agent, tools/chats/workflows |
| 5 | [nearai/ironclaw](https://github.com/nearai/ironclaw) | 12,476 | Rust | Secure AI assistant, WASM sandbox, OpenClaw-inspired |
| 6 | [leon-ai/leon](https://github.com/leon-ai/leon) | 17,340 | TypeScript | Open-source personal assistant, voice/text |
| 7 | [atemerev/lethe](https://github.com/atemerev/lethe) | 118 | Rust (100%) | Brain-inspired cognitive runtime; Cortex/Hippocampus/DMN/Brainstem |
| 8 | [agentic-in/elephant-agent](https://github.com/agentic-in/elephant-agent) | 565 | Python | Personal-Model First Self-Evolving AI Agent |
| 9 | [open-gitagent/gitagent](https://github.com/open-gitagent/gitagent) | 565 | TypeScript | Git-native AI agent; agent lives inside version-controlled repo |
| 10 | [yifaaan/tinybot](https://github.com/yifaaan/tinybot) | <500 | Go | Lightweight AI assistant, nanobot rewrite in Go |
| 11 | [Aitne-sh/Aitne](https://github.com/Aitne-sh/Aitne) | 1 | TypeScript | Local-first proactive personal AI agent |
| 12-21 | Nanobot forks, menor expressão | <100 | Variado | Forks sem contribuição original significativa |

### Descoberta Crítica: Rust AI Agent Ecosystem 2026

Durante a análise, descobrimos que o ecossistema de agentes AI em Rust explodiu em Q1 2026. Repos Rust-native importantes:

| Repo | Stars | Descrição |
|---|---|---|
| zeroclaw-labs/zeroclaw | 32,036 | "Fast, small, fully autonomous AI personal assistant infrastructure" — 597 stars/day |
| nearai/ironclaw | 12,476 | "Agent OS focused on privacy, security and extensibility" — OpenClaw-inspired Rust |
| atemerev/lethe | 118 | Brain-inspired cognitive runtime with 5 brain regions, Kameo actor framework |
| vercel-labs/agent-browser | 26,316 | Browser automation in Rust |
| AlexsJones/llmfit | 20,856 | LLM function integration toolkit |
| RightNow-AI/openfang | 16,145 | Agent framework |

**Padrão:** Rust é a linguagem dominante para infraestrutura de agentes em 2026. ZeroClaw e Ironclaw são reimplementações Rust do ecossistema OpenClaw. Lethe é o único runtime cognitivo "brain-inspired" com arquitetura multi-agente.

### Naming Overlap: NousResearch Hermes

O projeto NousResearch/hermes-agent (202,916 ★) compartilha o nome "Hermes" com nosso `hermes_console_daemon`. Ambos derivam independentemente do deus mensageiro grego. A sobreposição arquitetural é real: ambos roteiam intenções do usuário para skills/ações. O deles usa LLM; o nosso usa MLP (16→8→3). Recomendamos manter o nome — é uma coincidência positiva que valida nossa direção.

## Deep-Dives Realizados

### 1. Lethe (atemerev/lethe) — Runtime Cognitivo em Rust

**Confirmação:** 100% Rust (não Python como GitHub Language Stats indica). v0.22.23, Rust 2024.

**Arquitetura cerebral (5 regiões):**

| Região | Módulo | Função |
|---|---|---|
| Cortex | `src/agent.rs` (~90KB) | Voz. Montagem de prompt, loop de ferramentas, delegação |
| Hippocampus | `src/memory/recall.rs` (~24KB) | Memória associativa híbrida (lexical + vetorial) sobre notes, archival, conversas |
| DMN | `src/actor/background.rs` (~14KB) | Default Mode Network — reflexão silenciosa de fundo, gating de notificações |
| Brainstem | `src/scheduler/brainstem.rs` (~10KB) | Heartbeat loop, emissões proativas, rate limiter com outbox |
| Curator | `src/scheduler/curator.rs` (~16KB) | Colheita, dedup, sumarização de memória |

**Padrões-chave portáveis:**
1. **NotificationGate** — 2 estágios: heurístico + LLM. O gate heurístico (dedup, startup hushed, low priority dropped, interruptibility threshold) é diretamente portável como MLP.
2. **ProactiveRateLimiter + ProactiveOutbox** — `VecDeque<u64>` de timestamps de envio. Outbox segura 1 mensagem deferida.
3. **Heartbeat Idle Gate** — tick salta LLM quando sem reminders E sem open work. Open work = subagentes inacabados.
4. **ActorRegistry** — Kameo actor framework. HashMap<String, Actor> com permissão hierárquica. Persistência SQLite write-through.
5. **MemoryBlockSystem** — `identity.md`, `human.md`, `project.md` como blocos endereçáveis com metadados.

### 2. GitAgent (open-gitagent/gitagent) — Git-Native Agent

**Conceito central:** O agente É um repositório git. Identidade, regras, memória, ferramentas e skills são arquivos versionados.

**Arquivos do agente:**
- `agent.yaml` — manifesto (modelo, ferramentas, runtime)
- `SOUL.md` — identidade e personalidade
- `RULES.md` — constraints comportamentais
- `memory/MEMORY.md` — memória primária (git commit em cada save)
- `skills/<name>/SKILL.md` — skills com YAML frontmatter
- `tools/*.yaml` — definições declarativas de ferramentas
- `hooks/hooks.yaml` — lifecycle hook scripts

**Padrões-chave portáveis:**
1. **Memória como snapshots versionados** — Cada save = COW page table entry
2. **Identidade composta de múltiplos arquivos** — `IDENTITY.aios` + `PERSONALITY.aios` + `RULES.aios`
3. **Skills descobertas por scanning** — Walk de frame chains no lugar de readdir
4. **Awakening Mode** — Personalidade diferente quando memória vazia vs. estabelecida
5. **Tool descriptors binários** — Ferramentas como structs packed em faixa de endereço conhecida

### 3. NousResearch Hermes Agent — Self-Improving Agent

**Arquitetura:**
- `agent/conversation_loop.py` (~264KB) — loop principal
- `agent/context_engine.py` — ContextEngine trait pluggável
- `agent/memory_manager.py` — MemoryProvider + MemoryManager orchestrador
- `agent/curator.py` — Background self-improvement loop
- `acp_adapter/` — Agent Communication Protocol (JSON-RPC sobre WS)

**Padrões-chave portáveis:**
1. **ContextEngine trait** — `should_compress()` + `compress()` para compactação de memória
2. **MemoryProvider + MemoryManager** — Backend pluggável com prefetch/sync em background
3. **Capability-Based Tool Approval** — TrustCache expandido com per-skill policies
4. **IterationBudget** — Máximo de ciclos de poll por tarefa com grace cycle
5. **Skill Metadata Frontmatter** — Structured metadata na Skill trait (versão, autor, descrição ≤60 chars)

### 4. Rust Ecosystem: ZeroClaw + Ironclaw

**ZeroClaw** (32k ★):
- Runtime de agente single-binary. 30+ channel adapters (Discord, Telegram, Matrix, email, CLI)
- WASM sandbox com capability-based permissions
- GPIO/I2C/SPI/USB via `Peripheral` trait (Raspberry Pi, STM32, Arduino, ESP32)
- SOP engine (Standard Operating Procedures) — event-triggered workflows com approval gates
- Security-first: supervised autonomy default, Landlock/Bubblewrap/Seatbelt sandboxes, tool receipts

**Ironclaw** (12k ★):
- Reimplementação Rust do OpenClaw focada em privacidade/segurança
- WASM sandbox para ferramentas não-confiáveis
- PostgreSQL + pgvector para memória persistente com hybrid search
- Prompt injection defense em múltiplas camadas
- Credential protection com AES-256-GCM, leak detection

## Ideias Extraídas

### Legenda
- ✅ Imediata — <100 LOC, sem novas dependências, opera sobre tipos existentes
- 🟡 Baixa — <300 LOC, pode requerer refactor menor
- 🟠 Média — <600 LOC, requer nova abstração ou integração com subsistema existente
- 🔴 Alta — >600 LOC, requer redesign ou depende de sprint futuro
- ⏳ Teórica — Requer hardware inexistente ou stack de rede

---

### ✅ Imediata / Simples (Sprint 24)

#### #199 — IterationBudget com Grace Cycle (Hermes Agent)
**Fonte:** NousResearch/hermes-agent — `agent/iteration_budget.py`

**Descrição:** Adicionar `AgentTask.iteration_budget: Option<u16>` no executor. Quando o budget exaure, a task recebe UM ciclo extra (grace) para finalizar, depois é forcada a completar.

**Implementação no_std:** ~50 LOC no executor — `AtomicU16` por task, check no loop de poll.

**Dependências:** Nenhuma — NeuralExecutor já existe.

**Complexidade:** ✅ Imediata

---

#### #200 — Skill Metadata Frontmatter (Hermes Agent / OpenClaw)
**Fonte:** NousResearch/hermes-agent (/learn) + OpenClaw (skill marketplace)

**Descrição:** Adicionar `version`, `author`, `description` (≤60 chars), `tags` à `Skill` trait. A constraint de 60 chars para routing é específica — garante que descrições cabem em uma linha VGA 80-col.

**Implementação no_std:** ~80 LOC no `skill-registry`. Modificar `McpManifest` para incluir campos opcionais.

**Dependências:** Nenhuma — Skill trait e McpManifest existem.

**Complexidade:** ✅ Imediata

---

#### #201 — Audit Ring Buffer (GitAgent / OpenClaw)
**Fonte:** GitAgent (`.gitagent/audit.jsonl`) + OpenClaw (execution traces)

**Descrição:** Ring buffer fixo de eventos de auditoria no executor. Cada entrada: timestamp (LAPIC tick), task_id, tool_name, outcome. Expor via syscall para monitoramento externo.

**Implementação no_std:** ~80 LOC no executor. `heapless::Deque<AuditEntry, 1024>` ou array fixo com AtomicUsize head/tail.

**Dependências:** `heapless` crate (já temos acesso a `alloc`, mas heapless é melhor para ring buffer fixo).

**Complexidade:** ✅ Imediata

---

#### #202 — Agent Identity Awakening Mode (GitAgent / PAI)
**Fonte:** GitAgent (`src/context.ts` — awakening mode) + PAI (SOUL.md)

**Descrição:** Dois modos de personalidade no Hermes: "Awakening" (primeiro boot, sem memória) e "Established" (memória presente). MLP weights diferentes para cada modo, selecionados via flag `HAS_MEMORY` no hardware context tensor.

**Implementação no_std:** ~50 LOC no `hermes.rs`. Segundo conjunto de weights em .rodata.

**Dependências:** Nenhuma — IntentMlp já tem weights em .rodata.

**Complexidade:** ✅ Imediata

---

### 🟡 Complexidade Baixa (Sprint 24-27)

#### #203 — Context Fencing + Streaming Scrubber (Hermes Agent)
**Fonte:** NousResearch/hermes-agent — `agent/memory_manager.py` (StreamingContextScrubber)

**Descrição:** Marcar mensagens do EventBus com byte-level type markers: `[UserInput]`, `[HardwareTelemetry]`, `[AgentResponse]`. Scrubber state machine (estados Outside/InsideSpan) remove marcadores na recepção.

**Implementação no_std:** ~150 LOC. Enum `MessageTag` + função `scrub_message()` no event-bus.

**Dependências:** event-bus crate (já existe).

**Complexidade:** 🟡 Baixa

---

#### #204 — Heartbeat Idle Gate com Open Work Digest (Lethe)
**Fonte:** atemerev/lethe — `src/scheduler/brainstem.rs` + `heartbeat.rs`

**Descrição:** Extender o Watchdog existente para detectar "idle" vs "active". Um tick é idle apenas quando NÃO há reminders pendentes E NÃO há subagentes ativos (open work). Open work = unfinished tasks, subagentes bloqueados.

**Implementação no_std:** ~200 LOC no executor. Track de open work via `AtomicU16` de subagentes ativos.

**Dependências:** Watchdog já existe (Sprint 8). NeuralExecutor já spawna tarefas.

**Complexidade:** 🟡 Baixa

---

#### #205 — ProactiveRateLimiter com Deferred Outbox (Lethe)
**Fonte:** atemerev/lethe — `src/scheduler/proactive.rs`

**Descrição:** Rate limiter de mensagens proativas baseado em rolling window (24h de ticks) + cooldown mínimo entre envios. Outbox segura 1 mensagem deferida; mensagens novas superseded older ones; stale entries (>6h TTL) descartadas.

**Implementação no_std:** ~150 LOC. `heapless::Deque<u64, 64>` para timestamps + `AtomicU64` para outbox.

**Dependências:** `heapless` ou ring buffer manual.

**Complexidade:** 🟡 Baixa

---

#### #206 — Lifecycle Hooks via Pre/Post Poll Callbacks (GitAgent)
**Fonte:** GitAgent — `hooks/hooks.yaml`

**Descrição:** `HookRegistry` no executor com hooks `pre_poll` e `post_poll`. Hooks retornam `HookAction::Allow | Block | Modify`. Permite validação de segurança antes de executar tarefas.

**Implementação no_std:** ~200 LOC. Array fixo de function pointers com slots.

**Dependências:** Nenhuma — EventBus com CapabilityToken já fornece substrato.

**Complexidade:** 🟡 Baixa

---

### 🟠 Complexidade Média (Sprint 27-28)

#### #207 — MemoryProvider + MemoryManager Trait (Hermes Agent / Lethe)
**Fonte:** NousResearch/hermes-agent (`agent/memory_manager.py`) + atemerev/lethe (`src/memory/store.rs`)

**Descrição:** Trait `MemoryProvider` com `prefetch_all()`, `sync_all()`, `build_context_block()`. `MemoryManager` orchestrator que gerencia um provider ativo + callbacks de background. Mapeado sobre MHI tiers: `DramMemoryProvider`, `NvmeMemoryProvider`.

**Implementação no_std:** ~400 LOC. Extensão do MHI existente (Sprint 21).

**Dependências:** MHI (Sprint 21, já implementado). EventBus para callbacks.

**Complexidade:** 🟠 Média

---

#### #208 — Capability-Based Tool Permission Model (Hermes Agent / Ironclaw)
**Fonte:** NousResearch/hermes-agent (`acp_adapter/permissions.py`) + nearai/ironclaw (WASM sandbox permissions)

**Descrição:** Expandir TrustCache para verificar `(token, skill, tier)` antes da execução. Cada skill declara quais tiers de memória pode acessar e quais tokens são autorizados. Permissões: Allow, Deny, AllowOnce.

**Implementação no_std:** ~400 LOC sobre TrustCache existente (Sprint 22).

**Dependências:** TrustCache (já existe). Syscall de tier check.

**Complexidade:** 🟠 Média

---

#### #209 — Actor Registry com Permission Model (Lethe)
**Fonte:** atemerev/lethe — `src/actor/registry.rs` (~46KB)

**Descrição:** Registry de subagentes com: spawn/terminate/kill, `can_message()` hierarchical permission system, task state machine (Planned→Running→Blocked→Done), open_work tracking, persistência opcional.

**Implementação no_std:** ~500 LOC. BTreeMap + Slab allocator para atores.

**Dependências:** Slab Allocator (Sprint 19). EventBus para comunicação entre atores.

**Complexidade:** 🟠 Média

---

### 🔴 Complexidade Alta (Sprint 29+)

#### #210 — Subagent Crash-Recovery Persistence (Lethe / Ironclaw)
**Fonte:** atemerev/lethe (`src/actor/persistence.rs`) + nearai/ironclaw (PostgreSQL state)

**Descrição:** Persistir estado de subagentes em região de memória reservada (battery-backed SRAM ou página dedicada). No boot, walk da região e rehidrata agentes unfinished. Serialização via postcard/bincode.

**Implementação no_std:** ~600 LOC. Região de checkpoint + parser de serialização.

**Dependências:** Bitmap Frame Allocator. `postcard` crate para serialização (no_std).

**Complexidade:** 🔴 Alta

---

#### #211 — ComputeBackend Trait (ZeroClaw / Ironclaw / Hermes Agent)
**Fonte:** ZeroClaw (Peripheral trait + provider-agnostic), Ironclaw (WASM/Docker sandbox), Hermes (ACP protocol)

**Descrição:** Trait `ComputeBackend` que abstrai os 3 rings: `Ring0Npu`, `Ring1Gpu`, `Ring2Wasm`. `execute(&task) -> Result`. O intent router chama `COMPUTE_BACKEND.execute()` sem saber qual ring executa. Plugável: WASM, shell, GPU kernel, NPU micro-op.

**Implementação no_std:** ~800 LOC. Refactor do executor + trait dispatch.

**Dependências:** WASM sandbox (#183). GPU Tensor (#31-38). NPU micro-ops (#39-42).

**Complexidade:** 🔴 Alta — requer refactor significativo do executor

---

### ⏳ Teórica / Futuro

#### #212 — Plugin System via Loadable Page Ranges (GitAgent / OpenClaw marketplace)
**Fonte:** GitAgent (`plugins/<id>/plugin.yaml`) + OpenClaw (plugin marketplace)

**Descrição:** Plugin = região page-aligned em memória física contendo `PluginDescriptor` + tool descriptors + hook function pointers. Descoberta por walking de linked list de regiões. Inspirado no GitAgent: plugins são arquivos em SFS `plugins/`.

**Status:** ⏳ Futuro — depende de SFS Layer 2 (Sprint 24+) + page table manipulation avançada

---

#### #213 — WASM + Docker Sandbox para Skills (ZeroClaw / Ironclaw)
**Fonte:** ZeroClaw (WASM sandbox + Landlock/Bubblewrap), Ironclaw (WASM + Docker sandbox)

**Descrição:** Ferramentas não-confiáveis executam em WASM containers isolados com capability-based permissions (HTTP, secrets, tool invocation) + rate limiting + resource limits (memória, CPU, tempo).

**Status:** ⏳ Futuro — depende de #183 WASM Sandbox + Network Sprint

---

### ❌ Descartadas

| Fonte | Ideia | Motivo |
|---|---|---|
| ZeroClaw | 30+ channel adapters (Discord, Telegram, Matrix, email) | Requer rede madura + TLS. Inv -iável sem Network Sprint completo |
| Ironclaw | PostgreSQL + pgvector + hybrid search | `no_std` sem SQLite/Postgres. SQL não é portável |
| Hermes Agent | LLM-based curator auto-skill-authoring | Nosso Hermes usa MLP, não LLM. O state machine determinístico (#209) é portável, o LLM não |
| GitAgent | Git como persistence layer | `git` não existe em no_std bare-metal. COW page tables substituem |
| OpenClaw | Plugin marketplace com download | Requer HTTP client + TLS. Network Sprint (Sprint 24) |
| ZeroClaw | GPIO/I2C/SPI/USB via Peripheral trait | Hardware específico (RPi/STM32). Nosso target é x86-64 APU |

## Mapa de Implementação

```
Sprint 24── #199 IterationBudget (imediata)
           #200 Skill Metadata Frontmatter (imediata)
           #201 Audit Ring Buffer (imediata)
           #202 Agent Identity Awakening Mode (imediata)
Sprint 27── #203 Context Fencing (baixa)
           #204 Heartbeat Idle Gate (baixa)
           #205 ProactiveRateLimiter (baixa)
           #206 Lifecycle Hooks (baixa)
Sprint 28── #207 MemoryProvider + MemoryManager (média)
           #208 Capability-Based Tool Permissions (média)
           #209 Actor Registry Permission Model (média)
Sprint 29+──#210 Crash-Recovery Persistence (alta)
           #211 ComputeBackend Trait (alta)
           #212 Plugin System via Page Ranges (teórica)
           #213 WASM + Docker Sandbox (teórica)
Descartado── 6 ideias (ver tabela acima)
```

## Resumo

- **15 ideias implementáveis** (#199-213) em Sprints 24-29+
- **2 ideias futuras** (#212-213) dependentes de SFS + Network
- **6 ideias descartadas** por incompatibilidade com bare-metal `no_std`
- **Total de 15 ideias novas** extraídas de 21 repos + 4 deep-dives aprofundados
- **Descoberta crítica:** Ecossistema Rust AI Agent explodiu em 2026 — 5 repos Rust-native com >10k stars (ZeroClaw, Ironclaw, Lethe, agent-browser, llmfit)

## Referências

- ADR-0020: Crom Ecosystem Analysis (Tier 0) — 12 ideias
- ADR-0021: Life OS Ecosystem Analysis (Tier 1) — 22 ideias
- IDEA_BANK.md: Itens #199-213 (este documento)
- Sprint 22: Block 5 (Skills + Trust Cache)
- Sprint 23: Network Sprint (VirtIO-net + smoltcp)
- Sprint 24: Bugfix + I/O Sprint
- atemerev/lethe v0.22.23: https://github.com/atemerev/lethe
- NousResearch/hermes-agent: https://github.com/NousResearch/hermes-agent
- zeroclaw-labs/zeroclaw: https://github.com/zeroclaw-labs/zeroclaw
- nearai/ironclaw: https://github.com/nearai/ironclaw
- open-gitagent/gitagent: https://github.com/open-gitagent/gitagent
- openclaw/openclaw: https://github.com/openclaw/openclaw

## Changelog

| Date | Change | Author |
|---|---|---|
| 2026-06-25 | Initial draft — 21 repos analyzed, 15 ideas extracted (#199-213), 4 deep-dives | IDA IA + Dev |
