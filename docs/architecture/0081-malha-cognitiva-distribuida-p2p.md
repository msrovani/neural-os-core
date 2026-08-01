# ADR-0081: Malha Cognitiva Distribuída — P2P, Brain Mesh e Ecossistema Global

**Status:** Active  
**Lifecycle:** implementação  
**Criação:** 2026-07-30  
**Última revisão:** 2026-07-31  
**Tags:** p2p, mesh, distributed, datacenter, brain-mesh, broadcast, skynet, federated, crdt, depin, safety, ethics, dsd, speculative, security, mitm  
**Fontes:** `LEGACY/v1.9.9-test/k_nano/p2p/`, `LEGACY/v1.9.9-test/k_nano/net/brain_mesh.rs`, `ADR-0042 §N4`, `IDEA_BANK #189, #312f, #315.26, #315.27`, `SESSION_143`, `SESSION_152`, `SESSION_233`, `SESSION_234`, `SESSION_235`, `ADR-0021 §Life OS`, `ADR-0057`, `SKYNET DePIN (C:\Users\msrov\OneDrive\Área de Trabalho\SKYNET)`, `Petals (bigscience-workshop/petals)`, `Hivemind (learning-at-home/hivemind)`, `exo (exo-explore/exo)`, `Parallax (GradientHQ/parallax)`, `crdt-merge (mgillr/crdt-merge)`, `DeAI (lucasdemeritt-ops/deai)`, `Bacalhau`, `Akash Network`, `io.net`, `Bittensor`, `Gensyn`

---

## Contexto

O projeto neural-os-core precisa de um ecossistema de computação distribuída para atingir sua visão de **datacenter cognitivo global**: múltiplos nós AIOS se auto-descobrindo, elegendo liderança, e distribuindo inferência de ML sem servidor central.

``` 
Cada AIOS vira um nó em um cluster cognitivo global.
Os nós se auto-descobrem (broadcast UDP), 
elegem mestres (CapacityScore), 
distribuem inferência (P2P MoE).

Sem servidor central. Sem configuração. Zero-touch.
```

Três tecnologias foram identificadas no LEGACY que formam a base deste ecossistema:

1. **P2P Networking** (`p2p/`: NoProto, LogicalClock, MPMC) — camada de transporte e coordenação
2. **Brain Mesh Engine** (`net/brain_mesh.rs`) — cluster autônomo com eleição e papéis
3. **UDP Broadcast** — descoberta de nós sem configuração

Este ADR separa estas tecnologias do esforço geral de reintegração LEGACY (ADR-0080) e as trata como um domínio arquitetural próprio.

---

## Ecossistema de Ideias — IDEA_BANK

O IDEA_BANK contém 4 ideias diretamente relacionadas à malha distribuída, todas atualmente com status ⏳ (adiadas para pós-v2.0):

| IDEA | Título | Status | LOC | Dependências |
|------|--------|--------|-----|-------------|
| **#189** | **Federated Cluster / P2P Workers** | ⏳ Futuro | ~300 | Stack de rede + scheduler distribuído + WASM remoto |
| **#312f** | **Federated Learning** | ⏳ Pós-MVP | ~200 | B-01 (rede) + MCP |
| **#315.26** | **Multi-device sync (CRDT)** | ⏳ defer | ~300 | `depends_on: lan` |
| **#315.27** | **SKYNET Mesh Node** | ⏳ defer | ~500 | `depends_on: lan` |

### Detalhamento por Ideia

#### #189 — Federated Cluster / P2P Workers
- **Descrição original:** "Mesh de AI compute (gaming PC, Mac, RPi, Android). Auto-descoberta, pareamento PIN, checkpoint distribuído."
- **Status:** ⏳ Futuro — "Depende de toda stack de rede + scheduler distribuído + WASM remoto."
- **Relação com Brain Mesh:** O `BrainMeshEngine` já implementa a auto-descoberta e eleição que o #189 descreve. O que falta é o transporte UDP real e a execução remota de WASM.
- **Valor:** Permite que qualquer dispositivo na rede local contribua com poder de inferência — um notebook de 8 cores vira um nó de compute.

#### #312f — Federated Learning
- **Descrição original:** "Múltiplos AIOS compartilham gradientes (não dados). Agregador central (Hermes Master) combina updates → modelo global melhor."
- **Status:** ⏳ Pós-MVP — "Depende de B-01 + MCP."
- **Relação com Brain Mesh:** O Mesh já elege um Master — esse Master é o agregador natural de gradientes. O `LogicalClock` do P2P garante ordenação causal dos updates.
- **Valor:** O SleepCycle de cada nó treina localmente; só os gradientes (não os dados) trafegam na rede. Privacidade + melhoria contínua.

