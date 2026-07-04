# Sprint 77 — Foundation Quick Wins

**Data:** 2026-07-04
**v0.77.0**
**Cargo check:** 0 errors
**Validação:** QEMU (WHPX) + VirtualBox (2 vCPUs) — boot limpo em ambos

## Itens Implementados

### 1. 60.1b — Prompt `>` interativo (1 LOC)
- `display/console.rs`: `show_prompt` default alterado de `false` para `true`
- NeuralConsole sempre mostra `> ` quando ocioso

### 2. 67.0.3 — Pre-Flight Principle (~36 LOC)
- `skill-registry/skill.rs`: `verify()` adicionado ao trait `Skill`
- `skill-registry/registry.rs`: `verify()` chamado em `execute_skill()` + `execute_skill_unchecked()`
- 5 skills implementaram `verify()` — EchoSkill (trivial), SystemStatusSkill (checa MHI), HardwareInfoSkill (checa ARCH), HwIdentifySkill (trivial), NetDiagnosticSkill (trivial)
- DiagnosticSkill em agents.rs também implementou

### 3. 67.2.2 — Completion Contracts (~68 LOC)
- `skill-registry/contract.rs`: `CompletionContract` struct com `verify()` + `CONTRACT_NONEMPTY` + `CONTRACT_UTF8`
- Integration em `execute_skill()` e `execute_skill_unchecked()` — WarnOnly/RejectOutput/RetrySkill
- `contracts: Vec::new()` adicionado a todos os 7 McpManifests existentes

### 4. 72.2 — TaskSchema + JobPreconditions (~47 LOC)
- `skill-registry/task.rs`: `TaskSchema`, `JobPreconditions`, `TaskStatus`
- Tipos para schema estruturado de tarefas com precondições, timeout, retries

### 5. 72.6 — SkillIndex + MCP Catalog (~55 LOC)
- `skill-registry/index.rs`: `SkillIndex::find(query)` — busca textual por nome/desc/capabilities
- `McpCatalog` struct com `search()` + `register()` + `all()`
- `CatalogEntry` com metadados completos

### 6. 67.2.1 — `/learn` command + DynamicSkill (~75 LOC)
- `skill-registry/dynskill.rs`: `DynamicSkill` struct que implementa `Skill`
- `hermes.rs`: Novo `Command::Learn(name, desc)` separado de `AddSkill`
- `agents.rs`: Handler registra `DynamicSkill` diretamente no `SkillRegistry` + `SkillLoader`
- `/learn <name> <desc>` cria skill sem LLM dependency

### 7. 67.2.3 — Background Fan-out (~94 LOC)
- `skill-registry/fanout.rs`: `FanOutPool` com `spawn()`/`poll_all()`/`take_result()`
- Sub-tasks armazenadas como `Box<dyn FnOnce + Send>` em BTreeMap
- `FANOUT_POOL` global registrado em main.rs

## Bugfix Adicional

### VirtualBox SMP hang (Sprint 77b)
- `smp/mod.rs`: Adicionado `AP_COUNT` static, setado pela `PlatformAgent` baseado no MADT `lapic_count`
- Se AP_COUNT == 0, `init_smp()` retorna sem INIT-SIPI-SIPI
- VirtualBox com 2 vCPUs agora boota confiavelmente (1 AP bem-sucedido)

## Total: ~376 LOC, 0 erros

| Classe | LOC |
|---|---|
| skill-registry (novos módulos) | ~260 |
| hermes/agents (modificações) | ~75 |
| display/console | ~1 |
| smp (bugfix) | ~15 |
| main.rs (globals + skills) | ~15 |
| net.rs (verify) | ~3 |
| agents.rs (verify + learn) | ~7 |
