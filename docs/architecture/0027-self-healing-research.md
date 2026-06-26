# Self-Healing OS — Pesquisa Acadêmica + Adições Viáveis

## Fontes

### arXiv:2606.01416 — Self-Healing Agentic Orchestrators (Maio/2026)
**Autores:** Babu & Agrawal  
**Resultado:** 98.8% task success com recovery orquestrado vs 94.5% retry-only

Conceitos-chave:
- **Failure taxonomy** — mapear sinais observáveis → classes de falha (tool timeout, malformed args, stale context)
- **Budgeted recovery** — tentativas limitadas por budget, não infinitas
- **Verifier-guided healing** — verificador pós-recovery confirma se funcionou
- **Silent failure detection** — LLM verifica se output está correto mesmo sem erro explícito

### arXiv:2605.06737 — Self-Healing Framework for LLM Agents (Maio/2026)
**Autores:** Jeong & Shin  

Conceitos-chave:
- **Reliability assessment model** — métrica quantitativa de confiabilidade por agente
- **Corrective prompting** — em vez de reiniciar, re-prompt com contexto do erro
- **Multi-agent workflow recovery** — falha em um agente não derruba o workflow inteiro

### arXiv:2505.11743 — Cloud AI Self-Healing (Maio/2025)
**Autores:** Ji & Luo  

Conceitos-chave:
- **Multi-level architecture** — supervised (classificação) + unsupervised (anomalias)
- **LLM sobre logs** — processa logs em linguagem natural para detectar padrões
- **Previsão de falhas** — antes do erro ocorrer, baseado em tendências

---

## Lacunas no nosso sistema (identificadas pela pesquisa)

### 1. Exception handlers sem recuperação
Page Fault, Double Fault, GPF — todos fazem halt puro.

### 2. Executor não respawna tasks
`spawn()` existe mas `run()` é `-> !`. Tasks que panickam são perdidas.

### 3. SelfHeal só loga, não executa
`RestartDaemon` e `CreateSkill` viram `serial_println!` — ninguém executa.

### 4. Sem failure taxonomy
Não classificamos erros de forma estruturada. Tudo vira "Panic".

### 5. Sem corrective prompting
O LLM recebe o erro mas não é re-consultado com contexto + histórico.

### 6. Sem verificação pós-recovery
Não confirmamos se o recovery funcionou antes de continuar.

---

## Adições Viáveis (por ordem de impacto/esforço)

### Adição 1 🟢 — Failure Taxonomy (IMPACTO ALTO, ESFORÇO BAIXO)
Adicionar classificação estruturada de erros no SelfHeal:

```rust
pub enum FailureClass {
    MemoryFault,        // Page Fault, OOM
    ExecutionFault,     // GPF, Invalid Opcode
    ResourceFault,      // Skill not found, device timeout
    LogicFault,         // Assertion failed, state invalid
    ExternalFault,      // Network timeout, HW failure
}
```

**Baseado em:** arXiv:2605.06737 — failure taxonomy  
**Esforço:** ~30 LOC  
**Já temos:** `ErrorContext` struct, só falta o enum

### Adição 2 🟢 — Exception Handlers com SelfHeal (IMPACTO ALTO, ESFORÇO MÉDIO)
Modificar page_fault_handler e GPF handler para:
1. Coletar contexto (CR2, error code)
2. Publicar KERNEL_ERROR
3. Chamar SelfHeal::analyze()
4. Tentar recovery antes de halt

**Esforço:** ~80 LOC (modificar interrupts.rs)  
**Já temos:** SelfHeal, EventBus, panic_handler como template

### Adição 3 🟡 — Respawn de Tasks pelo Executor (IMPACTO ALTO, ESFORÇO ALTO)
Modificar executor para aceitar novas tasks via canal/EventBus:
- `TOPIC_DAEMON_RESPAWN` — publica com nome do daemon
- Executor escuta e recria a task com `AgentTask::new(daemon_fn())`
- SelfHeal usa `RestartDaemon` → publica `DAEMON_RESPAWN`
- Requer: mapear nome do daemon → função (closed set de 8 tasks)

**Esforço:** ~150 LOC  
**Pré-requisito:** Adição 2 (exception handlers)

### Adição 4 🟡 — Corrective Prompting (IMPACTO MÉDIO, ESFORÇO MÉDIO)
Quando um erro ocorre, re-consultar o LLM com:
```
"Erro X ocorreu no daemon Y. Contexto: {registers, file, line}.
 Histórico de falhas: {lessons}.
 Qual a melhor estratégia de recuperação?"
```

**Baseado em:** arXiv:2605.06737 — corrective prompting  
**Esforço:** ~60 LOC (modificar SelfHeal::analyze)  
**Já temos:** cortex_llm daemon + LLM_REQUEST/LLM_RESPONSE

### Adição 5 🟡 — Verifier Pós-Recovery (IMPACTO MÉDIO, ESFORÇO MÉDIO)
Após executar recovery, verificar se funcionou:
- Task reiniciada → check se completou 1 tick sem panic
- Skill criada → check se registrada no SKILL_REGISTRY
- Se falhar → record_failure() + tentar próxima estratégia

**Baseado em:** arXiv:2606.01416 — verifier-guided healing  
**Esforço:** ~80 LOC  
**Já temos:** lessons list no SelfHeal

### Adição 6 🔴 — Erros no EventLog (IMPACTO BAIXO, ESFORÇO BAIXO)
Adicionar `EventKind::KernelError` ao EventLog:
```rust
pub enum EventKind {
    UserInput, HermesResponse, SkillExecuted, SystemEvent,
    ContextCompacted, KernelError,  // NOVO
}
```
Publicar também no EventLog quando um KERNEL_ERROR ocorrer.

**Esforço:** ~10 LOC  
**Já temos:** EventLog, KERNEL_ERROR topic

---

## Mapa para Implementação (Priorizado)

```
Sprint 34: 🟢 Failure Taxonomy + 🟢 Exception Handlers 
           + 🔴 Erros no EventLog  (~120 LOC)

Sprint 35: 🟡 Respawn de Tasks + 🟡 Corrective Prompting  
           (~210 LOC)

Sprint 36: 🟡 Verifier Pós-Recovery + LLM training 
           (corrective + taxonomy data)  (~140 LOC)
```

## Resumo

| Adição | Base Acadêmica | Esforço | Já temos |
|---|---|---|---|
| Failure Taxonomy | arXiv:2605.06737 | ~30 LOC | ErrorContext |
| Exception Handlers | — | ~80 LOC | SelfHeal + EventBus |
| Respawn Tasks | — | ~150 LOC | AgentTask::new() |
| Corrective Prompting | arXiv:2605.06737 | ~60 LOC | cortex_llm + LLM_REQUEST |
| Verifier | arXiv:2606.01416 | ~80 LOC | lessons list |
| Erros no EventLog | — | ~10 LOC | EventLog |

**Total:** ~410 LOC para self-healing completo, dos quais ~110 LOC são extremamente simples e de alto impacto (failure taxonomy + exception handlers + EventLog).
