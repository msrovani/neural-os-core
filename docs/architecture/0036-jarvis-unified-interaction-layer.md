# ADR-0036: J.A.R.V.I.S. — Camada Interativa Unificada do Neural AIOS

**Data:** 2026-07-04
**Status:** Accepted
**Substitui:** ADR-0034 (JARVIS Conscious Interaction Layer) + ADR-0035 (JARVIS Deep Research)
**Sprint Target:** 77-80 (JARVIS Core) + N+1 (Voice) + N+2 (Mesh)
**Depende de:** ADR-0031 (AIOS Evolution), ADR-0032 (WASM Agent Apps), ADR-0033 (On-Device Micro-Learning)

---

## 1. Visão Geral

J.A.R.V.I.S. (**J**ust **A** **R**ather **V**ery **I**ntelligent **S**ystem) é a **camada interativa in-out** do Neural AIOS Hermes. É a persona gráfica e responsiva que o usuário vê, ouve e com quem conversa. JARVIS não substitui Hermes — JARVIS **é** Hermes com rosto, voz, memória cognitiva e aprendizado recursivo contínuo.

```
┌─────────────────────────────────────────────────────────────────────┐
│                         NEURAL AIOS STACK                             │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  LAYER 5: J.A.R.V.I.S. — Persona Interativa                     │ │
│  │  ├─ SOUL.md (personalidade, tom, humor, empatia)                │ │
│  │  ├─ I/O Multimodal (texto, voz, imagens, vídeo, docs, stream)  │ │
│  │  ├─ Memória Cognitiva (Ego, Dreaming, Reflexive, Babel-Index)  │ │
│  │  ├─ Notification Gate (alertas proativos, urgency gating)       │ │
│  │  ├─ Emotion Analysis (BitNet classifier, adjust_tone)           │ │
│  │  ├─ IPW Monitor (Intelligence Per Watt, RAPL MSR)               │ │
│  │  └─ Aprendizado Recursivo (auto-skill, session compression)     │ │
│  └──────────────────────────┬──────────────────────────────────────┘ │
│                              │                                        │
│  ┌──────────────────────────▼──────────────────────────────────────┐ │
│  │  LAYER 4: HERMES — Orquestrador de Workflows                    │ │
│  │  ├─ Intent Routing (ReAct 7 fases, Council, Handoff)            │ │
│  │  ├─ Persona Pipeline (16 stages, OVOS-inspired)                 │ │
│  │  ├─ Semantic Cache (5-tier routing, 97.5% reduction)            │ │
│  │  ├─ Capability Contract + Consent Gates                         │ │
│  │  └─ ADE Pipeline (Spec→Execute→Review→Recover)                  │ │
│  └──────────────────────────┬──────────────────────────────────────┘ │
│                              │                                        │
│  ┌──────────────────────────▼──────────────────────────────────────┐ │
│  │  LAYER 3: CORTEX — Sistema Nervoso Central                      │ │
│  │  ├─ BitNet LLM (1.5B params, 2-bit ternary, ADD/SUB)           │ │
│  │  ├─ Trinity MoE (Router + hw_identify + rust_coder + experts)   │ │
│  │  ├─ Medusa Speculative Decode                                   │ │
│  │  ├─ Emotion Classifier Expert                                    │ │
│  │  └─ IPW-aware Routing                                           │ │
│  └──────────────────────────┬──────────────────────────────────────┘ │
│                              │                                        │
│  ┌──────────────────────────▼──────────────────────────────────────┐ │
│  │  LAYER 2: KERNEL — Base do Sistema                              │ │
│  │  ├─ 247+ Agentes (20 nativos + 147 Agency + 80 importados)     │ │
│  │  ├─ SkillRegistry + SafetyAgent + SecurityAgent                 │ │
│  │  ├─ DiskIntelligenceAgent (6 ctrl, 10+ FS, SMART, NVMe, ARC)  │ │
│  │  ├─ MemoryAgent (adaptive heap, MHI, dynamic tick)              │ │
│  │  ├─ EventBus + KnowledgeGraph + MemoryTree                      │ │
│  │  ├─ Ed25519 Trust + TPM 2.0 + Merkle Audit                     │ │
│  │  └─ WASM Runtime (wasmi, sandbox, fuel metering)                │ │
│  └──────────────────────────┬──────────────────────────────────────┘ │
│                              │                                        │
│  ┌──────────────────────────▼──────────────────────────────────────┐ │
│  │  LAYER 1: BOOT — Inicialização Agente-First                     │ │
│  │  ├─ SafeHarbor → Serial + Framebuffer + IDT                     │ │
│  │  ├─ MemoryCore → Frame allocator + Page tables + Heap + SIMD    │ │
│  │  ├─ SystemBringup → CortexAgent ACORDA (pre-HW)                 │ │
│  │  ├─ Diagnostics → DiagnosticSkill                               │ │
│  │  ├─ HardwareDiscovery → PCI + ACPI + APIC + SMP + GPU           │ │
│  │  ├─ DriverInit → Net + USB + ATA + NVMe                         │ │
│  │  ├─ AgentFleet → 247+ agentes registrados                       │ │
│  │  └─ Runtime → HermesAgent + AgentScheduler::run()               │ │
│  └─────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.1 Princípio Fundamental

> **JARVIS não é uma camada separada. JARVIS é a persona do Hermes.**
> Tudo que JARVIS faz, Hermes orquestra. Tudo que Hermes orquestra, Cortex pensa.
> Tudo que Cortex pensa, Kernel executa. Tudo que Kernel executa, Boot inicializou.

### 1.2 O que JARVIS é

- **Interface gráfica e responsiva** — framebuffer BGRA32, NeuralConsole 1280×720
- **Persona com alma** — SOUL.md define personalidade, tom, humor, empatia
- **Memória cognitiva** — 12 layers (mem0-supabase inspired): perception → ego
- **Aprendizado recursivo contínuo** — aprende com cada interação, sonha, consolida
- **I/O multimodal** — texto (hoje), voz/imagens/vídeo/documentos/streaming (futuro)
- **Proativa** — notifica, sugere, alerta sem ser perguntada
- **Eficiente** — IPW monitoring (tokens por watt), semantic cache (97.5% reduction)

### 1.3 O que JARVIS NÃO é

- Não é um chatbot separado do OS
- Não é uma aplicação userspace
- Não é um wrapper sobre Hermes
- Não é um LLM standalone
- Não é cloud-dependent

---

## 2. Fontes de Inspiração (27 projetos + 20+ papers arXiv)

### 2.1 Ecossistema Próprio (6 repositórios)

| Repositório | Insight Chave Portado |
|---|---|
| **msrovani/JARVIS** (C#/.NET MAUI) | SOUL.md personality engine, emotion analysis pipeline, embedding storage |
| **msrovani/Jotape** (Kotlin) | RAG pipeline (STT→RAG→LLM→TTS), ethical control, voice verification |
| **msrovani/SKYNET** (TS/Rust→WASM) | DSD speculative decoding, thermal management, circadian scheduling, genetic evolution, FL |
| **msrovani/BeFree** (TS) | JARBAS persona template, reputation system, automation engine, DID (Ed25519) |
| **msrovani/Android_AI_Car** (Kotlin) | ConversationManager→detect_intent→execute_action pattern |
| **msrovani/mem0-supabase** (Python) | 12-layer cognitive memory, dreaming engine, ego layer, context firewall |

### 2.2 Open-Source (27 projetos)

| Projeto | Stars | Insight Portado |
|---|---|---|
| **OpenJarvis Stanford** | 7.3K★ | IPW, session compression, skill discovery (DSPy/ACE), energy telemetry |
| **Piper TTS** | 11.2K★ | Neural TTS local C++, 100+ vozes, offline-first |
| **Mycroft** | 6.6K★ | Skill system + messagebus architecture (pioneiro) |
| **agenticSeek** | 26.6K★ | Fully local Manus AI — valida local-first approach |
| **Leon AI** | 17.3K★ | Open-source personal assistant, skill packages |
| **Pipecat** | 13.2K★ | Voice/multimodal conversational AI, real-time pipeline |
| **Moltis** | 2.8K★ | Single binary Rust agent server, consent-gated tools |
| **Priler/jarvis** | 2.9K★ | Offline-first, Vosk STT, Rustpotter wake word |
| **SynkraAI/aiox** | 3K★ | CLI First, 12 agents, ADE pipeline |
| **OVOS** | 278★ | Persona pipeline 16 stages |
| **Rhasspy3** | 382★ | Wyoming protocol IPC, 8-domain voice pipeline |
| **NeonCore** | 205★ | Multi-user, signal manager, skill hot-reload |
| **OpenCrust** | 137★ | Multi-agent Rust, self-learning skills, RAG, MCP |
| **NabaOS** | 5★ | 5-tier cache-first routing (97.5% reduction), Ed25519, 130 agents, WASM |
| **NEOTH** | 1★ | 5-tier memory, consent-gated tools, Babel-Index, signed audit |
| **Cratos** | 4★ | Auto-skill generation, model routing, event sourcing |
| **Claudette** | 11★ | Air-gapped personal AI, zero telemetry |
| **Residuum** | 8★ | Sessionless continuous thread, no context loss |
| **Nexus** | 2★ | Graph memory, DID identity, privacy-as-config |

### 2.3 Academia (20+ papers arXiv 2026)

| Paper | Insight Portado |
|---|---|
| **AgenticOS** (2606.21129) | OS como "intent filter", 4-layer architecture |
| **Governed MCP** (2604.16870) | 6-layer MCP governance, Anima OS bare-metal Rust |
| **Unfireable Safety Kernel** (2606.26057) | Fail-closed invariant, SMT proof, signed evidence |
| **Fluid Personality** (2607.01034) | Context-adaptive persona + personality intensity |
| **Cognitive Digital Twins** (2606.23094) | 5A governance framework |
| **TopoClaw** (2605.15556) | Cross-device action placement, cross-user identity |
| **Right to History** (2602.20214) | Merkle tree audit logs, capability isolation |
| **Aegis** (2603.16938) | Cryptographic runtime governance, immutable ethics |
| **Qualixar OS** (2604.06392) | 12 topologies, 3-layer model routing, consensus judge |
| **Dynamic Malicious Skills** (2606.16287) | SKILL.md injection defense, read-only mounts |

---

## 3. Arquitetura de 5 Camadas — Conversão Rust no_std

### 3.1 Layer 1: BOOT (Bootloader → Kernel Init)

JARVIS não existe no boot. Mas o boot já prepara o terreno:

```rust
// Boot Phase 2: CortexAgent acorda ANTES do hardware
// JARVIS persona é carregada do FAT32 via DiskAgent Tier 0
fn boot_phase2_system_bringup() {
    // ...
    // Carrega SOUL.md do FAT32 (se existir)
    if let Some(soul_bytes) = fat32_read("/system/jarvis/soul.md") {
        SOUL_MD.lock().parse(&soul_bytes);
    }
    // CortexAgent acorda com persona JARVIS como system prompt
    cortex_agent::init(SOUL_MD.lock().as_system_prompt());
}
```

**Dependência JARVIS no boot:** Nenhuma. Boot funciona sem JARVIS. JARVIS é ativado na Phase 7 (Runtime).

### 3.2 Layer 2: KERNEL (Agentes + Skills + Hardware)

JARVIS usa os agentes do kernel como ferramentas. Cada capability do JARVIS mapeia para um ou mais agentes:

```rust
// JARVIS capabilities → Kernel agents mapping
pub struct JarvisKernelBridge {
    disk: &'static DiskIntelligenceAgent,
    memory: &'static MemoryAgent,
    net: &'static NetAgent,
    display: &'static DisplayAgent,
    security: &'static SecurityAgent,
    safety: &'static SafetyAgent,
    cron: &'static CronAgent,
    event_bus: &'static EventBus,
    kg: &'static KnowledgeGraph,
    memory_tree: &'static MemoryTree,
}

