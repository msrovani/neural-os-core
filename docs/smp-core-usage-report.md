# Relatório de Uso de Cores — Neural OS Hermes K³CHJ

**Data:** 2026-08-20
**Referência:** ADR-0089 (Per-CPU Run-Queues), ADR-0057 (Compute Dispatch), ADR-0055 (FeatureGate)

## 1. Mapa Atual: Quem Roda Onde

### BSP (Core 0) — TUDO roda aqui

| Agent | Schedule | Tipo | Carga Estimada | Pico |
|-------|----------|------|----------------|------|
| HwBridgeAgent | Continuous | IRQ bridge (scancode) | ~0.1% | 5% (tecla) |
| InputAgent | Continuous | Keyboard PS/2 + USB xHCI | ~0.1% | 5% (tecla) |
| DisplayAgent | Continuous | Framebuffer compositor 60Hz | ~2% | 10% (resize) |
| NetAgent | Continuous | smoltcp poll + HTTP | ~0.5% | 30% (bulk) |
| CortexAgent | Continuous | LLM decode + MoE router | **~60%** | 95% (inference) |
| HermesAgent | Continuous | Intent routing + ReAct + WASM | ~10% | 40% (skill exec) |
| CronAgent | Continuous | Cron scheduler | ~0.1% | 0.5% |
| SecurityAgent | Continuous | 5 detectores + pipeline | ~0.2% | 1% |
| SafetyAgent | Continuous | 4 invariantes I1-I4 | ~0.1% | 0.5% |
| OptimizerAgent | Continuous | Self-optimization | ~0.1% | 0.5% |
| WifiAgent | Continuous | 802.11 scan + WPA2 | ~0.3% | 5% (scan) |
| SelfHealAgent | PollEvery(1000) | SelfHeal + PT pool check | ~0.01% | 0.1% |
| AutoLearnAgent | PollEvery(200) | Detecta necessidade → treina | ~0.05% | 2% (treino) |
| SleepCycleAgent | PollEvery(1000) | 5 fases REPLAY→DREAM | ~0.01% | 0.1% |

**BSP total:** ~73% (dominado por CortexAgent em inference)

### APs (Cores 1-3) — Só compute jobs

| Worker | Tipo | Carga Estimada |
|--------|------|----------------|
| parallel_matmul (AVX2/AVX512) | Matmul 64×64 | 0-90% (só quando enfileirado) |
| work-stealing (Chase-Lev) | Steal de matmul | ~5% (quando há trabalho) |

**APs total:** ~10% em média (90% do tempo em hlt/mwait C1)

## 2. Gargalos Identificados

### Gargalo 1: CortexAgent monopoliza BSP
O decode serial de LLM (BitNet ternário, 1.58-bit) roda em loop cooperativo no
BSP. Cada tick do CortexAgent executa `decode_step()` que gasta ~50-200µs por
token. Com 50+ agents no mesmo loop, o CortexAgent consome 60-95% do tempo do
BSP, starving agents de latência (Input, HwBridge, Display).

**Impacto:** Input latency sobe de ~1ms para ~10ms; display frame drop; network
timeout em bulk transfers.

### Gargalo 2: WASM sandbox compete com agents críticos
O wasmi runtime (fuel metering, capability gates) roda dentro de
`HermesAgent::tick()`. Skills WASM complexas (RustCoder, Generator) podem
gastar 10-50ms por tick, bloqueando o BSP inteiro.

### Gargalo 3: SGDB sync no mesmo core que inferência
O neural-sgdb (CRDT sync + MCP server) roda em `k_ai` que é pollado no BSP.
Vector search (HNSW) + index building competem com inference por cache L2/L3.

### Gargalo 4: APs subutilizados
Com 3 APs dormindo 90% do tempo, o sistema desperdiça 75% da capacidade
computacional. Matmul paralelo só é enfileirado quando CortexAgent decide
usar `cortex::compute::parallel_matmul()` — ~5% do tempo.

## 3. Distribuição Alvo

### Core 0 (BSP) — System Critical
**Papel:** System
**Agents:** HwBridge, Input, Display, Cron, Security, Safety, Optimizer
**Justificativa:** Latência-crítica (IRQ, framebuffer, mouse). Ring0 agents
NUNCA migram (affinity_ring=0). Budget watchdog ativo.

### Core 1 (AP1) — Compute
**Papel:** Compute
**Agents:** CortexAgent (LLM decode), parallel matmul workers
**Justificativa:** LLM decode é compute-heavy e tolera latência (~50ms/token).
APs workers executam matmul via work-stealing. CorePair: (Compute, Worker).

### Core 2 (AP2) — Orchestration + WASM
**Papel:** Worker
**Agents:** HermesAgent (orchestration), WASM sandbox (wasmi)
**Justificativa:** Orquestração não é latência-crítica (100ms tolerance).
WASM sandbox isolado em core dedicado previne starvation de system agents.

### Core 3 (AP3) — Network + Memory
**Papel:** Memory
**Agents:** NetAgent, WifiAgent, SGDB sync, SelfHeal
**Justificativa:** Network I/O é event-driven (pouco CPU, beaucoup blocking).
SGDB sync (HNSW) é memória-bound e não deve competir com inference no L2.

## 4. Papéis CorePair (ADR-0057)

```
CorePair 0: (BSP=System,   AP1=Compute)  — inference + matmul
CorePair 1: (AP2=Worker,   AP3=Memory)   — WASM + network + SGDB
```

| Core | Papel | Ring | Agents |
|------|-------|------|--------|
| 0 (BSP) | System | R0 | HwBridge, Input, Display, Cron, Security, Safety, Optimizer |
| 1 (AP1) | Compute | R1 | CortexAgent, matmul workers |
| 2 (AP2) | Worker | R2 | HermesAgent, WASM sandbox |
| 3 (AP3) | Memory | R2 | NetAgent, WifiAgent, SGDB, SelfHeal |

## 5. Impacto Estimado

| Métrica | Antes (BSP-only) | Depois (4-core) |
|---------|-------------------|------------------|
| BSP load | 73% | ~15% (system agents only) |
| LLM decode throughput | 1x | ~2.5x (dedicated core + AP workers) |
| Input latency | ~10ms | ~1ms (core dedicado) |
| WASM sandbox | Compete com system | Isolado em core próprio |
| SGDB search | Compete com inference | Core Memory dedicado |
| AP utilization | ~10% | ~60% (agents + matmul) |
