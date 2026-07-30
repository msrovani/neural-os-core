# ADR-0081: Malha Cognitiva Distribuída — P2P, Brain Mesh e UDP Broadcast

**Status:** Proposed  
**Lifecycle:** planejamento  
**Criação:** 2026-07-30  
**Tags:** p2p, mesh, distributed, datacenter, brain-mesh, broadcast  
**Fontes:** `LEGACY/v1.9.9-test/k_nano/p2p/`, `LEGACY/v1.9.9-test/k_nano/net/brain_mesh.rs`, `ADR-0042 §N4`

---

## Contexto

O projeto neural-os-core precisa de um ecossistema de computação distribuída para atingir sua visão de **datacenter cognitivo global**: múltiplos nós AIOS se auto-descobrindo, elegendo liderança, e distribuindo inferência de ML sem servidor central.

Três tecnologias foram identificadas no LEGACY que formam a base deste ecossistema:

1. **P2P Networking** (`p2p/`: NoProto, LogicalClock, MPMC) — camada de transporte e coordenação
2. **Brain Mesh Engine** (`net/brain_mesh.rs`) — cluster autônomo com eleição e papéis
3. **UDP Broadcast** — descoberta de nós sem configuração

Este ADR separa estas tecnologias do esforço geral de reintegração LEGACY (ADR-0080) e as trata como um domínio arquitetural próprio.

---

## Arquitetura Proposta

### Stack de 3 Camadas

```
┌─────────────────────────────────────────────────────┐
│              Application Layer (Hermes)               │
│  Compute Dispatch  │  MoE Routing  │  Skill Sync     │
├─────────────────────────────────────────────────────┤
│           Brain Mesh Engine (k_nano::net::mesh)       │
│  Descoberta  │  Eleição  │  Heartbeat  │  Papéis     │
├─────────────────────────────────────────────────────┤
│          P2P Transport Layer (k_nano::net::p2p)       │
│  NoProto  │  LogicalClock  │  MPMC  │  UDP Broadcast │
└─────────────────────────────────────────────────────┘
```

### Camada 1: P2P Transport

Responsável pela comunicação confiável entre nós via UDP.

| Componente | Arquivo | Status | Descrição |
|-----------|---------|--------|-----------|
| **NoProto Parser** | `k_nano/src/net/noproto.rs` | ✅ Restaurado | Zero-copy packet format (`AiosTaskPacket` with magic/clock/task_type) |
| **LogicalClock** | `k_nano/src/sync/clock.rs` | ✅ Restaurado | Lamport clock para ordenação causal |
| **VectorClock** | `k_nano/src/sync/clock.rs` | ✅ Restaurado | 16-node causality tracking |
| **MPMC Queue** | `k_nano/src/mpmc.rs` | ✅ Restaurado | Lock-free inter-node message buffer |
| **UDP Broadcast** | `k_nano/src/net/udp_broadcast.rs` | ❌ Pendente | Envio/recepção de datagramas broadcast via e1000 + smoltcp |

### Camada 2: Brain Mesh Engine

Cluster autônomo sem configuração.

| Componente | Arquivo | Status | Descrição |
|-----------|---------|--------|-----------|
| **NodeDiscovery** | `k_nano/src/net/mesh.rs` | ✅ Restaurado | Broadcast para descobrir nós vivos |
| **CapacityScore** | `k_nano/src/net/mesh.rs` | ✅ Restaurado | Pontuação: cores×clock + RAM×SIMD + cache×bonus |
| **Master Election** | `k_nano/src/net/mesh.rs` | ✅ Restaurado | Ancorado | maior capacidade | round-robin |
| **Role Assignment** | `k_nano/src/net/mesh.rs` | ✅ Restaurado | Master, Memory, Compute, Worker |
| **Heartbeat** | `k_nano/src/net/mesh.rs` | ✅ Restaurado | Monitoramento com timeout |
| **Symbiosis** | `k_nano/src/net/mesh.rs` | ✅ Restaurado | Acordos P2P para distribuição de inferência |

### Camada 3: Aplicação (Hermes)

O HermesAgent usa o Mesh para decidir onde executar cada requisição.

| Componente | Responsabilidade |
|-----------|-----------------|
| **Compute Dispatch** | Roteia inferência para o nó mais adequado (ou local) |
| **MoE Routing** | Distribui especialistas entre nós |
| **Skill Sync** | Sincroniza skills entre nós do cluster |

---

## Integração com o Ecossistema Existente

### Dependências