impl JarvisKernelBridge {
    /// Lê arquivo do sistema (via DiskAgent)
    pub fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.disk.read_file(path)
    }

    /// Publica evento no EventBus (via EventBus)
    pub fn publish(&self, topic: &str, payload: &[u8]) {
        self.event_bus.publish(Topic::from_str(topic), payload);
    }

    /// Consulta Knowledge Graph (via KG)
    pub fn kg_query(&self, subject: &str, predicate: &str) -> Vec<KgTriple> {
        self.kg.query(subject, predicate)
    }

    /// Verifica segurança antes de executar skill (via SafetyAgent)
    pub fn safety_check(&self, skill: &str, args: &[u8]) -> SafetyResult {
        self.safety.intercept(skill, args)
    }

    /// Mede energia via RAPL MSR (via MemoryAgent)
    pub fn read_rapl_uj(&self) -> u64 {
        // MSR 0x610 (PKG_ENERGY_STATUS) — Ring 0 only
        unsafe { core::arch::x86_64::_rdmsr(0x610) }
    }
}
```

### 3.3 Layer 3: CORTEX (Sistema Nervoso Central + Inferência)

JARVIS usa o Cortex como cérebro. O Cortex processa inferências com aprendizado contínuo:

```rust
/// JARVIS inference pipeline — usa Cortex + Trinity MoE
pub struct JarvisInference {
    cortex: &'static CortexAgent,
    trinity: &'static TrinityRouter,
    ipw: IpwMonitor,
    semantic_cache: SemanticCache,
}

