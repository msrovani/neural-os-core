# ADR-0025 — Tier 3 Ecosystem Analysis: Sandbox, Security & Semantic Filesystem

**Status:** Aprovado  
**Data:** 2026-06-25  
**Contexto:** Análise de 5 repositórios de segurança/sandbox/filesystem para extrair padrões portáveis ao neural-os-core.

---

## Resumo

5 repositórios analisados (InnerWarden, ai-jail, vexfs, Chisel, cori-kernel).  
33 padrões extraídos → 12 viáveis para neural-os-core → classificados por complexidade.

---

## 1. Projetos Analisados

### 1.1 InnerWarden (159 ⭐ — 2057 commits — 7900+ testes)

**Descrição:** Camada de segurança local para agentes AI. Monitora comandos, tráfego MCP, atividade Linux com eBPF, bloqueia comportamento perigoso.

**Stack:** Rust + eBPF + SQLite WAL + ONNX (local warden) + LLM opcional

**Estrutura:** 
- 45 programas eBPF (tracepoints, kprobes, LSM, XDP)
- 82 detectores stateful 
- 69 regras de correlação cross-layer (7 estágios kill chain)
- Knowledge Graph (11 tipos de node, 53 relações)
- 90+ técnicas MITRE ATT&CK
- 208 regras Sigma
- 3 playbooks SOC (exfil, Log4Shell, credential stuffing)
- Graduated Enforcement (Observe→Warn→Contain→Enforce)
- Hash chain audit trail (tamper-evident)
- AI Triage opcional (ONNX local ~91MB ou LLM remoto)

### 1.2 ai-jail (595 ⭐ — 222 commits)

**Descrição:** Sandbox wrapper multi-OS para agentes AI coding. Linux (bwrap + Landlock + seccomp) e macOS (sandbox-exec).

**Stack:** Rust + bubblewrap + Landlock LSM + seccomp-bpf

**Padrões:**
- Namespace isolation (PID, UTS, IPC, mount, network)
- Landlock LSM V3/V4 (arquivos + rede)
- Seccomp-bpf (~30 syscalls bloqueadas)
- OverlayFS copy-on-write (--overlay-map)
- Private home mode (tmpfs $HOME)
- Mask secrets (--mask, --deny-path)
- Path confinement
- Browser profile isolation (hard/soft)
- Resource limits (RLIMIT_NPROC, NOFILE, CORE)
- Sensitive /sys masking

### 1.3 vexfs (24 ⭐ — 129 commits)

**Descrição:** Filesystem kernel-native com vector search e memória semântica. Três camadas: FUSE + API Server + Dashboard.

**Stack:** Rust + FUSE + React + ChromaDB/Qdrant API

**Padrões:**
- Filesystem como interface de vector search
- Multi-dialect API (ChromaDB + Qdrant + Native)
- HNSW indexing
- FUSE filesystem (operações básicas)
- Kernel module (instável, VM only)

### 1.4 Chisel (12 ⭐)

**Descrição:** Ferramentas Rust de precisão para agentes AI com confinamento de path enforced pelo kernel.

**Stack:** Rust

**Padrões:**
- Path confinement rules para agentes
- File operations com permissão verificada

### 1.5 cori-kernel (17 ⭐)

**Descrição:** Kernel seguro para agentes AI fazerem coisas reais.

**Stack:** Rust

**Padrões:**
- Safe kernel design principles
- Capability-based access control

---

## 2. Padrões Portáveis — Análise Detalhada

### 2.1 Sprint 24 — Imediatas/Simples (< 100 LOC)

#### P1: Path Confinement para Skills
**Fonte:** Chisel + ai-jail  
**Descrição:** SkillRegistry verifica se path de arquivo está na allowlist do token antes de permitir operação.  

**Implementação:**
```rust
struct PathPolicy {
    allowed_prefixes: Vec<String>,
    denied_prefixes: Vec<String>,
    mask_patterns: Vec<String>,  // files to replace with empty
}
// SkillRegistry::check_path_access(token, path) -> bool
// Token carrega PathPolicy
```

