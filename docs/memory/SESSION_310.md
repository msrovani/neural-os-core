# SESSION_310 — Self-Heal AIOS: 4 Fases (Unificação → RESPAWN → LLM Loop → Security Detectors)

**Data:** 2026-09-07 | **Sprint:** v1.9.99-s308 TEST | **Status:** Phase 1-4 complete, `cargo check --release` 0 erros

---

## Premissa

O AIOS (neural-os-core) é um OS com inteligência própria. O self-healing deve ser um
**loop fechado**: detectar → classificar → IA diagnostica → recuperar → verificar.
Antes desta sessão, o pipeline era **open-loop** — detectava erros mas nunca fechava
o ciclo. O Falcon3 3B (CortexAgent) sentava ocioso enquanto o sistema logava erros
e fazia nada.

## Problema (antes)

| Gap | Detalhe |
|-----|---------|
| **3 instâncias SELF_HEAL isoladas** | `main.rs` (IrqSafeLock), `hermes/globals.rs` (TicketLock), `k_ai` (struct local). Boot e runtime não compartilham `lessons[]`. |
| **RecoveryAction não executa** | `RestartDaemon` publica `DAEMON_RESPAWN` no EventBus → ninguém consome. `CreateSkill` publica `SKILL_CREATE` → ninguém gera skill. |
| **BudgetedRecovery = sempre true** | `can_execute()` retorna `true` sem checar budget. Sem rate-limiting. |
| **`analyze()` incompleto** | `ExecutionFault` (GPF/DF) e `LogicFault` → `LogAndContinue`. A classe mais perigosa sem recuperação. |
| **SecurityAgent = stub inline** | Detectores hardcoded com contadores simples. Detectores reais em `k_ai::security_detectors` nunca instanciados. |
| **NET_EVENT = tópico morto** | SecurityAgent assina `NET_EVENT` mas **ninguém publica**. Zero dados de rede chegam aos detectores. |
| **SafetyInvariants I2/I3 = stubs** | I2 contava agentes hardcoded. I3 verificação delegada mas sem implementação. |

## Fase 1: Unificação — Single Source of Truth

### Mudança

Remover 2 statics duplicados e criar um canonical em `k_ai`:

| Antes | Depois |
|-------|--------|
| `main.rs` → `IrqSafeLock<SelfHeal>` (bin) | **Removido** |
| `hermes/globals.rs` → `TicketLock<SelfHeal>` | **Removido** |
| `k_ai` → struct local no agent | **`GLOBAL_SELF_HEAL: spin::Lazy<IrqSafeLock<SelfHeal>>`** |

### Arquivos

| Arquivo | Mudança |
|---------|---------|
| `k_ai/src/self_heal.rs` | Adicionado `GLOBAL_SELF_HEAL`, `register_respawn_bridge()`, `push_respawn()` |
| `k_ai/src/self_heal_agent.rs` | Reescrito: usa instância canônica |
| `hermes/src/agents.rs` | `BootSelfHealAgent` usa `k_ai::self_heal::GLOBAL_SELF_HEAL` |
| `hermes/src/globals.rs` | Removido `SELF_HEAL` static + import `SelfHeal` |
| `neural-kernel/src/main.rs` | Removido `SELF_HEAL` static, adicionado `push_respawn_bridge()` |
| `neural-kernel/src/boot_log_agent.rs` | Usa `k_ai::self_heal::GLOBAL_SELF_HEAL` |

### Habilita

- **Boot findings informam runtime**: scan de VID-gate do `BootSelfHealAgent` → `lessons[]` visível ao `SelfHealAgent` contínuo
- **Recovery executa**: `RestartDaemon` → `push_respawn()` → `RESPAWN_QUEUE` → scheduler cria nova instância
- **Fonte única**: checkpoint, histórico de falhas, pending fixes compartilhados

## Fase 2: RESPAWN Wiring

### Mudança

O `SelfHealAgent` publicava `DAEMON_RESPAWN` no EventBus (consumidor inexistente).
Agora usa function pointer bridge registrada no boot:

```
SelfHealAgent::tick()
  → RecoveryAction::RestartDaemon(name)
    → push_respawn(name)
      → RESPAWN_QUEUE.lock().push(name)
        → scheduler's respawn_fn() cria nova instância
```

### BudgetedRecovery enforcement

`can_execute()` agora **verifica** `budget > 0`:
- `consume()` decrementa budget em cada ação de recovery
- `maybe_reset()` reseta a cada 10000 ticks (~100s a 5ms/tick)
- Prevenção de restart loops infinitos

## Fase 3: Closed-Loop LLM — Falcon3 3B Diagnóstica Erros

### O que muda

O loop de self-healing agora é **fechado com IA**:

```
Error → Classify → HEALING_LLM_REQUEST → CortexAgent (Falcon3 3B)
→ HEALING_LLM_RESPONSE → SelfHealAgent parse JSON → Executa recovery
```

### Novos tópicos

| Tópico | Publisher | Consumer |
|--------|-----------|----------|
| `HEALING_LLM_REQUEST` | `SelfHealAgent` | `CortexAgent` |
| `HEALING_LLM_RESPONSE` | `CortexAgent` | `SelfHealAgent` |

### Prompt de healing

```text
You are the AIOS self-healing engine. Diagnose the error and recommend
a recovery action. Respond ONLY with JSON:
{"action":"<restart_daemon|checkpoint_restore|create_skill|log_continue>",
 "reason":"<brief explanation>","params":{}}
```

### Fallback

Quando LLM indisponível → resposta heurística `log_continue` mantém sistema rodando.

### `analyze()` estendido

| FailureClass | Antes | Depois |
|--------------|-------|--------|
| ExecutionFault (GPF/DF) | LogAndContinue | **CheckpointRestore** (se checkpoint válido) |
| LogicFault | LogAndContinue | **AwaitLLM** + alert |
| ExternalFault | LogAndContinue | **AwaitLLM** + alert |
| MemoryFault | RestartDaemon (1x) | RestartDaemon + LLM fallback |
| ResourceFault | CreateSkill + LLM_REQUEST | CreateSkill + **HEALING_LLM_REQUEST** (tópico dedicado) |

### SafetyInvariants I2

- **Antes**: contava agentes hardcoded
- **Depois**: `k_ai::agent_stats::current_agent_count()` — snapshot `AtomicUsize` atualizado pelo scheduler via `sched_metrics_hook`
- Warning se < 8 agentes, Violation se < 5

### Arquivos (Fase 3)

| Arquivo | Mudança |
|---------|---------|
| `k_ai/src/self_heal.rs` | `TOPIC_HEALING_LLM_REQUEST/RESPONSE`, `analyze()` estendido, healing prompt |
| `k_ai/src/self_heal_agent.rs` | Reescrito: assina `HEALING_LLM_RESPONSE`, parse JSON, budget enforcement |
| `k_ai/src/safety_invariants.rs` | I2: contagem real via `agent_stats` |
| `k_ai/src/agent_stats.rs` | **Novo**: `AtomicUsize` snapshot, `update_agent_count()`, `current_agent_count()` |
| `k_ai/src/trust.rs` | Adicionado `entry_count()` para I3 futuro |
| `k_ai/src/lib.rs` | `pub mod agent_stats;` |
| `hermes/src/agents.rs` | CortexAgent: assina `HEALING_LLM_REQUEST`, gera healing prompt, publica resposta |
| `neural-kernel/src/main.rs` | `sched_metrics_hook` → `agent_stats::update_agent_count()` |

## Fase 4: Security Detectors — Wired to Real Network Events

### O que muda

O SecurityAgent tinha detectores inline (contadores hardcoded) e assinava `NET_EVENT`,
mas **ninguém publicava** `NET_EVENT`. Os detectores reais em `k_ai::security_detectors`
nunca foram instanciados.

Agora:
1. SecurityAgent usa os 5 detectores reais de `k_ai`
2. Dados de rede chegam via `NET_EVENT` publicado por:
   - TCP connect → `netstack.rs`
   - ARP cache → `k_nano::net::mesh::peer_set_mac()`