impl JarvisInference {
    /// Pipeline completo: input → cache → classify → expert → output
    pub fn infer(&mut self, input: &str, context: &JarvisContext) -> JarvisResponse {
        // 1. Semantic cache check (5-tier, NabaOS-inspired)
        if let Some(cached) = self.semantic_cache.lookup(input) {
            self.ipw.record_cache_hit();
            return cached;
        }

        // 2. IPW measurement start
        let energy_start = self.ipw.read_rapl_uj();

        // 3. Trinity Router classifica intenção
        let intent = self.trinity.classify(input);

        // 4. Roteia para expert correto
        let response = match intent {
            Intent::Hardware => self.trinity.hw_identify(input),
            Intent::Code => self.trinity.rust_coder(input),
            Intent::DiskDiag => self.trinity.disk_diag(input),
            Intent::Security => self.trinity.security_expert(input),
            Intent::Emotion => self.trinity.emotion_classifier(input),
            Intent::General => self.cortex.generate_text(input, context.as_prompt()),
            _ => self.cortex.generate_text(input, context.as_prompt()),
        };

        // 5. IPW measurement end
        let energy_end = self.ipw.read_rapl_uj();
        self.ipw.record_inference(energy_end - energy_start, response.token_count());

        // 6. Cache store
        self.semantic_cache.store(input, &response);

        response
    }
}

/// IPW Monitor — Intelligence Per Watt (OpenJarvis Stanford)
pub struct IpwMonitor {
    pub tokens_generated: u64,
    pub energy_uj: u64,
    pub ipw_score: f32,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

impl IpwMonitor {
    pub fn read_rapl_uj(&self) -> u64 {
        // MSR 0x610 — PKG_ENERGY_STATUS, Ring 0 only
        unsafe { core::arch::x86_64::_rdmsr(0x610) }
    }

    pub fn record_inference(&mut self, energy_delta_uj: u64, tokens: u64) {
        self.energy_uj += energy_delta_uj;
        self.tokens_generated += tokens;
        if self.energy_uj > 0 {
            self.ipw_score = self.tokens_generated as f32
                / (self.energy_uj as f32 / 1_000_000.0);
        }
    }

    pub fn record_cache_hit(&mut self) { self.cache_hits += 1; }
    pub fn record_cache_miss(&mut self) { self.cache_misses += 1; }

    pub fn report(&self) -> &'static str {
        // Formata relatório IPW para JARVIS exibir
        // "IPW: 42.3 tok/W | Cache: 87% | Total: 1234 tok, 29 J"
        static mut BUF: [u8; 128] = [0; 128];
        // ... formatação no_std ...
        ""
    }
}

/// Semantic Cache — 5-tier routing (NabaOS-inspired)
pub struct SemanticCache {
    // Tier 1: Exact match (hash)
    exact: BTreeMap<[u8; 32], CachedResponse>,
    // Tier 2: Semantic match (embedding similarity > 0.95)
    semantic: Vec<(Embedding, CachedResponse)>,
    // Tier 3: Pattern match (intent + entities)
    pattern: BTreeMap<(Intent, u64), CachedResponse>,
    // Tier 4: Provider fallback (round-robin)
    fallback_idx: usize,
    // Tier 5: Cold start (first available)
    cold: Option<CachedResponse>,
}

impl SemanticCache {
    pub fn lookup(&self, input: &str) -> Option<CachedResponse> {
        // Tier 1: SHA-256 exact match
        let hash = sha256(input.as_bytes());
        if let Some(cached) = self.exact.get(&hash) {
            return Some(cached.clone());
        }
        // Tier 2: Embedding similarity > 0.95
        let emb = embed(input);
        for (stored_emb, cached) in &self.semantic {
            if cosine_similarity(&emb, stored_emb) > 0.95 {
                return Some(cached.clone());
            }
        }
        // Tier 3-5: pattern/fallback/cold (simplified)
        None
    }

    pub fn store(&mut self, input: &str, response: &JarvisResponse) {
        let hash = sha256(input.as_bytes());
        let cached = CachedResponse::from(response);
        self.exact.insert(hash, cached.clone());
        let emb = embed(input);
        self.semantic.push((emb, cached));
        // Limit semantic cache to 1024 entries (LRU eviction)
        if self.semantic.len() > 1024 {
            self.semantic.remove(0);
        }
    }
}
```

### 3.4 Layer 4: HERMES (Orquestrador de Workflows)

JARVIS delega toda orquestração ao Hermes. Hermes roteia intents, executa ReAct, e gerencia agents:

```rust
/// Persona Pipeline — 16 stages (OVOS-inspired, adapted para no_std)
pub enum PersonaStage {
    SafetyCheck,          // SafetyAgent intercepta primeiro
    StopHandler,          // /stop, /cancel, /abort
    ConverseHandler,      // Conversação contínua (contexto ativo)
    SkillHighPriority,    // Skills críticas (disk, security, system)
    PersonaHandler,       // JARVIS personality response (LLM fallback)
    SkillMedium,          // Skills normais (file, net, time)
    CommonQA,             // Perguntas frequentes (cache)
    FallbackLow,          // Último recurso (echo, help)
    // ... 8 more stages for completeness
    ReflexiveCheck,       // Eventos críticos bypassam pipeline
    DreamingQueue,        // Enfileira para consolidação noturna
    EgoUpdate,            // Atualiza auto-modelo
    SessionCompress,      // Comprime se > max_tokens
    NotificationGate,     // Verifica notificações pendentes
    HeartbeatCheck,       // Proactive heartbeats
    BabelIndex,           // Entropy monitoring
    AuditLog,             // Merkle audit trail
}