**LOC estimado:** ~60  
**Arquivos:** `skill-registry/src/policy.rs`  
**Risco:** Mínimo. TrustCache já faz validação similar.

#### P2: Mask Secrets para Skills
**Fonte:** ai-jail `--mask`  
**Descrição:** TrustCache/SkillRegistry pode mascarar paths/env vars sensíveis antes de expor para skills.  

**Implementação:**
```rust
struct MaskPolicy {
    patterns: Vec<String>,  // glob patterns
}
// SkillRegistry::apply_mask(payload, token) -> Vec<u8>
// Substitui ocorrências de paths/pattern por "[REDACTED]"
```

**LOC estimado:** ~50  
**Arquivos:** `skill-registry/src/mask.rs`  
**Risco:** Mínimo.

#### P3: Graduated Enforcement (State Machine)
**Fonte:** InnerWarden (Observe→Warn→Contain→Enforce)  
**Descrição:** SkillRegistry com PolicyState que evolui conforme comportamento.  

**Estados:**
- `Observe`: Skill executa, logs são registrados
- `Warn`: Skill executa, alerta é publicado
- `Contain`: Skill executa com permissões reduzidas
- `Enforce`: Skill bloqueada até revisão

**Implementação:**
```rust
enum PolicyState { Observe, Warn, Contain, Enforce }
struct PolicyStateMachine {
    state: PolicyState,
    violation_count: u64,
    last_violation_tick: u64,
}
// SkillRegistry::check_policy_state(token, skill) -> PolicyDecision
```

**LOC estimado:** ~80  
**Arquivos:** `skill-registry/src/policy.rs`  
**Risco:** Baixo. Apenas adiciona estado ao flow existente.

#### P4: Posture-Aware Alerting
**Fonte:** InnerWarden  
**Descrição:** Skill verifica estado do hardware antes de tomar decisão. Se e1000 link down → não tenta configurar rede.  

**Implementação:**
```rust
// HardwarePosture trait
trait HardwarePosture {
    fn link_up(&self) -> bool;
    fn has_ip(&self) -> bool;
    fn memory_pressure(&self) -> f32;
}
// Skills consultam posture antes de agir
```

**LOC estimado:** ~40  
**Arquivos:** `crates/neural-kernel/src/posture.rs`  
**Risco:** Mínimo.

---

### 2.2 Sprint 25 — Complexidade Média (100-300 LOC)

#### P5: Event→Detector→Response Pipeline
**Fonte:** InnerWarden (núcleo da arquitetura)  
**Descrição:** Pipeline completo de segurança: EventBus → Detector (stateful) → Correlation → Response Skill.  

**Arquitetura:**
```
HW_NET_E1000 → PortScanDetector (stateful: conta conexões por IP)
             → CorrelationRule (se > 10 portas em 1s → port_scan)
             → ResponseSkill (block_ip, kill_process, alert)
```

**Detectores iniciais (3-5):**
1. `PortScanDetector` — Múltiplas portas TCP de um IP em curto período
2. `ArpSpoofDetector` — Múltiplos ARP replies para mesmo IP
3. `PingFloodDetector` — ICMP echo requests excessivos
4. `DhcpStarvationDetector` — Múltiplos DHCP Discover
5. `TimerAnomalyDetector` — Interrupções de timer em intervalo anormal

**Implementação:**
```rust
trait Detector: Send + Sync {
    fn name(&self) -> &str;
    fn process_event(&mut self, event: &Event) -> Option<SecurityAlert>;
}
struct SecurityAlert {
    severity: u8,  // 0-100
    detector: String,
    description: String,
    recommended_action: ResponseAction,
}
```

**LOC estimado:** ~200  
**Arquivos:** `crates/security-pipeline/` (novo crate)  
**Risco:** Médio. Novo crate, integração com EventBus existente.

#### P6: Decision Review + Human Escalation
**Fonte:** InnerWarden  
**Descrição:** Quando detector tem baixa confiança, publica `NEEDS_REVIEW` no EventBus com timeout.  