3. Timer drift → check direto no tick do SecurityAgent

### Payload format (NET_EVENT)

```
CONNECT src_ip=X.X.X.X dst_port=N
ICMP src_ip=X.X.X.X
ARP src_ip=10.0.X.1 src_mac=XX:XX:XX:XX:XX:XX
DHCP_DISCOVER src_mac=XX:XX:XX:XX:XX:XX
```

### Detector coverage

| Detector | Source | Wired? |
|----------|--------|--------|
| PortScanDetector | TCP connect (smoltcp) | ✅ Real |
| ArpSpoofDetector | Mesh ARP cache updates | ✅ Real |
| PingFloodDetector | ICMP echo requests (raw Ethernet RX) | ✅ Real |
| DhcpStarvationDetector | DHCP lease events (SYSTEM_EVENT) + tx/rx ratio | ✅ Real |
| TimerAnomalyDetector | Self-contained tick drift | ✅ Real |

### Arquivos (Fase 4)

| Arquivo | Mudança |
|---------|---------|
| `hermes/src/security.rs` | Reescrito: usa reais `k_ai::security_detectors`, `publish_net_event()` helper, parse de payloads |
| `hermes/src/netstack.rs` | `tcp_session_connect()` chama `publish_net_event("CONNECT", ...)` |
| `k_nano/src/net/mesh.rs` | `peer_set_mac()` publica `NET_EVENT ARP` em updates |

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                        SELF-HEAL PIPELINE                          │
│                                                                     │
│  ┌──────────┐    ┌──────────────┐    ┌──────────────────────────┐  │
│  │  Error    │───>│ SelfHealAgent│───>│   RecoveryAction         │  │
│  │ (EventBus │    │ (k_ai R2)    │    │                          │  │
│  │  KERNEL_  │    │              │    │  RestartDaemon → RESPAWN │  │
│  │  ERROR)   │    │ analyze()    │    │  CreateSkill → self_evolve│  │
│  └──────────┘    │              │    │  CheckpointRestore → chk  │  │
│                  │  Budget: 10  │    │  AwaitLLM → push lessons  │  │
│                  │  / window    │    │  LogAndContinue → log     │  │
│                  └──────┬───────┘    └──────────────────────────┘  │
│                         │                                          │
│                  HEALING_LLM_REQUEST                               │
│                         │                                          │
│                  ┌──────▼───────┐                                  │
│                  │  CortexAgent │                                  │
│                  │  (Falcon3 3B)│                                  │
│                  │  diagnosis   │                                  │
│                  └──────┬───────┘                                  │
│                         │                                          │
│                  HEALING_LLM_RESPONSE                              │
│                         │                                          │
│                  ┌──────▼───────┐                                  │
│                  │  SelfHealAgent│                                  │
│                  │  apply_ai_   │                                  │
│                  │  diagnosis()  │                                  │
│                  └──────────────┘                                  │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                     SECURITY PIPELINE                               │
│                                                                     │
│  ┌──────────┐    ┌──────────────┐    ┌──────────────────────────┐  │
│  │ Network  │───>│ NET_EVENT    │───>│ SecurityAgent            │  │
│  │ Code     │    │ (EventBus)   │    │ (hermes R3)              │  │
│  │          │    │              │    │                          │  │
│  │ TCP:     │    │ CONNECT ...  │───>│ PortScanDetector (k_ai)  │  │
│  │  netstack│    │ ARP ...      │───>│ ArpSpoofDetector (k_ai)  │  │
│  │ Mesh:    │    │ ICMP ...     │───>│ PingFloodDetector (k_ai) │  │
│  │  peer_mac│    │ DHCP ...     │───>│ DhcpStarvationDetector   │  │
│  └──────────┘    └──────────────┘    │ TimerAnomalyDetector     │  │
│                                      │                          │  │
│                                      │ → SECURITY_ALERT         │  │
│                                      │ → Hermes (response)      │  │
│                                      └──────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                     INSTANCE UNIFICATION                            │
│                                                                     │
│  Before:                    After:                                  │
│  ┌──────────┐              ┌──────────────────────────┐            │
│  │ main.rs  │──┐           │ k_ai::self_heal.rs       │            │
│  │ IrqSafe  │  │ isolated  │ GLOBAL_SELF_HEAL          │            │
│  └──────────┘  │           │ spin::Lazy<IrqSafeLock>  │            │
│  ┌──────────┐  │           │                          │            │
│  │ hermes/  │──┤           │ boot_log_agent ─────────>│            │
│  │ TicketLock│  │           │ BootSelfHealAgent ──────>│ (shared)   │
│  └──────────┘  │           │ SelfHealAgent ──────────>│            │
│  ┌──────────┐  │           │                          │            │
│  │ k_ai/    │──┘           │ push_respawn() ─────────>│ RESPAWN_Q  │
│  │ local    │              └──────────────────────────┘            │
│  └──────────┘                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Aceite

