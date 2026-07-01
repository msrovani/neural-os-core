# Sprint 67 — Auto-Skills + Agency Expansion

**v0.67.0** — O Hermes que se auto-melhora. Skills que criam skills. Agentes que importam agentes.

**Foco:** Três Oportunidades Reais identificadas na análise de links externos — nenhuma depende de LAN.

**Dependências externas:** Nenhuma. Tudo é glue code Rust + integração de formatos.

---

## Sub-Sprint 67.0 — Meta-Skill: Portar "One Skill to Rule Them All"

**Target:** v0.67.0  
**LOC:** ~400  
**Fonte:** [github.com/rebelytics/one-skill-to-rule-them-all](https://github.com/rebelytics/one-skill-to-rule-them-all) (829★, CC BY 4.0)  
**Status atual:** `skill_gen.rs` tem TaskPattern registry + `maybe_auto_skill()` mas nunca é chamado no ciclo principal.

### O que tem que acontecer

O repo `one-skill-to-rule-them-all` contém uma **meta-skill** chamada `task-observer` que:
1. Observa sessões de trabalho
2. Detecta padrões repetitivos → sugere novas skills
3. Detecta correções do usuário → sugere melhorias em skills existentes
4. Propaga princípios cross-cutting entre todas as skills
5. Mantém um log de observações persistente entre sessões

A SKILL.md deles (~22KB) tem metodologia completa, protocolo de observação, taxonomia (open-source vs internal), e cadência de revisão semanal.

### Sub-itens

#### 67.0.1 — Observation Protocol (~150 LOC)

- [ ] Criar `skill_observer.rs` — módulo separado, não acoplado ao skill_gen.rs atual
- [ ] Implementar `ObservationLog` persistente (formato markdown, como o original)
- [ ] `Observer::watch_task(name, steps, corrections)` → loga observação
- [ ] `Observer::watch_correction(skill, issue, suggestion, principle)` → loga melhoria
- [ ] `Observer::flush()` → salva log no VFS `/system/observations/log.md`
- [ ] Arquivo: `crates/neural-kernel/src/skill_observer.rs` (novo)

Formato de cada observação (adaptado do original):
```rust
pub struct Observation {
    pub number: u32,
    pub date: u64,              // tick
    pub session_context: String,
    pub skill: String,           // nome da skill ou "New skill candidate"
    pub classification: ObsClass, // OpenSource | Internal
    pub phase: String,
    pub issue: String,
    pub suggestion: String,
    pub principle: String,
}
```

#### 67.0.2 — Trigger Integration (~80 LOC)

- [ ] Chamar `Observer::watch_task()` no `OptimizerAgent::tick()` quando detecta repetição
- [ ] Chamar `Observer::watch_correction()` quando `SafetyAgent` ou `SecurityAgent` bloqueiam comando
- [ ] Chamar `Observer::flush()` no `CronAgent` a cada 100 ticks
- [ ] Comando `/observations` — lista observações abertas
- [ ] Comando `/observe <name>` — loga observação manual

#### 67.0.3 — Pre-Flight Principle (~80 LOC)

- [ ] Adaptar "Pre-Flight Principle" do original: toda skill deve verificar sua própria saída
- [ ] Implementar `Skill::verify(&self, input, output) -> bool`
- [ ] Adicionar verificação no `SkillRegistry::execute_skill()` pós-execução
- [ ] Se verificação falha: loga observação automaticamente

#### 67.0.4 — Comprehensive Review (~90 LOC)

- [ ] Criar `ReviewSession` no `CronAgent` (agendado: seg/qua/sex)
- [ ] `ReviewSession::run()`: lê log de observações, agrupa por skill
- [ ] Para cada observação: `maybe_auto_skill()` (já existe!) ou `suggest_improvement()`
- [ ] Marcar observações como ACTIONED/OPEN/DECLINED
- [ ] Report: "N observações processadas, M skills melhoradas, K novas"

### Arquivos modificados

| Arquivo | Ação |
|---|---|
| `crates/neural-kernel/src/skill_observer.rs` | NOVO |
| `crates/neural-kernel/src/skill_gen.rs` | Refatorar (separar observation de generation) |
| `crates/neural-kernel/src/cron.rs` | Adicionar review schedule |
| `crates/neural-kernel/src/optimizer.rs` | Chamar observer |
| `crates/neural-kernel/src/main.rs` | `mod skill_observer;` |
| `crates/neural-kernel/src/shell.rs` | Comandos `/observations`, `/observe` |

---

## Sub-Sprint 67.1 — Portar agency-agents para The Agency

**Target:** v0.67.1  
**LOC:** ~600 (dados) + ~200 (glue)  
**Fonte:** [github.com/msitarzewski/agency-agents](https://github.com/msitarzewski/agency-agents) (123k★, 20.1k forks, MIT)  
**Status atual:** `agency.rs` tem 147 agentes especialistas em 12 divisões. O repo agency-agents tem 16 divisões com ~100 agentes em formato SKILL.md.

### O que tem que acontecer

O repo `msitarzewski/agency-agents` contém **16 divisões** de agentes AI especializados, cada um como um arquivo `.md` com:
- Identidade & personalidade
- Missão & workflows
- Entregáveis técnicos com exemplos de código
- Métricas de sucesso & estilo de comunicação

**Já suporta Hermes** no install script (`./scripts/install.sh --tool hermes`). O formato dos agentes é markdown estruturado, compatível com nosso `AgentManifest`.

### Sub-itens

#### 67.1.1 — Parse do formato agency-agents (~100 LOC)

- [ ] Analisar estrutura dos `.md` do agency-agents (ex: `engineering/engineering-frontend-developer.md`)
- [ ] Extrair: `name`, `specialty`, `when_to_use`, `personality`, `workflows`, `deliverables`
- [ ] Converter para `AgentManifest`:
```rust
AgentManifest {
    name: "frontend-developer",
    kind: AgentKind::Specialist,
    schedule: ScheduleKind::EventDriven,
    auto_start: false,
    persist: false,
    division: "engineering",
    specialty: "React/Vue/Angular, UI implementation",
}
```
- [ ] Arquivo: `crates/neural-kernel/src/agency_importer.rs` (novo)

#### 67.1.2 — Importar divisões (~500 LOC de dados)

- [ ] Portar **Engineering Division** (~25 agentes): Frontend, Backend, Mobile, AI, DevOps, Network, Embedded, Security, etc.
- [ ] Portar **Design Division** (~8 agentes): UI, UX, Brand, Visual Storyteller, Whimsy Injector
- [ ] Portar **Marketing Division** (~25 agentes): Growth, Content, Social, SEO, Email, ASO
- [ ] Portar **Sales Division** (~8 agentes): Outbound, Discovery, Deal Strategy, Sales Engineering
- [ ] Portar **Security Division** (~8 agentes): AppSec, Pentest, Cloud Security, Incident Response
- [ ] Portar **Product + PM + Testing + Support** (~15 agentes)
- [ ] Ignorar divisões China-specific (WeChat, Douyin, Xiaohongshu, etc.) — ou portar como stub

Cada agente vira uma entrada em `agency.rs`:

```rust
agent("frontend-developer", "engineering", "React/Vue/Angular, UI implementation, performance",
    &["react", "vue", "angular", "css", "typescript", "ui", "component", "design-system"]),
```

#### 67.1.3 — Integrar com install script deles (~100 LOC)

- [ ] O script `install.sh` deles suporta `--tool hermes`
- [ ] Analisar o script para entender o formato de saída esperado
- [ ] Adaptar nosso `agency.rs` para aceitar o formato de entrada
- [ ] Opcional: criar `tools/import_agency_agents.py` que baixa e converte automaticamente

### Arquivos modificados

| Arquivo | Ação |
|---|---|
| `crates/neural-kernel/src/agency_importer.rs` | NOVO |
| `crates/neural-kernel/src/agency.rs` | Adicionar ~100 novos agentes |
| `tools/import_agency_agents.py` | NOVO (opcional) |

---

## Sub-Sprint 67.2 — Inspiração: Hermes Agent v0.18

**Target:** v0.67.2  
**LOC:** ~300  
**Fonte:** [Hermes Agent v0.18 "The Judgment Release"](https://x.com/IBuzovskyi/status/2072423320784363576) (207k★)  
**Status atual:** Nada implementado ainda. Referência externa.

### O que NÃO vamos fazer

Não vamos portar o Hermes Agent deles (é um projeto TypeScript/Python completamente diferente do nosso Neural OS Hermes). Vamos extrair **conceitos** e implementar no nosso contexto Rust bare-metal.

### Sub-itens

#### 67.2.1 — `/learn` command (SKILL.md generator) (~120 LOC)

**Inspiração:** Hermes Agent v0.18 tem `/learn <directory>` que distilla uma SKILL.md automaticamente.

**Implementação:**
- [ ] Comando `/learn <pattern-name>` no shell
- [ ] Examina `TASK_PATTERNS` para o padrão com mais usos recentes
- [ ] Gera `SKILL.md` no formato one-skill-to-rule-them-all:
```markdown
---
name: <pattern-name>
description: <auto-generated from steps>
---

# <Pattern-Name> Skill

## Workflow
1. <step 1>
2. <step 2>
...

## Verification
- <auto-generated checklist>
```
- [ ] Salva em `/system/skills/<name>/SKILL.md` no VFS
- [ ] Registra no `SkillRegistry`

#### 67.2.2 — Completion Contracts (~100 LOC)

**Inspiração:** "done means evidence, not a claim" — skills definem "o que é done" e o sistema verifica.

**Implementação:**
- [ ] Adicionar `completion_check: fn(output) -> bool` ao `SkillManifest`
- [ ] `SkillRegistry::execute_skill()` chama completion_check pós-execução
- [ ] Se falha: evento `SKILL_INCOMPLETE` no EventBus
- [ ] Re-aproveitar `verify.rs` (já existe eBPF-style verifier!)

#### 67.2.3 — Background Fan-out (~80 LOC)

**Inspiração:** `delegate_task` executa múltiplos subagentes em paralelo sem bloquear o chat.

**Implementação:**
- [ ] `Agency::delegate(pattern_name, args)` — spawna N subagentes
- [ ] Subagentes são `Agent` normais com `ScheduleKind::Oneshot`
- [ ] Quando todos completam: evento `DELEGATION_COMPLETE` com resultados consolidados
- [ ] Usar `AgentRegistry::spawn_temp()` (precisa existir ou criar)

### Arquivos modificados

| Arquivo | Ação |
|---|---|
| `crates/neural-kernel/src/shell.rs` | Comando `/learn` |
| `crates/neural-kernel/src/skill_gen.rs` | Gerador de SKILL.md |
| `crates/neural-kernel/src/verify.rs` | Completion contracts |
| `crates/neural-kernel/src/agency.rs` | `delegate()` |
| `crates/neural-kernel/src/agents.rs` | `spawn_temp()` |
| `crates/skill-registry/src/lib.rs` | `completion_check` no manifest |

---

## Dependências

```
67.0 (Meta-Skill) ──→ 67.2.1 (/learn depende do observer)
                   └─→ 67.2.2 (completion contracts independente)

67.1 (Agency Import) ──→ independente (só adiciona agentes)

67.2.3 (Fan-out) ──→ independente (só adiciona funcionalidade)
```

---

## Summary

| Sub-Sprint | Feature | LOC | Fonte | Prioridade |
|---|---|---|---|---|
| 67.0.1 | Observation Protocol | ~150 | one-skill-to-rule-them-all | 🔴 Alta |
| 67.0.2 | Trigger Integration | ~80 | one-skill-to-rule-them-all | 🔴 Alta |
| 67.0.3 | Pre-Flight Principle | ~80 | one-skill-to-rule-them-all | 🟡 Média |
| 67.0.4 | Comprehensive Review | ~90 | one-skill-to-rule-them-all | 🟡 Média |
| 67.1.1 | Parse agency-agents | ~100 | agency-agents | 🔴 Alta |
| 67.1.2 | Import divisões | ~500 | agency-agents | 🔴 Alta |
| 67.1.3 | Integração com install.sh | ~100 | agency-agents | 🟢 Leve |
| 67.2.1 | `/learn` command | ~120 | Hermes Agent v0.18 | 🟡 Média |
| 67.2.2 | Completion Contracts | ~100 | Hermes Agent v0.18 | 🟢 Leve |
| 67.2.3 | Background Fan-out | ~80 | Hermes Agent v0.18 | 🟢 Leve |
| **Total** | **10 features** | **~1400 LOC** | 3 fontes externas | |

## Ordem de Implementação

```
Fase 1 (67.0): Observation Protocol → triggers → pre-flight → review
Fase 2 (67.1): Parser → import → integrar
Fase 3 (67.2): /learn → completion contracts → fan-out
```

Fase 1 e Fase 2 são independentes — podem ser feitas em paralelo. Fase 3 depende de Fase 1 (/learn precisa do observer).

---

## Como Cada Fonte se Conecta

```
one-skill-to-rule-them-all
├── Metodologia de meta-observação → skill_observer.rs
├── Protocolo de logging → ObservationLog
├── Pre-Flight Principle → Skill::verify()
├── Comprehensive Review → CronAgent semanal
└── SKILL.md → nosso format

agency-agents (msitarzewski)
├── 16 divisões → nossas divisões em agency.rs
├── Formato dos agentes .md → AgentManifest
├── Specialty triggers → agrupamento por especialidade
└── install.sh --tool hermes → integração bidirecional

Hermes Agent v0.18
├── /learn → nosso skill_gen::generate_skill()
├── Completion contracts → verify::execute_verified()
├── Background fan-out → Agency::delegate()
└── MoA → Council skill (já existe!)
```

---

## Licenças e Atribuições

| Fonte | Licença | Obrigação |
|---|---|---|
| one-skill-to-rule-them-all | CC BY 4.0 | Atribuição + link para repo |
| agency-agents | MIT | Aviso de copyright |
| Hermes Agent v0.18 | Inspiração (código próprio) | Nenhuma |

Todas as atribuições serão incluídas em `docs/ATTRIBUTIONS.md` e nos cabeçalhos dos arquivos portados.