impl HermesAgent {
    /// Pipeline de resolução de intent — JARVIS persona integrada
    pub fn resolve_intent(&mut self, input: &str, ctx: &mut JarvisContext) -> HermesResponse {
        for stage in PERSONA_PIPELINE {
            match stage {
                PersonaStage::SafetyCheck => {
                    if let SafetyResult::Blocked(reason) = self.safety.intercept(input, &[]) {
                        return HermesResponse::blocked(reason);
                    }
                }
                PersonaStage::StopHandler => {
                    if is_stop_command(input) {
                        return HermesResponse::stop();
                    }
                }
                PersonaStage::ConverseHandler => {
                    if ctx.has_active_conversation() {
                        // Continua conversação com contexto
                        let response = self.jarvis_inference(input, ctx);
                        ctx.append_turn(input, &response);
                        return HermesResponse::converse(response);
                    }
                }
                PersonaStage::SkillHighPriority => {
                    if let Some(skill) = self.skill_registry.match_high_priority(input) {
                        return self.execute_skill(skill, input, ctx);
                    }
                }
                PersonaStage::PersonaHandler => {
                    // JARVIS personality response via LLM
                    let response = self.jarvis_inference(input, ctx);
                    ctx.append_turn(input, &response);
                    return HermesResponse::persona(response);
                }
                // ... remaining stages ...
                PersonaStage::AuditLog => {
                    self.audit_log.record(input, &ctx.last_response());
                }
                _ => {}
            }
        }
        HermesResponse::fallback("Desculpe, não entendi. Tente /help.")
    }
}
```

### 3.5 Layer 5: J.A.R.V.I.S. (Persona Interativa)

A camada JARVIS propriamente dita — persona, memória cognitiva, I/O multimodal:

```rust
/// J.A.R.V.I.S. — Agente principal da camada interativa
pub struct JarvisAgent {
    pub soul: SoulMd,
    pub context: JarvisContext,
    pub emotion: EmotionState,
    pub notification_gate: NotificationGate,
    pub session_compressor: SessionCompressor,
    pub ego: EgoLayer,
    pub dreaming: DreamingEngine,
    pub babel_index: BabelIndex,
    pub audit: MerkleAudit,
}

impl Agent for JarvisAgent {
    fn manifest(&self) -> AgentManifest {
        AgentManifest {
            name: "JARVIS",
            kind: AgentKind::Console,
            capabilities: &[
                Capability::EventSubscribe,
                Capability::EventPublish,
                Capability::SkillCall,
                Capability::SmartQuery,
                Capability::MhiQuery,
                Capability::LogWrite,
            ],
            auto_start: true,
            persist: true,
            schedule: ScheduleKind::Continuous,
            trust_tokens: &[1], // User-level trust
        }
    }

    fn tick(&mut self, tick: u64, _tick_count: u64) -> AgentTickResult {
        // 1. Notification Gate — coleta eventos do EventBus
        self.notification_gate.collect_from_event_bus();

        // 2. Proactive Heartbeats — verifica se há algo para dizer
        if let Some(heartbeat) = self.check_heartbeats(tick) {
            self.deliver_notification(heartbeat);
        }

        // 3. Babel-Index — monitora entropia da memória
        self.babel_index.check_entropy(&self.context.memory);

        // 4. Dreaming — se idle por > 1000 ticks, consolida memórias
        if tick - self.context.last_interaction > 1000 {
            self.dreaming.consolidate(&mut self.context.memory);
        }

        // 5. Session Compression — se buffer > max_tokens
        if self.context.token_count() > self.session_compressor.max_tokens {
            self.session_compressor.compress(&mut self.context);
        }

        AgentTickResult::Done
    }
}

/// SOUL.md — Personalidade (parser no_std)
pub struct SoulMd {
    pub name: &'static str,
    pub tone: PersonalityTone,
    pub humor_level: u8,      // 0-10
    pub formality: u8,        // 0-10
    pub empathy: u8,          // 0-10
    pub greeting_morning: &'static str,
    pub greeting_afternoon: &'static str,
    pub greeting_evening: &'static str,
    pub farewell: &'static str,
    pub notification_rules: Vec<NotificationRule>,
}

pub enum PersonalityTone {
    ProfessionalWarm,
    CasualFriendly,
    FormalButler,
    TechnicalPrecise,
    AdaptiveContext,  // muda baseado em contexto (Fluid Personality paper)
}

impl SoulMd {
    /// Parser minimalista para SOUL.md (no_std, sem serde)
    pub fn parse(data: &[u8]) -> Self {
        // Parser linha-por-linha, key=value simples
        // Suporta: name, tone, humor_level, formality, empathy,
        //          greeting_morning, greeting_afternoon, etc.
        // Notification rules: [[rules]] trigger=... message=... urgency=...
        let mut soul = SoulMd::default();
        let text = core::str::from_utf8(data).unwrap_or("");
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("name:") {
                // ... parse fields ...
            }
            // ... etc ...
        }
        soul
    }

    /// Gera system prompt para o Cortex LLM
    pub fn as_system_prompt(&self) -> &'static str {
        // Constrói prompt com personalidade JARVIS
        // "Você é J.A.R.V.I.S., assistente de IA do Neural AIOS.
        //  Seu tom é professional_warm, humor nível 3, empatia 9.
        //  Sempre responda de forma concisa e útil."
        ""
    }

    /// Ajusta tom baseado em contexto (Fluid Personality)
    pub fn adapt_tone(&self, context: &JarvisContext, emotion: &EmotionState) -> PersonalityTone {
        match (context.urgency, emotion.primary) {
            (true, _) => PersonalityTone::TechnicalPrecise,
            (_, Emotion::Sadness) => PersonalityTone::ProfessionalWarm, // mais empático
            (_, Emotion::Anger) => PersonalityTone::FormalButler,       // mais formal
            _ => self.tone,
        }
    }
}

/// Emotion Analysis — BitNet classifier (~50KB, Trinity expert)
pub struct EmotionState {
    pub primary: Emotion,
    pub intensity: f32,   // 0.0 - 1.0
    pub sarcasm: f32,     // 0.0 - 1.0
    pub urgency: u8,      // 0-10
}

pub enum Emotion {
    Joy, Sadness, Anger, Fear, Surprise, Disgust, Neutral,
}

