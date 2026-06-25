# ADR-0021: Life OS / Personal OS Ecosystem Analysis (Tier 1)

**Status:** Draft  
**Date:** 2026-06-25  
**Author:** IDA IA + Dev  
**PR:** TBD

## Context

Após analisar o ecossistema Crom (Tier 0 — MrJc01/75 repos) para ideias portáveis para neural-os-core, expandimos a análise para Tier 1: o ecossistema Life OS / Personal OS. Foram analisados 20 repositórios cobrindo desde assistentes pessoais baseados em Claude Code (PAI) até sistemas operacionais completos com agentes multi-propósito (ArgentOS, OpenDAN, taOS, PrismOS-AI).

O objetivo: extrair ideias implementáveis em `no_std` bare-metal x86-64, classificá-las por viabilidade técnica e complexidade, e listar as descartadas com justificativa.

## Repos Analisados

| # | Repo | Stars | Stack | Função Central |
|---|---|---|---|---|
| 1 | [danielmiessler/LifeOS (PAI)](https://github.com/danielmiessler/LifeOS) | 16.1k | TypeScript/Bun | Life OS baseado em Claude Code, Pulse daemon, Algorithm v6.3, ISA |
| 2 | [mkbhardwas12/prismos-ai](https://github.com/mkbhardwas12/prismos-ai) | 175 | Tauri 2/Rust/React | OS local-first, 8 agentes, Spectrum Graph 7D, WASM sandbox |
| 3 | [YuxuanSha/openceph](https://github.com/YuxuanSha/openceph) | 11 | TypeScript/Node | OS proativo com "Tentáculos" autônomos, Heartbeat, MCP Bridge |
| 4 | [jaylfc/taOS](https://github.com/jaylfc/taOS) | 258 | Python/FastAPI | Desktop web, cluster distribuído, 16 frameworks, 108 apps |
| 5 | [0xsyncroot/nimbus-os](https://github.com/0xsyncroot/nimbus-os) | 1 | Bun/TypeScript | Agente com SOUL/IDENTITY, Runtime SDD, memória entre sessões |
| 6 | [ArgentAIOS/argentos-core](https://github.com/ArgentAIOS/argentos-core) | 112 | TypeScript/Node | Memória híbrida, Heartbeat, Contemplation, AppForge, 64 conectores |
| 7 | [alfred-os/AlfredOS](https://github.com/alfred-os/AlfredOS) | 4 | Python/Docker | Multi-usuário/persona, segurança hardening, pré-implementação |
| 8 | [fiatrete/OpenDAN](https://github.com/fiatrete/OpenDAN) | 2k | Python/Docker | AIOS all-in-one, Agentes/Workflows, Knowledge Base, AIGC |
| 9 | [zmrlk/bOS](https://github.com/zmrlk/bOS) | 10 | Markdown/Shell | 19 agentes via Claude Code, ADHD-first, Energy tracking |
| 10 | [Matiasxth/capability-os](https://github.com/Matiasxth/capability-os) | 2 | Python/React | 22 plugins, workflow visual, workers Redis, multi-canal |
| 11 | [sielay/eidan](https://github.com/sielay/eidan) | 1 | TypeScript/Node | matbot runtime, Postgres, MCP + A2A + AG-UI, plugins |
| 12 | [itseffi/agentic-os](https://github.com/itseffi/agentic-os) | 72 | Markdown/Python | AGENTS.md multi-runtime, skills padronizadas |
| 13 | [nbramia/LifeOS](https://github.com/nbramia/LifeOS) | 15 | Python/FastAPI | Integração Obsidian+Gmail+iMessage, CRM, agente #agent |
| 14 | [joinlifeos/lifeos-core](https://github.com/joinlifeos/lifeos-core) | 0 | TypeScript | Schema-driven vault, Entidades/Eventos/Relações, kernel imutável |
| 15-20 | picturpoet, jasonhnd, drshailesh88, luneth90, anguiguirao, marcusglee11 | <5 | Variado | Forks menores de LifeOS/PAI, sem código original significativo |

## Categorias Detectadas

### 1. PAIs (Personal AI Infrastructure) — Claude Code-based
- danielmiessler/LifeOS (PAI) — 16.1k stars, dominante no ecossistema
- zmrlk/bOS — ADHD-first design, energy tracking
- itseffi/agentic-os — multi-runtime, skills padronizadas
- nbramia/LifeOS — integração serviços externos (Gmail, iMessage, Obsidian)

### 2. Full-stack Personal OS Platforms
- **PrismOS-AI** (175 stars) — Tauri 2 + Rust + React, 8 agentes, 100% local
- **taOS** (258 stars) — Web desktop, cluster, 108 apps, Temporal Knowledge Graph
- **ArgentOS Core** (112 stars) — Memória híbrida, Heartbeat, AppForge
- **OpenDAN** (2k stars) — All-in-one AIOS Docker-based
- **Capability OS** — 22 plugins, workflow builder visual
- **Eidan** — MCP + A2A + AG-UI, matbot runtime
- **AlfredOS** — Multi-persona, pré-implementação

### 3. Proactive AI OS
- **OpenCeph** — "Tentáculos" autônomos, push proativo, MCP Bridge
- **Nimbus OS** — SOUL/IDENTITY/MEMORY, Runtime SDD

### 4. Schema-driven Knowledge System
- **joinlifeos/lifeos-core** — Vault schema-driven, kernel imutável

### 5. Forks Menores (sem contribuição original)
- picturpoet, jasonhnd, drshailesh88, luneth90, anguiguirao, marcusglee11 — forks sem alterações significativas

## Ideias Extraídas

### ✅ Imediata / Simples

#### #177 — 7D Spectrum Graph Leve (PrismOS-AI)
**Fonte:** mkbhardwas12/prismos-ai — Spectrum Graph com 7 dimensões + Edge Prophecy

**Descrição:** Grafo de conhecimento multidimensional onde cada aresta carrega 7 eixos (tempo, trust score, recência, frequência, intensidade, contexto, tipo de relação). Edge Prophecy prevê conexões futuras via similaridade Jaccard entre nós.

**Por que cabe no neural-os:**
- O EventBus atual usa `BTreeMap<&str, Vec<Receiver>>` — roteamento por string, sem memória associativa
- Edge Prophecy é similar ao CDC Rabin (#165) — ambos baseados em rolling hash + similaridade
- Dá ao Hermes "memória associativa" real: "você mencionou X há 3 dias, conectando com Y"

**Implementação no_std:**
- `Vec<(u64, u64, u8, u64)>` — 4 words por aresta (src, dst, relation_type, timestamp)
- 7 dimensões compactadas em 2 u64 bitset + 1 u8 + flags
- Edge Prophecy: Jaccard similarity entre vizinhança de dois nós, threshold configurável
- ~200 LOC sobre o EventBus existente

**Dependências:** Nenhuma. Só refatorar EventBus internals.

**Complexidade:** Imediata — ~200 LOC, só opera sobre tipos existentes

---

#### #178 — Runtime SDD (Structured Decision Document) (Nimbus OS)
**Fonte:** 0xsyncroot/nimbus-os — SDD antes de executar tarefas não-triviais

**Descrição:** Antes de executar qualquer ação não-trivial, o agente escreve um mini-spec de 5 seções: (1) Goal, (2) Context, (3) Plan, (4) Expected Outcome, (5) Rollback. Mostra uma linha de status "[SDD] Reasoning..." e executa.

**Por que cabe no neural-os:**
- O Intent Router atual executa MLP → argmax → skill direto, sem transparência
- SDD adiciona "thinking visible" — essencial para depuração de decisões erradas
- Similar ao "chain-of-thought" mas deterministicamente estruturado

**Implementação no_std:**
- Struct `Sdd { goal: &'static str, context: [u8; 64], plan_id: u8 }` em pilha
- Formatação VGA: `[SDD] Intent=status Reason=user_asked Plan=skill:system_status`
- ~80 LOC no `intent_router_daemon`

**Dependências:** Nenhuma

**Complexidade:** Imediata — ~80 LOC, só formatação de string

---

### 🟡 Complexidade Baixa

#### #179 — File System as Context (PAI / bOS)
**Fonte:** danielmiessler/LifeOS (PAI Algorithm v6.3.0) + zmrlk/bOS — filesystem como índice de conhecimento

**Descrição:** Em vez de RAG embedding-heavy, usa o próprio filesystem como índice de contexto. Arquivos markdown com cross-references + ripgrep-style scan substituem banco vetorial. O "contexto" é construído catando arquivos relevantes por padrão de nome/conteúdo, não por similaridade coseno.

**Por que cabe no neural-os:**
- Nosso SFS (Semantic File System) é Layer 2 no roadmap (Sprint 24+)
- Esta técnica bridge permite busca semântica sem chromadb ou sqlite-vec
- Só precisa do CDC Rabin (#165) chunking + grep-like scan dos arquivos `.bitmem`

**Implementação no_std:**
- Scan linear de diretório (quando tivermos NVMe/VirtIO-blk)
- CDC Rabin chunks → hash table → lookup por substring de intent
- ~300 LOC sobre o chunker (#165)

**Dependências:** #165 CDC Rabin Chunking, VirtIO-blk ou NVMe driver (Sprint 24+)

**Complexidade:** Baixa — o chunker já existe (#165), falta o driver de bloco

---

#### #180 — DA Identity Layer (PAI / Nimbus / AlfredOS)
**Fonte:** danielmiessler/LifeOS (SOUL.md/IDENTITY.md/TELOS.md), 0xsyncroot/nimbus-os (SOUL/IDENTITY/MEMORY), alfred-os/AlfredOS (multi-persona)

**Descrição:** O Hermes atual é stateless — cada sessão começa do zero, sem personalidade. Uma DA Identity Layer adiciona persona persistente: `SOUL.md` (quem sou eu), `IDENTITY.md` (nome, voz, personalidade), `TELOS.md` (propósito). O Intent Router carrega no boot.

**Por que cabe no neural-os:**
- O Hermes teria uma voz consistente entre sessões
- O system prompt do Intent Router incorpora a identidade
- A persona pode ser trocada via comando `/persona <nome>`

**Implementação no_std:**
- Struct `DaIdentity { name: [u8; 32], voice: VoiceType, personality: [u8; 64] }`
- Parser markdown mínimo (~50 LOC) para ler arquivo de config
- Carregado no boot antes do executor
- ~100 LOC no kernel init

**Dependências:** VirtIO-blk (para ler arquivo de identidade) ou identidade hardcoded como fallback

**Complexidade:** Baixa — parser markdown simples, struct em .rodata

---

#### #184 — Intent Transparency / "Why this response?" (PrismOS-AI)
**Fonte:** mkbhardwas12/prismos-ai — reasoning band visibility após cada resposta

**Descrição:** Após cada resposta, mostrar "query type: X, reasoning band: Y, agents involved: [A, B, C], confidence: Z%". Permite ao usuário entender *por que* o Hermes respondeu aquilo.

**Por que cabe no neural-os:**
- Depuração do Intent Router é crítica: "por que argmax escolheu skill X?"
- O Hermes atual executa MLP → argmax silenciosamente
- Intent Transparency mostra o reasoning completo no VGA/serial

**Implementação no_std:**
- Struct `IntentTrace { query_type: u8, confidence: f32, skill_id: u8, alternatives: [u8; 3] }`
- Log estruturado após cada inferência
- ~200 LOC no `intent_router_daemon`

**Dependências:** Nenhuma — o Intent Router já tem o grafo de execução

**Complexidade:** Baixa — ~200 LOC, só log estruturado

---

#### #185 — Energy / Circadian Tracking (bOS)
**Fonte:** zmrlk/bOS — ADHD-first design, energy tracking, task-to-capacity matching

**Descrição:** Usuário reporta energia (1-10) diariamente via comando `/energy 7`. O scheduler casa tarefas com capacidade real do usuário, não slots de calendário. Máximo 5-8 linhas por bloco, 15-25 min task chunks, dopamine hooks para conclusão.

**Por que cabe no neural-os:**
- Self-Optimization (#157-163) foca em padrões de comando
- Energy tracking adiciona dimensão fisiológica — "Hermes sabe quando você está mais produtivo"
- Simples de implementar como skill

**Implementação no_std:**
- `AtomicU8 ENERGY_LEVEL` atualizado por comando `/energy`
- Timer tick counting para tracking de horário produtivo
- Exibição no status bar do Hermes console
- ~150 LOC como skill

**Dependências:** #157 Usage Pattern Analyzer (para correlacionar energia com produtividade real)

**Complexidade:** Baixa — ~150 LOC, skill isolada

---

### 🟠 Complexidade Média

#### #181 — Temporal Knowledge Graph (taOS / taOSmd)
**Fonte:** jaylfc/taOS — taOSmd com 97% accuracy no LongMemEval-S, temporal validity windows, contradiction detection

**Descrição:** Grafo temporal onde cada aresta tem validity window (start_time, end_time). Suporte a contradiction detection: "chefe é João (2020-2023)" vs "chefe é Maria (2023-)" detecta sobreposição inválida. Archive zero-loss append-only. 97% de acurácia no LongMemEval-S (benchmark de memória de longo prazo).

**Por que cabe no neural-os:**
- O BTreeMap do EventBus atual é atemporal — não sabe *quando* algo aconteceu
- O Hermes poderia lembrar "o usuário pediu X ontem" com precisão temporal
- Contradiction detection evita que o Hermes dê respostas conflitantes

**Implementação no_std:**
- Extensão do Spectrum Graph (#177): cada aresta ganha `(t_start, t_end)`
- Append-only archive em `Vec<(u64, u64, u64, u64, u8)>` — (t_start, t_end, entity_a, entity_b, relation)
- Contradiction detection: verifica overlap temporal antes de inserir
- ~500 LOC sobre #177

**Dependências:** #177 Spectrum Graph como base evolution

**Complexidade:** Média — estende #177, requer validação temporal

---

#### #182 — Proactive Push / Heartbeat Scheduler (OpenCeph / ArgentOS)
**Fonte:** YuxuanSha/openceph (Tentáculos) + ArgentAIOS/argentos-core (Heartbeat/Contemplation)

**Descrição:** Tentáculos autônomos rodando 24/7 que monitoram fontes (Hacker News, arXiv, GitHub releases) e fazem push proativo via CLI/Telegram/console com dedup inteligente. ArgentOS expande com Heartbeat (pulso periódico de sanity check) + Contemplation (modo de reflexão noturna).

**Por que cabe no neural-os:**
- NeuralExecutor atual é puramente reativo: poll tasks + hlt
- Heartbeat adiciona push agendado: "Hermes te avisa sem você perguntar"
- Contemplation noturna: o Hermes revisa o dia, sugere melhorias

**Implementação no_std:**
- Scheduler baseado em LAPIC timer ticks (já temos: 8,388,608 ticks)
- Dedup hash + priority queue de tarefas push
- ~400 LOC no executor

**Dependências:** Network Sprint (Sprint 24) para push externo

**Complexidade:** Média — scheduler já existe, precisa de agendamento push

---

#### #183 — WASM Sandbox para Skills (PrismOS-AI / taOS)
**Fonte:** mkbhardwas12/prismos-ai (WASM sandbox com fuel metering) + jaylfc/taOS (16 agent frameworks, per-agent allow-lists, auto-rollback)

**Descrição:** Skills executam dentro de wasmtime containers com limites de memória (1-16 MB), fuel metering (limite de instruções por chamada), per-agent allow-lists (quais syscalls cada skill pode chamar), e auto-rollback em anomalia.

**Por que cabe no neural-os:**
- SkillRegistry atual executa no mesmo espaço de endereçamento que o kernel
- Uma skill maliciosa ou com bug pode corromper o kernel inteiro
- WASM daria isolamento real + limite de recursos

**Implementação no_std:**
- Alternativa a `wasmtime` (que não suporta `no_std` nativamente): sandbox via paging
- Executar skill em página separada com PTE NX + tabela de páginas custom
- Comunicar via shared memory controlado
- ~800 LOC

**Dependências:** #172 MCP Server (para protocolo skill↔kernel), Slab Allocator (#94)

**Complexidade:** Média-alta — isolamento via paging é viável mas requer cuidado com TLB

---

### 🔴 Complexidade Alta

#### #186 — AppForge / App Store (ArgentOS / taOS)
**Fonte:** ArgentAIOS/argentos-core (AppForge) + jaylfc/taOS (108 apps catalog, one-click install, hardware-aware filtering)

**Descrição:** Plataforma de apps com bases, campos, registros, visualizações. Store com 108 apps catalog, instalação one-click, filtering por capacidade de hardware. Apps podem ser Skills, Dashboards, ou Workflows.

**Por que cabe:**
- Nosso SFS (Layer 2) + MCP (#172) seriam a base
- Users poderiam instalar "Skills" como apps
- Store backend poderia rodar sobre MCP

**Desafios:**
- Store frontend requer interface visual — VGA text 80×25 é muito limitado
- Instalação one-click requer download via HTTP (Network Sprint)
- Catalog com filtering requer parser de metadados

**Implementação no_std:**
- Backend de catalog puro: sim. Frontend: inviável sem framebuffer
- ~1500 LOC no total (backend ~400, frontend impossível sem FB)

**Dependências:** Network Sprint + SFS + MCP Server + framebuffer (💰 Sponsor)

**Complexidade:** Alta — frontend é o gargalo, backend é factível

---

#### #187 — Multi-User / Multi-Persona com Isolamento (AlfredOS / ArgentOS)
**Fonte:** alfred-os/AlfredOS (multi-user/persona, hardening) + ArgentAIOS/argentos-core (64 conectores, Certificate Authority)

**Descrição:** Vários usuários com personas próprias, memória isolada, trust tiers diferentes por usuário. Dual-LLM split: um LLM "quarentena" para execução de código, outro para planejamento estratégico. Certificate Authority para identidade de agente.

**Por que cabe:**
- Nosso kernel é single-user por design (kernel bare-metal pessoal)
- Multi-user requer IPC entre agentes de usuários diferentes
- Trust tiers por usuário aumentam flexibilidade

**Desafios:**
- Kernel bare-metal single-user: multi-user requer redesign do scheduler
- Isolamento de memória entre personas: PTable switching por usuário
- Certificate Authority: implementação Ed25519 já prevista (#176)

**Implementação no_std:**
- PerCpu → PerUser struct (estende PerCpu existente)
- TrustCache vira `BTreeMap<u64, UserContext>` com tiers por usuário
- ~600 LOC

**Dependências:** PerCpu (#24-33, já implementado), #176 Ed25519 Trust Identity

**Complexidade:** Alta — requer redesign do scheduler + isolamento de tabela de páginas

---

### ⏳ Teórica / Futuro

#### #188 — Visual Workflow Builder (Capability OS / ArgentOS)
**Fonte:** Matiasxth/capability-os (ReactFlow builder, 13 tipos de nó) + ArgentAIOS/argentos-core (AppForge builder)

**Descrição:** Drag-and-drop pipeline builder com nós Trigger, Tool, Agent, Condition, Loop, Gate. AI workflow designer via chat. Visual DAG editing.

**Por que é teórica:**
- Requer framebuffer VESA com mouse — não temos (VGA text only)
- Terminal CLI 80×25 não suporta drag-and-drop
- Poderia ser CLI-based com DAG em ASCII, mas perde todo o valor visual

**Status:** ⏳ Futuro — só quando tivermos framebuffer (💰 Sponsor)

---

#### #189 — Federated Cluster / P2P Workers (taOS)
**Fonte:** jaylfc/taOS — "any device → mesh of AI compute", pairing por PIN, auto-discovery

**Descrição:** Combina ANY device (gaming PC, Mac, Raspberry Pi, Android phone) em um mesh de AI compute. Workers com auto-descoberta, pareamento por PIN, distribuição de tarefa com checkpoint.

**Por que é teórica:**
- Cluster P2P com descoberta de nós requer stack de rede madura (mDNS/DNS-SD)
- Distribuição de tarefa com checkpoint requer scheduler distribuído
- Viável tecnicamente mas anos-luz do nosso estágio atual

**Status:** ⏳ Futuro — depende de toda a camada de rede + scheduler distribuído + WASM remoto

---

### ❌ Descartadas

| Fonte | Ideia | Motivo do Descarte |
|---|---|---|
| danielmiessler/LifeOS | Pulse Daemon localhost:31337 com web dashboard | Requer HTTP server + web UI. Inviável em bare-metal no_std |
| Todos | One-liner curl install | Nosso deploy via `cargo bootimage` + QEMU. Não há SO hospedeiro |
| nbramia/LifeOS | Integração Gmail/Google Calendar/iMessage | Requer OAuth2 + TLS + APIs REST. Stack de rede limitada |
| Quase todos | PostgreSQL/SQLite com FTS5 | `no_std` sem `mmap` inviável. SFS é Layer 2 sem SQL |
| Todos | LLM Provider Router (GPT/Claude/Llama) | Nosso BitNet LLM é on-device 2-bit ternário. Não há "provedores" |
| zmrlk/bOS | macOS launchd / Linux systemd integração | Platform-specific. Não rodamos sobre Linux/macOS — somos o kernel |
| alfred-os, taOS | Docker / LXC containers | Container isolation requer Linux. Nosso isolamento é ring 0/1/2 |
| Matiasxth/capability-os | Redis / RabbitMQ workers | Middleware externo. TicketLock substitui filas |

---

## PAI Deep-Dive — danielmiessler/LifeOS v5.0.0

O PAI (Personal AI Infrastructure) é o repositório mais maduro do ecossistema Life OS (16.1k stars, 45 skills, 37 hooks, 171 workflows, Algorithm v6.3.0). Uma análise de código aprofundada (~50+ arquivos lidos) revelou 9 conceitos adicionais portáveis para neural-os-core, além dos 13 já extraídos na análise inicial.

### Estrutura do Repositório (Releases/v5.0.0/.claude/)

```
.claude/
├── CLAUDE.md          — Routing table, modes, operational rules
├── ISA.md             — Ideal State Artifact (12-section spec)
├── PAI/
│   ├── ALGORITHM/     — v6.3.0 (7-phase loop), capabilities, escalation gate
│   ├── DOCUMENTATION/ — Architecture, Memory, Skills, Hooks, Security systems
│   ├── MEMORY/        — v7.6: WORK/KNOWLEDGE/LEARNING + typed graph
│   ├── PULSE/         — Life Dashboard (localhost:31337), unified daemon
│   ├── TEMPLATES/     — ISA templates, report templates
│   ├── TOOLS/         — Inference.ts, validate-protected.ts
│   ├── USER/          — Principal identity, DA identity, TELOS, projects
│   └── PAI_SYSTEM_PROMPT.md — Constitutional rules (20 KB)
├── hooks/             — 37 hooks (PromptProcessing, SecurityPipeline, etc.)
├── skills/            — 45 skills (ISA, Telos, Council, Loop, Evals, etc.)
└── install.sh         — One-line installer (curl | bash)
```

**Stack:** TypeScript/Bun, Claude Code native. MIT license. Desenvolvido por Daniel Miessler.

### Conceitos Portados (#190-#198)

| # | Conceito | Ideia | Complexidade | Sprint | Ref |
|---|---|---|---|---|---|
| 190 | **Algorithm loop 7 fases** | THINK antes de agir, VERIFY depois. Não só MLP→argmax→skill. THINK carrega contexto, VERIFY checa ISC | 🟡 Baixa | 27 | `PAI_SYSTEM_PROMPT.md` Sec "Mode Architecture", `ALGORITHM/v6.3.0.md` |
| 191 | **Council skill** | 3 vozes (otimista, cético, pragmático) votam antes de decisão ambígua | 🟡 Baixa | 27 | `skills/Council/SKILL.md` |
| 192 | **Loop Detection** | Monitora repetição de agente, break após N≥3 sem progresso | ✅ Imediata | 24 | `hooks/RepeatDetection.hook.ts` |
| 193 | **Bitter Pill Engineering** | Força etapas obrigatórias (testar antes de deploy), recusa atalhos | 🟡 Baixa | 27 | `skills/BitterPillEngineering/SKILL.md` |
| 194 | **ISA como formato de sprint** | Cada sprint tem ISA com ISCs verificáveis (Problem→Goal→Criteria→Verification) | 🟡 Baixa | 27 | `ISA.md` (16 KB, 12 seções, 30 ISCs) |
| 195 | **Hermes Rating** | 👍/👎 após cada resposta. Alimenta Success Engine + Trust Cache | 🟡 Baixa | 27 | `hooks/SatisfactionCapture.hook.ts` (18 KB) |
| 196 | **Evals skill** | Avalia respostas contra critérios antes de mostrar. Re-executa se confiança baixa | 🟠 Média | 28 | `skills/Evals/SKILL.md` |
| 197 | **Container Zones** | Trust token define quais memórias/skills pode acessar. Containment em bare-metal | 🟠 Média | 28 | `hooks/lib/containment-zones.ts`, `ContainmentGuard.hook.ts` |
| 198 | **Boot security policy** | Regexes de segurança compiladas no boot. Valida skills contra patterns | 🟡 Baixa | 27 | `.pai-protected.json` (17 KB, 17 categorias, 100+ regexes) |

### Referências para Futuras Atualizações

O PAI evolui rapidamente (v4→v5→v6 em 3 meses). Para reavaliar este repositório em sprints futuros:

- **URL do repo:** https://github.com/danielmiessler/LifeOS
- **Última release analisada:** v5.0.0 (2026-04-30)
- **Arquivos-chave a monitorar:**
  - `Releases/v*/README.md` — release notes com novas features
  - `Releases/v*/.claude/PAI/PAI_SYSTEM_PROMPT.md` — constitutional rules (evolução dos princípios)
  - `Releases/v*/.claude/PAI/ALGORITHM/v*.md` — versão do Algorithm (features de execução)
  - `Releases/v*/.claude/PAI/DOCUMENTATION/` — docs de sistema (arquitetura, memória, hooks)
  - `Releases/v*/.claude/skills/*/SKILL.md` — novas skills
  - `Releases/v*/.claude/hooks/*.hook.ts` — novos hooks
- **Métricas de maturidade:** stars (atualmente 16.1k), número de skills (45), hooks (37), versão do Algorithm (v6.3.0)
- **Roadmap público:** Local Model Support, Granular Model Routing, Remote Access, Outbound Phone Calling, External Notifications

## Mapa de Implementação

```
Sprint 24 ─── #177 Spectrum Graph (imediata)
           ─── #178 Runtime SDD (imediata)
           ─── #192 Loop Detection (imediata)
Sprint 27 ─── #179 FS as Context (baixa)
           ─── #180 DA Identity Layer (baixa)
           ─── #184 Intent Transparency (baixa)
           ─── #185 Energy/Circadian Tracking (baixa)
           ─── #190 Algorithm 7-phase loop (baixa)
           ─── #191 Council skill (baixa)
           ─── #193 Bitter Pill Engineering (baixa)
           ─── #194 ISA sprint format (baixa)
           ─── #195 Hermes Rating (baixa)
           ─── #198 Boot security policy (baixa)
Sprint 28 ─── #181 Temporal Knowledge Graph (média)
           ─── #182 Proactive Heartbeat (média)
           ─── #183 WASM Sandbox (média)
           ─── #196 Evals skill (média)
           ─── #197 Container Zones (média)
Sprint 29+ ── #186 AppForge (alta)
           ─── #187 Multi-User (alta)
           ─── #188 Workflow Builder (teórica)
           ─── #189 Federated Cluster (teórica)
Descartado ── 9 ideias (ver tabela acima)
```

## Resumo

- **22 ideias implementáveis** (#177-198) em Sprints 24-28
- **2 ideias futuras** (#188-189) dependentes de hardware/framebuffer
- **9 ideias descartadas** por incompatibilidade com bare-metal `no_std`
- **Total de 24 ideias novas** extraídas de 21 repos (20 Life OS + PAI deep-dive)
- **6 forks menores** sem contribuição original (repos 15-20)

## Referências

- ADR-0020: Crom Ecosystem Analysis (Tier 0)
- IDEA_BANK.md: Itens #177-198
- Sprint 22: Block 5 (Skills + Trust Cache)
- Sprint 23: Network Sprint (VirtIO-net + smoltcp)
- Sprint 24: I/O Sprint (xHCI, VirtIO-blk)
- PAI v5.0.0: https://github.com/danielmiessler/LifeOS/Releases/v5.0.0/

### Links para Re-análise Futura

Para manter esta análise atualizada conforme o PAI evolui:

```
# Verificar última versão do PAI
curl -s https://api.github.com/repos/danielmiessler/LifeOS/releases/latest | jq .tag_name

# Verificar número de skills (métrica de maturidade)
curl -s https://api.github.com/repos/danielmiessler/LifeOS/contents/Releases/v5.0.0/.claude/skills | jq length

# Verificar versão do Algorithm
curl -s https://raw.githubusercontent.com/danielmiessler/LifeOS/main/Releases/v5.0.0/.claude/PAI/ALGORITHM/LATEST

# Verificar número de hooks
curl -s https://api.github.com/repos/danielmiessler/LifeOS/contents/Releases/v5.0.0/.claude/hooks | jq length

# Verificar novas skills (comparar lista com skills/CLAUDE.md)
curl -s https://raw.githubusercontent.com/danielmiessler/LifeOS/main/Releases/v5.0.0/.claude/skills/CLAUDE.md
```

## Changelog

| Date | Change | Author |
|---|---|---|
| 2026-06-25 | Initial draft — 20 repos analyzed, 13 ideas extracted (#177-189) | IDA IA + Dev |
| 2026-06-25 | PAI deep-dive — ~50+ files read, 9 additional ideas extracted (#190-198) | IDA IA + Dev |
