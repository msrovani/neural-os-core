# ADR-0034: J.A.R.V.I.S. — Camada de Interação Consciente do Hermes

> **SUPERSEDED:** preservada como histórico. A decisão canônica é a [ADR-0036](0036-jarvis-unified-interaction-layer.md).

**Data:** 2026-07-03
**Status:** Superseded
**Sprint Target:** 77-78 (J.A.R.V.I.S. como persona) + N+1 (voz, emotion, cross-device)

---

## 1. Visão Geral

J.A.R.V.I.S. é a **camada de personalidade consciente** acima do Hermes. Inspirado no JARVIS da Marvel, no projeto JARVIS do usuário (C#, .NET MAUI, Semantic Kernel, TensorFlow), e em 11 projetos open-source analisados (Stanford OpenJarvis, Priler/jarvis, SynkraAI, terminal-jarvis, dyoburon-jarvis, etc.).

```
User ←→ J.A.R.V.I.S. (persona, voz, notificações, emoções)
            ←→ Hermes (intent routing, ReAct, multi-agent)
                ←→ Cortex (LLM BitNet + Trinity MoE)
                    ←→ Kernel (agentes, skills, hardware)
```

## 2. Fontes de Inspiração

| Fonte | Tipo | Ideias Aproveitadas |
|---|---|---|
| **msrovani/JARVIS** (C#/.NET MAUI) | Projeto do usuário | SOUL.md personalidade, emotion analysis, voice, Semantic Kernel plugin pattern, embedding storage |
| **OpenJarvis/Stanford** (7.3K★) | Pesquisa acadêmica | IPW (Intelligence Per Watt), session compression, skill discovery (DSPy/ACE), energy telemetry |
| **Priler/jarvis** (2.9K★) | Offline-first | Vosk local STT, Rustpotter wake word, privacy-by-design |
| **SynkraAI/aiox** (3K★) | Dev framework | CLI First, 12 specialized agents, ADE pipeline (Spec→Execute→Review) |
| **terminal-jarvis** | Harness system | Capability contract (9-cap), `command_on_path` security check |
| **dyoburon-jarvis** | Desktop GPU | WebGPU rendering, plugin IPC bridge, local Whisper |
| **TabNews Kitsune** | Artigo BR | Model orchestration, Asimov Laws (já temos), 80/20 princípio |
| **Medium atifhabib** | Relato | LLM = reasoning engine, workflow > model, híbrido local/cloud |

## 3. Arquitetura

```
┌─────────────────────────────────────────────────────────────────┐
│                      J.A.R.V.I.S. LAYER                          │
│                                                                   │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────────┐    │
│  │ Persona  │ │ Emotion  │ │ Voice    │ │ Notification     │    │
│  │ SOUL.md  │ │ Analysis │ │ TTS/STT  │ │ Gate             │    │
│  │          │ │          │ │ (futuro) │ │                  │    │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────────┬─────────┘    │
│       │            │            │                │              │
│  ┌────▼────────────▼────────────▼────────────────▼──────────┐  │
│  │              Context Engine                                │  │
│  │  - Session compression (OpenJarvis)                       │  │
│  │  - Embedding storage (SQLite style → KG)                  │  │
│  │  - Emotional state tracking                               │  │
│  │  - User profile (personalidade adaptativa)                │  │
│  │  - IPW monitoring (Intelligence Per Watt)                 │  │
│  └──────────────────────┬────────────────────────────────────┘  │
│                         │                                        │
│  ┌──────────────────────▼────────────────────────────────────┐  │
│  │              Skill Interface                                │  │
│  │  - Capability contract (terminal-jarvis 9-cap pattern)     │  │
│  │  - Skill discovery (DSPy/ACE optimization)                 │  │
│  │  - ADE pipeline (Spec→Execute→Review→Recover)             │  │
│  └──────────────────────┬────────────────────────────────────┘  │
│                         │                                        │
└─────────────────────────┼────────────────────────────────────────┘
                          │
┌─────────────────────────▼────────────────────────────────────────┐
│                         HERMES                                    │
│  Intent routing, ReAct loop, multi-agent orchestration            │
└─────────────────────────────────────────────────────────────────┘
```

## 4. Componentes Detalhados

### 4.1 SOUL.md — Personalidade (Prioridade: Sprint 77)

Inspirado no NousResearch Hermes SOUL.md + msrovani/JARVIS personality engine.

```toml
# /etc/jarvis/soul.md
name: J.A.R.V.I.S.
version: 2.0

[personality]
tone = "professional_warm"
humor_level = 0.3
formality = 0.6
empathy = 0.9

[voice]
tts_engine = "piper"  # ou "espeak" para fallback
wake_word = "jarvis"
language = "pt-BR"

[notifications]
# Gate rules: quais eventos mostrar, com que urgência
[[rules]]
trigger = "DISK_HEALTH"
message = "Alerta: {disk} com {attr} em {value}/{threshold}"
urgency = "high"

[[rules]]
trigger = "UPDATE_AVAILABLE"
message = "Nova versão {version} disponível. Deseja atualizar?"
urgency = "medium"

[learning]
# Como J.A.R.V.I.S. aprende do usuário
adaptive_personality = true
emotional_tracking = true
session_compression = true
skill_discovery = true
```

### 4.2 Emotion Analysis (Prioridade: Sprint 78)

Inspirado no msrovani/JARVIS EmotionalAnalysisService + TensorFlow.

```rust
pub struct EmotionState {
    pub primary: Emotion,   // Joy, Sadness, Anger, Fear, Surprise, Disgust
    pub intensity: f32,     // 0.0 - 1.0
    pub sarcasm: f32,       // 0.0 - 1.0
    pub urgency: u8,        // 0-10
}

impl EmotionState {
    pub fn from_text(input: &str) -> Self {
        // Usa o BitNet classifier (hw_identify-style, ~50KB)
        // -> retorna emoção predominante + intensidade
    }
    
    pub fn adjust_tone(&self, response: &str) -> String {
        // Ajusta tom da resposta baseado na emoção do usuário
        // Ex: se triste → mais empático; se irritado → mais formal
    }
}
```

### 4.3 Session Compression (Prioridade: Sprint 77)

Inspirado no OpenJarvis (Stanford) — compacta conversas longas sem perder contexto.

```rust
pub struct SessionCompressor {
    max_tokens: usize,
    strategy: CompressStrategy,  // Summarize, DropLowest, MergeSimilar
}

impl SessionCompressor {
    pub fn compress(&self, history: &[Message]) -> Vec<Message> {
        // Se ultrapassou max_tokens:
        // 1. Agrupa mensagens similares (embeddings)
        // 2. Sumariza grupos com o BitNet
        // 3. Mantém últimas N mensagens literais
        // 4. Descarta mensagens de baixa importância
    }
}
```

### 4.4 IPW — Intelligence Per Watt (Prioridade: Sprint 77)

Inspirado no OpenJarvis (Stanford, paper com energy telemetry).

```rust
pub struct IpwMonitor {
    pub tokens_generated: u64,
    pub energy_uj: u64,  // microjoules via RAPL MSR
    pub ipw_score: f32,  // tokens por watt
}

impl IpwMonitor {
    pub fn measure<RAPL: RAPLReader>(&mut self, rapl: &RAPL) {
        let start = rapl.read_uj();
        // ... executa inferência ...
        let end = rapl.read_uj();
        self.energy_uj += end - start;
        self.tokens_generated += 1;
        self.ipw_score = self.tokens_generated as f32 / (self.energy_uj as f32 / 1_000_000.0);
    }
    
    pub fn report(&self) -> String {
        format!("IPW: {:.2} tok/W, total: {} tok, {} J",
            self.ipw_score, self.tokens_generated, self.energy_uj / 1_000_000)
    }
}

// Leitura de energia via RAPL (Running Average Power Limit) — MSR 0x610+
pub trait RAPLReader {
    fn read_uj(&self) -> u64;  // microjoules desde boot
}
```

### 4.5 Capability Contract (Prioridade: Sprint 78)

Inspirado no terminal-jarvis (9-cap harness) + nosso SkillRegistry.

```rust
pub struct CapabilityContract {
    pub name: &'static str,
    pub caps: CapabilitySet,  // bitmask de capacidades
}

pub enum Capability {
    DiskRead,
    DiskWrite,
    NetHttp,
    TimeAccess,
    RandomAccess,
    LogWrite,
    EventPublish,
    EventSubscribe,
    SkillCall,
    SmartQuery,
    MhiQuery,
}

// Cada skill declara: "preciso destas capacidades"
// J.A.R.V.I.S. verifica: "o usuário autorizou estas capacidades?"
```

### 4.6 Skill Discovery — DSPy/ACE (Prioridade: Sprint 78)

Inspirado no OpenJarvis + SynkraAI ADE pipeline.

```rust
pub struct SkillDiscoverer {
    usage_log: Vec<UsageRecord>,
    optimizer: DspyLikeOptimizer,  // otimiza prompt da skill
}

impl SkillDiscoverer {
    pub fn discover_candidates(&self) -> Vec<SkillCandidate> {
        // 1. Analisa padrões de uso repetitivo (SkillObserver já faz!)
        // 2. Sugere novas skills para padrões frequentes
        // 3. Otimiza prompts de skills existentes (DSPy-style)
        // 4. Remove skills nunca usadas
    }
}
```

### 4.7 Notification Gate (Prioridade: Sprint 78)

Inspirado no msrovani/JARVIS — notificações proativas com regras do SOUL.md.

```rust
pub struct NotificationGate {
    rules: Vec<NotificationRule>,  // do SOUL.md
    queue: VecDeque<Notification>,
}

impl NotificationGate {
    pub fn tick(&mut self, event_bus: &EventBus) {
        for event in event_bus.pending_events() {
            for rule in &self.rules {
                if rule.matches(&event) {
                    self.queue.push_back(Notification {
                        message: rule.format(&event),
                        urgency: rule.urgency,
                    });
                }
            }
        }
    }
    
    pub fn deliver(&mut self) -> Option<String> {
        // Só entrega notificação quando Hermes estiver ocioso
        self.queue.pop_front().map(|n| n.message)
    }
}
```

## 5. Integração com o Ecossistema Existente

| Componente J.A.R.V.I.S. | Integração com AIOS | Status |
|---|---|---|
| **SOUL.md** | HermesAgent lê `/etc/jarvis/soul.md` via VFS | 🟡 Nova skill |
| **Emotion Analysis** | BitNet classifier (~50KB, treinado com 10K emoções) | 🟡 Novo expert Trinity |
| **Session Compression** | HermesAgent.buffer → compressão automática | 🟡 ~200 LOC |
| **IPW Monitoring** | MemoryAgent + RAPL MSR (0x610) | 🟡 ~150 LOC |
| **Capability Contract** | SkillRegistry + SecurityAgent | 🟡 ~100 LOC |
| **Skill Discovery** | SkillObserver + TrainingAgent | 🟡 ~300 LOC |
| **Notification Gate** | EventBus + HermesAgent.tick() | 🟡 ~200 LOC |
| **Voice TTS/STT** | Futuro (pós B-01): Piper TTS + Vosk STT | 🔴 Pós-MVP |
| **Vosk Wake Word** | Rustpotter crate → EventBus | 🔴 Pós-MVP |
| **Embedding Storage** | KnowledgeGraph (já existe!) | ✅ |
| **Multiplataforma** | Bare-metal + Hermes Chat Console | ✅ |

## 6. Roteiro

### Sprint 77 — J.A.R.V.I.S. Persona + IPW + Session Compression

| Item | LOC | O quê |
|---|---|---|
| SOUL.md parser + personality engine | ~300 | Lê /etc/jarvis/soul.md, ajusta tom |
| IPW Monitoring | ~150 | MemoryAgent + RAPL MSR 0x610 |
| Session Compression | ~200 | Hermes buffer, compacta ao atingir limite |
| Notification Gate | ~200 | EventBus → regras SOUL.md → notificações |

### Sprint 78 — Emotion + Discovery + Contracts

| Item | LOC | O quê |
|---|---|---|
| Emotion Analysis | ~250 | BitNet classifier, ajusta tom da resposta |
| Capability Contract | ~100 | SkillRegistry valida capacidades |
| Skill Discovery (DSPy) | ~300 | Observer + TrainingAgent otimizam skills |
| ADE Pipeline | ~200 | Spec→Execute→Review→Recover |

### Sprint N+1 — Voice + Cross-Device (pós B-01)

| Item | LOC | O quê |
|---|---|---|
| TTS (Piper/espeak) | ~200 | Síntese de voz (já temos PC speaker) |
| STT (Vosk) | ~400 | Reconhecimento de voz local |
| Wake Word (Rustpotter) | ~100 | "Jarvis, ..." → ativa Hermes |
| Embedding search | ~200 | KG + similaridade semântica |
| Multi-device sync | ~300 | Backup/sincronia via HTTP (B-01) |

## 7. Matriz de Viabilidade

| Feature | Viability | LOC | Fonte | Dependências |
|---|---|---|---|---|
| **SOUL.md** | 9/10 | ~300 | NousResearch + msrovani/JARVIS | — |
| **IPW Monitor** | 9/10 | ~150 | OpenJarvis Stanford | RAPL MSR (Ring 0) |
| **Session Compression** | 8/10 | ~200 | OpenJarvis | HermesAgent.buffer |
| **Emotion Analysis** | 8/10 | ~250 | msrovani/JARVIS | BitNet classifier |
| **Capability Contract** | 8/10 | ~100 | terminal-jarvis | SkillRegistry |
| **Skill Discovery** | 7/10 | ~300 | OpenJarvis + SynkraAI | TrainingAgent |
| **Notification Gate** | 9/10 | ~200 | msrovani/JARVIS | EventBus |
| **ADE Pipeline** | 7/10 | ~200 | SynkraAI | AgentScheduler |
| **Voice TTS** | 6/10 | ~200 | Priler/jarvis | Piper crate |
| **Voice STT** | 5/10 | ~400 | Priler/jarvis (Vosk) | B-01 + Vosk crate |
| **Wake Word** | 7/10 | ~100 | Priler/jarvis (Rustpotter) | Rustpotter crate |
| **Embedding Search** | 8/10 | ~200 | msrovani/JARVIS + KG | KnowledgeGraph |

## 8. Stack Final

```
User
  ↕ (teclado / voz futura)
J.A.R.V.I.S. (persona, emoção, notificações)
  ↕ skills
Hermes (intent, ReAct, orquestração)
  ↕ LLM_REQUEST
Cortex (BitNet 1.5B via GGUF + Trinity MoE)
  ↕ kernel agents
DiskAgent | NetAgent | DisplayAgent | SecurityAgent | MemoryAgent
```

**Tudo são agentes. Tudo expõe skills. Tudo passa pelo SafetyAgent.**