impl EmotionState {
    /// Classifica emoção do texto via BitNet classifier
    pub fn from_text(input: &str, classifier: &BitNetClassifier) -> Self {
        let logits = classifier.forward(input.as_bytes());
        let primary = Emotion::from_argmax(&logits);
        let intensity = softmax_max(&logits);
        let sarcasm = detect_sarcasm(input); // heuristic pattern matching
        let urgency = detect_urgency(input); // keyword + punctuation analysis
        EmotionState { primary, intensity, sarcasm, urgency }
    }

    /// Ajusta tom da resposta baseado na emoção detectada
    pub fn adjust_tone(&self, response: &str) -> &'static str {
        match self.primary {
            Emotion::Sadness if self.intensity > 0.7 => {
                // Mais empático: "Entendo como se sente. " + response
                ""
            }
            Emotion::Anger if self.intensity > 0.7 => {
                // Mais formal e direto: response sem floreios
                ""
            }
            Emotion::Joy if self.intensity > 0.7 => {
                // Mais caloroso: "Excelente! " + response
                ""
            }
            _ => response,
        }
    }
}

/// Session Compression — compacta conversas longas (OpenJarvis + SKYNET Segment Means)
pub struct SessionCompressor {
    pub max_tokens: usize,
    pub strategy: CompressStrategy,
    pub keep_recent: usize, // mantém últimas N mensagens literais
}

pub enum CompressStrategy {
    Summarize,     // BitNet sumariza grupos de mensagens
    DropLowest,    // Remove mensagens de menor importância
    MergeSimilar,  // Agrupa mensagens similares (embedding similarity)
    SegmentMeans,  // SKYNET-style: divide em segmentos, média de cada
}

impl SessionCompressor {
    pub fn compress(&self, history: &mut Vec<ConversationTurn>) {
        if history.len() <= self.keep_recent { return; }

        let (to_compress, to_keep) = history.split_at_mut(history.len() - self.keep_recent);

        match self.strategy {
            CompressStrategy::Summarize => {
                // Agrupa em chunks de 10, sumariza cada chunk via BitNet
                // Substitui chunk por [System Note: "Resumo: ..."]
            }
            CompressStrategy::DropLowest => {
                // Remove mensagens com importance < threshold
                // importance = recency * relevance * emotional_weight
            }
            CompressStrategy::MergeSimilar => {
                // Embedding similarity > 0.90 → merge em uma mensagem
            }
            CompressStrategy::SegmentMeans => {
                // Divide em N segmentos, calcula "média" de cada
                // (representative message por segmento)
            }
        }
    }
}

/// Notification Gate — alertas proativos com regras SOUL.md
pub struct NotificationGate {
    rules: Vec<NotificationRule>,
    queue: VecDeque<Notification>,
    last_delivery_tick: u64,
    min_interval_ticks: u64, // evita spam
}

pub struct NotificationRule {
    pub trigger: &'static str,     // "DISK_HEALTH", "UPDATE_AVAILABLE", etc.
    pub message_template: &'static str,
    pub urgency: Urgency,
}

pub enum Urgency { Critical, High, Medium, Low }

impl NotificationGate {
    pub fn collect_from_event_bus(&mut self) {
        // Subscribes to EventBus topics, matches against rules
        // Enqueues matching notifications
    }

    pub fn try_deliver(&mut self, current_tick: u64) -> Option<Notification> {
        if current_tick - self.last_delivery_tick < self.min_interval_ticks {
            return None; // Rate limiting
        }
        // Critical: deliver immediately
        // High: deliver within 30s (360 ticks)
        // Medium: deliver when Hermes is idle
        // Low: log only
        self.queue.pop_front().map(|n| {
            self.last_delivery_tick = current_tick;
            n
        })
    }
}

/// Ego Layer — auto-modelo do JARVIS (mem0-supabase Layer 12 + Twin Agents paper)
pub struct EgoLayer {
    pub self_model: SelfModel,
    pub capabilities: Vec<&'static str>,
    pub limitations: Vec<&'static str>,
    pub confidence_scores: BTreeMap<&'static str, f32>,
}

pub struct SelfModel {
    pub name: &'static str,
    pub version: &'static str,
    pub knows_about: Vec<&'static str>,
    pub does_not_know: Vec<&'static str>,
    pub last_updated_tick: u64,
}

impl EgoLayer {
    /// JARVIS sabe o que sabe e o que não sabe
    pub fn can_answer(&self, question: &str) -> ConfidenceLevel {
        // Verifica se a pergunta está dentro do conhecimento do JARVIS
        // Se não sabe, diz honestamente "Não sei sobre isso"
        ConfidenceLevel::Medium // placeholder
    }

    /// Atualiza auto-modelo baseado em interações
    pub fn update(&mut self, interaction: &ConversationTurn, tick: u64) {
        // Se JARVIS respondeu com sucesso → aumenta confidence
        // Se JARVIS errou → diminui confidence, adiciona a limitations
        self.self_model.last_updated_tick = tick;
    }
}

/// Dreaming Engine — consolidação de memórias durante idle (mem0-supabase Layer 6)
pub struct DreamingEngine {
    pub pending_memories: Vec<MemoryCandidate>,
    pub consolidation_interval_ticks: u64,
    pub last_consolidation_tick: u64,
}

impl DreamingEngine {
    /// "Sonha" — consolida memórias do dia durante períodos de idle
    pub fn consolidate(&mut self, memory: &mut MemoryTree) {
        // 1. Agrupa memórias similares (embedding similarity)
        // 2. Gera insights sintéticos via BitNet
        // 3. Remove contradições (memórias conflitantes)
        // 4. Promove memórias frequentes para LTM
        // 5. Aplica Ebbinghaus decay em memórias antigas
    }

    /// Verifica se é hora de "sonhar"
    pub fn should_dream(&self, current_tick: u64, last_interaction: u64) -> bool {
        let idle_ticks = current_tick - last_interaction;
        idle_ticks > self.consolidation_interval_ticks
    }
}

/// Babel-Index — monitora entropia da memória (NEOTH-inspired)
pub struct BabelIndex {
    pub entropy_score: f32,
    pub contradiction_rate: f32,
    pub staleness_index: f32,
    pub collapse_threshold: f32,
}

