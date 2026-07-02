# Sprint 72 — Evolução Agêntica: Crew, Flow, Cache, Workflow, Graph, Index

**v0.72.0** — Padrões de 10 fontes externas adaptados ao Neural OS Hermes.
Sem FAT12 (só boot). Usando VFS neural (`/system/`, `/chat/`, `/proc/`, `/dev/`).
Hot-load via EventBus + Orchestrator, sem YAML compilado.

---

## Arquitetura

```
HermesAgent (Router)
 ├── CrewPool: grupos de agentes com goal comum
 ├── IntentCache: evita re-classificar intents repetidos
 ├── WorkflowEngine: THINK→PLAN→EXECUTE→VERIFY→REFINE
 └── PriorityQueue: prioridades explicitas (0-9)

CortexAgent (Inference)
 ├── ContextLoader: carrega preconditions do VFS
 ├── OutputCache: cacheia outputs de skills idempotentes
 └── SelfCritique: verifica output antes de entregar

AgentRegistry (Core)
 ├── StateGraph: substitui round-robin por grafo de estados
 ├── FlowTrigger: @start, @listen(topic), @router(topic→fn)
 └── TaskSchema: expected_output para validacao de skills

SkillRegistry (Skills)
 ├── SkillIndex: catalogo por dominio/capacidade
 ├── JobPreconditions: contexto carregado do VFS antes de executar
 └── FlowIntegration: agent handoff + pipeline composition

EventBus
 ├── CREW_STATUS, TASK_ASSIGN, TASK_RESULT
 ├── HUMAN_INPUT_REQUEST, HUMAN_INPUT_RESPONSE
 └── FLOW_TRIGGER, WORKFLOW_PHASE
```

---

## Itens

### 72.1 — Crew + FlowTrigger (~300 LOC)
- `Crew` struct: nome, goal, agents, process type
- `FlowTrigger::Listen/Start/Router` no AgentManifest
- AgentRegistry.create_crew() / kickoff()
- HermesAgent como ManagerAgent em process hierarchical
- EventBus topics: CREW_CREATED, TASK_ASSIGNED, TASK_DONE

### 72.2 — TaskSchema + JobPreconditions (~200 LOC)
- `OutputSchema` enum para validar output de skills
- `JobPreconditions` no McpManifest: contexto carregado do VFS via `read_vfs("/system/jobs/<name>.md")`
- SkillRegistry valida output contra schema
- Se falha: publica SKILL_MISMATCH, Hermes re-tenta

### 72.3 — IntentCache + OutputCache (~200 LOC)
- HermesAgent: cache de intenções (hash do input → intent)
- SkillRegistry: cache de outputs idempotentes (TTL por ticks)
- Bypassa LLM em hits de cache

### 72.4 — WorkflowEngine + SelfCritique (~250 LOC)
- HermesAgent.run_workflow(): THINK→PLAN→EXECUTE→VERIFY→REFINE
- CortexAgent critica output, se confianca < threshold, re-gera
- Human-in-loop: EventBus HUMAN_INPUT_REQUEST (pausa workflow)

### 72.5 — StateGraph Scheduler (~200 LOC)
- StateGraph substitui round-robin no AgentRegistry.run()
- Nos = agentes, arestas = condicoes (funcao sobre EventBus)
- Condicoes: "NET_CONFIGURED publicado" → ativa NetAgent

### 72.6 — SkillIndex + MCP Catalog (~150 LOC)
- SkillIndex por dominio: BTreeMap<dominio, Vec<SkillRef>>
- Carregado via `read_vfs("/system/skills/index.md")` a quente
- Progressive disclosure: Hermes mostra so skills relevantes ao contexto

---

## Integracoes

### Com HermesAgent
- `Crew` delegation: Hermes recebe `CREW_STARTED`, consulta Cortex, delega tasks
- `Workflow`: `run_workflow()` substitui `tick()` quando workflow ativo
- `PriorityQueue`: filtra `TASK_ASSIGNED` por prioridade

### Com CortexAgent
- `IntentCache`: se hash(input) no cache, retorna intent sem LLM
- `OutputCache`: coresponde com `Cortex::confidence()` para decidir se re-executa
- `SelfCritique`: Cortex recebe output da skill, analisa, retorna confidence score

### Com Kernel (VFS/EventBus)
- `JobPreconditions`: `read_vfs("/system/jobs/<name>.md")` carrega contexto
- `SkillIndex`: `read_vfs("/system/skills/index.md")` carrega catalogo
- Hot-load: qualquer agente pode publicar `SKILL_INDEX_RELOAD` no EventBus

---

## Resumo

| Item | LOC | Arquivos |
|---|---|---|
| 72.1 Crew + FlowTrigger | 300 | agent-core/src/crew.rs, flow.rs, lib.rs |
| 72.2 TaskSchema + JobPreconditions | 200 | skill-registry/src/manifest.rs, job.rs |
| 72.3 IntentCache + OutputCache | 200 | hermes.rs, skill-registry/src/cache.rs |
| 72.4 WorkflowEngine + SelfCritique | 250 | hermes.rs, cortex.rs, skill-registry |
| 72.5 StateGraph Scheduler | 200 | agent-core/src/state_graph.rs |
| 72.6 SkillIndex + MCP Catalog | 150 | skill-registry/src/index.rs |
| **Total** | **~1.300** | **10+ arquivos** |

Dependencias: nenhuma (aditivo ao AgentRegistry/SkillRegistry/EventBus existentes).