#### #315.26 — Multi-device Sync (CRDT)
- **Descrição original:** "Sincronização de memória/contexto entre dispositivos via CRDT (Automerge-style)."
- **Status:** ⏳ defer + `depends_on: lan` — SESSION_143: "não gate v2.0.0."
- **Relação com Brain Mesh:** CRDT permite que múltiplos nós editem o mesmo estado (memória do SGDB, skills registrados) sem conflito — convergência garantida mesmo com partições de rede.
- **Valor:** JARBAS em múltiplos dispositivos compartilha a mesma memória episódica.

#### #315.27 — SKYNET Mesh Node
- **Descrição original:** "Participa da malha SKYNET como nó L1 (PC) ou L2 (workstation). Speculative decoding distribuído."
- **Status:** ⏳ defer + `depends_on: lan` — SESSION_152: "Fora: CRDT/SKYNET, SoftMAC, fake HTTPS."
- **Relação com Brain Mesh:** O SKYNET é a visão de longo prazo — uma malha global de nós AIOS. O Brain Mesh é a implementação de curto prazo (LAN). O protocolo NoProto com `AiosTaskPacket` pode transportar tokens de speculative decoding entre nós.
- **Valor:** Speculative decoding distribuído = múltiplos nós geram tokens candidatos em paralelo. Master verifica em O(1).

---

## Lições das Sessions

### SESSION_143 — Gate v2.0
- **Decisão:** `#315.26-27` (CRDT sync e SKYNET Mesh) estão FORA do gate v2.0.0. Rotulados como `depends_on: lan`.
- **Implicação:** A Fase C desta ADR (computação distribuída) só deve ser iniciada APÓS o gate v2.0.0. Até lá, apenas Fase A e B (transporte UDP + mesh local) são validades.

### SESSION_152 — Pós-LAN
- **Decisão:** CRDT/SKYNET marcados como "Fora gate" junto com WiFi, SoftMAC e fake HTTPS.
- **Contexto:** SESSION_152 foi o fechamento da Onda 7 LAN (E1000 RX/TX funcional, DNS raw, HTTP). A rede local acabava de ficar operacional.
- **Implicação:** O transporte P2P pode usar e1000 + smoltcp (que estão funcionais desde SESSION_152). UDP broadcast é o próximo passo lógico.

### SESSION_163 — Broadcast e AP Workers
- **Lição:** SIPI broadcast (para acordar APs) causava corrupção de stack com ≥2 APs. A solução foi IPI direcionado.
- **Implicação para P2P:** Broadcast UDP para descoberta de nós deve ser cuidadoso com resposta simultânea de múltiplos nós (thundering herd). O Brain Mesh já implementa `jitter` e `backoff` nos heartbeats.

### SESSION_146 — GPU Direct Storage
- **Contexto:** `#423 GDS` marcado como "▶️ AWAITING_HW — sem P2P fake".
- **Implicação:** GPU Direct Storage (NVMe→VRAM via P2P PCIe) é uma tecnologia irmã — permite transferência zero-copy entre dispositivos no mesmo barramento. Não deve ser confundida com P2P de rede.

---

## Relação com ADRs Existentes

| ADR | Relação | Status |
|-----|---------|--------|
| **ADR-0042 §N4 (Multi-node)** | Base conceitual para mesh multi-node — "Especificação de boot e wireframe para nós K³CHJ se descobrirem em LAN" | ✅ Proposta |
| **ADR-0057 (Compute Dispatch)** | Mesh role (Master/Worker/Compute) determina dispatcher de compute — AP workers locais + P2P remotos | ✅ Completa |
| **ADR-0060 (BEI) A.2 e A.6** | AffectVector do BEI guia decisões de distribuição — "nós com mais energia/ociosos recebem mais trabalho" | ✅ 7/7 ondas |
| **ADR-0080 (Legado Tecnológico)** | Este ADR separa componentes P2P do legado geral — Fase 3 original vira ADR-0081 | ✅ Proposta |
| **ADR-0021 (Life OS Ecosystem)** | Origem da ideia #189 (Federated Cluster) — análise de ecossistemas Life OS com 20 repos | 📚 Pesquisa |
| **ADR-0016 (Network Strategy)** | Estratégia de rede original — SLIP/COM2 como fallback debug, e1000/smoltcp como gate canônico | ✅ Modernizada |
| **ADR-0046 (AirLLM GGUF Streaming)** | Streaming de modelos entre nós — peer pode servir layer weights via P2P em vez de HTTP | ✅ MVP |

---

## Arquitetura Proposta

### Stack de 3 Camadas