impl BabelIndex {
    /// Prevê quando a memória vai colapsar (perder coerência)
    pub fn check_entropy(&mut self, memory: &MemoryTree) {
        // Entropy: diversidade de topics vs. capacidade
        // Contradiction: memórias conflitantes / total
        // Staleness: memórias antigas não-validadas / total
        self.entropy_score = self.calculate_entropy(memory);
        self.contradiction_rate = self.calculate_contradictions(memory);
        self.staleness_index = self.calculate_staleness(memory);

        if self.collapse_risk() > self.collapse_threshold {
            // Dispara consolidação automática via DreamingEngine
        }
    }

    pub fn collapse_risk(&self) -> f32 {
        (self.entropy_score + self.contradiction_rate + self.staleness_index) / 3.0
    }
}

/// Merkle Audit Trail — log imutável de todas ações (PunkGo + NEOTH)
pub struct MerkleAudit {
    pub chain: Vec<AuditEntry>,
    pub last_hash: [u8; 32],
}

pub struct AuditEntry {
    pub tick: u64,
    pub agent: &'static str,
    pub action: &'static str,
    pub payload_hash: [u8; 32],
    pub prev_hash: [u8; 32],
    pub signature: [u8; 64], // Ed25519
}

impl MerkleAudit {
    pub fn record(&mut self, agent: &str, action: &str, payload: &[u8], tick: u64) {
        let payload_hash = sha256(payload);
        let entry = AuditEntry {
            tick,
            agent: "", // static str
            action: "",
            payload_hash,
            prev_hash: self.last_hash,
            signature: [0; 64], // Ed25519 sign
        };
        self.last_hash = sha256(&entry.to_bytes());
        self.chain.push(entry);
        // Limit chain to 4096 entries (ring buffer)
        if self.chain.len() > 4096 {
            self.chain.remove(0);
        }
    }

    /// Verifica integridade da chain
    pub fn verify_chain(&self) -> bool {
        for i in 1..self.chain.len() {
            if self.chain[i].prev_hash != sha256(&self.chain[i-1].to_bytes()) {
                return false;
            }
        }
        true
    }
}

/// JarvisContext — contexto persistente da conversa
pub struct JarvisContext {
    pub session_id: u64,
    pub user_name: &'static str,
    pub conversation: Vec<ConversationTurn>,
    pub memory: MemoryTree,
    pub preferences: BTreeMap<&'static str, &'static str>,
    pub last_interaction: u64,
    pub urgency: bool,
}

pub struct ConversationTurn {
    pub tick: u64,
    pub role: TurnRole,
    pub content: &'static str,
    pub emotion: Option<EmotionState>,
    pub importance: f32,
}

pub enum TurnRole { User, Jarvis, System }

impl JarvisContext {
    pub fn token_count(&self) -> usize {
        self.conversation.iter().map(|t| t.content.len() / 4).sum()
    }

    pub fn append_turn(&mut self, input: &str, response: &JarvisResponse) {
        self.conversation.push(ConversationTurn {
            tick: 0, // current tick
            role: TurnRole::User,
            content: "", // leaked static str
            emotion: None,
            importance: 0.5,
        });
        self.conversation.push(ConversationTurn {
            tick: 0,
            role: TurnRole::Jarvis,
            content: "",
            emotion: None,
            importance: 0.5,
        });
    }

    pub fn has_active_conversation(&self) -> bool {
        !self.conversation.is_empty()
    }

    pub fn as_prompt(&self) -> &'static str {
        // Constrói prompt com contexto da conversa para o Cortex
        ""
    }
}

/// JarvisResponse — resposta formatada do JARVIS
pub struct JarvisResponse {
    pub text: &'static str,
    pub emotion_adjusted: bool,
    pub tokens_used: usize,
    pub ipw_score: f32,
    pub cached: bool,
    pub confidence: f32,
}

impl JarvisResponse {
    pub fn token_count(&self) -> u64 { self.tokens_used as u64 }
}
```

---

## 4. I/O Multimodal — Camada Interativa In-Out

### 4.1 Modalidades Suportadas

| Modalidade | Status | Implementação | Sprint |
|---|---|---|---|
| **Texto** (teclado) | ✅ Hoje | InputAgent → HermesAgent → DisplayAgent | — |
| **Texto** (framebuffer) | ✅ Hoje | DisplayAgent BGRA32 1280×720 | — |
| **Voz** (TTS) | 🔴 Pós B-01 | Piper C++ binary via WASM host function | N+1 |
| **Voz** (STT) | 🔴 Pós B-01 | Vosk/Whisper.cpp via WASM host function | N+1 |
| **Wake Word** | 🔴 Pós B-01 | Rustpotter crate → EventBus | N+1 |
| **Imagens** | 🔴 Futuro | Framebuffer capture + BitNet vision expert | N+2 |
| **Vídeo** | 🔴 Futuro | Framebuffer streaming + temporal analysis | N+3 |
| **Documentos** | 🔴 Futuro | DiskAgent read + Cortex summarization | N+2 |
| **Streaming** | 🔴 Futuro | NetAgent + smoltcp + buffer management | N+3 |

### 4.2 Voice Pipeline (Wyoming Protocol, Rhasspy3-inspired)

```rust
/// Voice Pipeline — 8 domínios (Rhasspy3 Wyoming Protocol)
pub enum VoiceDomain {
    Mic,      // audio input
    Wake,     // wake word detection
    Asr,      // speech to text
    Vad,      // voice activity detection
    Intent,   // intent recognition from text
    Handle,   // intent or text input handling
    Tts,      // text to speech
    Snd,      // audio output
}

/// Voice Pipeline — processa áudio de mic até speaker
pub struct VoicePipeline {
    pub domains: [Option<VoiceProcessor>; 8],
    pub active: bool,
}

