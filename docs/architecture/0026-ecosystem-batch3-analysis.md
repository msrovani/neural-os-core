# ADR-0026: Ecosystem Analysis Batch 3 — 12 Repos Portados

**Data:** 2026-06-29
**Contexto:** Análise profunda de 12 repositórios para extrair padrões portáveis ao neural-os-core.

---

## 1. redox-os/redox ★ 16.4k — Capability Scheme Namespace
**Conexão:** Redox usa URLs estilo Plan9 para I/O (tcp://, file://). Cada "scheme" é um daemon com tabela de capacidades.

**Padrão portado:** Nosso EventBus já faz pub/sub, mas lacks um namespace hierárquico. Implementei `Scheme` trait:
```rust
pub trait Scheme: Send {
    fn name(&self) -> &str;        // "gpu", "usb", "pci"
    fn open(&self, path: &str) -> Result<Vec<u8>, &str>;
    fn read(&self, id: u64, buf: &mut [u8]) -> Result<usize, &str>;
}
```
**LOC:** ~100
**Arquivo:** `crates/event-bus/src/scheme.rs`

## 2. theseus-os/Theseus ★ 3.2k — Type-State Agent Lifecycle
**Conexão:** Theseus usa tipo-estado para páginas de memória. Podemos usar para ciclo de vida de agentes.

**Padrão portado:** Agent<Boot> → Agent<Running> → Agent<Faulted> em tempo de compilação:
```rust
pub struct Agent<State> { /* ... */ }
pub struct Boot;
pub struct Running;
pub struct Faulted;
impl Agent<Boot> { fn activate(self) -> Agent<Running> { } }
impl Agent<Running> { fn crash(self) -> Agent<Faulted> { } }
impl Agent<Faulted> { fn reset(self) -> Agent<Boot> { } }
```
**LOC:** ~80
**Arquivo:** `crates/agent-core/src/state.rs`

## 3. embassy-rs/embassy ★ 9.5k — no_std Async Executor
**Conexão:** Nosso scheduler atual é polling loop (cada tick chama cada agente). Embassy usa timer wheel.

**Padrão portado:** Timer wheel para agendamento periódico agendável:
```rust
pub struct TimerWheel {
    slots: [VecDeque<u64>; 64],
    tick: u64,
}
impl TimerWheel {
    pub fn schedule(&mut self, agent_id: u64, delay: u64) { }
    pub fn pop_due(&mut self, now: u64) -> Vec<u64> { }
}
```
**LOC:** ~120
**Arquivo:** `crates/agent-core/src/timer_wheel.rs`

## 4. openai/swarm ★ 21.8k — Agent Handoff Protocol
**Conexão:** Swarm permite agente passar controle para outro agente transparentemente. Perfeito para HermesAgent.

**Padrão portado:** Handoff entre agentes via EventBus:
```rust
pub enum Handoff {
    SwitchTo(String),            // muda para outro agente
    Escalate(String, String),     // escala com contexto
    Delegate(String, Vec<u8>),    // delega tarefa
}
impl HermesAgent {
    fn handoff(&mut self, target: &str, payload: &[u8]) { }
}
```
**LOC:** ~60
**Arquivo:** `crates/neural-kernel/src/hermes.rs` (extensão)

## 5. tock/tock ★ 5.3k — Protected Kernel Capsules
**Conexão:** Tock usa capsules (módulos no_std) com typed MMIO. Nosso driver RTL8139 usa Port<T> raw.

**Padrão portado:** Register abstração com volatile + offset:
```rust
pub struct Register<T: Copy> { ptr: *mut T }
impl Register<u32> {
    pub fn read(&self) -> u32 { unsafe { self.ptr.read_volatile() } }
    pub fn write(&self, v: u32) { unsafe { self.ptr.write_volatile(v) } }
}
pub struct PciBar { pub regs: &'static PciRegisters }
#[repr(C)]
pub struct PciRegisters { vendor: Register<u16>, device: Register<u16>, command: Register<u16>, /* ... */ }
```
**LOC:** ~80
**Arquivo:** `crates/neural-kernel/src/mmio.rs`

## 6. VRSEN/agency-swarm ★ 4.5k — Hierarchical Agent Teams
**Conexão:** Já temos 12 divisões com 147 agentes. Agency Swarm adiciona delegação em cadeia.

**Padrão portado:** TeamAgent com supervisor + membros:
```rust
pub struct TeamAgent<S, M> {
    supervisor: S,
    members: Vec<M>,
    delegation_chain: Vec<DelegationRule>,
}
```
**LOC:** ~100 (já temos via SpecialistAgent)
**Arquivo:** `crates/neural-kernel/src/agents.rs` (já integrado)

## 7. pydantic/pydantic-ai ★ 18k — Type-safe Tool Manifests
**Conexão:** Nosso McpManifest é manual. Pydantic gera schemas automaticamente.

**Padrão portado:** Derive macro opcional para gerar McpManifest:
```rust
// Futuro: #[derive(SkillManifest)] que gera nome/descrição/args
```
**LOC:** ~0 (conceito, requer proc-macro)
**Arquivo:** conceitual

## 8. browser-use/browser-use ★ 101k — Context Tree Extraction
**Conexão:** Extrai "árvore de contexto" (DOM → LLM). Análogo à nossa árvore de dispositivos PCI.

**Padrão portado:** Device tree context para LLM:
```rust
pub fn get_device_tree() -> String {
    // PCI bus → device → BARs → capabilities formatado para LLM
}
```
**LOC:** ~80 (já temos em HwRegistry::llm_context())
**Arquivo:** `crates/neural-kernel/src/hw_agents.rs` (já implementado)

## 9. raga-ai-hub/RagaAI-Catalyst ★ 16k — Execution Graph Tracing
**Conexão:** Cada skill call vira um span com parent/child IDs. Essencial para debugar o Hermes Cognitive.

**Padrão portado:** Span tracer com ring buffer:
```rust
pub struct Span {
    id: u64, parent: Option<u64>,
    agent: String, skill: String,
    start_tick: u64, end_tick: u64,
    status: SpanStatus,
}
pub struct Tracer { spans: [Span; 256], next_id: u64 }
impl Tracer {
    pub fn start_span(&mut self, parent: Option<u64>, agent: &str, skill: &str) -> u64 { }
    pub fn end_span(&mut self, id: u64, status: SpanStatus) { }
    pub fn trace_tree(&self) -> String { }
}
```
**LOC:** ~150
**Arquivo:** `crates/neural-kernel/src/tracer.rs`

## 10. micro/go-micro ★ 22.9k — Registry Discovery
**Conexão:** Nosso AgentRegistry já registra agentes. go-micro adiciona endpoints discovery.

**Padrão portado:** Endpoints como parte do AgentManifest:
```rust
pub struct AgentManifest {
    pub name: &'static str,
    pub kind: AgentKind,
    pub schedule: ScheduleKind,
    pub endpoints: &'static [&'static str], // skills que este agente expõe
}
```
**LOC:** ~30 (modificação no AgentManifest)
**Arquivo:** `crates/agent-core/src/lib.rs`

## 11. kyegomez/swarms ★ 6.9k — Tree-of-Thought Orchestration
**Conexão:** Nosso CouncilSkill já faz 3 vozes. Swarms permite orquestração recursiva.

**Padrão portado:** OrchestratorAgent que spawna sub-agentes efêmeros:
```rust
pub struct OrchestratorAgent {
    pool: Vec<u64>, // agent IDs reutilizáveis
}
impl OrchestratorAgent {
    pub fn decompose(&mut self, task: &str) -> Vec<SubTask> { }
    pub fn collect(&mut self, results: Vec<SubTaskResult>) -> String { }
}
```
**LOC:** ~120
**Arquivo:** `crates/neural-kernel/src/orchestrator.rs`

## 12. TransformerOptimus/SuperAGI ★ 16k+ — Tool Marketplace
**Conexão:** Skills podem ser avaliadas por performance. Scheduler escolhe a melhor.

**Padrão portado:** Skill scoring table:
```rust
pub struct SkillScore {
    agent: String, skill: String,
    avg_ticks: u64, success_rate: f32, calls: u32,
}
pub struct SkillMarket { scores: BTreeMap<(String, String), SkillScore> }
impl SkillMarket {
    pub fn record(&mut self, agent: &str, skill: &str, ticks: u64, ok: bool) { }
    pub fn best_agent(&self, skill: &str) -> Option<&str> { }
}
```
**LOC:** ~80
**Arquivo:** `crates/neural-kernel/src/skill_market.rs`

---

## Resumo
| Repo | Estrelas | Padrão | LOC | Arquivo | Status |
|---|---|---|---|---|---|
| redox-os/redox | 16.4k | Scheme namespace | 100 | scheme.rs | 🆕 |
| theseus-os/Theseus | 3.2k | Type-state lifecycle | 80 | state.rs | 🆕 |
| embassy-rs/embassy | 9.5k | Timer wheel | 120 | timer_wheel.rs | 🆕 |
| openai/swarm | 21.8k | Agent handoff | 60 | hermes.rs ext | 🆕 |
| tock/tock | 5.3k | Typed MMIO registers | 80 | mmio.rs | 🆕 |
| agency-swarm | 4.5k | TeamAgent | 100 | agents.rs | ✅ já temos |
| pydantic-ai | 18k | Type-safe manifests | — | conceitual | ⏳ |
| browser-use | 101k | Context tree | 80 | hw_agents.rs | ✅ já temos |
| RagaAI Catalyst | 16k | Span tracing | 150 | tracer.rs | 🆕 |
| go-micro | 22.9k | Endpoints discovery | 30 | agent-core | 🆕 |
| kyegomez/swarms | 6.9k | Tree-of-thought | 120 | orchestrator.rs | 🆕 |
| SuperAGI | 16k+ | Skill marketplace | 80 | skill_market.rs | 🆕 |

**Total:** ~820 LOC novos em 8 novos arquivos.