```
┌──────────────────────────────────────────────────────────┐
│               Application Layer (Hermes)                  │
│  Compute Dispatch  │  MoE Routing  │  Skill Sync  │  CRDT │
├──────────────────────────────────────────────────────────┤
│            Brain Mesh Engine (k_nano::net::mesh)          │
│  Descoberta  │  Eleição  │  Heartbeat  │  Papéis  │  Grad │
├──────────────────────────────────────────────────────────┤
│           P2P Transport Layer (k_nano::net::p2p)          │
│  NoProto  │  LogicalClock  │  MPMC  │  UDP Broadcast     │
└──────────────────────────────────────────────────────────┘
```

### Mapa de Ideias → Camadas

| Ideia | Camada | Componente |
|-------|--------|-----------|
| #189 Federated Cluster | Mesh + Application | Brain Mesh + Hermes dispatch |
| #312f Federated Learning | Application | Hermes Master agrega gradientes |
| #315.26 CRDT Sync | Application | Memória SGDB replicada entre nós |
| #315.27 SKYNET Mesh | Mesh + Transport | Brain Mesh + NoProto + LogicalClock |

### Camada 1: P2P Transport

Responsável pela comunicação confiável entre nós via UDP.

| Componente | Arquivo | Status | Descrição |
|-----------|---------|--------|-----------|
| **NoProto Parser** | `k_nano/src/net/noproto.rs` | ✅ Restaurado | Zero-copy packet format (`AiosTaskPacket` com magic/clock/task_type) |
| **LogicalClock** | `k_nano/src/sync/clock.rs` | ✅ Restaurado | Lamport clock para ordenação causal |
| **VectorClock** | `k_nano/src/sync/clock.rs` | ✅ Restaurado | 16-node causality tracking (para CRDT) |
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
| **Heartbeat** | `k_nano/src/net/mesh.rs` | ✅ Restaurado | Monitoramento com timeout + jitter |
| **Symbiosis** | `k_nano/src/net/mesh.rs` | ✅ Restaurado | Acordos P2P para distribuição de inferência |
| **GradientAggregator** | `k_nano/src/net/mesh.rs` | ❌ Pendente | Agrega gradientes federados (#312f) |

### Camada 3: Aplicação (Hermes)

O HermesAgent usa o Mesh para decidir onde executar cada requisição.

| Componente | Responsabilidade | Ideia relacionada |
|-----------|-----------------|-------------------|
| **Compute Dispatch** | Roteia inferência para o nó mais adequado | #189 Federated Cluster |
| **MoE Routing** | Distribui especialistas entre nós | #189 |
| **Skill Sync** | Sincroniza skills entre nós do cluster | #315.27 SKYNET |
| **CRDT Sync** | Estado compartilhado sem conflito | #315.26 Multi-device sync |
| **Gradiente Share** | Gradientes (não dados) para federated learning | #312f |

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
  
SecurityAgent::tick()
  └→ verify node identity (Ed25519) → trust chain

SleepCycleAgent::consolidate()
  └→ if mesh.role() == Master → aggregate gradients (#312f)
  └→ sync SGDB state via CRDT (#315.26)
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

### Fase 0 — Pré-requisitos (JÁ FEITO)

| Item | Componente | Status |
|------|-----------|--------|
| NoProto Parser | `k_nano/src/net/noproto.rs` | ✅ Restaurado do LEGACY |
| LogicalClock | `k_nano/src/sync/clock.rs` | ✅ Restaurado do LEGACY |
| VectorClock | `k_nano/src/sync/clock.rs` | ✅ Restaurado do LEGACY |
| MPMC Queue | `k_nano/src/mpmc.rs` | ✅ Restaurado do LEGACY |
| Brain Mesh Engine | `k_nano/src/net/mesh.rs` | ✅ Restaurado do LEGACY |
| Ed25519 identity | `k_nano/src/identity.rs` | ✅ Existente no código |

### Fase A — Transporte UDP (Fundação)

| Passo | LOC | Dependências | Riscos |
|-------|-----|-------------|--------|
| A1: UDP broadcast socket (smoltcp) | ~200 | e1000 TX/RX funcional | smoltcp UDP pode não suportar broadcast |
| A2: NoProto serialization over UDP | ~100 | noproto.rs restaurado | Nenhum |
| A3: Packet receive → MeshEvent | ~100 | mesh.rs, EventBus | Nenhum |

### Fase B — Mesh Ativo (Cluster local)

| Passo | LOC | Dependências | Riscos |
|-------|-----|-------------|--------|
| B1: Conectar mesh.tick() ao NetAgent | ~50 | NetAgent | Concorrência com bootstrap |
| B2: Heartbeat real via timer | ~100 | APIC timer | Timeout em rede lenta |
| B3: Master election em broadcast | ~100 | UDP broadcast funcional | Split-brain em rede particionada |

### Fase C — Computação Distribuída (PÓS-GATE v2.0)

| Passo | LOC | Dependências | Riscos |
|-------|-----|-------------|--------|
| C1: Compute dispatch via Mesh role | ~150 | HermesAgent, compute dispatch | Latência de rede |
| C2: MoE expert distribution | ~200 | MoE router, P2P transporte | Consistência de especialistas |
| C3: Skill sync entre nós | ~100 | SkillRegistry, WASM skills | Conflitos de versão |
| C4: CRDT memory sync (#315.26) | ~300 | SGDB, CRDT library | Convergência em partição |
| C5: Gradiente share federado (#312f) | ~200 | TrainingAgent, Mesh Master | Largura de banda |

**Gate v2.0.0:** As fases A e B são permitidas ANTES do gate v2.0.0. A fase C DEPENDE do gate (SESSION_143): só iniciar após v2.0.0 liberado.

### Riscos e Mitigações

| Risco | Probabilidade | Impacto | Mitigação |
|-------|--------------|---------|-----------|
| smoltcp não suporta UDP broadcast | Alta | Transporte bloqueado | Usar TCP multicast ou P2P TCP |
| Split-brain na eleição | Média | Dois mestres | Consensus reforçado (RAFT-like) |
| Latência de rede inviável para MoE | Média | Compute dispatch local | Threshold: só distribuir >500ms de inferência |
| Segurança: nós maliciosos | Média (rede compartilhada) | Cluster comprometido | **Fase A: TOFU + fail-closed + anti-replay (plano acima); Fase B: X25519+ChaCha20-Poly1305** |
| CRDT converge lentamente | Baixa | Estado inconsistente | Merkle clock para detectar divergência |
| Thundering herd no broadcast | Baixa | Storm de resposta | Jitter + backoff (já implementado no mesh) |

---

## Visão de Longo Prazo (SKYNET)

A visão SKYNET (#315.27) é a malha global:

```
Nó L1 (datacenter):
  ─ 64+ cores, GPU, NVMe, 100GbE
  ─ Role: Master global, agrega gradientes
  ─ Armazena modelo global consolidado

Nó L2 (workstation):
  ─ 8-16 cores, GPU opcional, SSD, 1GbE
  ─ Role: Compute + Memória local
  ─ Treina experts locais, compartilha gradientes

Nó L3 (edge):
  ─ 4-8 cores, eMMC, WiFi
  ─ Role: Worker leve, inferência apenas
  ─ Recebe modelo consolidado, nunca treina

Nó L4 (IoT):
  ─ 1-2 cores, bateria, malha mesh
  ─ Role: Sensor, acionador
  ─ Comunica via NoProto sobre UDP lightweight
```

Cada nó executa o mesmo kernel AIOS. A diferença é o `CapacityScore`:

```rust
// k_nano::net::mesh::NodeCapabilities
pub struct NodeCapabilities {
    pub tier: NodeTier,        // L1..L4
    pub cores: u32,
    pub ram_mb: u32,
    pub has_gpu: bool,
    pub bandwidth_mbps: u32,
    pub energy_mw: u32,       // Battery? → Worker leve
    pub simd_width: u32,       // AVX-512? → Compute pesado
    pub cache_mb: u32,        // L3 cache → MoE expert sizing
}
```

---

## Status dos Componentes

| Componente | LEGACY | Ativo | Status |
|-----------|--------|-------|--------|
| NoProto Parser | `p2p/noproto.rs` ~448 LOC | `k_nano/src/net/noproto.rs` | ✅ Restaurado |
| LogicalClock | `p2p/clock.rs` ~284 LOC | `k_nano/src/sync/clock.rs` | ✅ Restaurado |
| VectorClock | `p2p/clock.rs` — incluso | `k_nano/src/sync/clock.rs` | ✅ Restaurado |
| MPMC Queue | `p2p/mpmc.rs` ~207 LOC | `k_nano/src/mpmc.rs` | ✅ Restaurado |
| Brain Mesh Engine | `net/brain_mesh.rs` ~707 LOC | `k_nano/src/net/mesh.rs` | ✅ Restaurado |
| UDP Broadcast | — | `k_nano/src/net/udp_broadcast.rs` | ✅ **Implementado** (SESSION_234 — frame Ethernet+IP+UDP manual, NIC via nic_globals) |
| Transporte P2P (heartbeat/RX) | — | `k_nano/src/net/mesh.rs::p2p_tick` | ✅ **Implementado** (SESSION_234 — wired no boot via bei_init.rs) |
| Marketplace P2P | — | `hermes/src/skill_marketplace.rs` | ✅ **Implementado** (SESSION_235 — 14 skills reais broadcast, throttle TIMER_TICKS) |
| Skill sync | — | `hermes/src/skill_sync.rs` | ✅ **Implementado** (SESSION_234/235 — Master push + Worker apply + PROMOTE via EventBus) |
| Role Assignment (propagação) | — | `k_nano/src/net/mesh.rs::assign_roles` | ✅ **Implementado** (SESSION_235 — `ROLE\0target\0role`, receptor aplica set_role) |
| Compute dispatch via Mesh | — | `cortex/src/compute.rs` | ✅ **Implementado** (SESSION_235 item 4 — feature `p2p`, matmul Worker→Master `MW/MR`) |
| Node identity Ed25519 | — | `k_nano/src/identity.rs` | ⚠️ **Parcial** — assinatura existe mas verificação é fail-open (ver seção Segurança) |
| CRDT sync (#315.26) | — | SGDB + CRDT lib | ❌ Pendente (Fase C) |
| Gradiente share (#312f) | — | `k_ai/src/fl_trainer.rs` | ❌ Pendente (Fase C — padrão MW/MR já validado, desbloqueio direto) |
| SKYNET protocol (#315.27) | — | NoProto + NodeTier | ❌ Pendente (Fase C) |
| DSD SpeculativeDecoder | — | `cortex/src/speculative.rs` | ❌ Pendente (Fase C) |
| SemanticRouter | — | `hermes/src/router.rs` | ❌ Pendente (Fase C) |
| FedYogi FL | — | `k_ai/src/fl_trainer.rs` | ❌ Pendente (Fase C) |
| Fragmentação MTU / assíncrono | — | `k_nano/src/net/udp_broadcast.rs` | ❌ Pendente (gate 1200B; matmul grande + FL precisam) |

---

## Integração com SKYNET DePIN

### Descoberta

O projeto **SKYNET** (`C:\Users\msrov\OneDrive\Área de Trabalho\SKYNET`) é um DePIN super app para inferência de IA distribuída, do mesmo autor (`msrovani`). É um monorepo TypeScript/Rust com 8 pacotes, 642 testes, e lógica de cluster cognitivo global.

SKYNET e neural-os-core são **altamente complementares**: neural-os-core fornece a fundação bare-metal (Ring 0-3), SKYNET fornece a lógica de malha distribuída (userspace/cloud).

### Hierarquia de Tiers Alinhada

| Tier | SKYNET | ADR-0081 | neural-os-core |
|------|--------|----------|---------------|
| **L0** | Smartphones, IoT (1-3B params) | Sensor/Edge | `CapacityScore` baixo, sem GPU |
| **L1** | PCs, Consoles (7-13B params) | Worker/Compute | Desktop AVX2, e1000, 8-16 cores |
| **L2** | Workstations, Smart TVs (13-70B) | Memory/Master | Servidor Xeon/EPYC, GPU, 64+ cores |
| **L3** | Datacenter (70B+ params) | Master Global | Cluster dedicado, 100GbE |

### 5 Conexões Diretas com ADR-0081

#### 1. Distributed Speculative Decoding (DSD)
- **SKYNET:** `speculative-decoding.ts` — draft/verify/rejection com TreeSpecDecoder e LightweightVerifier (MLP 1k params)
- **ADR-81:** Pode ser portado para `cortex::speculative` — níveis L0 (draft) + L1 (verify) do mesh
- **Valor:** 2.89× speedup benchmarkado. O maior ganho de performance para inferência distribuída.

#### 2. Semantic Router (HNSW)
- **SKYNET:** `semantic-router.ts` — índice HNSW em TypeScript puro, routing por cosine similarity, OATS embedding refinement
- **ADR-81:** Complementa o HermesAgent — intent routing por similaridade semântica (não só keyword/MLP)
- **Valor:** Routing adaptativo que melhora com uso (OATS). FlatNav reduz 38% memória. CRouting reduz 41.5% distancia.

#### 3. TEE Attestation + Trust Chain
- **SKYNET:** `tee-attestation-layer/` — SGX/SEV/CCA/GPU-CC, ProofOfTime (FLOPS + SHA-256), CRA collective attestation, NEAR MPC-TEE
- **ADR-81:** Conecta com `k_ai::trust` — nós do mesh precisam provar identidade via Ed25519 + TEE
- **Valor:** Trust sem servidor central. Verificação O(1) de O(n) nós via CRA.

#### 4. Federated Learning (FedYogi)
- **SKYNET:** `fl-training-client/` — FedYogi, QLocalAdam (3.37× menos RAM), FEDADAVR (high churn), zk-SNARKs FL prover, PFed1BS (99% redução comunicação via FHT)
- **ADR-81:** Conecta com #312f (Federated Learning) — Master agrega gradientes do SleepCycle de cada nó
- **Valor:** Privacidade dos dados (só gradientes trafegam) + melhoria contínua global

#### 5. CRDT Multi-Device Sync
- **SKYNET:** Automerge CRDT v2/v3, LoRaWAN CRDT sync, Acoustic CRDT sync, OpportunisticRouter (IP→LoRa→Acoustic)
- **ADR-81:** Conecta com #315.26 (Multi-device sync) — SGDB replicado entre nós com convergência garantida
- **Valor:** JARBAS em múltiplos dispositivos compartilha memória sem conflito. Fallback extremo via LoRa/acústico.

### Stack Comparativa

| Camada | neural-os-core | SKYNET | Bridge |
|--------|---------------|--------|--------|
| **Kernel** | Bare-metal Ring 0-3 | Node.js/TypeScript | wasmi (Roda WASM SKYNET no kernel) |
| **Transporte** | e1000 + smoltcp | WebTransport + WebRTC | Expor e1000 como endpoint WebTransport |
| **Scheduling** | AgentRegistry round-robin | Circadian + Thermal + Carbon | Importar lógica para OptimizerAgent |
| **Routing** | Trinity + Hermes | SemanticRouter (HNSW) | Adaptar HNSW para Hermes |
| **Trust** | TrustCache + Ed25519 | TEE + MPC + CRA | Prover TEE como backend TrustCache |
| **FL** | BitNetTrainer (ADD/SUB) | FedYogi + QLocalAdam + zk-SNARKs | Portar FedYogi para CPU ternary |
| **Spec Decode** | Medusa heads | DSD (draft/verify/rejection) | Unificar: Medusa gera draft, DSD verifica |

### Recomendações de Implementação

1. **Curto prazo (pré-v2.0):** Portar DSD LightweightVerifier MLP para `cortex::speculative` (~400 LOC). Já temos Medusa heads, DSD adiciona o verifier distribuído.
2. **Médio prazo (pós-v2.0):** Adaptar SemanticRouter HNSW para Hermes (`hermes::router`). Substituir routing por keyword por ANN routing.
3. **Longo prazo (pós-SKYNET beta):** Bridge completa — SKYNET WebTransport ↔ neural e1000, trust TEE ↔ TrustCache, FedYogi ↔ SleepCycle.
4. **Pesquisa:** CRA collective attestation (O(1) verification) — ideal para mesh com milhares de nós.

---

## Segurança do Mesh (modelo de ameaças + plano de hardening)

**Estado atual (SESSION_235): o mesh é seguro para rede isolada/QEMU, NÃO para rede confiável.** A assinatura Ed25519 existe (`identity::sign_session`/`verify_signature`) mas é **cosmética**: o caminho RX verifica com a chave pública LOCAL (não a do peer) e, se a verificação falha, **aceita o pacote mesmo assim** (fail-open). Não há registro de chaves de peers, nem criptografia, nem anti-replay.

### Modelo de ameaças (rede compartilhada/L2)

| Vetor | Como | Impacto |
|-------|------|---------|
| **Spoof de eleição** | Injetar heartbeat com `node_id` menor → vence tie-break | Atacante vira Master → controla papéis/skills/offers |
| **Fake ROLE** | Injetar `ROLE\0...` não assinado (aceito) | Rebaixa Master real, reatribui papéis |
| **Fake MR (compute)** | Responder matmul com tensor forjado | Envenena resultado do Worker (dest_id filtro só lógico) |
| **Fake MW/DoS** | Flood de requests no Master | Exaustão/negação |
| **Fake Sync/PROMOTE** | Injetar skill maliciosa | Skill não-autorizada no Master |
| **Eavesdrop** | Payloads em claro (skills, tensores w/x, papéis) | Confidencialidade zero |
| **Replay** | Campo `clock` existe mas não é validado | Pacote antigo re-aplicado |

### Causas raiz

1. **Chave errada na verificação** — `mesh.rs` RX usa `session_public_key()` (local) contra assinatura do peer → sempre falha → cai no `None => rx` (aceita).
2. **Fail-open** — o fallback `None => rx` aceita pacote sem assinatura/inválida. Deveria ser `continue` (drop).
3. **Sem tabela de peers** — não há `node_id → pk` (TOFU/PKI); grep por `peer_key`/`known_keys`/`TOFU` = vazio.
4. **Sem criptografia** — única dep cripto no workspace é `ed25519-compact`; nenhuma primitiva simétrica/ECDH.

### Fase A — Autenticação TOFU + fail-closed (PLANEJADA, prioridade 1)

| Item | Detalhe | Custo |
|------|---------|-------|
| Tabela de peers | `[Option<(u8, [u8;32])>; 16]` em `k_nano::net::mesh` (array fixo, hot path) | ~40 LOC |
| Handshake TOFU | 1º heartbeat com assinatura → guarda `(node_id, pk)`; próximos verificam contra a chave guardada | ~40 LOC |
| Fail-closed | `None => rx` vira `continue` (drop) quando assinatura presente e inválida; pacote sem assinatura → drop (exceto handshake) | ~30 LOC |
| Anti-replay | Janela de `clock` (±N ticks do último visto) | ~30 LOC |
| Helper identity | `get_peer_pk`/`put_peer_pk` | ~30 LOC |

**Total Fase A: ~150-200 LOC, 2 arquivos (`mesh.rs` + `identity.rs`), sem deps novas, ~1.5-2h** (inclui revalidar todo o mesh QEMU: descoberta/eleição/skills/matmul). Elimina spoof, fake ROLE/MR/Sync/PROMOTE e replay simples. **Responde o MITM: "não, agora é autenticado".**

### Fase B — Criptografia do payload (PLANEJADA, prioridade 2)

| Item | Detalhe | Custo |
|------|---------|-------|
| Deps novas | `chacha20poly1305` + `x25519-dalek` (no_std, ~5 crates transitivos) — **primeira dep cripto além do ed25519** | — |
| Key exchange | No handshake TOFU: troca X25519 pubkeys (1 pacote extra) | ~60 LOC |
| Encrypt | Após header NoProto: nonce(12) + ciphertext + tag(16) | ~50 LOC |
| Decrypt | Antes do parse; falha → drop | ~40 LOC |
| Modo dev | Pacotes não-encriptados → drop OU aceitar só em modo dev (flag) | ~20 LOC |

**Total Fase B: ~250-350 LOC, 2-3 arquivos, dep nova, risco alto (nonce mgmt, rejeição de reuse), ~4-6h.** Necessária quando houver tráfego sensível (FL com gradientes, matmul de dados reais) em rede não-isolada.

### Sugestões (oracle review recomendado antes de implementar)

1. **Fase A é o 90% do problema por ~30% do esforço** — implementar primeiro, sem deps, mesh continua funcional.
2. **TOFU tem limitação inerente** (primeira vez sem autenticação): em LAN controlada é aceitável; para SKYNET global, usar TEE attestation + CRA (já mapeado na seção SKYNET — `tee-attestation-layer/`).
3. **Key rotation**: incluir `generation` no payload assinado para permitir troca de chave de sessão sem quebrar o mesh.
4. **Rate-limit** no handshake (1 TOFU por node_id por janela) para evitar envenenamento da tabela de peers.
5. **Nonce management** na Fase B: nonce = (node_id, contador) — nunca aleatório puro.

---

## Decisões

1. **Feature gate:** Todo o ecossistema P2P fica atrás de `#[cfg(feature = "p2p")]`, default-off
2. **Separação clara:** P2P NÃO faz parte da reintegração LEGACY geral (ADR-0080). É domínio próprio.
3. **Prioridade:** Fase A (transporte UDP) antes de qualquer lógica de cluster
4. **Primeiro passo:** UDP broadcast via smoltcp + e1000 — validar antes de prosseguir
5. **Segurança:** Ed25519 identity para cada nó, trust chain para mensagens. **Hardening em 2 fases (ver seção Segurança do Mesh):** Fase A = autenticação TOFU + fail-closed + anti-replay (~150-200 LOC, sem deps, ~2h) antes de qualquer rede não-isolada; Fase B = X25519 + ChaCha20-Poly1305 (~250-350 LOC, dep nova, ~4-6h) quando houver tráfego sensível (FL/matmul real).
6. **Gate v2.0.0:** Fase C (computação distribuída, CRDT, federated) apenas APÓS v2.0.0
7. **IDEA_BANK:** As ideias #189, #312f, #315.26, #315.27 são absorvidas por esta ADR. Seus status no IDEA_BANK passam de ⏳ para 🟡 (planejamento) com referência a esta ADR.
8. **SKYNET:** A arquitetura SKYNET (L1-L4) é a visão de longo prazo. O Brain Mesh é a implementação LAN imediata. O NoProto + NodeTier + CapacityScore formam a ponte entre as duas.
9. **Pesquisa Global:** Esta ADR incorpora análise de 20+ projetos open source, 25+ papers acadêmicos e ecossistema DePIN completo.

---

## Anexo: Pesquisa Global — Estado da Arte em Malhas de Inferência Distribuída

### Projetos de Código Aberto (20+ analisados)

| Projeto | Stack | Relevância para ADR-81 |
|---------|-------|------------------------|
| **Petals** (bigscience-workshop) | Python/PyTorch, BitTorrent layer sharding | Alta: mesma arquitetura — nós compartilham layers do transformer. 6 tok/s Llama 70B. NeurIPS 2023. |
| **Hivemind** (learning-at-home) | Python + Go (libp2p), DHT descentralizado | Alta: base do Petals. MoE distribuído, treino tolerante a falhas. |
| **exo** (exo-explore) | Python/MLX + Rust, RDMA Thunderbolt | Crítica: kernel Rust para RDMA. Tensor parallelism P2P. 4x M3 Ultra rodando DeepSeek 671B. |
| **Parallax** (GradientHQ) | Python + SGLang/vLLM, Lattica (Go) | Crítica: two-phase scheduler + pipeline parallelism P2P. Suporta DeepSeek, Qwen. |
| **crdt-merge** (mgillr) | Python, CRDT-compliant model merging | Crítica: 26/26 estrategias de merge via OR-Set. E4 Trust Lattice. 7.24B params, 100 nos. |
| **DeAI** (lucasdemeritt-ops) | Python + Solidity, Proof-of-Useful-Inference | Alta: optimistic verification + slashing. Testnet Sepolia. |
| **Akash Network** | Go + Cosmos SDK | Media: 428% YoY growth, 80% utilization. Leilao reverso de GPU. |
| **io.net** | Solana + Rust, GPU DePIN | Media: 435k+ GPU containers. Prova que mercado DePIN e viabil. |
| **Bittensor** | Python + Substrate, 128+ subnets | Media: mercado de inteligencia descentralizado. Dynamic TAO 2025. |
| **Gensyn** | Python/P2P (Go libp2p) | Alta: axl P2P node com MCP/A2A. Verificacao de treino ML. |

### Papers Chave (25+ analisados)

| Paper | Descoberta | Viabilidade |
|-------|-----------|-------------|
| **DSD** (arXiv:2511.11733, 2025): 2.56-2.59x speedup em 3-8 nos descentralizados. Latencia de rede vira throughput de computacao. | Implementado no Parallax |
| **SpecHub** (Sun et al., 2024): Multi-draft speculative decoding via LP. +0.05-0.27 tokens/step. | Codigo aberto |
| **Jupiter** (Ye et al., IEEE INFOCOM 2025): Pipeline parallelism intra-sequence. 26.1x latencia reduzida. | Implementado |
| **CRDT Merge State** (arXiv:2605.19373, 2026): 26/26 estrategias de merge via two-layer OR-Set. Resolve consistencia em FL P2P sem coordenador. | crdt-merge v0.10.0 |
| **E4 Trust Lattice** (Gillespie, 2026): Trust como dimensao algebrica do CRDT. Ed25519 + ML-DSA-65 pos-quantum. | crdt-merge v0.9.6 |
| **SWARM Parallelism** (Ryabinin et al., ICML 2023): Pipelines randomizados rebalanceados. Treinou 13B em T4s com <200Mb/s. | Hivemind |

### AI Safety & Ethics (Leis da Robotica)

| Risco | Mitigacao | Status |
|-------|-----------|--------|
| Self-preservation: agente replicado resiste a desligamento | Dead Man's Switch: se heartbeat para, kill via EventBus. | Implementar |
| Bostrom Paperclip P2P: agente otimizador sequestra GPUs | E4 Trust Lattice: trust-decay automatico para anomalias. | Adaptar TrustCache |
| Swarm runaway: swarm coordena sem intervencao humana | Policy-gated execution: (token, agent, skill) -> CapGate -> TrustScore. | Ja existe |
| 1a Lei (nao causar dano): no executa codigo malicioso | Trust por agente + validacao de conteudo. | Falta validacao |
| 2a Lei (obedecer humanos): sem kill switch global | EventBus AGENT_DEATH + timeout N ticks. | Implementar |

### Recomendacoes de Implementacao

| Prioridade | Acao | Esforco | Riscos | Fonte |
|------------|------|---------|--------|-------|
| Critica | Dead Man's Switch: EventBus timeout -> AgentDeath -> SecurityAgent shutdown | Muito baixo (dias) | Baixo | Asimov 2a Lei |
| Critica | Policy-gated execution com TrustScore threshold | Baixo (semana) | Baixo | E4 Trust Lattice |
| Alta | CRDT OR-Set para estado compartilhado do mesh | Medio (2 sem) | Baixo | crdt-merge v0.10.0 |
| Alta | P2P transport via rust-libp2p ou smoltcp custom | Medio (3 sem) | Medio | exo, Parallax |
| Media | Pipeline parallelism (divisao de layers) | Alto (1 mes) | Medio | Parallax, Jupiter |
| Media | E4 Trust Lattice no TrustCache | Medio (2 sem) | Baixo | crdt-merge |
| Media | DSD (Distributed Speculative Decoding) | Alto (1 mes) | Medio | DSD arXiv:2511.11733 |
| Ignorar | zk-proofs para ML | Proibitivo | N/A | — |
| Proibido | Auto-replicacao de agentes | Proibido por seguranca | Asimov | — |