impl VoicePipeline {
    /// Pipeline completo: mic → wake → asr → vad → intent → handle → tts → snd
    pub fn process(&mut self, audio: &[i16]) -> Option<VoiceOutput> {
        // 1. Wake word detection (Rustpotter)
        let wake = self.domains[1].as_ref()?.detect(audio)?;

        // 2. VAD — detecta início/fim de fala
        let speech = self.domains[3].as_ref()?.detect(audio)?;

        // 3. ASR — speech to text (Vosk/Whisper.cpp)
        let text = self.domains[2].as_ref()?.transcribe(&speech)?;

        // 4. Intent — classificação (Hermes/Cortex)
        let intent = self.domains[4].as_ref()?.recognize(&text)?;

        // 5. Handle — execução (JARVIS/Hermes)
        let response = self.domains[5].as_ref()?.handle(&intent)?;

        // 6. TTS — text to speech (Piper)
        let audio_out = self.domains[6].as_ref()?.synthesize(&response)?;

        // 7. Snd — audio output
        self.domains[7].as_ref()?.play(&audio_out);

        Some(VoiceOutput { text: response, audio: audio_out })
    }
}
```

---

## 5. Feature Convergence Matrix

### 5.1 Features que JÁ TEMOS (17 validadas)

| # | Feature | Componente | Validação Externa |
|---|---|---|---|
| 1 | Intent routing | HermesAgent | AgentOS KDD paper |
| 2 | Multi-agent orchestration | AgentScheduler + 247 agents | SynkraAI, Qualixar OS |
| 3 | Safety interceptor (Asimov) | SafetyAgent | Aegis paper |
| 4 | Model orchestration (MoE) | Trinity MoE | TabNews "small > big" |
| 5 | Knowledge Graph | event-bus KG | mem0-supabase Graphic Layer |
| 6 | Hybrid search | event-bus HybridSearch | mem0-supabase RRF Fusion |
| 7 | Semantic dedup | event-bus Dedup | mem0-supabase Semantic Compression |
| 8 | Privacy/PII masking | SecurityAgent mask_secrets | mem0-supabase Context Firewall |
| 9 | Ebbinghaus forgetting | event-bus Lifecycle | mem0-supabase Lifecycle Layer |
| 10 | Ed25519 trust | BootTrustAgent | BeFree DID, Aegis IEPL |
| 11 | Self-healing | SelfHealAgent | SKYNET Genetic Evolution |
| 12 | Speculative decoding | Cortex Medusa | SKYNET DSD |
| 13 | WASM sandbox | ADR-0032 | OpenJarvis WASM runner |
| 14 | On-device training | ADR-0033 | OpenJarvis GRPO/SFT |
| 15 | MCP Layer | SkillRegistry | Governed MCP paper |
| 16 | Cron scheduling | CronAgent | BeFree Automation Engine |
| 17 | Dashboard | agent-core Dashboard | Qualixar OS 24-tab |

### 5.2 Features para ADOTAR (28 novas)

| # | Feature | Fonte | LOC | Sprint | Prioridade |
|---|---|---|---|---|---|
| 1 | **SOUL.md Personality Engine** | JARVIS C# + BeFree JARBAS | ~300 | 77 | 🔴 Crítica |
| 2 | **IPW Monitoring** (RAPL MSR) | OpenJarvis + SKYNET | ~150 | 77 | 🟡 Alta |
| 3 | **Session Compression** | OpenJarvis + SKYNET + mem0 | ~200 | 77 | 🟡 Alta |
| 4 | **Notification Gate** | JARVIS C# + BeFree | ~200 | 77 | 🟡 Alta |
| 5 | **Sessionless Thread** | Residuum | ~100 | 77 | 🟢 Baixa |
| 6 | **Emotion Analysis** | JARVIS C# | ~250 | 78 | 🟡 Alta |
| 7 | **Capability Contract + Consent Gates** | terminal-jarvis + Moltis | ~200 | 78 | 🟡 Alta |
| 8 | **Skill Discovery (DSPy/ACE)** | OpenJarvis + SynkraAI | ~300 | 78 | 🟡 Média |
| 9 | **ADE Pipeline** | SynkraAI | ~200 | 78 | 🟡 Média |
| 10 | **Semantic Cache (5-tier)** | NabaOS | ~150 | 78 | 🟡 Alta |
| 11 | **Persona Pipeline Stages** | OVOS | ~100 | 78 | 🟡 Alta |
| 12 | **Dreaming/Consolidation** | mem0-supabase Layer 6 | ~200 | 79 | 🟡 Média |
| 13 | **Ego Layer** (self-model) | mem0-supabase Layer 12 | ~250 | 79 | 🟡 Média |
| 14 | **Proactive Heartbeats** | mem0-supabase Layer 12 | ~100 | 79 | 🟡 Média |
| 15 | **Tool-State Save Game** | mem0-supabase Layer 9 | ~100 | 79 | 🟢 Baixa |
| 16 | **Auto-Skill Generation** | Cratos | ~150 | 79 | 🟡 Média |
| 17 | **Babel-Index** (entropy) | NEOTH | ~100 | 79 | 🟢 Baixa |
| 18 | **Fail-Closed Safety** | Unfireable Safety Kernel | ~200 | 80 | 🟡 Média |
| 19 | **Merkle Audit Trail** | PunkGo + NEOTH | ~200 | 80 | 🟢 Baixa |
| 20 | **Fluid Persona** (context-adaptive) | Fluid Personality paper | ~100 | 80 | 🟢 Baixa |
| 21 | **Piper TTS** | Piper 11.2K★ | ~100 | N+1 | 🔴 Pós B-01 |
| 22 | **Vosk/Whisper STT** | Priler/jarvis + Rhasspy3 | ~400 | N+1 | 🔴 Pós B-01 |
| 23 | **Wake Word** (Rustpotter) | Priler/jarvis | ~100 | N+1 | 🔴 Pós B-01 |
| 24 | **Wyoming Protocol IPC** | Rhasspy3 | ~300 | N+1 | 🔴 Pós B-01 |
| 25 | **Voice Pipeline (8-domain)** | Rhasspy3 | ~200 | N+1 | 🔴 Pós B-01 |
| 26 | **Multi-device sync (CRDT)** | SKYNET + BeFree | ~300 | N+1 | 🔴 Pós B-01 |
| 27 | **SKYNET Mesh Node** | SKYNET DSD | ~300 | N+2 | 🔴 Pós B-01 |
| 28 | **Gamification** | Jotape | ~200 | N+1 | 🟢 Baixa |

### 5.3 Features REJEITADAS (10 incompatíveis)

| Feature | Motivo |
|---|---|
| Tauri desktop | Somos bare-metal, não desktop app |
| Node.js/Python runtime | no_std incompatível |
| LuaJIT scripting | no_std incompatível, WASM é melhor |
| YOLO mode (bypass segurança) | Contradiz SafetyAgent |
| Multiplayer presence | Irrelevante para OS kernel |
| Google OAuth | Bare-metal sem browser |
| n8n/OpenClaw workflow | Ecossistema Node.js |
| Cloud-first routing | Somos local-first |
| C++ orchestrator | Rust-only |
| TensorFlow dependency | BitNet local é suficiente |

---

## 6. Sprint Plan — JARVIS Sprints (77-80 + N+1 + N+2)

### Sprint 77 — JARVIS Persona + IPW + Compression (~950 LOC)

| # | Item | LOC | Dependências |
|---|---|---|---|
| 77.1 | SOUL.md parser + personality engine | ~300 | FAT32 reader (existe) |
| 77.2 | IPW Monitor (RAPL MSR 0x610) | ~150 | MemoryAgent |
| 77.3 | Session Compression (4 strategies) | ~200 | HermesAgent.buffer |
| 77.4 | Notification Gate (4 urgency levels) | ~200 | EventBus |
| 77.5 | Sessionless Thread mode | ~100 | EventLog |

**Entregável:** JARVIS tem personalidade, mede eficiência energética, compacta conversas, notifica proativamente.

### Sprint 78 — Emotion + Discovery + Contracts + Cache (~1200 LOC)

| # | Item | LOC | Dependências |
|---|---|---|---|
| 78.1 | Emotion Analysis (BitNet classifier) | ~250 | Trinity MoE |
| 78.2 | Capability Contract + Consent Gates | ~200 | SkillRegistry + SafetyAgent |
| 78.3 | Skill Discovery (DSPy/ACE) | ~300 | SkillObserver |
| 78.4 | ADE Pipeline (Spec→Execute→Review→Recover) | ~200 | AgentScheduler |
| 78.5 | Semantic Cache (5-tier routing) | ~150 | KnowledgeGraph |
| 78.6 | Persona Pipeline Stages (16 stages) | ~100 | HermesAgent |

**Entregável:** JARVIS detecta emoções, descobre skills, valida capacidades, cache semântico 97.5%.

### Sprint 79 — Dreaming + Ego + Heartbeats + Auto-Skills (~900 LOC)

| # | Item | LOC | Dependências |
|---|---|---|---|
| 79.1 | Dreaming/Consolidation (CronAgent noturno) | ~200 | CronAgent |
| 79.2 | Ego Layer (self-model + identity synthesis) | ~250 | SOUL.md |
| 79.3 | Proactive Heartbeats | ~100 | CronAgent |
| 79.4 | Tool-State Save Game | ~100 | AgentScheduler |
| 79.5 | Auto-Skill Generation | ~150 | SkillObserver + TrainingAgent |
| 79.6 | Babel-Index (entropy monitoring) | ~100 | MemoryTree |

**Entregável:** JARVIS "sonha", tem auto-consciência, age proativamente, gera skills automaticamente.

### Sprint 80 — Security Hardening + Audit (~500 LOC)

| # | Item | LOC | Dependências |
|---|---|---|---|
| 80.1 | Fail-Closed Safety Invariant | ~200 | SafetyAgent |
| 80.2 | Merkle Audit Trail (Ed25519 signed) | ~200 | BootLogAgent |
| 80.3 | Fluid Persona (context-adaptive) | ~100 | SOUL.md |

**Entregável:** JARVIS é seguro por arquitetura (fail-closed), auditável (Merkle), adapta personalidade.

### Sprint N+1 — Voice + Cross-Device (~1600 LOC, pós B-01)

| # | Item | LOC |
|---|---|---|
| N1.1 | Piper TTS Integration (C++ binary) | ~100 |
| N1.2 | STT (Vosk/Whisper.cpp) | ~400 |
| N1.3 | Wake Word (Rustpotter) | ~100 |
| N1.4 | Wyoming Protocol IPC | ~300 |
| N1.5 | Voice Pipeline (8-domain) | ~200 |
| N1.6 | Multi-device sync (CRDT) | ~300 |
| N1.7 | Gamification | ~200 |

### Sprint N+2 — SKYNET Mesh + Vision (~500 LOC, pós B-01)

| # | Item | LOC |
|---|---|---|
| N2.1 | SKYNET mesh node (L1/L2) | ~300 |
| N2.2 | Distributed speculative decoding | ~200 |

---

## 7. Total LOC Estimado

| Sprint | LOC | Foco |
|---|---|---|
| **77** | ~950 | Persona + IPW + Compression + Notifications + Sessionless |
| **78** | ~1200 | Emotion + Discovery + Contracts + Cache + Persona Pipeline |
| **79** | ~900 | Dreaming + Ego + Heartbeats + Auto-Skills + Babel-Index |
| **80** | ~500 | Security + Audit + Fluid Persona |
| **N+1** | ~1600 | Voice (Piper+Vosk+Wyoming) + Cross-Device + Gamification |
| **N+2** | ~500 | SKYNET Mesh |
| **TOTAL** | **~5650** | JARVIS completo |

---

## 8. Decisão

**ADOTAR** as 28 features, implementadas em 4 sprints (77-80) totalizando ~3550 LOC de kernel + ~2100 LOC pós-B-01.

**REJEITAR** as 10 features incompatíveis (Tauri, Node.js, Python, LuaJIT, YOLO, multiplayer, cloud-first).

**INTEGRAR** SKYNET como backend distribuído do JARVIS quando B-01 estiver pronto (Sprint N+2).

**VALIDAR** com benchmarks da academia: LiveClawBench, GTA-2, SocialMemBench.

**SUBSTITUIR** ADR-0034 e ADR-0035 por este documento (ADR-0036).

**PRINCÍPIO:** JARVIS não é uma camada separada. JARVIS é a persona do Hermes. Tudo são agentes. Tudo expõe skills. Tudo passa pelo SafetyAgent.

---

## 9. Referências

### ADRs Substituídas
- ADR-0034: J.A.R.V.I.S. Conscious Interaction Layer (persona, emotion, IPW, session compression)
- ADR-0035: J.A.R.V.I.S. Deep Research — Ecosystem Convergence (6 repos, 27 projects, 20+ arXiv, 28 features)

### ADRs Relacionadas
- ADR-0031: AIOS Evolution (Cross-OS WASM-first, Self-Update A/B, Hybrid Agents)
- ADR-0032: WASM Agent Apps (developer contract, 15 skills, marketplace)
- ADR-0033: On-Device Micro-Learning (Self-training MoE via Candle sidecar + BitNet ADD/SUB)

### Referências Completas
Ver ADR-0035 Seção 8 para lista completa de 6 repositórios próprios, 27 projetos open-source, e 20+ papers arXiv.