```
HermesAgent::tick()
  └→ mesh.tick()  →  heartbeat  →  election  →  role update
      └→ Transport::recv()  →  NoProto parse  →  MeshEvent
      └→ Transport::send()  →  NoProto serialize  →  UDP broadcast
      
NetAgent::tick()
  └→ UDP socket recv  →  mesh::on_packet()
  
CortexAgent::compute()
  └→ if mesh.role() == Worker → dispatch to master via P2P
```

### Gate Features

```rust
// O Mesh inteiro é gated por feature "p2p" (default=off)
// Ativar: cargo build --features "p2p"
#[cfg(feature = "p2p")]
mod mesh;

#[cfg(not(feature = "p2p"))]
pub fn mesh() -> Option<&'static MeshEngine> { None }
```

---

## Plano de Implementação

### Fase A — Transporte UDP (Fundação)

| Passo | LOC | Dependências | Riscos |
|-------|-----|-------------|--------|
| A1: UDP broadcast socket (smoltcp) | ~200 | e1000 TX/RX funcional | smoltcp UDP pode não suportar broadcast |
| A2: NoProto serialization over UDP | ~100 | noproto.rs restaurado | Nenhum |
| A3: Packet receive → MeshEvent | ~100 | mesh.rs, EventBus | Nenhum |

### Fase B — Mesh Ativo (Cluster)

| Passo | LOC | Dependências | Riscos |
|-------|-----|-------------|--------|
| B1: Conectar mesh.tick() ao NetAgent | ~50 | NetAgent | Concorrência com bootstrap |
| B2: Heartbeat real via timer | ~100 | APIC timer | Timeout em rede lenta |
| B3: Master election em broadcast | ~100 | UDP broadcast funcional | Split-brain em rede particionada |

### Fase C — Computação Distribuída (Valor)

| Passo | LOC | Dependências | Riscos |
|-------|-----|-------------|--------|
| C1: Compute dispatch via Mesh role | ~150 | HermesAgent, compute dispatch | Latência de rede |
| C2: MoE expert distribution | ~200 | MoE router, P2P transporte | Consistência de especialistas |
| C3: Skill sync entre nós | ~100 | SkillRegistry, WASM skills | Conflitos de versão |

### Riscos e Mitigações

| Risco | Probabilidade | Impacto | Mitigação |
|-------|--------------|---------|-----------|
| smoltcp não suporta UDP broadcast | Alta | Transporte bloqueado | Usar TCP multicast ou P2P TCP |
| Split-brain na eleição | Média | Dois mestres | Consensus reforçado (RAFT-like) |
| Latência de rede inviável para MoE | Média | Compute dispatch local | Threshold: só distribuir >500ms de inferência |
| Segurança: nós maliciosos | Baixa | Cluster comprometido | Ed25519 identity + trust chain |

---

## Relação com ADRs Existentes

| ADR | Relação |
|-----|---------|
| ADR-0042 §N4 (Multi-node) | Base conceitual para mesh multi-node |
| ADR-0057 (Compute Dispatch) | Mesh role determina dispatcher de compute |
| ADR-0080 (Legado Tecnológico) | Este ADR separa componentes P2P do legado geral |
| ADR-0060 (BEI) | AffectVector do BEI pode guiar decisões de distribuição |

---

## Status dos Componentes

| Componente | LEGACY | Ativo | Status |
|-----------|--------|-------|--------|
| NoProto Parser | `p2p/noproto.rs` ~448 LOC | `k_nano/src/net/noproto.rs` | ✅ Restaurado |
| LogicalClock | `p2p/clock.rs` ~284 LOC | `k_nano/src/sync/clock.rs` | ✅ Restaurado |
| MPMC Queue | `p2p/mpmc.rs` ~207 LOC | `k_nano/src/mpmc.rs` | ✅ Restaurado |
| Brain Mesh Engine | `net/brain_mesh.rs` ~707 LOC | `k_nano/src/net/mesh.rs` | ✅ Restaurado |
| UDP Broadcast | — | `k_nano/src/net/udp_broadcast.rs` | ❌ Pendente |
| NetAgent integration | — | `hermes/src/agents.rs` | ❌ Pendente |
| Compute dispatch via Mesh | — | `cortex/src/compute.rs` | ❌ Pendente |
| Skill sync | — | `hermes/src/skill_loader.rs` | ❌ Pendente |

---

## Decisões

1. **Feature gate:** Todo o ecossistema P2P fica atrás de `#[cfg(feature = "p2p")]`, default-off
2. **Separação clara:** P2P NÃO faz parte da reintegração LEGACY geral (ADR-0080). É domínio próprio.
3. **Prioridade:** Fase A (transporte UDP) antes de qualquer lógica de cluster
4. **Primeiro passo:** UDP broadcast via smoltcp + e1000 — validar antes de prosseguir
5. **Segurança:** Ed25519 identity para cada nó, trust chain para mensagens
