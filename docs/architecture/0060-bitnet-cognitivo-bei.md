# ADR-0060: BitNet Cognitivo — Ecossistema BEI (A.1–A.8)

**Data:** 2026-07-21  
**Status:** Proposed — plano de implementação aprovado; auditoria de ADRs/IDEA_BANK completa  
**Lifecycle:** `por_fazer`  
**Sprint:** v1.9.5+ → gate v2.0.0  
**Ideias:** #470 (BEI main), #471 (Celular), #472 (Evolução), #473 (Memória), #474 (Afeto), #475 (Supervisor), #476 (Economia), #477 (MoE Dinâmico), #478 (MPMC)  
**Depreca:** #136 (LLM decide memory tier — substituído por política determinística BEI)  
**Absorve:** #218, #224, #281d (A.1 — tiers de memória existentes); #314, #314c, #314d (A.4 — SleepCycle morphogenesis); #169, #375, #449, #468 (A.6 — compression tiers + FitPolicy); ADR-0047-HMI §7 Soul Mirror (A.7 — orb emocional); #315.13 Ego Layer, #315.17 Babel-Index, #168 PonderNet, #190 7-phase loop, #199 IterationBudget (A.8 — meta-cognição); ADR-0036 EmotionState (A.7 — classifier frontend); ADR-0046 Q4_0/Q5_0/Q8_0 (A.6 — dequant tiers)  
**Propósito:** Transformar o BitNet de um motor de inferência estático em um **Organismo Cognitivo Operacional** — células concorrentes, especialistas que nascem/morrem, memória hierárquica L0–L7, afeto computacional, supervisão meta-cognitiva — tudo em `no_std` Rust bare-metal.

**Nome de produto:** **BEI** (BitNet Ecosystem Intelligence). Também referido como **BitNet Cognitivo**.

---

## 1. Contexto

O Neural-OS-Core atingiu maturidade operacional com o marco v1.8.6 (K³CHJ wire + k-HAL). O BitNet 1.58b ternário (`k_ai::ternary`), o MoE de especialistas compartilhados (`cortex::moe`), o pipeline de inferência (`cortex::cortex::TransformerModel`), e o roteador de intenções (`hermes::cognitive_bridge`) formam uma base sólida. No entanto, a arquitetura atual ainda é **monolítica por design**:

- O TransformerModel é um bloco único — não há concorrência interna entre sub-modelos
- A memória é plana (VFS flat + BGE embedding + 4-tier pipeline #218) — sem hierarquia temporal de 8 níveis
- O afeto é classificação discreta (`EmotionState` 7 classes, ADR-0036) — não vetor contínuo que modula decisões
- SleepCycle (#314) faz pruning/consolidação batch — mas não há morfogênese contínua por entropia
- Não há ciclo de vida de especialistas — Trinity (#311) é MoE estático sem birth/merge/split
- Ego Layer (#315.13) + Babel-Index (#315.17) tocam meta-cognição mas sem supervisor executivo unificado

### 1.1 Mapeamento BEI × Stack Existente

| Volume BEI | Crate | Módulo | Base existente | Gap |
|-----------|-------|--------|----------------|-----|
| A.1 Memory-Native | `hermes` | `memory/` L0–L7 | 4-tier pipeline (#218 ✅ v0.56), Atkinson-Shiffrin (#224), MHI ARC (#281d) | Estender para 8 tiers; unified facade |
| A.2 Cellular BitNet | `cortex` | `cellular` | Actor Registry (#209), SPSC ring (#319) | **Micro-atores com MPMC** — nada similar existe |
| A.3 Dynamic MoE | `cortex` | `moe` (ext) | Trinity MoE estático (#311 ✅), BitNetTrainer (#312b), SkillOpt (#411) | Birth/merge/split em runtime — nada similar existe |
| A.4 Living + A.5 Sparse | `cortex` | `evolution` | SleepCycle 5-fase (#314 🟡), EWC (#314c), Pruning (#314d) | Morfogênese contínua (hoje é batch/sonho) |
| A.6 Expert Compression | `k_ai` | `economy` | Q4_0/Q5_0/Q8_0 (ADR-0046 ✅), VQ (#169 ✅), FitPolicy (#468 ✅) | Unified CompressionTier enum + BudgetManager |
| A.7 Emotional BitNet | `hermes` | `affect` | EmotionState 7-class (ADR-0036 🟡), Soul Mirror (ADR-0047-HMI ✅ PoC) | Vetor contínuo 5D + modulation routing |
| A.8 Meta-Cognitive | `hermes` | `executive` | Ego Layer (#315.13 🟡), Babel-Index (#315.17 ✅), PonderNet (#168), 7-phase loop (#190), IterationBudget (#199 ✅) | Supervisor executivo unificado |

### 1.2 Deprecação

- **#136** (LLM decide memory tier): substituído por política determinística de tiers auto-gerenciados (BEI usa regras, não LLM, para alocação de memória). Marcado como `substituida` → ADR-0060.

---

## 2. Decisão

Implementar a arquitetura BEI em **7 ondas sequenciais**, cada uma com `cargo check --release` ao final, sem quebrar funcionalidade existente. Cada módulo novo nasce com implementação completa: structs, trait implementations, self-test, e integração.

### 2.1 Mapeamento de anéis (ADR-0041 §9)

```
k-nano (R0) ── MPMC queue
   │
k-ai (R2) ─── economy (compression tiers + BudgetManager)
   │
cortex (R2) ─ cellular, evolution, moe-dinâmico
   │
hermes (R3) ─ memory/L0–L7, affect (Soul Mirror FE), executive
   │
jarbas (R3) ─ Soul Mirror (orb emocional via AffectVector)
```

Nenhum módulo novo toca BAR de device ou MMIO.

### 2.2 Restrições de ambiente (`no_std` bare-metal)

1. **Zero dependências externas novas** — tudo com `alloc` + primitivas existentes
2. **Comunicação entre células via MPMC** — sem `std::sync::mpsc`
3. **Persistência via VFS** — sem SQLite/Postgres
4. **Matemática SIMD** (AVX2/AVX-512 via `core::arch`) — sem GPU driver
5. **Orçamento de hardware** medido em bytes + ciclos

### 2.3 Non-goals

- Execução GPU para células (Layer S/HW)
- Substituir SleepCycle — BEI adiciona trigger contínuo; SleepCycle mantém batch
- Substituir Trinity MoE — BEI adiciona lifecycle dinâmico; Trinity mantém routing fixo
- Substituir Soul Mirror existente — BEI adiciona AffectVector como fonte; Soul Mirror renderiza

---

## 3. Onda 0 — Infraestrutura de Comunicação (k_nano)

**Arquivo novo:** `crates/k_nano/src/sync/mpmc.rs`  
**Ideia:** #478  
**Base:** SPSC queue existente (`k_nano::smp::spsc`, #319) estendida para multi-produtor

### 3.1 MPMC Queue

Ring buffer lock-free multi-producer multi-consumer com slots CAS:

```rust
pub struct MpmcQueue<T> {
    buffer: RawVec<Slot<T>>,
    capacity: usize, mask: usize,
    enqueue_pos: AtomicUsize,
    dequeue_pos: AtomicUsize,
}

struct Slot<T> {
    state: AtomicUsize,  // EMPTY=0, STORING=1, READY=2, LOADING=3
    data: MaybeUninit<T>,
}
```

Protocolo: produtor `fetch_add` `enqueue_pos` → CAS `EMPTY→STORING` → escreve → `STORING→READY`. Consumidor `fetch_add` `dequeue_pos` → CAS `READY→LOADING` → lê → `LOADING→EMPTY`.

### 3.2 Herança

- Reaproveita padrão de `SpscQueue` (power-of-2 capacity, `try_send`/`try_recv` assinatura)
- Diferencial: slots com estado atômico permitem N produtores sem condição de corrida

---

## 4. Onda 1 — Economia de Recursos (k_ai)

**Arquivos novos:** `crates/k_ai/src/economy.rs`, `crates/k_ai/src/expert_lifecycle.rs`  
**Ideias:** #476, #477  
**Absorve:** ADR-0046 (Q4_0/Q5_0/Q8_0), #169 (VQ codebook), #468 (FitPolicy), #199 (IterationBudget)  
**Depreca:** #136 (LLM decide tier — política determinística)

### 4.1 `economy.rs` — CompressionTier + BudgetManager

Tiers baseados em implementações existentes:

| Tier | Bits/peso | Fonte | Status |
|------|-----------|-------|--------|
| Binary1bit | 1 | TernaryTensor packing estendido | Novo |
| Ternary2bit | 2 | `k_ai::ternary` + Codebook VQ (#169) | ✅ Existente |
| Int4 | 4 | ADR-0046 Q4_0 dequant | ✅ Existente |
| Int5 | 5 | ADR-0046 Q5_0 dequant | ✅ Existente |
| Int8 | 8 | ADR-0046 Q8_0 dequant | ✅ Existente |
| Bf16 | 16 | GGUF F16 | ✅ Existente |
| F32 | 32 | `cortex::tensor::Tensor` | ✅ Existente |

```rust
pub enum CompressionTier {
    Binary1bit, Ternary2bit, Int4, Int5, Int8, Bf16, F32,
}

pub struct BudgetManager {
    max_memory_bytes: usize,
    used_memory_bytes: AtomicUsize,
    tier_policy: TierPolicy,  // {Conservative, Balanced, Performance}
    temperature: f32,         // 0.0..1.0 — histerese
}
```

Política baseada em `FitPolicy` (#468) + `IterationBudget` (#199):
- `can_promote(current, target, size)` → verifica orçamento com grace cycle
- `suggest_tier(importance, age, memory_pressure)` → `CompressionTier`
- `pressure()` → f32 0.0..1.0

### 4.2 `expert_lifecycle.rs` — ExpertLifecycleManager

```rust
pub struct ExpertMetadata {
    id: u64, name: String, birth_tick: u64,
    hits: u64, avg_confidence: f32,
    entropy: f32, tier: CompressionTier, last_active: u64,
}
```

Métodos: `register`, `record_hit`, `candidates_for_merge`, `candidates_for_split`, `stale_experts`.

Birth usa `BitNetTrainer` (ADR-0033, #312b) como mecanismo de fine-tuning on-device. Merge usa `SkillOpt` (#411, MS Research) como engine de otimização.

---

## 5. Onda 2 — Células Cognitivas + Evolução (cortex)

**Arquivos novos:** `crates/cortex/src/cellular.rs`, `crates/cortex/src/evolution.rs`  
**Ideias:** #471, #472  
**Absorve:** #314 SleepCycle (pruning/consolidação batch), #314c EWC, #314d Synaptic Homeostasis, #209 Actor Registry

### 5.1 `cellular.rs` — CognitiveCell + CellNetwork

Células são micro-atores que se comunicam via MPMC. Tipo de célula determina função:

```rust
pub enum CellType { Reasoning, Memory, Perception, Motor }

pub struct CognitiveCell {
    id: u64, cell_type: CellType,
    state: CellState,  // Idle | Active | Blocked | Dead
    inbox: mpmc::Receiver<CellMessage>,
    outbox: mpmc::Sender<CellMessage>,
    weights_handle: Option<WeightsRef>,
}
```

`CellNetwork` como grafo direcionado + `CellScheduler` round-robin com budget.

Estados de lifecycle baseados em `TypedAgent<Boot|Running|Faulted>` (#280b):
- `Idle`: célula viva sem trabalho
- `Active`: processando mensagem
- `Blocked`: aguardando resposta de outra célula
- `Dead`: marcada para prune

### 5.2 `evolution.rs` — PlasticityController + SleepCycle Bridge

**Novidade:** BEI adiciona trigger contínuo por entropia. **Batch:** SleepCycle (#314) mantém fases REPLAY→DREAM→CONSOLIDATE(EWC)→PRUNE→REFLECT.

```
PlasticityController (contínuo, BEI)
  ├── region_entropy > growth_threshold → spawn célula
  ├── region_activation < prune_threshold → marca para morte
  └── feed metrics para SleepCycle REFLECT

SleepCycle (batch, #314 existente)
  ├── DREAM → gera variações sintéticas
  ├── CONSOLIDATE(EWC) → protege skills existentes
  ├── PRUNE → zera pesos fracos (~18% redução, #314d)
  └── REFLECT → confidence tracking + gaps
```

```rust
pub struct PlasticityController {
    region_entropy: Vec<f32>,      // contínuo, tick a tick
    region_error_rate: Vec<f32>,
    region_activation: Vec<f64>,
    growth_threshold: f32,
    prune_threshold: f32,
}
```

Métodos: `observe(region, entropy, error, activated)`, `should_grow(region)`, `should_prune(region)`.

Integração com SleepCycle: `PlasticityController::observe` alimenta `#314c Consolidation` (EWC). `SleepCycle::PRUNE` executado via `PlasticityController::should_prune`. 

---

## 6. Onda 3 — MoE Dinâmico (cortex)

**Modificação:** `crates/cortex/src/moe.rs` (extensão do `MoELayer` existente)  
**Ideias:** #477  
**Absorve:** ADR-0033 BitNetTrainer (birth via fine-tuning), #411 SkillOpt (merge engine), #445 Evolve WASM hot-swap

```rust
pub struct DynamicMoE {
    base: MoELayer,
    lifecycle: ExpertLifecycleManager,
    pending_births: Vec<ExpertBirth>,
    pending_merges: Vec<(usize, usize)>,
    pending_splits: Vec<usize>,
}
```

- **Birth** (`add_expert`): novo expert criado via fine-tuning rápido (100 exemplos, ADR-0033 BitNetTrainer) ou clonagem com ruído
- **Merge** (`merge_experts`): usa SkillOpt (#411) para gerar add/delete/replace edits nos pesos, aceitos só se melhoram score de validação
- **Split** (`split_expert`): clona expert com ruído gaussiano, divide rotas ao meio no router
- **Hot-swap** (`remove_expert` + `add_expert`): usa mecanismo de `Evolve WASM` (#445) — ledger + sandbox test + rollback

Birth triggers: `ExpertLifecycleManager::candidates_for_split` (entropia alta) ou `PlasticityController::should_grow` (região com erro crescente).

---

## 7. Onda 4 — Memória Hierárquica L0–L7 (hermes)

**Arquivo novo:** `crates/hermes/src/memory/mod.rs` (subdiretório)  
**Ideia:** #473  
**Absorve:** #218 4-Tier Pipeline (✅ v0.56), #224 Atkinson-Shiffrin (3 tiers), #281d MHI ARC suggest_tier, #282h auto tier migration, #207 MemoryProvider trait  
**Depreca:** #136 (LLM decide tier — política determinística)

### 7.1 Os 8 Tiers

| Nível | Nome | Capacidade | Persistência | Latência | Política de promoção |
|-------|------|-----------|--------------|----------|---------------------|
| **L0** | Cache | 1KB | Volátil (tick) | O(1) | Hit rate ≥ 3/tick |
| **L1** | Working | 64KB | Volátil | O(log n) | LRU, ARC (#281d) |
| **L2** | Short | 1MB | Volátil (TTL 60s) | O(log n) | Atkinson-Shiffrin freq (#224) |
| **L3** | Episodic | 16MB | VFS-backed | O(1) I/O | 4-tier pipeline (#218) |
| **L4** | Semantic | BTreeMap + BGE | VFS + index | O(log n) + emb | BGE similarity |
| **L5** | Procedural | Skills/WASM | VFS persistente | O(1) I/O | skill_loader |
| **L6** | VFS | memory_store | VFS + FAT32 | O(1) I/O | memory_store existente |
| **L7** | Archive | Disco FAT32 | Sob demanda | lento | ARC promotion (#281d) |

### 7.2 Arquitetura

```rust
pub enum MemoryLevel { L0 = 0, L1 = 1, L2 = 2, L3 = 3, L4 = 4, L5 = 5, L6 = 6, L7 = 7 }

pub trait MemoryTier {
    fn read(&self, key: &str) -> Option<Vec<u8>>;
    fn write(&mut self, key: &str, value: &[u8]);
    fn level(&self) -> MemoryLevel;
    fn capacity(&self) -> usize;
    fn latency_estimate(&self) -> u64;
}

pub struct MemoryStore {
    tiers: [Option<Box<dyn MemoryTier>>; 8],
    promote_on_read: bool,  // ARC policy (#281d)
}
```

**Herança do 4-tier pipeline (#218):** EventBus topics para transições entre tiers. Working→Episodic→Semantic→Procedural permanecem como `EventBus::publish(MEMORY_TIER_X)`.

**Herança Atkinson-Shiffrin (#224):** Frequência de acesso determina promoção L3→L2. Threshold: `access_count > 3 / minuto` → promove.

**Auto tier migration (#282h):** MhiScheduler existente faz rebalanceamento periódico entre L3/L6/L7 baseado em pressão de memória.

---

## 8. Onda 5 — Sistema de Afeto (hermes)

**Arquivos novos:** `crates/hermes/src/affect.rs` + modificação em `crates/hermes/src/cognitive_bridge.rs`  
**Ideia:** #474  
**Absorve:** ADR-0036 EmotionState (classifier frontend), ADR-0047-HMI §7 Soul Mirror (visual mapping)

### 8.1 AffectVector (contínuo 5D)

```rust
pub struct AffectVector {
    // PAD clássico
    pub valence: f32,      // -1..1
    pub arousal: f32,      // 0..1
    pub dominance: f32,    // 0..1

    // BEI estendido
    pub uncertainty: f32,  // 0..1
    pub urgency: f32,      // 0..1
    pub fatigue: f32,      // 0..1
    pub curiosity: f32,    // 0..1
    pub coherence: f32,    // 0..1
}
```

### 8.2 EmotionState como classifier frontend

`EmotionState` (ADR-0036) classifica texto de entrada em 7 emoções discretas + intensidade + sarcasmo + urgência. O `AffectRegulator` converte classificação discreta em vetor contínuo:

```rust
impl From<EmotionState> for AffectVector {
    fn from(e: EmotionState) -> Self {
        // Joy → +valence, +arousal
        // Sadness → -valence, -arousal, +fatigue
        // Anger → -valence, +arousal, -coherence
        // Fear → -valence, +arousal, +uncertainty
        // Surprise → +arousal, +curiosity
        // Disgust → -valence, -dominance
        // Neutral → valores neutros
    }
}
```

### 8.3 AffectRegulator

- `incorporate(event: AffectEvent)` — modifica vetor baseado em sucesso/erro/timeout/novidade
- `decay()` — todos valores decaem lentamente ao neutro
- `affect_modulated_score(raw_score, affect)` → score de roteamento ajustado:

| Condição | Efeito no roteamento |
|----------|---------------------|
| `urgency > 0.7` | Rota direta, sem deliberação |
| `uncertainty > 0.6` | Prefere LLM em vez de skill |
| `fatigue > 0.8` | Resposta curta, sem reflexão |
| `curiosity > 0.7` | Favorece pesquisa/exploração |
| `coherence < 0.3` | Dispara contradiction_detect |

### 8.4 Visual: Soul Mirror + AffectVector

O `SoulMirror` (ADR-0047-HMI §7) existente consome `AffectVector` e mapeia para 7 métricas visuais do Orb. A integração substitui as variáveis soltas (`HERMES_EMOTION`, `ACTIVE_AGENTS`) por uma fonte única:

```rust
// SoulMirror lê AffectVector e atualiza avatar
impl SoulMirror {
    pub fn from_affect(affect: &AffectVector) -> SoulMirrorUpdate {
        SoulMirrorUpdate {
            color: affect.valence_to_rgb(),           // cor → valência
            pulse: affect.arousal_to_pulse(),         // pulso → arousal
            size: affect.dominance_to_size(),         // tamanho → dominância
            rings: affect.curiosity_to_rings(),       // anéis → curiosidade
            rotation: (affect.urgency * 360.0) as u32,// rotação → urgência
            // ... demais mapeamentos
        }
    }
}
```

---

## 9. Onda 6 — Supervisor Meta-Cognitivo (hermes)

**Arquivo novo:** `crates/hermes/src/executive.rs`  
**Ideia:** #475  
**Absorve:** #315.13 Ego Layer (meta-cognitive identity), #315.17 Babel-Index (entropy/contradiction monitor), #168 PonderNet (dynamic inference stop), #190 7-phase loop (algorithm framework), #199 IterationBudget (grace cycle)

### 9.1 ExecutiveSupervisor

```rust
pub struct ExecutiveSupervisor {
    // Ego Layer (#315.13)
    pub confidence_by_domain: BTreeMap<String, f32>,  // "sei o que sei/não sei"
    pub domain_boundaries: Vec<DomainBoundary>,

    // Babel-Index (#315.17)
    pub entropy_monitor: EntropyMonitor,       // contradiction_rate, staleness
    pub collapse_warning: bool,

    // PonderNet (#168)
    pub inference_budget_dynamic: u64,         // quantos ciclos rodar
    pub stop_threshold: f32,                   // confiança mínima para parar

    // IterationBudget (#199)
    pub max_poll_cycles: u64,
    pub grace_cycles: u64,

    // 7-phase loop (#190)
    pub phase: LoopPhase,
}
```

### 9.2 Integração

O supervisor executa **antes** do router Hermes, seguindo o ciclo de 7 fases (#190):

```
1. OBSERVE  → supervisor.tick(route_decision, affect, memory)
2. THINK    → contradiction_detect() + inference_budget()
3. PLAN     → SupervisorVerdict (Proceed | Delay | Preempt | Escalate)
4. BUILD    → (router normal — se Proceed)
5. EXECUTE  → (skill/LLM exec — se Proceed)
6. VERIFY   → (SelfCritique existente)
7. LEARN    → affect.incorporate(success/fail) + update confidence_by_domain
```

**Ego Layer (#315.13):** mantém `confidence_by_domain` — mapa de quão confiável o sistema é em cada domínio. Se confiança < 0.3, `SupervisorVerdict::Escalate` para humano.

**Babel-Index (#315.17):** `EntropyMonitor` rastreia contradiction_rate e staleness_index. Se contradiction_rate > 0.2/100 ticks, dispara `SupervisorVerdict::Delay` para consolidação.

**PonderNet (#168):** `inference_budget_dynamic` ajusta quantas inference units gastar. Se confiança da resposta > `stop_threshold` após N ciclos, para cedo.

### 9.3 Veredito

```rust
pub enum SupervisorVerdict {
    Proceed,
    ProceedWithBudget(u64),
    Delay { reason: &'static str, until_tick: u64 },
    Preempt { reason: &'static str, alternative: RouteDecision },
    Escalate { reason: &'static str },
}
```

---

## 10. Onda 7 — Soul Mirror: Orb Afetivo (jarbas)

**Modificações:** `crates/jarbas/src/display/avatar.rs`, `crates/jarbas/src/display/soul_mirror.rs`  
**Ideia:** #474 (visual)  
**Absorve:** ADR-0047-HMI §7 Soul Mirror (completo, ✅ PoC), §7.3 8 estados de avatar

### 10.1 SoulMirror (ADR-0047-HMI §7)

Em vez da Onda 7 simplificada (3 modos de emissão), implementar **SoulMirror completo** conforme especificado em ADR-0047-HMI §7.2:

```rust
pub struct SoulMirror {
    avatar: &'static mut JarvisAvatar,
    affect: &'static AffectVector,  // lido via EventBus ou shared state
}
```

Mapeamento (ADR-0047-HMI tabela §7.1):

| Aspecto visual | Dado BEI | Range |
|---------------|----------|-------|
| Cor predominante | AffectVector.valence | Azul(-1) → Verde(0) → Laranja(+1) |
| Velocidade de pulso | AffectVector.arousal | 200ms(0) → 1000ms(1) |
| Tamanho | AffectVector.dominance | 20px(0) → 40px(1) |
| Anéis concêntricos | Inference budget (PonderNet) | 0–8 anéis |
| Explosões | HEALTH_ISSUE ou contradiction | Discretas |
| Rotação | Supervisor phase | 5 fases × 72° |
| Brilho | AffectVector.coherence | 0.0–1.0 |

### 10.2 8 Estados de Avatar (ADR-0047-HMI §7.3)

```rust
pub enum AvatarState {
    Idle,       // padrão — pulso lento azul
    Listening,  // ciano pulsante
    Processing, // laranja girando
    Speaking,   // verde com ondas
    Thinking,   // roxo com anéis (inferência em andamento)
    Dreaming,   // índigo partículas lentas (SleepCycle ativo)
    Alert,      // vermelho rápido (HEALTH_ISSUE ou contradição)
    Updating,   // amarelo rotação (boot/update)
}
```

---

## 11. Plano de Ondas — Revisado

| Onda | O quê | Crate | Arquivos | LOC | Incorpora |
|------|-------|-------|----------|-----|-----------|
| 0 | MPMC queue | k_nano | `sync/mpmc.rs` | ~120 | #319 SPSC pattern |
| 1 | Economy + Lifecycle | k_ai | `economy.rs`, `expert_lifecycle.rs` | ~350 | ADR-0046 tiers, #169 VQ, #468 FitPolicy, #199 IterationBudget |
| 2 | Cellular + Evolution | cortex | `cellular.rs`, `evolution.rs` | ~500 | #209 Actor Registry, #314 SleepCycle, #314c EWC, #314d Pruning |
| 3 | MoE Dinâmico | cortex | `moe.rs` (ext) | ~200 | ADR-0033 BitNetTrainer, #411 SkillOpt, #445 Evolve hot-swap |
| 4 | Memória L0–L7 | hermes | `memory/*.rs` (8) | ~600 | #218 4-tier, #224 Atkinson-Shiffrin, #281d ARC, #282h migration |
| 5 | Afeto + Modulation | hermes | `affect.rs` + `cognitive_bridge.rs` | ~300 | ADR-0036 EmotionState, ADR-0047-HMI Soul Mirror mapping |
| 6 | Supervisor | hermes | `executive.rs` | ~250 | #315.13 Ego Layer, #315.17 Babel-Index, #168 PonderNet, #190 7-phase |
| 7 | Soul Mirror | jarbas | `display/soul_mirror.rs` + `avatar.rs` | ~150 | ADR-0047-HMI §7 (completo), §7.3 8 estados |
| | **Total** | | **~16 arquivos** | **~2470** | |

### Dependências

```
Onda 0 → Onda 1 → Onda 2 → Onda 3
                              │
                              └→ Onda 4 → Onda 5 → Onda 6
                                               │
                                               └→ Onda 7
```

---

## 12. Deprecações e Substituições

| Item | Estado | Substituído por | Ação |
|------|--------|----------------|------|
| #136 (LLM decide memory tier) | ⏳ defer | BEI A.6 BudgetManager (política determinística) | Marcar `substituida` → ADR-0060 |
| ADR-0047-HMI §7 Soul Mirror | ✅ PoC | BEI Onda 7 (expandido com AffectVector) | Absorvido (código existente mantido) |
| ADR-0036 EmotionState | 🟡 Sprint 78 | BEI A.7 AffectVector (classifier como frontend) | Absorvido (classifier mantido) |
| #314 SleepCycle | 🟡 Sprint 82 | BEI A.4 evolução (SleepCycle como batch, BEI como contínuo) | Fundido (ambos coexistem) |
| #315.13 Ego Layer | 🟡 Sprint 79 | BEI A.8 ExecutiveSupervisor | Absorvido (como módulo do supervisor) |
| #315.17 Babel-Index | ✅ MVP | BEI A.8 contradiction_detect | Absorvido (como detector do supervisor) |
| #168 PonderNet | 🟡 Sprint 27 | BEI A.8 inference_budget | Absorvido (como policy) |
| #190 7-phase loop | 🟡 Sprint 24 | BEI A.8 supervisor framework | Absorvido (como ciclo do supervisor) |
| #199 IterationBudget | ✅ Sprint 24 | BEI A.6 BudgetManager | Absorvido (como grace cycle) |
| #169 Codebook VQ | ✅ Sprint 28 | BEI A.6 CompressionTier::Ternary2bit | Absorvido (como técnica) |

---

## 13. Riscos e Mitigações

| Risco | Prob | Impacto | Mitigação |
|-------|------|---------|-----------|
| MPMC ABA | Baixa | Alto | Tags ABA + teste 4P/4C exaustivo |
| SleepCycle × PlasticityController conflito | Média | Médio | Plasticity alimenta SleepCycle REFLECT; SleepCycle executa PRUNE |
| EmotionState discreto × AffectVector contínuo | Baixa | Baixo | `From<EmotionState>` conversion bridge |
| Soul Mirror já existe em PoC — diferença sutis | Média | Baixo | Manter código PoC; adicionar AffectVector como fonte |
| Ego Layer (#315.13) não implementado | Média | Médio | Começar com Babel-Index (#315.17 ✅) + IterationBudget (#199 ✅) |

---

## 14. Referências

### ADRs
- **ADR-0033:** On-Device Micro-Learning (BitNetTrainer, fine-tuning on-device)
- **ADR-0036:** JARVIS Unified Interaction Layer (EmotionState, 7 emoções)
- **ADR-0041:** K³CHJ Capability Rings (anéis R0–R3)
- **ADR-0042:** Adequação Boot → K³CHJ (wire crates)
- **ADR-0046:** AirLLM GGUF Streaming (Q4_0/Q5_0/Q8_0 dequant)
- **ADR-0047:** Latent Space AI-OS (LatentBus, Evolve, Probe)
- **ADR-0047-HMI:** Neural Desktop (H4 Soul Mirror — **absorvido**)
- **ADR-0057:** Compute Dispatch SMP+GPU+NPU
- **ADR-0058:** Generative Card Desktop
- **ADR-0059:** Runtime App Factory

### IDEIA_BANK
- **#169:** Codebook Compression (VQ ternário 97.56%)
- **#168:** PonderNet dynamic stop
- **#190:** Algorithm loop 7 fases
- **#199:** IterationBudget com Grace Cycle
- **#207:** MemoryProvider + MemoryManager Trait
- **#209:** Actor Registry com Permission Model
- **#218:** 4-Tier Memory Consolidation (**✅ v0.56**)
- **#224:** Atkinson-Shiffrin Cognitive Memory
- **#280b:** TypedAgent lifecycle
- **#281d:** MHI ARC suggest_tier
- **#282h:** Auto tier migration
- **#311:** Trinity Model Hub (MoE estático)
- **#312b:** Fine-tuning on-device (CPU)
- **#314:** SleepCycle Agent (5 fases, 🟡 Sprint 82)
- **#314c:** Elastic Weight Consolidation
- **#314d:** Synaptic Homeostasis (Pruning)
- **#315.13:** Ego Layer (meta-cognitive identity)
- **#315.17:** Babel-Index (entropy/contradiction monitor)
- **#319:** SPSC ring lockless
- **#397:** SleepCycle guard rails
- **#411:** SkillOpt (MS Research, merge engine)
- **#445:** Evolve WASM hot-swap
- **#468:** FitPolicy Neural (budget/packing)

### Código existente
- `k_ai::ternary` — base CompressionTier::Ternary2bit
- `cortex::moe::MoELayer` — base DynamicMoE
- `hermes::cognitive_bridge` — base modulation + trust
- `hermes::memory_store` — base L5/L6
- `hermes::self_evolve` — base ciclo observe→generate→register
- `k_nano::smp::spsc::SpscQueue` — padrão MPMC
- `jarbas::display::avatar::JarvisAvatar` — base Soul Mirror
