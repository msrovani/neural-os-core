# ADR-0047-HMI: Neural Desktop — HMI Generativa, Embedding Space Visual, Avatar Vivo

**Data:** 2026-07-16
**Status:** **Superseded (parcial) → [ADR-0058](0058-generative-card-desktop.md)** — H1 (UI_SPEC/UiDeclaration), H2/H5 (embedding/splats) e H4 (avatar telemetria) absorvidos pela ADR-0058 "Generative Card Desktop"; H3 (renderer neural/diffusion) permanece ❌ descartado. Histórico do MVP parcial (SESSION_126) preservado abaixo.
**Complemento de:** ADR-0047 (Pilares 1-3), ADR-0047-GPU (Compute Pipeline)
**Depende de:** ADR-0042 N5 (Jarbas persona), ADR-0036 (JARVIS Interaction Layer)
**Sprint:** 110+ (paralelo com ADR-0042, ADR-0047, ADR-0047-GPU)

---

## Índice

1. [Executive Summary](#1-executive-summary)
2. [Estado Atual do HMI](#2-estado-atual-do-hmi)
3. [Análise Comparativa: SotA 2025-2026](#3-análise-comparativa-sota-2025-2026)
4. [Pilar H1 — Generative App Windows: UI como Dado, Não como Código](#4-pilar-h1--generative-app-windows-ui-como-dado-não-como-código)
5. [Pilar H2 — Embedding Space Desktop: O Espaço Latente como Paisagem 3D](#5-pilar-h2--embedding-space-desktop-o-espaço-latente-como-paisagem-3d)
6. [Pilar H3 — Neural Renderer: Compositor que Aprendeu a Desenhar](#6-pilar-h3--neural-renderer-compositor-que-aprendeu-a-desenhar)
7. [Pilar H4 — Soul Mirror: Avatar Como Telemetria Viva](#7-pilar-h4--soul-mirror-avatar-como-telemetria-viva)
8. [Pilar H5 — Thought Canvas: O Agente Vê o Que Está Pensando](#8-pilar-h5--thought-canvas-o-agente-vê-o-que-está-pensando)
9. [Engenharia Reversa das Referências](#9-engenharia-reversa-das-referências)
10. [Roteiro de Implementação](#10-roteiro-de-implementação)
11. [Referências](#11-referências)

---

## 0. MVP PoC (SESSION_126+)

| Item | Status |
|------|--------|
| H1 UI_SPEC JSON → compositor | ✅ `display/ui_spec.rs` |
| H4 avatar telemetria (LatentBus norm) | ✅ DisplayAgent |
| H2 embedding points on FB | ✅ `display/embed_viz.rs` |
| H5 thought splats | ✅ `embed_viz::draw_thought_splat` |
| **H3 neural/diffusion compositor** | ❌ **DESCARTADO** — inviável soft-float bare-metal (modelo 263M+) |

## 1. Executive Summary

O HMI do neural-os-core é funcional mas **determinístico**: 7 apps fixos, 4 camadas fixas, avatar com 4 estados fixos. O estado-da-arte 2025-2026 mostra uma direção radicalmente diferente — **interfaces geradas por IA, espaços de embedding como paisagens 3D, renderizadores neurais, e avatares que são telemetria viva**.

Este ADR propõe **5 pilares** que transformam o HMI determinístico num **Neural Desktop** — uma interface que cresce, aprende, e se adapta:

| Pilar | O quê | Inspiração | Diferencial |
|-------|-------|-----------|-------------|
| **H1** | Janelas geradas por IA — UI como JSON declarativo renderizado nativamente | A2UI (Google), Generative UI (ACL 2026), Macaron-A2UI | Agente gera UI sob demanda; 72% preferência sobre chat |
| **H2** | Desktop 3D do espaço de embeddings — conceitos como geometria navegável | leOS (hypersphere desktop), Noosphere | O usuário "voa" pelo conhecimento do sistema |
| **H3** | Compositor neural — modelo leve aprende a desenhar a UI | NeuralOS (ICLR 2026) diffusion renderer | 1 modelo substitui 2000 LOC de compositor |
| **H4** | Avatar = telemetria viva — emoção, confiança, carga cognitiva | leOS displacement codec, JARVIS SOUL.md | Avatar reflete estado interno real do sistema |
| **H5** | Thought Canvas — agente visualiza pensamento durante raciocínio | leOS thought canvas (Gaussian splats) | Debug visual de inferência do modelo |

---

## 2. Estado Atual do HMI

### 2.1 O que temos hoje

```
crates/jarbas/src/display/  (~1700 LOC)
├── mod.rs              — módulo raiz
├── fb.rs               — Framebuffer BGRA32, DoubleBuffer, swap()
├── compositor.rs       — JarvisDesktop: 4 camadas (Layer::OrbBackground, HermesOverlay, AppWindows, DockBar)
├── console.rs          — NeuralConsole: layout multi-região
├── agent.rs            — DisplayAgent: subscreve HERMES_RESPONSE, renderiza
├── avatar.rs           — JarvisAvatar: 64 partículas, 4 estados (Idle/Listening/Processing/Speaking)
├── font.rs             — VGA 8x16 bitmap + text helpers
├── ttf_engine.rs       — TrueType rasterizer + FontManager
└── theme.rs            — 5 temas (hermes-dark, dracula, matrix, solarized, hermes-light)

Aplicativos:
  F1=HermesChat, F2=Settings, F3=Power, F4=BitNet IDE, F10=Camera, F11=AudioViz, F12=WASM
```

### 2.2 Limitações fundamentais

1. **Apps fixos**: o código de cada app está compilado no kernel. Adicionar um novo app = recompilar
2. **Compositor imperativo**: `render()` desenha pixels manualmente (fill_rect, draw_text, set_pixel). Sem abstração de "UI component"
3. **Avatar decorativo**: partículas bonitas mas não refletem estado real do sistema
4. **Sem visualização do espaço latente**: o modelo pensa em 1024 dimensões, o usuário vê texto
5. **Sem UI gerativa**: cada interação é chat de texto, não UI adaptativa
6. **Temas estáticos**: 5 temas fixos, sem geração dinâmica de tema

### 2.3 Pipeline de renderização atual

```
DisplayAgent::tick()
  ├─ poll EventBus (HERMES_RESPONSE, MOUSE_*, KEYBOARD)
  ├─ JarvisDesktop::render()
  │   ├─ Layer::OrbBackground  → background + avatar
  │   ├─ Layer::HermesOverlay  → últimas 8 linhas do chat (semi-transparente)
  │   ├─ Layer::AppWindows     → conteúdo do app ativo
  │   └─ Layer::DockBar        → ícones + cursor mouse
  └─ DoubleBuffer::swap()      → write_volatile back→front
```

---

## 3. Análise Comparativa: SotA 2025-2026

### 3.1 NeuralOS — ICLR 2026 (Rivard et al., University of Waterloo)

```
Paper: NeuralOS: Towards Simulating Operating Systems via Neural Generative Models
Publicação: ICLR 2026 Poster
Código: https://github.com/yuntian-group/neural-os
Demo interativa: https://neural-os.com/
```

**Ideia central**: SO inteiro simulado por rede neural. RNN mantém estado (apps abertos, janelas), diffusion UNet renderiza o frame.

```
Arquitetura (simplificada):
  Input (mouse, teclado) → RNN (2× LSTM 4096d) → Diffusion UNet (263M params) → Frame 512×384
```

**Achados chave**:
- **Gaussian heatmap para cursor**: reduziu erro de 130px → 1.6px codificando posição como mapa 2D gaussiano
- **Synthetic data → app inexistente**: treinou Doom com dados sintéticos — modelo "roda" Doom mesmo sem Doom instalado
- **Scheduled sampling**: essencial para sequências longas (>30 frames sem drift)
- **Currículo 4 estágios**: pretrain RNN → joint → scheduled sampling → extension

**Limitações**: 512×384, 1.8fps no H100, 23.000 GPU-horas de treino, keyboard impreciso.

**O que pegar**: o conceito de renderer neural. Não pra substituir nosso compositor (inviável em bare-metal CPU), mas pra **aumentar** — um modelo leve que desenha componentes específicos (ex: gráficos, avatares).

### 3.2 leOS — Latent Embedding Operating System (2026)

```
Repo: https://github.com/AnOversizedMooseWithSocks/leOS
Core: https://github.com/AnOversizedMooseWithSocks/leCore
Site: https://discoverleos.com/
```

**Ideia central**: Tudo — memória, ferramentas, decisões, padrões aprendidos — vive como vetores na superfície de uma hiperesfera. O desktop 3D (Three.js) é a **representação espacial do espaço de embeddings**.

```
Três razões para o desktop existir:
1. Dá olhos ao agente: descreve a tela, clica, digita, arrasta — vê o que está fazendo
2. Dá corpo ao sistema: o desktop 3D é o espaço de embeddings renderizado como geometria
3. Dá janela ao humano: dashboard, KB browser, plan viewer — tudo web
```

**Achados chave**:
- **Thought Canvas**: matriz 256×224 numpy onde agentes renderizam Gaussian splats (8 floats: posição, escala, rotação, cor, opacidade) enquanto pensam. Não decorativo — é espaço de trabalho espacial
- **SplatFitter**: decompõe imagens em representações de splats. Aprende a desenhar praticando
- **MonologueRenderer**: canvas + TTS + emoção → vídeo MP4 → triple-embed (visão+audio+texto)
- **Deslocamento codec**: trajetória de cada interação gravada como vetor tangente na hiperesfera. Compressão H.264-style (I-frames + P-frames)
- **LVM (Latent Virtual Machine)**: 4 modelos CPU-only (nomic-embed 768d, Qwen3 1024d, ImageBind 1024d)

**O que pegar**: desktop como embedding space. Thought canvas. Splat rendering.

### 3.3 Generative UI — ACL 2026 Findings (SALT-NLP, Stanford)

```
Paper: Generative Interfaces for Language Models
Código: https://github.com/SALT-NLP/GenUI
```

**Ideia central**: LLM gera UI dinamicamente pra cada query do usuário. Em vez de resposta textual, o modelo gera HTML/CSS/JS com lógica interativa.

```
Pipeline:
  Query → Requirement Spec → Structured Representation → UI Code → Iterative Refinement (×5)
                                                                          ↓
                                                              Adaptive Reward Function
```

**Resultados**:
- **72% preferência humana** sobre chat textual
- Structured representation com finite state machines para modelar interação
- Component codebase: 7 componentes reutilizáveis (chart, map, timer, video, code viewer)
- Iterative refinement: gera múltiplos candidatos → LLM avalia → regenera

**O que pegar**: pipeline de geração de UI por LLM. Aplicável ao nosso BitNet para gerar descrições de UI que o compositor renderiza.

### 3.4 A2UI — Google (2026)

```
Repo: https://github.com/google/A2UI
Site: https://a2ui.org/
Versão: v0.9 (Jul 2026)
```

**Ideia central**: Agentes "falam UI" via JSON declarativo. Cliente renderiza com componentes nativos. Seguro como dado, expressivo como código.

```
Agente → JSON A2UI → Transport (A2A/AG-UI) → Client → Renderer → UI Nativa
```

**Princípios**:
- **Declarativo, não executável**: agente envia intenção, não código arbitrário
- **Catálogo de componentes confiáveis**: cliente mantém lista de componentes pré-aprovados
- **Framework-agnostic**: mesmo JSON funciona em Flutter, Angular, Lit, React
- **Incrementalmente atualizável**: UI como lista plana de componentes com ID — LLM-friendly

**O que pegar**: formato declarativo de UI (JSON). Nosso compositor podia interpretar JSON A2UI ou similar em vez de hardcodar apps.

### 3.5 Macaron-A2UI (Apple, 2026)

```
Paper: Macaron-A2UI: A Model for Generative UI in Personal Agents
arXiv: 2605.24830
```

Modelos de 30B-754B treinados especificamente para gerar UI. 75.6 no A2UI-Bench, superando baselines com schema completo.

**O que pegar**: validação que um modelo pode ser treinado para gerar UI de alta qualidade. Nosso BitNet podia ter um expert específico para geração de layouts.

### 3.6 DuetUI (2025)

```
Paper: DuetUI: A Bidirectional Context Loop for Human-Agent Co-Generation
arXiv: 2509.13444
```

**Human-agent co-generation**: agente scaffolding da tarefa → usuário manipula UI → agente interpreta manipulação como guia implícita → regenera UI.

**Bidirectional Context Loop**:
1. Agente decompõe tarefa em UI scaffold
2. Usuário manipula UI (clica, arrasta, edita)
3. Agente interpreta manipulação como refinamento de intent
4. Agente regenera UI
5. Loop até conclusão

**O que pegar**: o loop bidirecional. Nossa interação hoje é linear (usuário→Hermes→resposta). DuetUI mostra interação iterativa onde UI é o meio de comunicação.

### 3.7 D-GUI — Dynamic Generative UI Framework (2026)

```
Paper: D-GUI: A Token-Based Architecture for AI-Native Interfaces
Zenodo: 10.5281/zenodo.19475356
```

Três planos separados: **intent** (o que usuário quer), **layout** (como compor), **interaction state** (o que usuário já fez). Safety envelope com mandatory policy layer, sandbox, deterministic fallback.

**O que pegar**: separação de concerns no design de UI gerativa. Safety envelope para garantir que UI gerada não quebra.

### 3.8 Noosphere / HyperView — Visualização de Embeddings

```
Noosphere: https://github.com/davidkny22/Noosphere
HyperView: https://github.com/Hyper3Labs/HyperView
```

**Noosphere**: 10K+ palavras embedadas → PaCMAP 3D → HDBSCAN → nuvem de pontos interativa no browser. Bias probe, analogias, nearest neighbor.

**HyperView**: Hiperbólico para datasets long-tail. Poincaré disk preserva hierarquia sem distorção.

**O que pegar**: técnicas de visualização de embedding space que podemos portar para o framebuffer.

### 3.9 Resumo comparativo

| Projeto | UI Gerativa | Embedding Viz | Neural Render | Avatar Vivo | Thought Canvas | Bare-metal |
|---------|------------|---------------|---------------|-------------|----------------|------------|
| **NeuralOS** (ICLR 2026) | ✅ Diffusion | ❌ | ✅ UNet 263M | ❌ | ❌ | ❌ (H100) |
| **leOS** (2026) | ❌ | ✅ Hypersphere 3D | ❌ | ❌ | ✅ Splats 256×224 | ❌ (Python) |
| **GenUI** (ACL 2026) | ✅ LLM → HTML | ❌ | ❌ | ❌ | ❌ | ❌ |
| **A2UI** (Google 2026) | ✅ JSON declarativo | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Macaron-A2UI** (2026) | ✅ 30B+ models | ❌ | ❌ | ❌ | ❌ | ❌ |
| **DuetUI** (2025) | ✅ Co-generation | ❌ | ❌ | ❌ | ❌ | ❌ |
| **D-GUI** (2026) | ✅ Token-based | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Noosphere** (2026) | ❌ | ✅ PaCMAP 3D | ❌ | ❌ | ❌ | ❌ |
| **HyperView** (2026) | ❌ | ✅ Hyperbolic | ❌ | ❌ | ❌ | ❌ |
| **neural-os-core (atual)** | ❌ Apps fixos | ❌ Sem viz | ❌ Compositor imperativo | ⚠️ Partículas genéricas | ❌ | ✅ Bare-metal |
| **neural-os-core (alvo)** | ✅ JSON declarativo + BitNet expert | ✅ Embedding space 3D no FB | ✅ Modelo leve para componentes específicos | ✅ Telemetria viva com dados reais | ✅ Splats 256×224 para debug | ✅ Bare-metal |

**Diferencial único**: seremos os únicos com UI gerativa + embedding viz + avatar vivo em bare-metal Rust.

---

## 4. Pilar H1 — Generative App Windows: UI como Dado, Não como Código

### 4.1 Conceito

Hoje, cada app (Settings, Power, Camera) é código Rust compilado no kernel. Para adicionar um app: codificar Rust → recompilar → reboot.

Com H1, apps são **descritos em JSON declarativo**, gerados pelo LLM sob demanda, e renderizados pelo compositor nativamente.

```
Fluxo H1:
  Usuário: "mostre o uso de CPU e memória nos últimos 60 segundos"
     ↓
  Hermes: interpreta intent → gera UI JSON
     ↓
  Compositor: recebe JSON → renderiza componentes nativos → exibe
     ↓
  Usuário interage (clica, arrasta) → eventos voltam pra Hermes
```

### 4.2 Formato de UI declarativo

```rust
// jarbas/src/display/ui_format.rs — NOVO

/// Componente de UI declarativo (inspirado em A2UI + GenUI)
/// O LLM gera este JSON, o compositor renderiza
#[derive(Serialize, Deserialize)]
pub struct UiDeclaration {
    pub id: String,                    // identificador único
    pub title: Option<String>,         // título da janela
    pub components: Vec<UiComponent>,  // lista de componentes
    pub layout: LayoutType,            // grid | flex | freeform
    pub bindings: Vec<DataBinding>,    // binds a dados do sistema
}

#[derive(Serialize, Deserialize)]
pub enum UiComponent {
    Text {
        id: String,
        content: String,
        style: TextStyle,
    },
    Chart {
        id: String,
        data_source: DataSource,     // "cpu.usage", "memory.heap"
        chart_type: ChartType,       // Line, Bar, Scatter
        window: u64,                 // últimos N ticks
    },
    Gauge {
        id: String,
        data_source: DataSource,
        min: f32,
        max: f32,
    },
    Table {
        id: String,
        headers: Vec<String>,
        data_source: DataSource,     // "process.list"
    },
    Button {
        id: String,
        label: String,
        action: UiAction,            // "toggle:wifi", "command:shutdown"
    },
    Slider {
        id: String,
        data_source: DataSource,     // "audio.volume"
        min: f32,
        max: f32,
    },
    SplatCanvas {
        id: String,
        width: u32,
        height: u32,                 // Thought canvas embutido
    },
    EmbeddingCluster {
        id: String,
        query: Option<String>,       // cluster em torno de conceito
        max_points: usize,           // 100-1000 pontos
    },
}
```

### 4.3 Renderização de componentes

Cada `UiComponent` tem uma implementação de renderização nativa no compositor:

```rust
// jarbas/src/display/renderer.rs — NOVO

pub struct UiRenderer {
    fb: &'static mut DoubleBuffer,
    font: &'static FontManager,
    components: BTreeMap<String, Box<dyn RenderableComponent>>,
}

impl UiRenderer {
    /// Renderizar declaração de UI inteira
    pub fn render(&mut self, decl: &UiDeclaration, rect: Rect) {
        match decl.layout {
            LayoutType::Grid(cols) => self.render_grid(decl, rect, cols),
            LayoutType::Flex(dir) => self.render_flex(decl, rect, dir),
            LayoutType::Freeform => self.render_freeform(decl, rect),
        }
    }

    /// Renderizar componente específico
    fn render_component(&mut self, comp: &UiComponent, rect: Rect) {
        match comp {
            UiComponent::Chart { data_source, chart_type, window, .. } => {
                let data = self.query_data(data_source, *window);
                self.draw_chart(&data, *chart_type, rect);
            }
            UiComponent::Gauge { data_source, min, max, .. } => {
                let val = self.query_scalar(data_source);
                self.draw_gauge(val, *min, *max, rect);
            }
            UiComponent::SplatCanvas { width, height, .. } => {
                // Thought canvas embutido numa janela
                self.render_splat_canvas(rect);
            }
            UiComponent::EmbeddingCluster { query, max_points, .. } => {
                // Visualização de embedding space numa janela
                self.render_embedding_cluster(query.as_deref(), *max_points, rect);
            }
            // ... outros componentes
        }
    }
}
```

### 4.4 Geração do JSON via BitNet expert

Em vez de chamar API externa, usamos nosso BitNet com um expert específico para geração de UI:

```rust
// cortex/src/experts/ui_gen.rs — NOVO

pub struct UiGenExpert {
    model: &'static BitNetModel,
    prompt_template: &'static str,
}

impl UiGenExpert {
    /// Gerar UI declaration a partir de descrição em linguagem natural
    pub fn generate(&self, description: &str, context: &UiContext) -> UiDeclaration {
        let prompt = alloc::format!(
            "Gere UI JSON para: {}\n\nComponentes disponíveis: Text, Chart, Gauge, Table, Button, Slider\n\
             Dados disponíveis: cpu.usage, memory.heap, process.list, audio.volume, net.traffic, \
             temp.cpu, fan.speed, disk.usage, wifi.signal, battery.level\n\
             Formato de saída: {{ \"id\": \"...\", \"title\": \"...\", \"components\": [...], \"layout\": \"grid|flex\" }}",
            description
        );
        let output = self.model.generate(&prompt, UiGenParams::default());
        serde_json::from_str(&output).unwrap_or_default()
    }
}

/// Geração iterativa com refinamento (inspirado GenUI ACL 2026)
pub fn generate_with_refinement(
    expert: &UiGenExpert,
    description: &str,
    max_iterations: usize,
) -> UiDeclaration {
    let mut best: Option<UiDeclaration> = None;
    let mut best_score: f32 = 0.0;

    for _ in 0..max_iterations {
        let candidate = expert.generate(description, &UiContext::current());
        let score = evaluate_ui(&candidate);  // LLM avalia qualidade
        if score > best_score {
            best_score = score;
            best = Some(candidate);
        }
    }

    best.unwrap_or_default()
}
```

### 4.5 Exemplo concreto

```json
// Gerado pelo UiGenExpert para "mostre CPU e memória"
{
  "id": "sysmon-001",
  "title": "Monitor do Sistema",
  "layout": "grid(2)",
  "components": [
    {
      "type": "Chart",
      "id": "cpu-chart",
      "data_source": "cpu.usage",
      "chart_type": "Line",
      "window": 300
    },
    {
      "type": "Chart",
      "id": "mem-chart",
      "data_source": "memory.heap",
      "chart_type": "Line",
      "window": 300
    },
    {
      "type": "Gauge",
      "id": "cpu-gauge",
      "data_source": "cpu.usage",
      "min": 0,
      "max": 100
    },
    {
      "type": "Gauge",
      "id": "mem-gauge",
      "data_source": "memory.heap",
      "min": 0,
      "max": 512
    },
    {
      "type": "Table",
      "id": "process-table",
      "headers": ["PID", "Nome", "CPU%", "Memória"],
      "data_source": "process.list"
    }
  ]
}
```

### 4.6 Integração com o LatentBus (ADR-0047 Pilar 1)

O JSON de UI pode ser **transmitido via LatentBus** em vez de EventBus textual:

```rust
// Publisher (UiGenExpert)
let decl = self.generate(&description, &context);
let latent = self.project_ui_to_latent(&decl);
LATENT_BUS.publish(LatentPacket {
    topic: hash("UI_DECLARATION"),
    vector: latent,
    source_agent: AGENT_ID_CORTEX,
    ..default()
});

// Subscriber (UiRenderer)
if let Some(packet) = latent_receiver.try_receive() {
    let decl = self.project_latent_to_ui(&packet.vector);
    self.render(&decl, self.window_rect);
}
```

### 4.7 Métricas esperadas

| Métrica | Apps fixos (hoje) | Generative UI | Ganho |
|---------|------------------|---------------|-------|
| **Tempo p/ novo app** | Dias (codificar + compilar) | Segundos (LLM gera JSON) | ~1000× |
| **Variedade de UIs** | 7 apps | Ilimitada (qualquer JSON válido) | ∞ |
| **Adaptação p/ usuário** | Nenhuma (todos veem mesma UI) | UI gerada por contexto/preferência | Personalizada |
| **Preferência humana** | Baseline | +72% sobre chat (GenUI) | Significativo |
| **Latência de geração** | N/A (compilado) | ~500ms (BitNet expert) | Aceitável |

---

## 5. Pilar H2 — Embedding Space Desktop: O Espaço Latente como Paisagem 3D

### 5.1 Conceito

O leOS mostrou que o desktop 3D pode ser a **representação espacial do espaço de embeddings**. Cada conceito que o sistema conhece tem uma posição no espaço latente. O desktop permite "voar" por esse espaço.

```
O que o usuário vê:
  • Pontos = conceitos/conhecimento do sistema
  • Cores = clusters semânticos (HDBSCAN)
  • Proximidade = similaridade semântica
  • Órbitas = trajetórias de pensamento do agente
  • Densidade = regiões de conhecimento vs ignorância
  • Anomalias = outliers, descobertas inesperadas
```

### 5.2 Projeção 1024D → 3D para framebuffer

```rust
// jarbas/src/display/embedding_viz.rs — NOVO

/// Visualização do espaço de embeddings no framebuffer
pub struct EmbeddingSpace {
    points: Vec<EmbeddingPoint>,
    projection: ProjectionMatrix,   // PCA ou PaCMAP pré-computado
    camera: Camera3D,
    clusters: Vec<Cluster>,
}

pub struct EmbeddingPoint {
    pub id: u64,
    pub label: Option<String>,
    pub pos_3d: (f32, f32, f32),      // após projeção 1024D → 3D
    pub cluster_id: usize,
    pub intensity: f32,                // 0.0-1.0 (importância/confiança)
    pub concept: ConceptType,          // Agent | Skill | Memory | Hardware | etc
}

impl EmbeddingSpace {
    /// Projetar embedding 1024D para 3D
    pub fn project(embedding: &[f32; 1024], pca: &PCA) -> (f32, f32, f32) {
        // Truncado para 3 componentes principais
        let x = pca.components[0].iter()
            .zip(embedding.iter()).map(|(w, v)| w * v).sum::<f32>();
        let y = pca.components[1].iter()
            .zip(embedding.iter()).map(|(w, v)| w * v).sum::<f32>();
        let z = pca.components[2].iter()
            .zip(embedding.iter()).map(|(w, v)| w * v).sum::<f32>();
        (x, y, z)
    }

    /// Renderizar no framebuffer (projeção 3D → 2D com profundidade)
    pub fn render(&self, fb: &mut DoubleBuffer, camera: &Camera3D) {
        // Ordenar pontos por profundidade (painter's algorithm)
        let mut visible: Vec<ProjectedPoint> = self.points.iter()
            .map(|p| {
                let (sx, sy, sz) = camera.project(p.pos_3d);
                ProjectedPoint { screen_x: sx, screen_y: sy, depth: sz, source: p }
            })
            .filter(|p| p.depth > 0.0 && p.screen_x >= 0 && p.screen_x < FB_WIDTH)
            .collect();
        visible.sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap());

        // Desenhar pontos (mais distantes primeiro)
        for p in &visible {
            let color = self.cluster_color(p.source.cluster_id);
            let radius = (1.0 / p.depth).min(4.0).max(1.0);
            fb.fill_circle(p.screen_x as i32, p.screen_y as i32, radius as i32, color);
        }

        // Desenhar conexões (arestas semânticas)
        for edge in &self.edges {
            let a = camera.project(self.points[edge.a].pos_3d);
            let b = camera.project(self.points[edge.b].pos_3d);
            if a.2 > 0.0 && b.2 > 0.0 {
                fb.draw_line(a.0 as i32, a.1 as i32, b.0 as i32, b.1 as i32, EDGE_COLOR);
            }
        }
    }
}
```

### 5.3 Clusterizaçao + Labeling

```rust
impl EmbeddingSpace {
    /// Executar HDBSCAN nos pontos projetados para identificar clusters
    pub fn cluster_points(&mut self, min_cluster_size: usize) {
        // Adaptação simplificada de HDBSCAN para no_std
        // Usa matriz de distância euclidiana 3D
        let n = self.points.len();
        let mut dists = vec![0.0f32; n * n];
        for i in 0..n {
            for j in 0..n {
                let a = self.points[i].pos_3d;
                let b = self.points[j].pos_3d;
                dists[i * n + j] = ((a.0 - b.0).powi(2) +
                                    (a.1 - b.1).powi(2) +
                                    (a.2 - b.2).powi(2)).sqrt();
            }
        }
        // ... core distance, mutual reachability, MST, cluster hierarchy
        // ponytail: HDBSCAN completo é complexo. Usar DBSCAN com epsilon fixo
        // Upgrade path: implementar HDBSCAN completo se DBSCAN for insuficiente
        self.dbscan(3.0, min_cluster_size);
    }

    /// Nomear clusters automaticamente (via BitNet)
    pub fn label_clusters(&mut self, llm: &BitNetModel) {
        for cluster in self.clusters.iter_mut() {
            let members: Vec<&str> = cluster.member_indices.iter()
                .filter_map(|i| self.points[*i].label.as_deref())
                .collect();
            let prompt = alloc::format!(
                "Dado este cluster de conceitos: {}\n\
                 Qual o tema comum? Responda em 2-3 palavras.",
                members.join(", ")
            );
            cluster.name = llm.generate(&prompt, Default::default());
        }
    }
}
```

### 5.4 Navegação por voo 3D

```rust
pub struct Camera3D {
    pub pos: (f32, f32, f32),
    pub target: (f32, f32, f32),
    pub up: (f32, f32, f32),
    pub fov: f32,
}

impl Camera3D {
    /// Projetar ponto 3D → tela 2D com profundidade
    pub fn project(&self, point: (f32, f32, f32)) -> (f32, f32, f32) {
        // LookAt matrix + perspective projection
        // ... (cálculo padrão de câmera 3D)
    }

    /// Mover câmera em direção a um embedding
    pub fn fly_to(&mut self, target: (f32, f32, f32), speed: f32) {
        self.pos = lerp(self.pos, target, speed);
        self.target = target;
    }

    /// "Teleport" semântico: voar para o cluster de um conceito
    pub fn semantic_teleport(&mut self, concept: &str, space: &EmbeddingSpace) {
        if let Some(point) = space.find_nearest(concept) {
            self.fly_to(point.pos_3d, 0.05);
        }
    }
}
```

### 5.5 Integração com o ciclo SleepCycle

Durante a fase DREAM do SleepCycle, o espaço de embeddings é atualizado:

```rust
// SleepCycleAgent modificado
fn phase_dream_embedding(&mut self) {
    // 1. Coletar embeddings de todas as skills, memórias, agentes
    let embeddings = self.collect_active_embeddings();

    // 2. Recalcular projeção PCA (incremental)
    EMBEDDING_SPACE.lock().update_projection(&embeddings);

    // 3. Recalcular clusters
    EMBEDDING_SPACE.lock().cluster_points(5);

    // 4. Nomear novos clusters
    EMBEDDING_SPACE.lock().label_clusters(&BITNET_MODEL);

    // 5. Publicar "EMBEDDING_SPACE_UPDATED" no EventBus
    // → DisplayAgent marka para re-renderizar
}
```

### 5.6 Métricas esperadas

| Métrica | Hoje (sem viz) | Com embedding space |
|---------|---------------|-------------------|
| **Compreensão do espaço de conhecimento** | Nenhuma | Visual: clusters, distâncias, outliers |
| **Debug de similaridade semântica** | Texto ("similar a X") | Visual: proximidade 3D |
| **Detecção de outliers** | Log analysis manual | Pontos isolados visíveis |
| **Navegação por conceitos** | Busca textual | "Voo" pelo espaço |

---

## 6. Pilar H3 — Neural Renderer: Compositor que Aprendeu a Desenhar

### 6.1 Conceito

Inspirado no NeuralOS (ICLR 2026), um modelo leve que aprende a renderizar a UI. Em vez de 2000 LOC de código imperativo (fill_rect, draw_text, etc.), um pequeno modelo recebe o "estado da UI" e gera o frame.

```
Estado da UI (80 bytes):
  - apps abertos (bitmask 8 bits)
  - posição do mouse (2 × u16)
  - tema ID (u8)
  - última mensagem (até 256 chars → embedding)
  - avatar state (u8)
  - FFT audio bins (16 × f32)

  → Neural Renderer (MLP 3 camadas, 32 hidden)
  → Frame 1280×720 BGRA32
```

### 6.2 Arquitetura do modelo

```rust
pub struct NeuralRenderer {
    // MLP leve: 80 entradas → 64 hidden → 128 hidden → frame chunk
    // Em vez de gerar frame inteiro (1280×720 = 3.7M pixels = inviável),
    // gerar "dicas de renderização" que o compositor usa
    model: MLP,
    output_shape: RenderHint,  // chunks 16×16
}

pub struct RenderHint {
    pub chunk_x: u16,          // posição do chunk
    pub chunk_y: u16,
    pub avg_color: [u8; 4],    // cor média do chunk (RGBA)
    pub dominant: DominantType, // Text | Gradient | Edge | Solid | Particle
    pub params: [f32; 8],      // parâmetros específicos do tipo
}

impl NeuralRenderer {
    /// Gerar hints de renderização para frame atual
    pub fn render_frame(&self, state: &UiState) -> Vec<RenderHint> {
        let mut hints = Vec::new();
        // Dividir frame em chunks 16×16
        for y in (0..FB_HEIGHT).step_by(16) {
            for x in (0..FB_WIDTH).step_by(16) {
                let input = self.build_input(state, x, y);
                let output = self.model.forward(&input);
                hints.push(RenderHint {
                    chunk_x: x, chunk_y: y,
                    avg_color: [output[0], output[1], output[2], output[3]],
                    dominant: decode_dominant(output[4]),
                    params: output[5..13].try_into().unwrap(),
                });
            }
        }
        hints
    }
}
```

### 6.3 Treinamento do modelo

```rust
/// Coletar dados de treino: frames reais do compositor + estado da UI
pub fn collect_training_data() {
    loop {
        // 1. Renderizar frame com compositor atual (ground truth)
        let frame = compositor.render_frame();
        let state = UiState::snapshot();

        // 2. Salvar par (state, frame_chunks)
        training_db.save(state, frame);

        // 3. A cada 1000 amostras, fine-tune modelo
        if training_db.count() % 1000 == 0 {
            neural_renderer.fine_tune(training_db.sample(100));
        }
    }
}
```

### 6.4 Integração híbrida

O neural renderer é **aumentativo**, não substitutivo:

```rust
// Compositor híbrido
pub fn render(&mut self, state: &UiState) {
    if NEURAL_RENDERER.is_trained() {
        // Modo neural: modelo gera hints, compositor só executa
        let hints = NEURAL_RENDERER.render_frame(state);
        self.execute_hints(&hints);
    } else {
        // Modo clássico: compositor imperativo
        self.render_classic(state);
    }
}
```

---

## 7. Pilar H4 — Soul Mirror: Avatar Como Telemetria Viva

### 7.1 Conceito

O avatar atual (partículas coloridas) é bonito mas não reflete o estado real do sistema. Com H4, cada aspecto visual do avatar mapeia **dados reais**:

```
Aspecto do avatar    → Dado real do sistema
─────────────────      ─────────────────────
Cor predominante     → Emoção do Hermes (ADE: valence 0-1)
Velocidade de pulso  → Carga cognitiva (agents ativos / total)
Tamanho              → Confiança do TrustAgent (0-1)
Partículas de fundo  → FFT do áudio ambiente (já implementado)
Anéis ao redor       → Tokens/s de inferência do BitNet
Explosões            → HEALTH_ISSUE detectado
Rotação              → Fase do SleepCycle (IDLE→REPLAY→DREAM→...)
Brilho               → Cache hit ratio do EventBus
```

### 7.2 Implementação

```rust
// jarbas/src/display/soul_mirror.rs — NOVO

pub struct SoulMirror {
    avatar: &'static mut JarvisAvatar,
    metrics: SystemMetrics,
}

impl SoulMirror {
    /// Atualizar avatar com dados reais do sistema
    pub fn tick(&mut self) {
        // Coletar métricas
        let valence = HERMES_EMOTION.lock().valence;  // 0.0-1.0
        let cognitive_load = ACTIVE_AGENTS.load(Ordering::Relaxed) as f32 / MAX_AGENTS as f32;
        let trust_level = TRUST_AVG.load(Ordering::Relaxed);  // 0.0-1.0
        let tps = TOKENS_PER_SEC.load(Ordering::Relaxed) as f32;
        let sleep_phase = SLEEP_PHASE.load(Ordering::Relaxed);
        let health_alerts = HEALTH_ALERT_COUNT.load(Ordering::Relaxed);

        // Mapear para avatar
        // Cor: valence → RGB (azul frio = triste, laranja = feliz, vermelho = alerta)
        self.avatar.primary_color = self.valence_to_color(valence);

        // Pulso: carga cognitiva → período de pulso (200-1000ms)
        self.avatar.pulse_period = 200 + (cognitive_load * 800.0) as u64;

        // Tamanho: confiança → raio base (20-40 pixels)
        self.avatar.base_radius = 20 + (trust_level * 20.0) as u32;

        // Anéis: tokens/s → número de anéis concêntricos
        self.avatar.ring_count = (tps / 10.0).min(8.0) as u32;

        // Explosões: health alerts → partículas explosivas
        if health_alerts > 0 {
            self.avatar.trigger_explosion(health_alerts);
        }

        // Rotação: SleepCycle phase → ângulo
        self.avatar.rotation_angle = (sleep_phase as f32) * 72.0_f32.to_radians();  // 5 fases, 72° cada
    }

    fn valence_to_color(&self, valence: f32) -> [u8; 4] {
        let r = (valence * 200.0 + 55.0) as u8;  // 55-255
        let b = ((1.0 - valence) * 200.0 + 55.0) as u8;
        [r, 100, b, 255]
    }
}
```

### 7.3 Estados expandidos do avatar

```rust
// AvatarStates expandido de 4 para 8 estados
pub enum AvatarState {
    Idle,           // padrão — pulso lento azul
    Listening,      // ciano pulsante
    Processing,     // laranja girando
    Speaking,       // verde com ondas
    Thinking,       // roxo com anéis concêntricos (inferência em andamento)
    Dreaming,       // índigo com partículas lentas (SleepCycle ativo)
    Alert,          // vermelho pulsando rápido (HEALTH_ISSUE)
    Updating,       // amarelo com rotação (boot/update em andamento)
}
```

### 7.4 Integração com o EmotionEngine (SOUL.md)

```rust
// hermes/src/jarvis.rs — modificação

impl SoulProfile {
    pub fn update_avatar_emotion(&self, avatar: &mut SoulMirror) {
        // ADE: Agent Dominant Emotion
        let emotion = self.current_emotion();
        avatar.set_valence(emotion.valence);
        avatar.set_arousal(emotion.arousal);
        avatar.set_dominance(emotion.dominance);

        // Publicar no LatentBus para outros agentes
        LATENT_BUS.publish(LatentPacket {
            topic: hash("AVATAR_STATE"),
            vector: self.emotion_to_latent(&emotion),
            source_agent: AGENT_ID_HERMES,
            ..default()
        });
    }
}
```

---

## 8. Pilar H5 — Thought Canvas: O Agente Vê o Que Está Pensando

### 8.1 Conceito

O leOS tem um "thought canvas" — uma matriz 256×224 onde o agente renderiza Gaussian splats enquanto pensa. Cada splat = 8 floats: posição, escala, rotação, cor, opacidade. O canvas não é decorativo — é um espaço de trabalho espacial onde o agente organiza conceitos visualmente durante o raciocínio.

### 8.2 Implementação

```rust
// jarbas/src/display/thought_canvas.rs — NOVO

/// Canvas de pensamento: 256×224 pixels, renderizado com Gaussian splats
pub struct ThoughtCanvas {
    pixels: [[Pixel; CANVAS_WIDTH]; CANVAS_HEIGHT], // array fixo, sem alloc
    splats: [Splat; MAX_SPLATS],
    splat_count: usize,
}

#[repr(C)]
pub struct Splat {
    pub x: f32,           // posição X (0-255)
    pub y: f32,           // posição Y (0-223)
    pub scale: f32,       // raio (0.5-8.0 pixels)
    pub rotation: f32,    // ângulo (0-2π)
    pub r: u8,            // cor R
    pub g: u8,            // cor G
    pub b: u8,            // cor B
    pub alpha: u8,        // opacidade (0-255)
}
// Total: 8 floats × 4 bytes = 32 bytes por splat
// 256 splats × 32 bytes = 8KB — cabe em L1 cache

impl ThoughtCanvas {
    /// Renderizar splats no canvas
    pub fn render(&mut self) {
        // Limpar canvas
        self.pixels = [[Pixel::TRANSPARENT; CANVAS_WIDTH]; CANVAS_HEIGHT];

        // Ordenar splats por alpha (translucência)
        let mut ordered: [usize; MAX_SPLATS] = core::array::from_fn(|i| i);
        ordered.sort_by(|&a, &b| self.splats[a].alpha.cmp(&self.splats[b].alpha));

        // Rasterizar cada splat como Gaussian 2D
        for &idx in ordered.iter().take(self.splat_count) {
            let s = self.splats[idx];
            let radius = s.scale.ceil() as i32;
            let cx = s.x as i32;
            let cy = s.y as i32;

            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && px < CANVAS_WIDTH && py >= 0 && py < CANVAS_HEIGHT {
                        // Gaussian falloff
                        let dist2 = (dx*dx + dy*dy) as f32;
                        let gauss = (-dist2 / (2.0 * s.scale * s.scale)).exp();
                        let alpha = (s.alpha as f32 * gauss) as u8;
                        self.pixels[py as usize][px as usize] = Pixel::rgba(s.r, s.g, s.b, alpha);
                    }
                }
            }
        }
    }

    /// Adicionar splat (CortexAgent publica durante inferência)
    pub fn add_splat(&mut self, splat: Splat) {
        if self.splat_count < MAX_SPLATS {
            self.splats[self.splat_count] = splat;
            self.splat_count += 1;
        }
    }

    /// Codificar canvas como LatentPacket para outros agentes
    pub fn to_latent(&self) -> [f16; LATENT_DIM] {
        // Pooling do canvas → vetor latente 256D
        let mut latent = [f16::ZERO; LATENT_DIM];
        for (i, row) in self.pixels.iter().enumerate() {
            for (j, pixel) in row.iter().enumerate() {
                let idx = (i * CANVAS_WIDTH + j) % LATENT_DIM;
                latent[idx] = latent[idx] + f16::from_f32(pixel.luma());
            }
        }
        latent
    }
}
```

### 8.3 Integração com CortexAgent

Durante a inferência do BitNet, as ativações podem ser projetadas como splats no thought canvas:

```rust
// Modificação em cortex/src/cortex.rs

impl BitNetModel {
    pub fn forward_with_thoughts(&self, tokens: &[u16], canvas: &mut ThoughtCanvas) -> (Tensor, Tensor) {
        let (last_hidden, logits) = self.forward_with_kv(tokens, &mut cache);

        // Projetar hidden state de cada layer como splat
        for (layer_idx, layer) in self.layers.iter().enumerate() {
            // Amostrar algumas dimensões do hidden state
            for head in 0..4 {
                let activation_slice = &last_hidden.data[head * 64..(head + 1) * 64];
                let mean = activation_slice.iter().sum::<f32>() / 64.0;
                let variance = activation_slice.iter()
                    .map(|v| (v - mean).powi(2)).sum::<f32>() / 64.0;

                canvas.add_splat(Splat {
                    x: (layer_idx as f32 / NUM_LAYERS as f32) * 255.0,
                    y: (head as f32 / 4.0) * 223.0,
                    scale: variance.sqrt() * 4.0,
                    rotation: mean,
                    r: (mean * 255.0) as u8,
                    g: 128,
                    b: (variance * 255.0).min(255.0) as u8,
                    alpha: 200,
                });
            }
        }

        (last_hidden, logits)
    }
}
```

### 8.4 Display no compositor

O thought canvas aparece como uma janela do compositor (F7 toggle) ou como overlay semi-transparente:

```rust
// Em compositor.rs
fn render_thought_canvas(&self, fb: &mut DoubleBuffer) {
    let canvas = THOUGHT_CANVAS.lock();
    canvas.render();  // renderizar splats → pixels

    // Mapear canvas 256×224 para região da tela
    let scale_x = 256.0 / 1280.0;  // ~5× zoom
    let scale_y = 224.0 / 720.0;   // ~3.2× zoom
    let offset_x = 40;
    let offset_y = 40;

    for py in 0..224 {
        for px in 0..256 {
            let pixel = canvas.pixels[py][px];
            if pixel.alpha > 0 {
                let sx = offset_x + (px as f32 * scale_x) as u32;
                let sy = offset_y + (py as f32 * scale_y) as u32;
                fb.set_pixel(sx, sy, pixel.to_rgba());
            }
        }
    }
}
```

---

## 9. Engenharia Reversa das Referências

### 9.1 O que cada projeto ensina para o neural-os-core

| Projeto | Lição | Aplicação no neural-os-core |
|---------|-------|---------------------------|
| **NeuralOS** (ICLR 2026) | Renderizador neural pode substituir compositor, mas precisa de GPU | Usar modelo leve (MLP 3 camadas) para hints de renderização, não frame inteiro |
| **leOS** (2026) | Desktop 3D como embedding space. Thought canvas com Gaussian splats | Portar conceito para framebuffer 2D. Splats cabem em 8KB |
| **GenUI** (ACL 2026) | UI gerada por LLM com refinamento iterativo (+72% preferência) | BitNet expert para gerar UiDeclaration JSON |
| **A2UI** (Google 2026) | UI declarativa como JSON seguro. Catálogo de componentes confiáveis | Nosso UiRenderer usa catálogo de componentes nativos, não HTML |
| **DuetUI** (2025) | Loop bidirecional: usuário manipula UI → agente adapta | DisplayAgent envia interações de volta pro Hermes como feedback |
| **D-GUI** (2026) | Separar intent, layout, interaction state. Safety envelope | UiDeclaration tem campos separados para cada plano |
| **Noosphere** (2026) | PaCMAP + HDBSCAN para embedding viz 3D | PCA truncado para 3D (mais simples, sem dependências) |
| **HyperView** (2026) | Embeddings hiperbólicos para dados hierárquicos | Poincaré disk se dados forem hierárquicos (skills, agent tree) |
| **Macaron-A2UI** (2026) | Modelo específico para UI generation (30B+) | Validar se BitNet 2B consegue gerar JSON de UI |
| **Feature Regions** (2026) | Hiperelipsoides > hipersferas para regiões semânticas | Embedding clusters como elipsoides, não esferas |
| **H2S** (2021/2026) | Hipersfera2esfera: visualizar distribuições como esferas | Clusters como círculos 2D com raio = variância |

### 9.2 Matriz de decisão

| Funcionalidade | Nosso approach | Referência | Esforço | Risco |
|---------------|---------------|------------|---------|-------|
| UI declarativa JSON | BitNet expert gera UiDeclaration → compositor renderiza | A2UI + GenUI | ~500 LOC | Médio (BitNet precisa gerar JSON válido) |
| Embedding viz 3D | PCA 1024→3D → pontos no framebuffer com profundidade | leOS + Noosphere | ~400 LOC | Baixo (só projeção, sem dependências) |
| Neural renderer | MLP leve gera hints 16×16 → compositor executa | NeuralOS | ~300 LOC + treino | Alto (precisa de dados de treino) |
| Soul mirror | Mapear métricas → avatar. 8 estados | leOS displacement | ~200 LOC | Baixo |
| Thought canvas | Splats 8KB durante inferência | leOS thought canvas | ~300 LOC | Médio (overhead de render) |

---

## 10. Roteiro de Implementação

### 10.1 Fases

```
Fase 0 — Fundação (Sprint 110, ~400 LOC)
├── jarbas/src/display/ui_format.rs   — UiDeclaration + UiComponent (150 LOC)
├── jarbas/src/display/renderer.rs    — UiRenderer base (200 LOC)
└── jarbas/src/display/soul_mirror.rs — SoulMirror struct + mapeamento inicial (50 LOC)

Fase 1 — Generative App Windows (Sprint 110-111, ~600 LOC)
├── cortex/src/experts/ui_gen.rs      — UiGenExpert (BitNet fine-tune) (300 LOC)
├── jarbas/src/display/renderer.rs    — render() para Chart, Gauge, Table (200 LOC)
└── display/agent.rs                  — DisplayAgent processa UiDeclaration (100 LOC)

Fase 2 — Soul Mirror + Avatar expandido (Sprint 111, ~400 LOC)
├── jarbas/src/display/avatar.rs      — 8 estados, métricas reais (200 LOC)
├── hermes/src/jarvis.rs              — Emotion→SoulMirror bridge (100 LOC)
└── k_ai/src/trust.rs                 — TrustAgent publica confiança (100 LOC)

Fase 3 — Embedding Space Desktop (Sprint 111-112, ~500 LOC)
├── jarbas/src/display/embedding_viz.rs — PCA, pontos, clusters, câmera (350 LOC)
├── jarbas/src/display/font.rs          — draw_point_cloud() helper (50 LOC)
└── sleep_cycle/agents.rs               — EmbeddingSpace update no DREAM (100 LOC)

Fase 4 — Thought Canvas (Sprint 112, ~400 LOC)
├── jarbas/src/display/thought_canvas.rs — Splats, render, latent (250 LOC)
├── cortex/src/cortex.rs                 — forward_with_thoughts() (100 LOC)
└── compositor.rs                        — F7 toggle, overlay (50 LOC)

Fase 5 — Neural Renderer (Sprint 113, ~300 LOC + treino)
├── jarbas/src/display/neural_renderer.rs — MLP, hints, treino (200 LOC)
├── cortex/src/models/renderer.rs         — Modelo do renderizador (50 LOC)
└── tools/train_neural_renderer.py        — Script de coleta + treino (50 LOC)
```

### 10.2 Total: ~2.600 LOC

### 10.3 Marcos

| Marco | Sprint | O quê | Critério de aceite |
|-------|--------|-------|-------------------|
| M-H1 | 110 | UiDeclaration + UiRenderer | "mostre CPU" → JSON → Chart renderizado |
| M-H2 | 110 | SoulMirror básico | Avatar muda cor com emoção do Hermes |
| M-H3 | 111 | UiGenExpert funcional | BitNet gera JSON de UI válido para 5 comandos |
| M-H4 | 111 | Avatar 8 estados | Cada estado mapeia dado real do sistema |
| M-H5 | 112 | Embedding viz | 100 pontos projetados, clusterizados, navegáveis |
| M-H6 | 112 | Thought canvas | Inferência gera splats visíveis no compositor |
| M-H7 | 113 | Neural renderer | Modelo gera hints de chunk → compositor renderiza |

---

## 11. Referências

### 11.1 Projetos analisados

1. **NeuralOS** — Rivard, L. et al. (2026). *NeuralOS: Towards Simulating Operating Systems via Neural Generative Models*. ICLR 2026. arXiv:2507.08800. https://neural-os.com/
   - RNN + diffusion renderer, GUI gerada por IA, dados sintéticos

2. **leOS** — (2026). *Latent Embedding Operating System*. https://github.com/AnOversizedMooseWithSocks/leOS
   - Desktop 3D como embedding space, thought canvas, displacement codec

3. **leCore** — (2026). *leOS vector-symbolic core*. https://github.com/AnOversizedMooseWithSocks/leCore
   - NumPy substrate: memória, geometria, física no mesmo espaço

4. **Generative UI / GenUI** — (2025/2026). *Generative Interfaces for Language Models*. ACL 2026 Findings. https://github.com/SALT-NLP/GenUI
   - LLM gera UI com refinamento iterativo, +72% preferência

5. **A2UI** — Google (2026). *A2UI: Agent-to-User Interface*. https://github.com/google/A2UI
   - UI declarativa JSON, catálogo de componentes, framework-agnostic

6. **Macaron-A2UI** — Kong, F. et al. (2026). *Macaron-A2UI: A Model for Generative UI*. arXiv:2605.24830
   - Modelo 30B+ específico para UI generation

7. **DuetUI** — (2025). *DuetUI: A Bidirectional Context Loop for Human-Agent Co-Generation*. arXiv:2509.13444
   - Loop bidirecional: usuário manipula ↔ agente adapta

8. **D-GUI** — Hruznov, V. (2026). *Dynamic Generative UI Framework*. Zenodo:10.5281/zenodo.19475356
   - Token-based, safety envelope, 3 planos separados

9. **Noosphere** — Kny, D. (2026). *Interactive 3D visualization of AI embedding spaces*. https://github.com/davidkny22/Noosphere
   - PaCMAP 3D, HDBSCAN, bias probe, analogias

10. **HyperView** — Hyper3Labs (2026). *Interactive Geometric Workbench for Embedding Space Analysis*. https://github.com/Hyper3Labs/HyperView
    - Embeddings hiperbólicos, Poincaré disk, fairness-aware

11. **Feature Regions** — Hyw, P. (2026). *Exploring feature directions — regions*. https://github.com/patrickhyw/regions
    - Hiperelipsoides > hipersferas para regiões semânticas

12. **H2S (Hypersphere2Sphere)** — (2021/2026). *Visualizing the geometry of labeled high-dimensional data with spheres*. arXiv:2107.00731
    - Distribuições como esferas 3D com raio = variância

### 11.2 Documentação interna

13. ADR-0036 — JARVIS Unified Interaction Layer (SOUL.md, persona)
14. ADR-0042 — K³CHJ N5: Jarbas ego/persona/frontend
15. ADR-0047 — LatentBus + EvolveAgent + NeuOS Probe
16. ADR-0047-GPU — GPU Compute Pipeline
17. `crates/jarbas/src/display/` — Compositor, avatar, font, theme (~1700 LOC)
18. `crates/jarbas/src/jarvis.rs` — SoulProfile, EmotionAnalysis, DreamEngine

---

## Apêndice A: Glossário HMI

| Termo | Definição |
|-------|-----------|
| **UiDeclaration** | JSON declarativo que descreve uma janela/grupo de UI, gerado pelo LLM |
| **UiComponent** | Tipo de componente renderizável (Chart, Gauge, Table, SplatCanvas, etc) |
| **Neural Renderer** | MLP leve que gera hints de renderização baseado no estado da UI |
| **RenderHint** | "Dica" 16×16: cor média, tipo dominante (text/gradient/edge/solid/particle) |
| **Embedding Space Desktop** | Visualização 3D do espaço de embeddings do sistema, projetado PCA 1024→3D |
| **Soul Mirror** | Avatar que reflete métricas reais do sistema (emoção, confiança, carga) |
| **Thought Canvas** | Matriz 256×224 onde agentes renderizam Gaussian splats durante raciocínio |
| **Splat** | 8 floats: pos, scale, rotation, cor, alpha. 32 bytes. Representa ativação |
| **Bidirectional Context Loop** | Usuário manipula UI → agente interpreta → regenera UI → ciclo (DuetUI) |
| **Safety Envelope** | Camada de segurança que valida UI gerada antes de renderizar (D-GUI) |
| **A2UI** | Google standard: UI declarativa JSON, catálogo de componentes confiáveis |

---

## Apêndice B: Comparação HMI neural-os-core vs SotA

| Funcionalidade | neural-os-core (hoje) | leOS | NeuralOS | GenUI | Google A2UI | neural-os-core (alvo) |
|---------------|---------------------|------|----------|-------|-------------|----------------------|
| Compositor | 4 camadas fixas | Three.js 3D | Diffusion UNet | N/A | N/A | Híbrido: imperativo + neural |
| Apps | 7 fixos (F1-F12) | Bone chains + Flask | Apenas simulação | LLM gera HTML | JSON → nativo | JSON declarativo → componentes nativos |
| Avatar | Partículas, 4 estados | N/A | N/A | N/A | N/A | Soul Mirror: 8 estados, métricas reais |
| Embedding viz | ❌ | ✅ Hypersphere 3D | ❌ | ❌ | ❌ | ✅ PCA 1024→3D + clusters |
| Thought canvas | ❌ | ✅ Splats 256×224 | ❌ | ❌ | ❌ | ✅ Splats durante inferência |
| Input | PS/2 mouse + teclado | Web (Flask) | RNN processa input | Texto | Texto + toque | PS/2 + EventBus + LatentBus |
| Tema | 5 fixos | CSS (web) | Gerado por diff | Gerado por LLM | Catálogo de componentes | Tema gerado por BitNet expert |
| Resolução | 1280×720 | Web (qualquer) | 512×384 | Web (qualquer) | Nativa | 1280×720 (sem dependência de GPU) |

---

*Fim do ADR-0047-HMI*