**Implementação:**
```rust
struct ReviewRequest {
    alert: SecurityAlert,
    confidence: f32,  // 0.0-1.0
    timeout_ticks: u64,
    status: ReviewStatus,  // Pending, AutoResolved, HumanApproved, HumanRejected
}
// Timeout → auto-resolve com registro
// Se severity > HIGH → nunca auto-resolve
```

**LOC estimado:** ~120  
**Arquivos:** `crates/security-pipeline/src/review.rs`  
**Risco:** Baixo.

#### P7: Hash Chain Audit Trail
**Fonte:** InnerWarden  
**Descrição:** EventLog (#231) com SHA-256 chain: cada evento contém hash do anterior.  

**Implementação:**
```rust
struct ChainedEvent {
    event: ConversationEvent,
    prev_hash: [u8; 32],
    cur_hash: [u8; 32],
}
// EventLog::push() calcula hash e encadeia
// EventLog::verify_chain() -> bool
```

**LOC estimado:** ~60  
**Arquivos:** `crates/neural-kernel/src/conversation.rs`  
**Risco:** Mínimo. Apenas adiciona hash ao push existente.

---

### 2.3 Sprint 26+ — Complexidade Alta (300-500 LOC)

#### P8: Knowledge Graph for Security Events
**Fonte:** InnerWarden (11 node types, 53 relations)  
**Descrição:** Grafo em memória para correlação de eventos de segurança.  

**Node Types (6 iniciais):**
- `Process` (pid, name, parent)
- `NetworkEndpoint` (ip, port, protocol)
- `File` (path, size, permissions)
- `Skill` (name, token, calls)
- `Hardware` (type, status, events)
- `User` (token_id, skills, violations)

**Relation Types (~20 iniciais):**
- `connects_to` (Process → NetworkEndpoint)
- `reads` (Process → File)
- `writes` (Process → File)
- `called_by` (Skill → Skill)
- `executed_on` (Skill → Hardware)
- `triggered_alert` (Process → Alert)

**Implementação:**
```rust
struct KnowledgeGraph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    indices: HashMap<NodeType, Vec<usize>>,
}
impl KnowledgeGraph {
    fn add_event(&mut self, event: &Event) -> Vec<CorrelationMatch>;
    fn query(&self, pattern: &GraphPattern) -> Vec<SubGraph>;
    fn export_tensor(&self) -> Tensor;  // para MLP
}
```

**LOC estimado:** ~400  
**Arquivos:** `crates/neural-kernel/src/graph.rs`  
**Risco:** Alto. Consumo de memória, complexidade de query.

#### P9: Cross-Layer Correlation Rules
**Fonte:** InnerWarden (69 regras)  
**Descrição:** Regras que combinam eventos de múltiplas fontes para detectar ataques multi-estágio.  

**Regras iniciais (5):**
1. `ARP Spoof → Port Scan → Data Exfil` — Escalation detection
2. `Timer Anomaly + High CPU → Crypto Miner` — Resource abuse
3. `Failed DHCP + Manual IP → Misconfig` — Network misconfiguration
4. `Keyboard Brute Force → Skill Abuse` — Auth bypass attempt
5. `PCI Enum + MMIO Write → HW Attack` — Hardware tampering

**LOC estimado:** ~300  
**Arquivos:** `crates/security-pipeline/src/correlation.rs`  
**Risco:** Alto. Regras precisam ser testadas contra falsos positivos.

---

### 2.4 Sprint 28+ — Ideia Futura

#### P10: Filesystem como Vector Search
**Fonte:** vexfs  
**Dependência:** Semantic Filesystem (SFS) implementado.  
**Descrição:** Operações de arquivo expõem vector search (xattr, readdir com score).

#### P11: Multi-dialect Vector API
**Fonte:** vexfs (ChromaDB/Qdrant compatível)  
**Dependência:** MemPalace ou SFS com embeddings.  
**Descrição:** API server compatível com ChromaDB para consultas externas.

#### P12: OverlayFS Copy-on-Write
**Fonte:** ai-jail  
**Dependência:** VFS implementada.  
**Descrição:** Writes de agentes vão para overlay separado, originais preservados.

---

## 3. Itens Descartados para neural-os-core

| Padrão | Motivo |
|---|---|
| eBPF monitoring | Requer Linux kernel. Equivalente neural: MMIO monitoring + IRQ counters. |
| Seccomp-bpf | Não temos syscalls. Equivalente: WASM host function restrictions. |
| Kernel module (vexfs) | Módulo Linux instável. Não aplicável a bare-metal. |
| FUSE filesystem | Requer Linux FUSE. Nosso VFS é bare-metal. |
| XDP packet drop | Requer kernel Linux + driver de rede específico. Equivalente: nosso e1000 RX filter. |
| Namespace isolation (PID/UTS/IPC) | Bare-metal só tem um kernel. Isolamento será por Ring (0/1/2). |

---

## 4. InnerWarden — Deep Code Architecture

### 4.1 Data Flow (portável)
```
eBPF events → Ring Buffer (1MB, lock-free)
           → 82 Detectors (stateful, each with own state)
           → Cross-layer Correlation Engine (69 rules)
           → Knowledge Graph (node/relation updates)
           → Algorithm Gate (skip low-sev, private IP)
           → Optional: ONNX Warden (local, ~91MB, 61ms p50)
           → Optional: LLM Triage (OpenAI/Anthropic/Ollama)
           → Skill Executor (block_ip, kill_process, honeypot)
           → Notification Gate (Telegram/Slack/Webhook)
           → SQLite WAL (hash chain audit trail)
```

### 4.2 Equivalente neural-os-core
```
IRQ/timer events → Atomic counters/ring (lock-free via TicketLock)
                → Detectors (stateful, inspects EventBus topics)
                → Correlation (checks multiple EventLog entries)
                → Knowledge Graph (in-memory Vec+HashMap)
                → MLP threshold (Hermes IntentMlp decide)
                → SkillRegistry::execute_skill (response)
                → EventLog (#231, hash chain audit trail)
```

### 4.3 Padrões de Código Específicos

**Detector Stateful Pattern:**
```rust
// InnerWarden style → neural-os-core:
trait Detector {
    fn process(&mut self, ctx: &EventBusContext) -> Option<Alert>;
    fn reset(&mut self);
}
```

**Correlation Rule Pattern:**
```rust
// Combina múltiplos eventos → alerta composto
struct CorrelationRule {
    conditions: Vec<Box<dyn Fn(&[Event]) -> bool>>,
    window_ticks: u64,
    action: ResponseAction,
}
```

**Graduated Enforcement State Machine:**
```rust
enum EnforcementLevel { Observe, Warn, Contain, Enforce }
struct EnforcementState {
    level: EnforcementLevel,
    violations: Vec<(u64, String)>,  // (tick, reason)
    cooldown_until: u64,
}
```

---

## 5. Estimativa de Esforço Total

| Sprint | Itens | LOC | Novos Arquivos |
|---|---|---|---|
| 24 | P1-P4 | ~230 | `policy.rs`, `mask.rs`, `posture.rs` |
| 25 | P5-P7 | ~380 | `security-pipeline/` (3-4 arquivos) |
| 26-27 | P8-P9 | ~700 | `graph.rs`, `correlation.rs` |
| 28+ | P10-P12 | ~800+ | Dependem de SFS/VFS |

**Total viável:** ~1310 LOC (Sprints 24-27)  
**Total futuro:** ~800+ LOC (Sprint 28+, dependente de SFS)

---

## 6. Referências

- InnerWarden: https://github.com/InnerWarden/innerwarden — 159 ⭐, 2057 commits
- ai-jail: https://github.com/akitaonrails/ai-jail — 595 ⭐, 222 commits
- vexfs: https://github.com/lspecian/vexfs — 24 ⭐, 129 commits
- Chisel: https://github.com/ckanthony/Chisel — 12 ⭐
- cori-kernel: https://github.com/cori-do/cori-kernel — 17 ⭐
