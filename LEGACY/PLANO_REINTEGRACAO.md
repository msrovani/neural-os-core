# Plano de Reintegração LEGACY → Ativo

**Base:** ADR-0075 (Emagrecer) — bin é glue/role_diff; lógica nas crates.
**Premissa:** Módulos legados por falta de uso/chamadas — devem ser INTEGRADOS ao ecossistema, não apenas copiados.

## Organização por Custo/LOC (mais simples → mais complexo)

| # | Módulo | LOC | Custo | Prioridade | Destino | Dependências |
|---|--------|-----|-------|-----------|---------|-------------|
| 1 | CFS Scheduler | ~19 | 🔵 Trivial | Alta | `k_nano/src/scheduler/cfs.rs` | Nenhuma |
| 2 | IpwMonitor (tokens/watt via MSR) | ~40 | 🔵 Trivial | Média | `jarbas/src/jarvis.rs` | MSR 0x610 |
| 3 | Merkle Audit Trail | ~80 | 🟢 Fácil | Alta | `k_ai/src/audit.rs` | Ed25519, SHA-256 |
| 4 | HookRegistry | ~59 | 🟢 Fácil | Alta | `agent-core/src/hooks.rs` | Nenhuma |
| 5 | BootLogAgent | ~186 | 🟢 Fácil | Média | `hermes/src/agents.rs` ou `k_ai/` | FAT32, logs |
| 6 | Crew/StateGraph | ~213 | 🟡 Médio | Média | `agent-core/src/crew.rs` | AgentRegistry |
| 7 | GGUF Streaming Loader | ~595 | 🟡 Médio | Alta | `cortex/src/gguf.rs` | ModelHub |
| 8 | LogicalClock + VectorClock | ~284 | 🟡 Médio | Média | `k_nano/src/sync/clock.rs` | Atomics |
| 9 | NoProto Zero-Copy Parser | ~448 | 🔴 Complexo | Baixa | `k_nano/src/net/noproto.rs` | DMA buffers |
| 10 | Brain Mesh Engine | ~400 | 🔴 Complexo | Baixa | `k_nano/src/net/mesh.rs` | P2P infra |
| 11 | CorePair + BipoleMode | ~581 | 🔴 Complexo | Baixa | `k_nano/src/scheduler/` | APIC, SMP |

**Total:** ~2.900 LOC para reintegração completa

## Plano de Integração ao Ecossistema

### 1. CFS Scheduler (~19 LOC) — Trivial
- **Problema:** Existe em LEGACY mas NUNCA foi chamado — agendamento é round-robin
- **Integração:** Adicionar `CfsScheduler` como política alternativa no `AgentRegistry::run()`. Se `feature = "cfs"`, usa vruntime; senão, round-robin.
- **Quem chama:** `AgentRegistry::run()` scheduler loop
- **Valor:** Agendamento justo — agentes com mais tick consomem menos prioridade

### 2. IpwMonitor (~40 LOC) — Trivial
- **Problema:** Monitora RAPL MSR mas resultado NUNCA é consumido
- **Integração:** JarbasEngine chama `ipw_monitor.sample()` no tick. Resultado aparece no SysInfo card como "IPW: X.Y tok/W"
- **Quem chama:** `JarbasEngine::tick()`
- **Valor:** Métrica de eficiência energética da IA

### 3. Merkle Audit Trail (~80 LOC) — Fácil
- **Problema:** Audit entries são criados mas NUNCA verificados
- **Integração:** SecurityAgent verifica integridade da chain a cada N ticks. Se quebrada, dispara HEALTH_ISSUE
- **Quem chama:** `SecurityAgent::tick()`
- **Valor:** Imutabilidade das ações do sistema

### 4. HookRegistry (~59 LOC) — Fácil
- **Problema:** Hooks registrados mas NINGUÉM os invoca
- **Integração:** Scheduler chama `PreTick` hook antes de cada `agent.tick()`. SecurityAgent registra hook para negar ticks de agentes comprometidos
- **Quem chama:** `AgentRegistry::run()` antes de cada tick
- **Valor:** Segurança — bloquear agente antes de executar

### 5. BootLogAgent (~186 LOC) — Fácil
- **Problema:** Analisa BOOT.LOG mas NUNCA é registrado como agente ativo
- **Integração:** Registrar no boot loop da AgentFleet. SysInfoAgent exibe últimas entradas do boot log
- **Quem chama:** Boot sequence → PollEvery(500) → SysInfo card
- **Valor:** Diagnóstico de boot pós-crash

### 6-11: Módulos Complexos (adiar para pós-v2.0)
- Requerem infraestrutura P2P, APIC SMP dedicado, ou rede distribuída
- Devem ser reavaliados após Ring3 + SIMD + WiFi estarem estáveis

## Implementação: Fase 1 (Triviais + Fáceis, ~384 LOC)

| Passo | Módulo | LOC | Ação |
|-------|--------|-----|------|
| 1 | CFS Scheduler | ~19 | Criar `k_nano/src/scheduler/cfs.rs` com `CfsScheduler` + vruntime |
| 2 | IpwMonitor | ~40 | Extrair de `LEGACY/jarvis/src/jarvis.rs` para `jarbas/src/jarvis.rs` |
| 3 | Merkle Audit | ~80 | Portar `LEGACY/k_ia/src/audit.rs` para `k_ai/src/audit.rs` |
| 4 | HookRegistry | ~59 | Portar `LEGACY/v1.9.9-test/agent-core/hooks.rs` para `agent-core/src/` |
| 5 | BootLogAgent | ~186 | Portar `LEGACY/k_ia/src/boot_log_agent.rs` para `k_ai/` ou `hermes/` |

## Agente Revisor

Criar `tools/legacy_integrity_lint.ps1` que verifica:
1. O módulo foi copiado para o destino correto (não ficou no LEGACY sem integração)
2. O `lib.rs` ou `main.rs` tem o `pub mod` correspondente
3. O módulo é REALMENTE chamado por alguém (grep pelo nome do struct/função pública)
4. O build passa (cargo check --release)
5. Nenhum símbolo do LEGACY ficou sem referência no ativo