- [x] `cargo clean -p neural-kernel && cargo check --release` — **0 erros**
- [x] Phase 1: 1 instância canônica, boot+runtime compartilham state
- [x] Phase 2: `RestartDaemon` → `RESPAWN_QUEUE` → scheduler cria agent
- [x] Phase 3: `HEALING_LLM_REQUEST/RESPONSE` loop com CortexAgent (Falcon3 3B)
- [x] Phase 3: `BudgetedRecovery` enforcement (10 ações/janela)
- [x] Phase 3: `SafetyInvariants I2` — contagem real via `agent_stats`
- [x] Phase 4: `NET_EVENT` publicado por TCP connect e mesh ARP
- [x] Phase 4: SecurityAgent usa detectores reais de `k_ai::security_detectors`
- [x] ICMP raw path: `detect_and_publish_icmp()` in `nic_recv()` intercepts Echo Request (type 8)
- [x] DHCP event: `publish_system_event("DHCP_LEASE")` in `dhcp_poll()` on Configured + `feed_lease()` detector
- [ ] SafetyInvariants I3 — trust cache real check (delegado a SecurityAgent)
- [ ] Security detectors em QEMU testável (TCP connect é o mais provável de disparar)

## Arquivos modificados

| Arquivo | Fase | Mudança |
|---------|------|---------|
| `crates/k_ai/src/self_heal.rs` | 1+2+3 | `GLOBAL_SELF_HEAL`, RESPAWN bridge, healing topics, `analyze()` estendido |
| `crates/k_ai/src/self_heal_agent.rs` | 1+3+5 | Canonical instance, LLM diagnosis, budget, NSGDB ingest/query, user notification |
| `crates/k_ai/src/agent_stats.rs` | 3 | **Novo**: agent count snapshot |
| `crates/k_ai/src/safety_invariants.rs` | 3 | I2 real check via `agent_stats` |
| `crates/k_ai/src/trust.rs` | 3 | `entry_count()` para I3 futuro |
| `crates/k_ai/src/lib.rs` | 3 | `pub mod agent_stats` |
| `crates/hermes/src/agents.rs` | 1+3 | BootSelfHealAgent canonical, CortexAgent healing handler |
| `crates/hermes/src/globals.rs` | 1 | Removido `SELF_HEAL` static |
| `crates/k_ai/src/security_detectors.rs` | 4 | Added `feed_lease()` to DhcpStarvationDetector |
| `crates/hermes/src/security.rs` | 4 | Reescrito: detectores reais, `publish_net_event()` helper |
| `crates/hermes/src/netstack.rs` | 4 | TCP connect → `NET_EVENT CONNECT`; ICMP Echo Request → `NET_EVENT ICMP` |
| `crates/k_nano/src/net/mesh.rs` | 4 | `peer_set_mac()` → `NET_EVENT ARP` |
| `crates/neural-kernel/src/main.rs` | 1+3 | Removido `SELF_HEAL`, bridge registration, `sched_metrics_hook` |
| `crates/neural-kernel/src/boot_log_agent.rs` | 1 | Usa `GLOBAL_SELF_HEAL` |
| `crates/agent-core/src/lib.rs` | 3 | `set_sched_metrics_hook` registration |
