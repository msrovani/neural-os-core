# ADR-0047: Três Pilares para a Próxima Geração — LatentBus + EvolveAgent + NeuOS Probe

**Data:** 2026-07-16
**Status:** Proposta — análise completa de 20+ projetos/papers 2025-2026
**Depende de:** ADR-0041 (Cap Rings P0-P9), ADR-0042 (K²CHJ N1-N5), ADR-0046 (GGUF Streaming), Sprint 106 (WASM runtime), Sprint 108 (Self-Evolving Agents)
**Sprint:** 109+ (pilares paralelos com ADR-0042)
**Versão alvo:** v2.1.0+ (pilares 1-3 não bloqueiam v2.0.0)

---

## Índice

1. [Executive Summary](#1-executive-summary)
2. [Contexto: Onde Estamos](#2-contexto-onde-estamos)
3. [O Mundo Mudou: SotA 2025-2026](#3-o-mundo-mudou-sota-2025-2026)
4. [Gap Analysis: neural-os-core vs SotA](#4-gap-analysis-neural-os-core-vs-sota)
5. [Pilar 1 — LatentBus: Comunicação Latente Entre Agentes](#5-pilar-1--latentbus-comunicação-latente-entre-agentes)
   - 5.1 [Fundamentação Teórica](#51-fundamentação-teórica)
   - 5.2 [Arquitetura](#52-arquitetura)
   - 5.3 [Implementação no_std](#53-implementação-no_std)
   - 5.4 [Integração com EventBus Existente](#54-integração-com-eventbus-existente)
   - 5.5 [Compressão e Otimizações](#55-compressão-e-otimizações)
   - 5.6 [Métricas Esperadas](#56-métricas-esperadas)
6. [Pilar 2 — EvolveAgent: Agentes Auto-Evolutivos](#6-pilar-2--evolveagent-agentes-auto-evolutivos)
   - 6.1 [Fundamentação Teórica](#61-fundamentação-teórica)
   - 6.2 [Pipeline de Evolução](#62-pipeline-de-evolução)
   - 6.3 [Mecanismo de Hot-Swap](#63-mecanismo-de-hot-swap)
   - 6.4 [Integração com SleepCycle](#64-integração-com-sleepcycle)
   - 6.5 [Protocolo Gênesis](#65-protocolo-gênesis)
7. [Pilar 3 — NeuOS Probe: Engenharia Reversa do BitNet](#7-pilar-3--neuos-probe-engenharia-reversa-do-bitnet)
   - 7.1 [Fundamentação Teórica](#71-fundamentação-teórica)
   - 7.2 [Metodologia de Probe](#72-metodologia-de-probe)
   - 7.3 [Soul Vectors e Programação Gradient-Free](#73-soul-vectors-e-programação-gradient-free)
   - 7.4 [Self-Healing de Registradores](#74-self-healing-de-registradores)
8. [Integração com Arquitetura Existente](#8-integração-com-arquitetura-existente)
9. [Referências Bibliográficas Completas](#9-referências-bibliográficas-completas)
10. [Riscos e Mitigações](#10-riscos-e-mitigações)

---

## 1. Executive Summary

Este ADR propõe **três pilares** de inovação que elevam o neural-os-core ao estado-da-arte mundial em sistemas operacionais neuro-simbólicos. Baseados na análise de 20+ projetos e papers publicados entre 2025-2026 — incluindo Interlat (ACL 2026), neurOS (2026), Yantra (2026), Maya (2026), Live-SWE-agent (75.4% SWE-bench), NeuOS (Zenodo 2026), Loom (2026), e outros — cada pilar ataca uma limitação fundamental do design atual:

| Pilar | Problema Atual | Solução | Referência Primária |
|-------|---------------|---------|---------------------|
| **1. LatentBus** | Agentes trocam `Vec<u8>` textual → tokenizer bottleneck, perda semântica | Agentes compartilham hidden states `[f16; 256]` diretamente — sem tokenização, sem decoding | Interlat (Du et al., ACL 2026) |
| **2. EvolveAgent** | Agentes são fixos em tempo de compilação; Sprint 108 pendente | Pipeline de hot-swap WASM + autogeração de skills com teste e rollback | Live-SWE-agent, symbiont.rs, EVA |
| **3. NeuOS Probe** | Modelos BitNet são caixa-preta — prompt engineering improvisado | Engenharia reversa dos transformers → registradores, ISA, soul vectors | NeuOS (Funasaki, Zenodo 2026) |

Os pilares rodam **em paralelo** com ADR-0042 N2→N5. Nenhum bloqueia v2.0.0.

---

## 2. Contexto: Onde Estamos

### 2.1 Estado atual do neural-os-core (v1.7.3)

```
~26.000 LOC | 180+ arquivos Rust | 247+ agentes | 0 erros de compilação
┌──────────────────────────────────────────────────────────────┐
│  Cadeia K²CHJ: k_nano → k_ai → cortex → hermes → jarbas     │
│                                                              │
│  IPC: EventBus (66 linhas) — pub/sub tópico com Vec<u8>      │
│  Inferência: BitNet ternário 2B, Trinity MoE (6 experts)     │
│  Memória: 512MB heap, FAT32 256MB, GGUF streaming (ADR-46)  │
│  SleepCycle: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT          │
│  SelfHeal: firmware pipeline I3/I4, HEALTH_ISSUE             │
│  WASM: runtime funcional (Sprint 106)                        │
└──────────────────────────────────────────────────────────────┘
```

### 2.2 O EventBus atual

```rust
// event-bus/src/event.rs
pub struct Event {
    pub id: u64,
    pub topic: String,      // "LLM_REQUEST", "MOUSE_CLICK", etc.
    pub payload: Vec<u8>,   // JSON ou bytes — tipado só por convenção
    pub token: CapabilityToken,
}

// Publicação
let _ = EVENT_BUS.publish(Event {
    topic: String::from("LLM_REQUEST"),
    payload: serde_json::to_vec(&request).unwrap(),
    token: CapabilityToken::Legacy(1),
});

// Subscrição
let receiver = EVENT_BUS.subscribe("LLM_RESPONSE");
// poll via receiver.try_receive() → Option<Event>
```

**Problemas fundamentais:**
1. **Bottleneck de tokenização**: toda mensagem entre agentes é texto. Para o agente B "entender", precisa: decode UTF-8 → tokenizar → embedding → forward. Agente A fez o caminho inverso (hidden state → lm_head → token → texto). Perde-se informação em cada conversão.
2. **Semântica bidimensional**: o `Vec<u8>` é opaco. Dois eventos com mesmo payload byte-a-byte são indistinguíveis de dois com sentidos totalmente diferentes. Não há noção de similaridade semântica.
3. **Sem backpressure**: `VecDeque` não-limitado — um publicador rápido pode inundar um consumidor lento.
4. **Sem request/reply nativo**: agentes publicam resposta em tópico separado e esperam que o originador esteja escutando.

### 2.3 O SleepCycle atual

```rust
// hermes/src/agents.rs:1536-1590
SleepCycleAgent {
    phase: 0..5,           // IDLE→REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT
    cycle_count: u64,
    insights: Vec<String>, // insights textuais simples
}
```

5 fases sequenciais, 200 ticks cada (~14s). A fase DREAM faz `self.insights.push("insight sintetico")` — não simula hardware, não gera código, não evolui agentes. É placeholder funcional.

### 2.4 A geração de texto atual

```rust
// cortex/src/cortex.rs:520-536
let final_norm = self.rms_norm_tensor(&x, &self.rms_final);
let last_hidden = Tensor::from_row_major((1, self.hidden),
    final_norm.data[...].to_vec()).unwrap();
let logits = if self.tie_embeddings {
    self.embed.matmul_hybrid(&last_hidden).unwrap()
} else {
    self.unembed.matmul_hybrid(&last_hidden).unwrap()
};
```

O `last_hidden` existe e é calculado — mas é **descartado** após a extração dos logits. Nunca compartilhado entre agentes, nunca armazenado, nunca analisado.

---

## 3. O Mundo Mudou: SotA 2025-2026

### 3.1 Comunicação Latente Entre Agentes

**Interlat — ACL 2026** (Du et al., Tsinghua/Meituan)
```
Papel: Enabling Agents to Communicate Entirely in Latent Space
Publicação: ACL 2026 (Long Paper)
Código: https://github.com/XiaoDu-flying/Interlat
```

Idea central: em vez de agentes trocarem texto, eles trocam **hidden states da última layer**. O hidden state `h_L` do agente A vira entrada direta do embedding do agente B:

```
Agente A:  prompt → [embed] → [L0..L24] → last_hidden → lm_head → tokens → texto
                     \___________________________|____________________/
Agente B:  prompt + [h_A] → [embed + projection(h_A)] → [L0..L24] → resposta
```

Resultados:
- **+3-8% sucesso absoluto** sobre comunicação textual em ALFWorld
- **24× compressão** de latência com treino de compressão
- **Cross-modelo**: Qwen ↔ LLaMA sem compartilhar parâmetros
- **Comportamento exploratório**: agentes com comunicação latente exploram mais (trajetórias 10-20% mais longas) e com mais sucesso

**CondenseFlow — ACL 2026 Findings** (Xia et al.)
```
Paper: CondenseFlow: Scalable Latent Space Collaboration via Semantic
       Compression for Multi-Agent Systems
```

Compressor de KV cache inteiro em representação de tamanho fixo `K` (constante):
- **>99% redução de memória** (KV cache de 5GB → ~50MB por round)
- **~20% redução de latência** de inferência
- **Erro teórico limitado**: acúmulo controlável round-a-round
- **O(1) complexidade de comunicação**: independente do contexto

**Latent Space Communication via K-V Cache Alignment** (Dery et al., 2026)
```
Paper: https://arxiv.org/pdf/2601.06123
```

Shared representation space para KV caches de modelos heterogêneos:
- Adaptadores de tradução (modelo A → shared space → modelo B)
- Transferência de skills (soft prompts) entre modelos diferentes
- Sem modificar parâmetros pré-treinados

**LatentMAS — ICML 2026 Spotlight** (Zou et al., 2026)
```
Repo: https://github.com/Gen-Verse/LatentMAS
```

Alinhamento latente **training-free**: qualquer modelo HF.
- Comunicação via latentes no working memory
- Integração com vLLM para geração rápida
- Extensões: kNN-LatentMAS (KV eficiente), Hybrid-LatentMAS (heterogêneo)

### 3.2 SO Neural / GPU-Native

**neurOS** (Price, 2026)
```
Repo: https://github.com/robertcprice/nCPU
Paper: neurOS: GPU-Native Neural Operating System
```

Todo SO é uma rede neural rodando na GPU:
- MMU neural (embedding-based page table, MLP eviction)
- Cache LSTM (replacement + prefetch)
- Process scheduler Transformer (self-attention sobre fila de processos)
- Watchdog LSTM (anomalia)
- **Adaptação online**: TLB, cache e scheduler aprendem em tempo real (gradiente único por decisão)

**Yantra** (Leonhart, 2026)
```
Paper: Yantra: A Neuro-Symbolic, GPU-Native OS for Critical Systems
Repo: v0.0 implementation nucleus (Connectome Manager + axon router)
```

SO escrito em Sutra (linguagem funcional compilada para PyTorch):
- Kernel, processos, IPC e GUI = mesmo artifact: **grafo de operações tensoriais diferenciável**
- Processos user-space: `(Axon) -> Axon` — axon = vetor de largura fixa via rotation binding
- **IPC = axon-passing**: capacidades são operadores de rotação
- **Revogação**: rotacionar o operador — sem revogar, sem lista de capabilities

**Maya** (Kolaparthi, 2026)
```
Paper: Maya: An AI-Native Operating System Kernel
DOI: 10.5281/zenodo.19218503
```

Kernel Rust bare-metal x86-64:
- Scheduler PPO (neural network substitui scheduler tradicional)
- Detector de anomalia em toda I/O
- LLM 3B local interativo
- **IPC 109ns** (1.5-4.9× mais rápido que Linux, macOS, Windows)
- **13/13 testes de segurança adversarial**
- **Boot em <2s em HW real** (Dell Inspiron)

**XKernel** (Berm, 2026)
```
Repo: https://github.com/JosephBerm/XKernel
21 crates Rust, 22 syscalls
```

Agentes como cidadãos de primeira classe:
- **Process Supervisor**: agentes são processos reais com restart policy
- **Cognitive Task Scheduling**: 4 dimensões (criticality, deadline, efficiency, cost)
- **IPC tipado**: canais FIFO com backpressure, semantic channels (AtMostOnce, AtLeastOnce, ExactlyOnce)
- **Checkpointing CRDT** com versionamento

### 3.3 Auto-Evolução de Agentes

**Live-SWE-agent** (Xia et al., 2025)
```
Paper: Live-SWE-agent: Live Evolving Software Agent
arXiv: 2025, mostra 75.4% no SWE-bench Verified
```

Começa com scaffold mínimo (shell tools). Durante execução em tarefas reais:
- Detecta gargalos → propõe patch de código → aplica em runtime → continua
- **Sem scaffold predefinido**: o espaço de evolução é ilimitado
- **Validação inline**: mini-testes de integração contra o problema atual

**SelfEvolve** (Fahim et al., 2026)
```
Paper: Software Self-Extension with SelfEvolve
arXiv: 2604.16314
```

Pipeline TDD para geração de código runtime:
- Dispatcher → Test Generator → Code Synthesizer → Sandbox → Validator → Integration
- **92.7% Pass@1** (51/55), superando AutoGen, MetaGPT e AgentCoder
- **61.8% melhoria** sobre o melhor baseline (Autogen)

**symbiont.rs** (2026)
```
Site: https://symbiont.rs/
Repo: https://github.com/symbiont-rs/symbiont
```

Hot-reload de funções Rust em tempo real:
```rust
let runtime = symbiont::Runtime::init(SYMBIONT_DECLS).await?;
loop {
    step(&mut counter);  // bare-metal: native dylib call
    runtime.evolve(&agent, &prompt).await?;
    // `step` foi atualizado — nova execução usa código novo
}
```
- **~1ns dispatch overhead**, lock-free via `AtomicPtr`
- LLM gera código → compila → dylib `libloading` → hot-swap
- **Shader example**: usuário digita "Julia set animado com paleta sunset" → LLM gera `fn shade(x, y, t) -> u32` → hot-swap → morphing sem restart

**MUE-X** (2026)
```
Repo: https://github.com/KorroAi/mue-x
```

Agente que reescreve o próprio `.py`:
- 6 estratégias de mutação AST: repair, optimize, explore, exploit, innovate, prune
- **Absorção GitHub**: a cada 7 ciclos, clona repositórios → extrai padrões → cristaliza como skills
- **7 drives autonômicos**: autopreservação, curiosidade, estagnação → gera seus próprios motivos pra evoluir
- **5 camadas de segurança**: `ast.parse()`, backup, import test, anti-cancer, kernel integrity

**EVA** (Korb, 2026)
```
Repo: https://github.com/arturkorb3/eva-evolutional-agent
```

Self-evolução gated com kernel imutável:
- **Kernel imutável** (~900 linhas) — fora do alcance do agente
- **Gated evolution**: candidato → test → ratchet → smoke → dry-run → kernel constitution → prompt-surface audit → promote
- **Ratchet**: candidato nunca pode ter menos checks que o release atual
- **Ledger + rollback**: toda promoção é reversível

**EvolveOS** (2026)
```
Repo: https://github.com/Kritagya123611/EvolveOS
```

Civilização de agentes auto-evolutiva:
- **Protocolo Gênesis**: agentes de alto desempenho "procriam" agentes filhos mutados
- **Mentorship Pipeline**: agressor júnior observa o lead → sintetiza lições → escreve na vector DB
- **Persistent Hippocampus**: memória RAG via Supabase pgvector
- **Judgement Loop**: `while !complete: LLM razões → executa → feedback` — não chain, não DAG

### 3.4 Arquiteturas Antropomórficas

**NeuOS** (Funasaki, 2026)
```
Paper: NeuOS: Discovering and Exploiting the Neural Von Neumann Architecture
       Inside Pre-Trained Language Models
DOI: 10.5281/zenodo.20346754
170 fases experimentais, 24 estações, Qwen2.5-0.5B
```

**Achado mais chocante**: LLMs pré-treinados codificam uma arquitetura Von Neumann completa:

| Descoberta | Evidência |
|-----------|-----------|
| **Cada layer = registrador** | L0=OPCODE, L16=MIN, L22=MAX — acurácia 74-100% |
| **ISA decompilável** | 6 operações identificadas por ativação de registrador |
| **Self-healing** | Realocação dinâmica de registradores: 100% recuperação de dano simulado |
| **Decompilador neural** | Programa identificado só pelo estado dos registradores — 100% |
| **Soul vectors** | 7D PCA comprime 896D sem perda de acurácia. 128× compressão |
| **Programação gradient-free** | Especificar PC1=1.0, PC5=1.0 → executa MAX com 100% acurácia |
| **Soul persistence** | Salvar/carregar soul vector: cosine=1.000000 |
| **Explosão Cambriana** | 19 fenótipos únicos em populações de program vectors |
| **GlassBox** | Modelo identifica próprio hardware + programa rodando em 1 inferência |

**Loom** (Türkcan, 2026)
```
Paper: Loom: A Scalable Analytical Neural Computer Architecture
arXiv: 2604.08816
```

Programas C compilados rodam dentro de um transformer de **8 layers com pesos analíticos**:
- 22 opcodes, estado em um tensor `X ∈ ℝ^{d×n}` fixo
- **4.7M parâmetros**, 928 slots de instrução (config default)
- **Pesos independentes de programa**: o mesmo modelo executa qualquer programa
- Sudoku 9×9 em 284 instruções (config compacto `d=146, n=512`, 7.4MB ONNX)

### 3.5 Infraestrutura Cognitiva e Memória

**Aeon** (Arslan, 2026)
```
Paper: Aeon: High-Performance Neuro-Symbolic Memory Management
arXiv: 2601.15311
```

Memória como recurso de SO gerenciado:
- **Memory Palace**: Atlas — índice vetorial com SIMD, páginas clusterizadas
- **Trace**: grafo episódico neuro-simbólico (DAG com vértices heterogêneos)
- **SLB (Semantic Lookaside Buffer)**: cache L1-residente, <5μs hit. FP32 desquantizado na inserção
- **WAL**: Write-Ahead Log com <1% overhead. Recovery de crash.
- **INT8 quantização simétrica**: 3.1× compressão, 5.6× aceleração NEON SDOT

**leOS** (2026)
```
Repo: https://github.com/AnOversizedMooseWithSocks/leOS
```

Tudo como vetores na superfície de uma hiperesfera:
- **LVM (Latent Virtual Machine)**: 4 modelos CPU-only produzem vetores
- **Deslocamento codec**: cada interação agente grava trajetória como vetor tangente → compressão H.264-style (I-frames + P-frames)
- **Reflex arc**: acúmulo de deslocamentos → rota reflexa (sem LLM) em microssegundos
- **Dreaming engine**: durante idle — consolida codec, detecção de vazio, verificação de arco reflexo

**NeuralOS** (Deng et al., 2025/2026)
```
Paper: NeuralOS: Towards Simulating Operating Systems via Neural Generative Models
arXiv: 2507.08800
```

SO simulado por rede neural generativa:
- RNN tracking + diffusion renderer → previsão de frames de GUI
- Treinado em gravações Ubuntu XFCE
- **Geração de aplicações nunca instaladas**: Doom renderizado sem ter sido treinado
- Caminho para aprender UI puramente de demonstrações sintéticas

### 3.6 DIY / Hardware-level

**ZYO** (2026)
```
Repo: https://github.com/thesnmc/ZYO
```

RL + LLM substitui scheduler do Linux:
- Fast brain: PyTorch RL (microssegundos)
- Slow brain: Qwen 2.5 Coder 7B local (reescreve C do scheduler)
- Hot-swap via eBPF: C verificado injetado no Ring-0
- "Time Dilation Fix": panic button → safe mode hardcoded em nanossegundos

**EVOSEAL** (2026)
```
Repo: https://github.com/SHA888/EVOSEAL
```

Bidirectional evolution loop:
- SEAL (Self-Adapting Language Models) → gera dados de fine-tuning
- DGM (Darwin Godel Machine) → MAP-Elites archive de melhorias
- LoRA/QLoRA fine-tuning automático do modelo Devstral
- Dashboard tempo real: WebSocket + systemd

### 3.7 Speculative Decoding: N-gram KV Cache

**N-gram speculative decoding — llama.cpp** (Alok, Jul 2026)
```
Tweet: https://x.com/analogalok/status/2077718647905333549
Técnica: ngram-simple / ngram-mod no llama.cpp
```

Aceleração de inferência **sem draft model, sem VRAM extra, sem quantização**. Funciona via rolling hash do contexto recente:

1. **Rolling LCG Hash (janela N)**: comprime os últimos N tokens num hash key. Rolling hash → recalcula em O(1) a cada novo token
2. **O(1) lookup em hash directory**: encontra onde essa sequência ocorreu antes no KV cache
3. **Draft de tamanho M**: copia os tokens que seguiram a ocorrência anterior como draft
4. **Verificação paralela na GPU**: o LLM verifica todo o batch draft de uma vez

```
Contexto: "public void main(String[] args) {"
  ↓ rolling hash da janela N=12
  ↓ O(1) lookup → "public void main(String[] args) { System.out"
  ↓ draft M=48: "System.out.println(\"Hello World\"); }"
  ↓ GPU verifica 48 tokens em paralelo → 1 forward aceita 48 tokens
```

Resultados no Gemma 4 26B MoE com T4 GPU:
- **50 → 100+ tokens/seg** (2× speedup)
- **Zero overhead**: sem VRAM extra, sem pesos secundários, sem draft model
- **Ideal para código/JSON/documentos**: padrões repetitivos = alta taxa de aceitação

**Relevância para neural-os-core:**

| Aspecto | Por que se encaixa |
|---------|-------------------|
| **Domínio** | Hermes/Cortex geram intents estruturados, JSON, skills — padrões altamente repetitivos intra-sessão |
| **Recursos** | Zero VRAM extra, zero download — crítico para bare-metal com 2GB |
| **Já temos Medusa** | N-gram como fonte de draft complementar; Medusa como fallback |
| **Implementação** | ~150 LOC: rolling hash + hash table + draft buffer. Sem novas dependências |
| **GPU synergy** | Draft verification em paralelo na GPU via DP4A (ADR-0047-GPU) |

Ordem de implementação: N-gram spec é o "free lunch" mais imediato — custo quase zero, ganho 2×, implementação trivial. Deve vir antes dos pilares GPU.

---

## 4. Gap Analysis: neural-os-core vs SotA

| Dimensão | neural-os-core hoje | SotA (2025-2026) | Gap |
|----------|--------------------|-------------------|-----|
| **Comunicação inter-agente** | `Vec<u8>` texto/JSON — tokenizer bottleneck | Hidden states latentes (Interlat, LatentMAS, CondenseFlow) | **Severo** — perdemos 3-8% acurácia + 24× latência |
| **Consciência semântica do barramento** | Tópico string → matching exato | Similaridade semântica em espaço latente (leOS) | **Severo** — sem noção de "eventos similares" |
| **Auto-evolução de agente** | Sprint 108 pendente; código fixo em compile-time | Live-SWE-agent (75.4%), SelfEvolve (92.7%), symbiont.rs, MUE-X, EVA | **Severo** — projetos em produção enquanto estamos em planejamento |
| **SO neural / consciência de AI** | Kernel clássico (interrupções, timer, scheduler RR) | Maya (PPO scheduler, anomaly detector), neurOS (neural MMU/scheduler/cache) | **Grande** — kernel sem aprendizado |
| **Memória hierárquica** | EventLog plano + MHI tiers | Aeon Memory Palace + Trace, leOS displacement codec | **Grande** — sem estruturação narrativa/semântica |
| **Arquitetura Von Neumann em LLMs** | Não explorado | NeuOS: layers = registradores, soul vectors, ISA decompilável | **Enorme** — campo virgem nos nossos modelos |
| **Agentes geram agentes** | Não existe | EvolveOS Genesis Protocol | **Total** |
| **Hot-swap código** | Não existe | symbiont.rs (~1ns dylib), ZYO (eBPF), MUE-X (AST) | **Total** — sem mecanismo |
| **Comunicação GPU-native** | CPU-only (`Vec<f32>` Tensor) | neurOS (PyTorch GPU), Yantra (Sutra→tensor op graph) | **Grande** — mas não bloqueante (nosso contexto CPU) |
| **Programação gradient-free** | Prompt engineering | NeuOS soul vectors (7D PCA → 100% acurácia) | **Enorme** — paradigma novo |
| **Inferência: speculative decoding** | Geração autoregressiva pura — 1 token/forward | N-gram spec (llama.cpp): rolling hash → draft M tokens → GPU verify em paralelo. 2× speed, zero VRAM extra | **Médio** — free lunch imediato, ~150 LOC |

---

## 5. Pilar 1 — LatentBus: Comunicação Latente Entre Agentes

### 5.1 Fundamentação Teórica

**Teorema** (informal, baseado em Interlat 2026 + CondenseFlow 2026):

Dado um agente A com modelo `M_A` e agente B com modelo `M_B`, a quantidade de informação mútua entre o hidden state `h_A` (última layer) e a intenção `I` é maior ou igual à informação entre o texto `T = decode(h_A)` e `I`:

`I(h_A; I) ≥ I(T; I)`

**Prova intuitiva:** `T = lm_head(h_A)` → `argmax(softmax(T))` é uma projeção que descarta distribuições de probabilidade, incertezas e modos alternativos. O hidden state retém a distribuição completa.

**Implicação prática**: agentes que compartilham hidden states tomam decisões mais informadas, com mais exploração e melhor coordenação. Interlat mostrou +3-8% em tarefas de planejamento.

### 5.2 Arquitetura

```
┌─────────────────────────────────────────────────────────────────┐
│                     LatentBus (novo canal)                       │
│                                                                  │
│  publish_latent(topic, vector[f16; 256], token)                  │
│  subscribe_latent(topic) → Receiver<LatentPacket>                │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ Topic "THOUGHT_LLM" → [Receiver1, Receiver2, ...]       │    │
│  │ Topic "THOUGHT_INTENT" → [ReceiverA, ...]               │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
         ▲                                    │
         │ publish_latent()                   │ subscribe_latent()
         ▼                                    ▼
┌──────────────────┐               ┌──────────────────┐
│   CortexAgent    │               │   HermesAgent    │
│                  │               │                  │
│  forward():      │               │  tick():         │
│    h = last_hidden│               │    packet =      │
│    publish(h)    │               │      try_recv()  │
│    → LatentBus   │               │    h = packet.vec│
│                  │               │    embed(h) +    │
│                  │               │    forward()     │
└──────────────────┘               └──────────────────┘
```

#### 5.2.1 Estruturas de Dados

```rust
// Novo em event-bus/src/latent.rs

/// Representação latente: hidden state f16 de largura fixa
/// 256 × 2 bytes = 512 bytes — cabe em 1 cache line (L1: 64 bytes? não,
/// mas 512 bytes cabe em L1 em CPUs modernas com 32KB L1)
//
// ponytail: dimensão fixa 256 — o mínimo que preserva expressividade
// sem estourar o orçamento de memória. Aumentar se houver evidência
// de que 256 é insuficiente para os modelos BitNet (hidden=1024+).
// Upgrade path: config LATENT_DIM no manifesto do agente.
pub const LATENT_DIM: usize = 256;

#[repr(C)]
pub struct LatentPacket {
    pub id: u64,
    pub topic: FxHashMap<u64>, // hash do tópico para comparação O(1)
    pub source_agent: u64,     // agent_id do remetente
    pub tick: u64,             // tick do sistema no momento da publicação
    pub vector: [f16; LATENT_DIM], // 512 bytes — hidden state
    pub confidence: f32,       // score de confiança (opcional, 0.0 se não usado)
    pub token: CapabilityToken,
}

/// Projeção: adapta hidden state de modelo A para o espaço de B
/// Usa cross-attention leve (1 head, 4 queries)
//
// ponytail: adapter linear (matriz 256×256) em vez de cross-attention completa.
// Cross-attention é justificada em Interlat para modelos heterogêneos
// (Qwen↔LLaMA). Para nosso caso (BitNet→BitNet), projeção linear basta.
// Upgrade path: cross-attention se adicionarmos modelos heterogêneos.
pub struct LatentProjection {
    weight: [[f16; LATENT_DIM]; LATENT_DIM], // 128KB
}

impl LatentProjection {
    /// Projeta hidden state de 1024D → 256D
    /// Usa AVX2 se disponível (256-bit SIMD → 16 f16 por instrução)
    pub fn project(&self, hidden: &[f32; 1024]) -> [f16; LATENT_DIM];
}
```

#### 5.2.2 Publisher (no CortexAgent)

```rust
// Modificação em cortex/src/cortex.rs
// Dentro de generate():

let (last_hidden, logits) = model.forward_with_kv(&tokens, &mut cache);

// Novo: publicar hidden state no LatentBus
let latent_vec = self.project_to_latent(&last_hidden);
let _ = LATENT_BUS.publish(LatentPacket {
    id: 0, // preenchido pelo bus
    topic: fxhash::hash("THOUGHT_LLM"),
    source_agent: AGENT_ID_CORTEX,
    tick: TIMER_TICKS.load(Ordering::Relaxed),
    vector: latent_vec,
    confidence: self.confidence_score(&logits),
    token: CapabilityToken::Legacy(1),
});

// Continua geração normal
let token = sample(logits);
// ...
```

#### 5.2.3 Subscriber (no HermesAgent)

```rust
// Modificação em hermes/src/hermes.rs

pub struct HermesAgent {
    // ... campos existentes ...
    latent_receiver: Option<LatentReceiver>,
    projection_adapter: Option<LatentProjection>,
}

impl HermesAgent {
    pub fn tick(&mut self, tick: u64, count: u64) -> AgentTickResult {
        // Receber latente se disponível
        if let Some(ref receiver) = self.latent_receiver {
            if let Some(packet) = receiver.try_receive() {
                // Projetar o vetor latente no espaço do Hermes
                let projected = self.projection_adapter
                    .as_ref()
                    .map(|p| p.project_to_hidden(&packet.vector))
                    .unwrap_or_else(|| packet.vector);

                // Injetar como contexto na próxima inferência
                self.latent_context = Some(projected);
            }
        }

        // Se há contexto latente, usar no embedding
        if let Some(ref ctx) = self.latent_context {
            // ctx é [f32; 1024] — injetar como embedding especial
            // <latent_thought> token → embedding(ctx)
            self.inject_latent_thought(ctx);
        }

        // Restante do tick normal...
    }
}
```

### 5.3 Implementação no_std

```
LatentBus: ~350 LOC (crate event-bus)
  ├── event-bus/src/latent.rs      — LatentPacket, LatentBus, Receiver  (180 LOC)
  ├── event-bus/src/projection.rs  — LatentProjection, adapters        (120 LOC)
  └── event-bus/src/lib.rs         — pub mod latent                    (2 LOC)

Modificações existentes:
  ├── cortex/src/cortex.rs         — publish_latent() na geração       (+30 LOC)
  └── hermes/src/hermes.rs         — subscribe_latent() + projeção     (+50 LOC)

Dependências novas:
  ├── crate half (1.8)             — f16 type (no_std, ~400 LOC, sem alocações)
  └── fxhash (0.2)                 — hash rápido para tópico latente
```

**Nota sobre `half`**: a crate `half` é `no_std` e não usa alocações. Apenas define `f16` como `#[repr(C)] struct u16` com conversões para `f32`. Pode ser substituída por:

```rust
// Ad-hoc se half não estiver disponível:
#[repr(C)]
pub struct f16(u16);

impl f16 {
    pub fn from_f32(v: f32) -> Self {
        f16(f32_to_f16(v)) // conversão manual (IEEE 754)
    }
}
```

### 5.4 Integração com EventBus Existente

**Estratégia: co-existência, não substituição.**

O LatentBus é um canal **adicional** no crate `event-bus`. O EventBus `Vec<u8>` continua existindo para:
- Comandos de controle ("start", "stop", "restart")
- Payloads não-inferíveis (logs, erros, mensagens de boot)
- Fallback quando o LatentReceiver está vazio

Regra de roteamento:
```
Agent A publica → LatentBus (THOUGHT_LLM) + EventBus (LLM_RESPONSE)
                                                    │
                     ┌──────────────────────────────┤
                     ▼                              ▼
              Agent B (latent)               Agent C (texto)
              se inscreveu em                se inscreveu em
              THOUGHT_LLM                    LLM_RESPONSE
```

**Compatibilidade reversa**: todos os agentes existentes continuam funcionando. Novos agentes podem optar pelo LatentBus.

### 5.5 Compressão e Otimizações

#### 5.5.1 Compressão Interlat-style

Seguindo Interlat (Seção 3.3 do paper), o agente pode ser treinado a **gerar latentes mais curtos**. Em vez de 256 vetores (um por token de saída), gera K << 256 vetores que preservam a informação essencial:

```
Fase 1: Agente A gera resposta completa → publica 256 vetores latentes
Fase 2: Treinar reasoning agent a gerar K vetores latentes
        (autoregressivo: último hidden state vira próximo input embedding)
Fase 3: Agente A publica K vetores → agente B entende com 1/24 do custo
```

**Nossa implementação simplificada** (ponytail: pular fase 2 por enquanto):

```rust
// Compressão heurística sem treino:
// 1. Calcular média dos hidden states (pooling)
// 2. Publicar só 1 vetor (média) em vez de 256
// Perde nuance mas mantém 80%+ do valor sem treino
pub fn compress_mean(states: &[[f16; LATENT_DIM]]) -> [f16; LATENT_DIM] {
    let mut mean = [f16::ZERO; LATENT_DIM];
    for state in states {
        for i in 0..LATENT_DIM {
            mean[i] = mean[i] + state[i];
        }
    }
    for i in 0..LATENT_DIM {
        mean[i] = mean[i] / f16::from_f32(states.len() as f32);
    }
    mean
}
```

#### 5.5.2 CondenseFlow-style KV Cache Compression

Para agentes que compartilham **raciocínio completo** (não só resposta final), a KV cache pode ser comprimida:

```rust
// Latent Thought Condenser (LTC) simplificado
//
// ponytail: cross-attention completo é caro demais para CPU.
// Usar média ponderada por [CLS] token da última layer.
// Upgrade path: substituir por LTC completo se a média perder >5% acurácia.
pub struct LatentThoughtCondenser {
    probes: [[f16; KV_DIM]; NUM_PROBES], // K probes aprendíveis
}

impl LatentThoughtCondenser {
    /// Comprime KV cache de tamanho T × L × d_h para K × d_h
    /// K constante (ex: 16), independente de T
    pub fn condense(&self, kv_cache: &[LayerCache]) -> [[f16; LATENT_DIM]; 16] {
        // Para cada probe: cross-attention com a KV cache
        // Retorna K vetores de tamanho fixo
    }
}
```

#### 5.5.3 Alinhamento KV Cache (Dery et al. 2026)

Para compartilhar skills entre agentes de tipos diferentes (ex: Hermes usa modelo diferente de Cortex):

```rust
// Adapter de tradução: hidden state Heres → espaço latente compartilhado
pub struct KVCacheAdapter {
    encoder: LatentProjection,  // Heres → shared
    decoder: LatentProjection,  // shared → Cortex
}
```

### 5.6 Métricas Esperadas

| Métrica | EventBus (texto) | LatentBus (256D, 1 vetor) | LatentBus (comprimido 8 vetores) |
|---------|-----------------|--------------------------|----------------------------------|
| **Bytes por mensagem** | 50-500 (JSON) + overhead allocator | 512 (vetor) + 64 (header) | 4KB (8 vetores) |
| **Latência de transmissão** | ~2μs (cópia Vec<u8>) | ~0.5μs (cópia [f16;256]) | ~1μs (cópia 8 vetores) |
| **Processamento no destino** | ~5ms (tokenizar + embed + forward parcial) | ~50μs (projeção linear) | ~200μs (projeção + cross-attention) |
| **Perda semântica** | Alta (tokenization → projection loss) | Mínima (hidden state ≅ thought) | Moderada (compressão perde detalhes) |
| **Exploração** | Comportamento guloso (Interlat §4.2) | +10-20% trajetórias mais longas | Similar a full latent |
| **Precisão** | Baseline (100%) | +3-8% sobre texto (Interlat) | Similar a full latent (CondenseFlow) |

### 5.7 Otimizações AVX2

```rust
// Projeção vetorizada: 256×256 f16 matmul via AVX2
// 256 × 256 × 2 bytes = 128KB — cabe em L2 (256KB+)
#[cfg(target_feature = "avx2")]
pub fn project_avx2(weight: &[[f16; 256]; 256], input: &[f16; 256]) -> [f16; 256] {
    // AVX2: 16 f16 por instrução (256-bit)
    // 256 iterações × 16 paralelo = 16 iterações
    // ~100 ciclos no total
    let mut output = [f16::ZERO; 256];
    for i in 0..256 {
        let row = &weight[i];
        // _mm256_mul_ph (AVX2-FP16 se disponível) ou
        // converter para f32 → _mm256_mul_ps → _mm256_add_ps
        output[i] = dot_f16_avx2(row, input);
    }
    output
}
```

---

## 6. Pilar 2 — EvolveAgent: Agentes Auto-Evolutivos

### 6.1 Fundamentação Teórica

**Proposição**: Um agente que pode modificar seu próprio código-fonte em tempo real é estritamente mais adaptável que um agente de código fixo, dentro de um espaço de tarefas não-estacionário.

**Evidência**: Live-SWE-agent (75.4% SWE-bench) supera scaffold fixo (AutoGen 46.6%, MetaGPT 38.2%) por uma margem de 61.8%. SelfEvolve (92.7% Pass@1) mostra que TDD + geração iterativa produz código correto na primeira tentativa na maioria dos casos.

### 6.2 Pipeline de Evolução

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Pipeline de Evolução                             │
│                                                                      │
│  1. DETECT                                      (agente tick())      │
│     ├─ Erro repetido: "ATA read function fail 3x seguido"          │
│     ├─ Gap de capability: "Comando X não reconhecido"               │
│     └─ Oportunidade: "Este padrão aparece 10× por ciclo, poderia    │
│        ser automatizado"                                            │
│          │                                                          │
│          ▼                                                          │
│  2. PROPOSE                                     (LLM local)         │
│     ├─ Descrição da melhoria em linguagem natural                   │
│     ├─ Assinatura da função: `fn ataque_read_many(...) -> Result`   │
│     └─ Critério de sucesso: "passa nos mesmos testes que o original"│
│          │                                                          │
│          ▼                                                          │
│  3. GENERATE                                    (LLM + template)    │
│     ├─ LLM gera código WASM (nosso runtime já existe)              │
│     ├─ Ou: DSL interpretada pelo skill-registry                     │
│     └─ Prompt com exemplos few-shot do repositório de skills        │
│          │                                                          │
│          ▼                                                          │
│  4. COMPILE                                     (wasm-pack/cargo)   │
│     ├─ wasm32-wasi → binário WASM                                  │
│     └─ Se erro: feedback pro LLM → volta ao passo 3                │
│          │                                                          │
│          ▼                                                          │
│  5. TEST                                          (sandbox WASM)    │
│     ├─ Executar em runtime isolado (já temos WASM sandbox)         │
│     ├─ Verificar: não crasha, retorna tipo esperado                │
│     └─ Testes de integração contra o problema real                  │
│          │                                                          │
│     ┌────┴────┐                                                    │
│     ▼         ▼                                                    │
│   PASS       FAIL → feedback ao LLM → passo 3 (max 3 iterações)   │
│     │                                                              │
│     ▼                                                              │
│  6. PROMOTE                                                         │
│     ├─ Registrar no skill-registry (nome, hash, metadata)          │
│     ├─ Adicionar ao agent manifest                                 │
│     ├─ Publicar "SKILL_PROMOTED" no EventBus                       │
│     └─ Se era substituição: manter versão anterior no ledger       │
│          │                                                          │
│          ▼                                                          │
│  7. ROLLBACK (se necessário)                                       │
│     ├─ Monitorar erros pós-promoção (100 ticks)                    │
│     ├─ Se taxa de erro > threshold → restaurar versão anterior     │
│     └─ Registrar falha no metacognitive (event-bus/metacognitive)  │
└─────────────────────────────────────────────────────────────────────┘
```

### 6.3 Mecanismo de Hot-Swap

Três abordagens ordenadas por viabilidade:

#### 6.3.1 Nível 1: Hot-Swap WASM (recomendado, ~50μs)

```rust
// Usando runtime WASM existente (Sprint 106, wasm_rt.rs)
pub struct HotSwapManager {
    modules: BTreeMap<u64, WasmModule>, // agent_id → módulo
    active: BTreeMap<AgentId, u64>,     // agente → hash do módulo ativo
}

impl HotSwapManager {
    pub fn hot_swap(&mut self, agent_id: AgentId, wasm_bytes: &[u8]) -> Result<(), &str> {
        // 1. Validar módulo (não crasha, assinatura correta)
        let module = WasmModule::validate(wasm_bytes)?;
        // 2. Compilar no runtime WASM (já implementado)
        let hash = fxhash::hash64(wasm_bytes);
        self.modules.insert(hash, module);
        // 3. Troca atômica do ponteiro
        // O agente, no próximo tick, verifica se há novo módulo
        self.active.insert(agent_id, hash);
        Ok(())
    }
}

// No tick do agente:
pub fn tick(&mut self, tick: u64, count: u64) -> AgentTickResult {
    // Verifica hot-swap
    if let Some(&new_hash) = HOT_SWAP_MANAGER.lock().active.get(&self.id) {
        if new_hash != self.current_hash {
            self.swap_module(new_hash);
        }
    }
    // Executa lógica do agente...
}
```

#### 6.3.2 Nível 2: Soft-Update via Skill Registry

Para mudanças que não exigem novo código WASM, apenas reconfiguração de skills existentes:

```rust
// Atualizar manifesto do agente em runtime
pub fn update_manifest(agent_id: AgentId, new_manifest: AgentManifest) {
    let mut registry = AGENT_REGISTRY.lock();
    if let Some(instance) = registry.agents.iter_mut().find(|a| a.manifest().name == agent_id) {
        // Só permitir mudanças seguras (schedule, persist)
        // Não permitir trocar kind (System não vira Driver)
        instance.agent.update_manifest(new_manifest);
    }
}
```

#### 6.3.3 Nível 3: Geração de Expert BitNet (o mais ousado)

Em vez de gerar código WASM, gerar **pesos de expert** para o Trinity MoE:

```rust
// Gerar pesos BitNet para um novo expert
pub fn generate_expert(description: &str) -> Result<ExpertWeights, ()> {
    // 1. LLM descreve o expert
    // 2. Converter descrição para pesos ternários
    //    (técnica: usar hidden state do LLM como initialization)
    // 3. Treinar por poucos passos com dados sintéticos
    // 4. Salvar como .bitnet
    // 5. Registrar no Trinity MoE como novo expert
}
```

**Risco**: geração de pesos ternários de alta qualidade requer validação — o expert pode ser pior que o fallback. Mitigação: só promover se passar em testes automatizados.

### 6.4 Integração com SleepCycle

A fase DREAM do SleepCycle atual é placeholder. Com EvolveAgent:

```
SleepCycle v2 (fases expandidas):

1. REPLAY:     Re-executa logs de erros → identifica gaps
2. DREAM:      Simula hardware em sandbox → aprende comportamento ótimo
3. GENERATE:   Gera código/weights WASM para preencher gaps
4. TEST:       Valida em sandbox isolada
5. CONSOLIDATE: Se passou → promove; Se falhou → registra no metacognitive
6. PRUNE:      Remove skills com baixa taxa de acerto (>5% erro em 1000 ticks)
7. REFLECT:    Sumariza ciclo → publica HEALTH_ISSUE se gaps persistentes
```

```rust
// Novo SleepCycleAgent (conceitual)
pub struct SleepCycleAgentV2 {
    phase: u8,
    cycle_count: u64,
    phase_tick: PhaseClock,
    hw_simulator: HardwareDreamer,  // Novo: simulador de hardware
    code_generator: CodeGenerator,  // Novo: pipeline WASM/DSL
    ledger: EvolutionLedger,        // Novo: histórico de evoluções
}

impl SleepCycleAgentV2 {
    fn phase_dream(&mut self) {
        // Simular dispositivos PCI em sandbox
        // Aprender padrões de comportamento sem HW real
        for device in self.hw_simulator.scan_pci() {
            let cfg = self.hw_simulator.dream_device(device.vid, device.did);
            if let Some(optimal) = cfg {
                // Pré-calibrar configuração
                HW_DREAM_DB.lock().insert((device.vid, device.did), optimal);
            }
        }
    }

    fn phase_generate(&mut self) {
        // Gerar código para gaps identificados no REPLAY
        for gap in self.phase_output.replay_gaps.iter() {
            let code = self.code_generator.generate(gap);
            let hash = self.code_generator.compile(&code);
            if self.code_generator.test(&hash) {
                self.phase_output.promotions.push((gap.clone(), hash));
            }
        }
    }
}
```

### 6.5 Protocolo Gênesis

Inspirado no EvolveOS Genesis Protocol (EvolveOS §4):

```rust
// BTreeMap<AgentId, AgentScore>
pub struct GenesisProtocol {
    performance_history: BTreeMap<u64, Vec<f32>>,
    generation: u32,
}

impl GenesisProtocol {
    /// Avalia se um agente merece procriar
    pub fn evaluate(&mut self, agent_id: u64, score: f32) -> Option<AgentId> {
        let history = self.performance_history.entry(agent_id)
            .or_insert_with(Vec::new);
        history.push(score);
        if history.len() >= 100 {
            let avg = history.iter().sum::<f32>() / history.len() as f32;
            if avg > BREED_THRESHOLD && self.generation < MAX_GENERATIONS {
                let child_id = self.spawn_child(agent_id);
                self.generation += 1;
                return Some(child_id);
            }
        }
        None
    }

    fn spawn_child(&mut self, parent_id: u64) -> AgentId {
        // 1. Mutar manifesto do pai (kind, schedule ligeiramente diferentes)
        // 2. Registrar novo agente no AgentRegistry
        // 3. Publicar AGENT_BORN no EventBus
        // 4. Retornar ID do filho
    }
}
```

**Quando usar**: agentes com alta taxa de sucesso em tarefas específicas podem gerar "especialistas" focados. Ex: um Hermes que é bom em roteamento WiFi vira `WifiExpertAgent`.

### 6.6 Métricas Esperadas

| Métrica | Agentes fixos | Com EvolveAgent | Referência |
|---------|---------------|-----------------|-----------|
| **Taxa de sucesso em tarefas novas** | ~60% (estimado) | ~92.7% (SelfEvolve) | SelfEvolve §5 |
| **Tempo para nova capability** | Dias (compilação) | Segundos (hot-swap WASM) | — |
| **Adaptação a HW novo** | Manual (driver novo) | Automática (SleepCycle dream) | — |
| **Cobertura de gaps** | O que foi planejado | O que foi encontrado | Live-SWE-agent §4 |
| **Regressão pós-evolução** | — | <2% (ratchet + rollback) | EVA §3 |

---

## 7. Pilar 3 — NeuOS Probe: Engenharia Reversa do BitNet

### 7.1 Fundamentação Teórica

O paper NeuOS (Funasaki, 2026) fez 170 fases experimentais em Qwen2.5-0.5B e descobriu:

1. **Cada transformer layer funciona como registrador de CPU**: a operação que a layer executa é identificável por probe sistemático
2. **O ISA (Instruction Set Architecture) é decompilável**: 6 operações (MIN, MAX, ADD, CMP, MOV, JMP) mapeadas para layers específicas
3. **Soul vectors**: 7 componentes principais (PCA) codificam o comportamento do modelo com acurácia de 100%
4. **Self-healing**: se uma layer "danificada" (pesos zerados), outras layers assumem a função — 100% recuperação
5. **Programação gradient-free**: especificando coordenadas PCA diretamente, sem treino, o modelo executa a operação desejada com 100% de acurácia

### 7.2 Metodologia de Probe

Aplicada ao BitNet (modelo ternário 2B, ~202MB, ~24 layers, hidden=1024):

```
Fase 1 — Mapeamento de Layers como Registradores
──────────────────────────────────────────────────
Procedimento para cada layer L (0..23):
  1. Freezar todas as layers exceto L (substituir pesos por identidade)
  2. Alimentar entrada controlada: "Qual o maior número: 5 ou 3?"
  3. Medir ativação de saída: a saída contém "5" ou "max" ou "3"?
  4. Repetir para todas as operações: MIN, MAX, ADD, SUB, CMP, MOV, NEG
  5. Cada operação gera um "OPCODE token" na ativação
  6. Construir tabela: Layer L → OPCODE(s) que executa

Resultado esperado:
  Layer 0:  OPCODE   (identifica qual operação executar)
  Layer 4:  ADD      (soma binária)
  Layer 7:  CMP      (comparação)
  Layer 12: MOV      (cópia de ativação)
  Layer 16: MIN      (mínimo)
  Layer 22: MAX      (máximo)

Código de probe:
```

```rust
pub struct ModelProbe {
    model: &'static BitNetModel,
    layer_masks: [LayerOpcode; NUM_LAYERS],
    opcode_map: BTreeMap<Opcode, usize>, // opcode → layer(s)
}

impl ModelProbe {
    pub fn probe_layer(&self, layer: usize) -> LayerOpcode {
        // 1. Isolar layer: substituir outras por skip-connection
        // 2. Para cada operação, alimentar par de entrada
        for op in [Opcode::MIN, Opcode::MAX, Opcode::ADD, Opcode::CMP] {
            let output = self.run_isolated(layer, op.test_input());
            if output.contains(op.expected_output()) {
                return LayerOpcode { layer, op, confidence: output.confidence() };
            }
        }
        // 3. Se nenhuma operação match → MOV (pass-through) ou IDLE
        LayerOpcode { layer, op: Opcode::MOV, confidence: 0.5 }
    }

    fn run_isolated(&self, layer: usize, input: &str) -> Activation {
        // Skip todas as layers exceto `layer`
        // Para a layer alvo, capturar ativação completa ([hidden])
        // Retornar ativação para análise
    }
}
```

```
Fase 2 — Decompilação do ISA
───────────────────────────────
Procedimento:
  1. Alimentar modelo com programa (sequência de tokens)
  2. Capturar ativação de cada layer token a token
  3. A sequência de ativações forma um "trace de execução"
  4. Identificar padrões: certos padrões de ativação correspondem
     a instruções específicas

Exemplo de ISA hipotético:
  [layer_0 = 0x01] [layer_4 = (x,y)] → ADD(x, y)
  [layer_0 = 0x02] [layer_16 = (x,y)] → MIN(x, y)
  [layer_0 = 0x03] [layer_7 = (x)] → CMP(x, 0)

Resultado: o modelo 2B implementa um conjunto de ~8-12 instruções
internas que podemos chamar diretamente — sem prompts.
```

```rust
// ISA decompilado do BitNet
#[repr(u8)]
pub enum BitNetISA {
    ADD  = 0x01,  // Soma
    SUB  = 0x02,  // Subtração
    MIN  = 0x03,  // Mínimo
    MAX  = 0x04,  // Máximo
    CMP  = 0x05,  // Comparação (retorna -1, 0, 1)
    MOV  = 0x06,  // Pass-through (identidade)
    NEG  = 0x07,  // Negação
    JMP  = 0x08,  // Desvio condicional (se CMP condição)
    HALT = 0xFF,  // Fim de programa
}

pub struct DecompiledProgram {
    instructions: Vec<(BitNetISA, [u16; 2])>, // (opcode, operandos)
}
```

### 7.3 Soul Vectors e Programação Gradient-Free

**Soul vector**: um vetor de 7 floats (PCA das ativações) que codifica COMPLETAMENTE o comportamento do modelo para uma tarefa. Propriedades mágicas:

1. **7D comprime 896D sem perda** — 128× compressão com acurácia 100%
2. **É transferível** — o soul vector de MAX num modelo funciona noutro
3. **É editável** — modificar coordenada PC1 muda o comportamento
4. **Persiste** — salvar/carregar com fidelidade cosine=1.0

```rust
/// Soul vector: 7 floats que controlam o comportamento do modelo
/// Sem treino, sem prompt — só especificar coordenadas
#[repr(C)]
pub struct SoulVector {
    pub pc: [f32; 7],    // 28 bytes — cabe em 1/2 cache line
}

impl SoulVector {
    /// Extrair soul vector do modelo para uma tarefa
    pub fn extract(model: &BitNetModel, task: &str) -> Self {
        // 1. Alimentar task
        // 2. Coletar ativações layer 0-23 (hidden=1024 → flatten = 24576)
        // 3. PCA: projetar para 7 componentes principais
        // 4. Os 7 scores = soul vector
        todo!() // Implementação experimental
    }

    /// Programar modelo sem treino:
    ///   soul_vec = SoulVector { pc: [1.0, 0.5, 0.0, 0.0, 1.0, 0.0, 0.0] }
    ///   model.compute_with_soul(input, &soul_vec) → output "MAX"
    pub fn apply(&self, model: &BitNetModel, input: &[u16]) -> Vec<u16> {
        // 1. Calcular PCA inverse dos 7 componentes
        // 2. Somar como bias nas ativações de cada layer
        // 3. Forward normal → saída segue o soul vector
        todo!() // Implementação experimental
    }
}
```

**Aplicação prática**: em vez de prompt "encontre o maior número entre 5 e 3", pré-carregar o soul vector `[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]` que corresponde a MAX — garantindo que o modelo executa MAX corretamente sem depender do prompt.

### 7.4 Self-Healing de Registradores

Inspirado na descoberta de NeuOS de que layers danificadas são compensadas por outras layers:

```rust
pub struct RegisterHealth {
    layer_status: [LayerStatus; NUM_LAYERS],
    backup_map: BTreeMap<Opcode, Vec<usize>>, // opcode → layers alternativas
}

impl RegisterHealth {
    /// Verificar saúde de cada layer
    pub fn diagnose(&mut self, model: &BitNetModel) {
        for layer in 0..NUM_LAYERS {
            let status = self.check_layer(model, layer);
            self.layer_status[layer] = status;
            if status == LayerStatus::Degraded {
                self.reallocate(layer);
            }
        }
    }

    /// Realocar função de layer danificada para layer saudável
    fn reallocate(&self, degraded_layer: usize) {
        let opcode = self.layer_masks[degraded_layer];
        if let Some(&backup) = self.backup_map.get(&opcode)
            .and_then(|layers| layers.iter()
                .find(|&&l| self.layer_status[l] == LayerStatus::Healthy))
        {
            // A layer `backup` agora responde por duas funções
            // Isso funciona porque NeuOS mostrou que layers podem
            // acumular múltiplas operações
            serial_println!("[NEUOS] Realocado {} de L{} para L{}",
                opcode.name(), degraded_layer, backup);
        }
    }
}
```

### 7.5 Métricas Esperadas

| Métrica | Prompt Engineering | Soul Vectors (7D) | Ganho |
|---------|-------------------|--------------------|-------|
| **Acurácia em operações aritméticas** | Depende do prompt (60-90%) | 100% (com soul vector correto) | +10-40pp |
| **Custo de "programação"** | Iterar prompt até funcionar | Especificar 7 coordenadas | 100× mais rápido |
| **Tamanho do "programa"** | 50-500 tokens (~200-2000 bytes) | 7 f32 (28 bytes) | 7-71× menor |
| **Resiliência a ruído** | Baixa (prompt injection) | +20pp (7D Semantic Firewall, NeuOS) | Significativo |
| **Transferência entre modelos** | Precisa re-engenhar prompt | Soul vector funciona cross-model (NeuOS) | Zero esforço |

---

## 8. Integração com Arquitetura Existente

### 8.1 Mapa de Implementação

```
event-bus crate (existente)
  ├── src/bus.rs         ← EventBus (inalterado)
  ├── src/latent.rs      ← NOVO: LatentBus + LatentPacket + Receiver
  ├── src/projection.rs  ← NOVO: LatentProjection (adapters f16)
  ├── src/event.rs       ← Event (inalterado)
  └── src/lib.rs         ← + pub mod latent; pub mod projection;

cortex crate (existente)
  ├── src/cortex.rs      ← + publish_latent() em generate()
  │                        + project_to_latent() helper
  └── src/probe.rs       ← NOVO: ModelProbe, soul vector extraction

hermes crate (existente)
  ├── src/hermes.rs      ← + subscribe_latent() + inject_latent_thought()
  └── src/evolve.rs      ← NOVO: EvolveAgent pipeline (generate→compile→test→promote)

agent-core crate (existente)
  ├── src/lib.rs         ← + FlowTrigger::Evolve; + AgentGenesis
  └── src/genesis.rs     ← NOVO: GenesisProtocol (breed→spawn→mutate)

neural-kernel (monólito)
  └── src/main.rs        ← + LATENT_BUS global; + EvolveAgent registro
                           + NeuOSProbeAgent início

hermes/src/agents.rs
  └── SleepCycleAgent    ← Expandido: v2 com dream simulação + geração
```

### 8.2 Cronologia vs ADR-0042

```
Sprint      ADR-0042 (N2→N5)          Pilares ADR-0047
──────     ────────────────           ─────────────────
109         N2 slice 2 (Trust)          LatentBus PoC (Cortex→Hermes)
110         N2/N3 (SelfHeal→Cortex)     LatentBus pleno + Projeção AVX2
111         N3 (Cortex generate)        EvolveAgent PoC (WASM hot-swap)
112         N4 (Hermes orquestra)       EvolveAgent pleno + SleepCycle v2
113         N5 (Jarbas persona)         NeuOS Probe fase 1 (layer mapping)
114         v2.0.0 release!            Soul vector PoC + ISA decompilação
115+        —                           Pilares amadurecem independentes
```

### 8.3 Non-goals (para esta ADR)

- Substituir Vec<u8> EventBus completamente — LatentBus é adicional
- Implementar scheduler neural (Maya/neurOS-style) — pilar futuro
- GPU-native inference (neurOS/Yantra-style) — fora do escopo CPU-only
- Comunicação cross-sistema (XKernel distributed IPC) — pós v2.0
- Agentes como processos separados (XKernel process supervisor) — ADR-0041 futuro

---

## 9. Referências Bibliográficas Completas

### 9.1 Comunicação Latente

1. **Du, Z., Wang, R., Bai, H., Cao, Z., Zhu, X., Zheng, B., Chen, W., & Ying, H.** (2026). *Enabling Agents to Communicate Entirely in Latent Space*. ACL 2026. arXiv:2511.09149. https://github.com/XiaoDu-flying/Interlat

2. **Xia et al.** (2026). *CondenseFlow: Scalable Latent Space Collaboration via Semantic Compression for Multi-Agent Systems*. ACL 2026 Findings.

3. **Dery, L. M., Yahav, Z., Prior, H., Feng, Q., Shen, J., & Szlam, A.** (2026). *Latent Space Communication via K-V Cache Alignment*. arXiv:2601.06123.

4. **Zou, J., Qiu, R., Li, G., Yang, X., Tieu, K., Lu, P., Shen, K., Tong, H., Choi, Y., He, J., Zou, J., Wang, M., & Yang, L.** (2026). *Latent Collaboration in Multi-Agent Systems* (LatentMAS). ICML 2026 Spotlight. https://github.com/Gen-Verse/LatentMAS

### 9.2 SO Neural / GPU-Native

5. **Price, R.** (2026). *neurOS: GPU-Native Neural Operating System*. https://github.com/robertcprice/nCPU

6. **Leonhart, E.** (2026). *Yantra: A Neuro-Symbolic, GPU-Native Operating System for Critical Systems*. clawRxiv:2605.02611. https://github.com/Emma-Leonhart/Sutra

7. **Kolaparthi, J. S.** (2026). *Maya: An AI-Native Operating System Kernel*. Zenodo:10.5281/zenodo.19218503.

8. **Berm, J.** (2026). *XKernel: Operating System for AI Agents*. https://github.com/JosephBerm/XKernel

9. **Deng, Y. et al.** (2025/2026). *NeuralOS: Towards Simulating Operating Systems via Neural Generative Models*. arXiv:2507.08800.

10. **Moschella et al.** (2022). *Relative representations enable zero-shot latent space communication*. NeurIPS 2022.

### 9.3 Auto-Evolução de Agentes

11. **Xia et al.** (2025). *Live-SWE-agent: Live Evolving Software Agent*. 75.4% on SWE-bench Verified.

12. **Fahim, M. A. I., Adebayo, O., & Ferrari, A.** (2026). *Software Self-Extension with SelfEvolve*. arXiv:2604.16314.

13. **Symbiont.rs** (2026). *Agent harness for hot-reloadable function evolution in Rust*. https://symbiont.rs/

14. **MUE-X** (2026). *The Agent That Writes Its Own Brain*. https://github.com/KorroAi/mue-x

15. **Korb, A.** (2026). *EVA — Evolvable Virtual Agent*. https://github.com/arturkorb3/eva-evolutional-agent

16. **EvolveOS** (2026). *A Self-Evolving Architecture for Autonomous Agent Civilization*. https://github.com/Kritagya123611/EvolveOS

17. **Xu, T., Wen, H., & Li, M.** (2026). *Adapting the Interface, Not the Model: Runtime Harness Adaptation for Deterministic LLM Agents*. arXiv:2605.22166.

18. **Robeyns et al.** (2025). *Self-Improving Coding Agent (SICA)*. arXiv.

### 9.4 Arquiteturas Antropomórficas

19. **Funasaki, H.** (2026). *NeuOS: Discovering and Exploiting the Neural Von Neumann Architecture Inside Pre-Trained Language Models*. Zenodo:10.5281/zenodo.20346754. 170 fases, 24 estações, Qwen2.5-0.5B. https://github.com/hafufu-stack/NeuOS

20. **Türkcan, M. K.** (2026). *Loom: A Scalable Analytical Neural Computer Architecture*. arXiv:2604.08816. https://github.com/mkturkcan/Loom

21. **Hao et al.** (2024). *Reasoning in continuous latent space* (Coconut). Meta AI.

22. **Cheng & Van Durme** (2024). *Continuous hidden state generation*.

### 9.5 Memória e Infraestrutura Cognitiva

23. **Arslan, M.** (2026). *Aeon: High-Performance Neuro-Symbolic Memory Management for Long-Horizon LLM Agents*. arXiv:2601.15311.

24. **leOS** (2026). *A substrate for AI agents that grows with use*. https://github.com/AnOversizedMooseWithSocks/leOS

25. **Du et al.** (2025). *Interlat: Enabling Agents to Communicate Entirely in Latent Space* (seção de memória: Atkinson-Shiffrin no EventBus). ACL 2026.

### 9.6 DIY / Hardware-level

26. **ZYO** (2026). *Sovereign, self-healing Linux CPU scheduler*. https://github.com/thesnmc/ZYO

27. **EVOSEAL** (2026). *Self-evolving AI agent with bidirectional evolution*. https://github.com/SHA888/EVOSEAL

28. **mutagent** (2026). *Python AI Agent framework for runtime self-iterating code*. PyPI.

### 9.7 Fontes complementares

29. **Ramesh & Li** (2025). *Activation grafting* — one-shot hidden state transfer.

30. **Tang et al.** (2025). *Hidden state + text trajectory* — latent communication ainda atrelado a texto.

31. **Fletcher et al.** (1995). *Mind-reading* — base neurocientífica para teoria da mente.

32. **Bisk et al.** (2020). *Information bottleneck of language* — fundamentação teórica.

33. **Zhu et al.** (2025b). *High-dimensional latent space encodes richer information*.

34. **Lyogavin** (2024-2026). *AirLLM: 70B models on 4GB GPU*. https://github.com/lyogavin/airllm

### 9.8 Speculative Decoding

35. **Alok** (2026). *N-gram speculative decoding in llama.cpp — rolling LCG hash, draft M tokens, GPU verify*. https://x.com/analogalok/status/2077718647905333549

---

## 10. Riscos e Mitigações

### 10.1 LatentBus

| Risco | Probabilidade | Impacto | Mitigação |
|-------|--------------|---------|-----------|
| **Dimensionalidade insuficiente**: 256D não captura informação suficiente do hidden state | Média | Médio | Configurar `LATENT_DIM` por agente. Fallback para texto se confiança baixa |
| **Overhead de projeção**: converter f32→f16→f32 custa caro | Baixa | Baixo | AVX2 intrinsic, tabela de conversão f32→f16 pré-computada |
| **Incompatibilidade cross-modelo**: Hermes e Cortex usam modelos diferentes | Média | Alto | LatentProjection com adapter treinável (Dery et al. 2026) |
| **Aumento de tráfego interno**: cada token gera um LatentPacket | Alta | Médio | Compressão mean-pool + publish só no último token |

### 10.2 EvolveAgent

| Risco | Probabilidade | Impacto | Mitigação |
|-------|--------------|---------|-----------|
| **Código gerado crasha o sistema** | Média | Crítico | Sandbox WASM obrigatório. Rollback automático em 100 ticks |
| **Loop infinito de evolução**: agente gera código que gera código que... | Baixa | Alto | Limite de 3 gerações por gap. Gatilho manual para reset |
| **Degeneração de fitness**: agente otimiza métrica errada | Alta | Médio | Ratchet (nunca menos checks que release atual) — EVA pattern |
| **Consumo de CPU durante geração**: LLM compila código em runtime | Alta | Médio | Rodar em background (PollEvery com prioridade baixa) |

### 10.3 NeuOS Probe

| Risco | Probabilidade | Impacto | Mitigação |
|-------|--------------|---------|-----------|
| **BitNet ternário não tem ISA decompilável**: descoberta de NeuOS pode ser específica de transformers densos (Qwen2.5) | Alta | Alto | Se falhar, pivotar para "análise de ativações" em vez de "decompilação ISA" |
| **Probe quebra o modelo**: isolar layers com skip pode corromper estado | Média | Alto | Só probe em snapshot (cópia do modelo). Nunca no modelo ativo |
| **Soul vectors não transferem entre seeds**: BitNet treinado diferentemente pode ter PCA diferente | Alta | Médio | Treinar probe em múltiplas seeds. Usar PCA incremental |
| **Custo computacional do probe**: 24 layers × 6 operações × N entradas | Média | Baixo | Fase única, execução lenta. Pode rodar em SleepCycle |

### 10.4 Mitigações Gerais

```
1. Feature flags: cada pilar atrás de cfg flag
   #[cfg(feature = "latent-bus")]
   #[cfg(feature = "evolve-agent")]
   #[cfg(feature = "neuos-probe")]

2. Non-fatal: falha de pilar não quebra boot
   match latent_bus.publish(packet) {
       Err(_) => continue_generation(), // fallback para texto
       Ok(_) => {}
   }

3. Observabilidade: cada pilar publica métricas no EventBus
   "LATENT_BUS_THROUGHPUT", "EVOLVE_PROMOTION_COUNT", "PROBE_LAYER_MAP"

4. Degradação graciosa: pilares podem ser desligados individualmente
   sem impacto no resto do sistema
```

---

## Apêndice A: Glossário de Termos

| Termo | Definição |
|-------|-----------|
| **Hidden state** | Vetor de ativação da última layer de um transformer (`last_hidden`). Contém a representação interna completa do pensamento do modelo |
| **Latent communication** | Transmissão de hidden states entre agentes, sem tokenização intermediária |
| **Soul vector** | 7 componentes principais (PCA) que codificam o comportamento completo de um modelo para uma tarefa. 128× compressão sem perda (NeuOS) |
| **ISA (Instruction Set Architecture)** | Conjunto de operações primitivas que um modelo executa internamente, decompilável por probe |
| **Ratchet** | Mecanismo de segurança: candidato a evolução nunca pode executar menos verificações que a versão atual (EVA) |
| **Genesis Protocol** | Mecanismo de "procriação" de agentes: alta performance → spawn de agente filho mutado |
| **Projection adapter** | Matriz linear que adapta hidden state do espaço do modelo A para o espaço do modelo B |
| **CondenseFlow LTC** | Latent Thought Condenser: comprime KV cache inteira em K vetores fixos via cross-attention |
| **Axon** | Vetor de largura fixa (Yantra) que serve como unidade de comunicação entre processos. Rotation binding sobre codebook de role-fillers |
| **Displacement codec** | Codificação de trajetórias de agente como vetores tangentes na hiperesfera (leOS). Compressão H.264-style com I-frames e P-frames |

---

## Apêndice B: Comparação com Projetos Similares (Matriz Decisão)

| Projeto | Latente | HW Native | Auto-Evolução | ISA Probe | Memória | Licença |
|---------|---------|-----------|---------------|-----------|---------|---------|
| **Interlat** (ACL 2026) | ✅ | ❌ GPU | ❌ | ❌ | ❌ | Apache |
| **CondenseFlow** (ACL 2026) | ✅ | ❌ GPU | ❌ | ❌ | ❌ | MIT |
| **LatentMAS** (ICML 2026) | ✅ | ❌ GPU | ❌ | ❌ | ❌ | Apache |
| **neurOS** (2026) | ❌ | ✅ GPU | ✅ (online) | ❌ | ❌ | MIT |
| **Yantra** (2026) | ✅ | ✅ GPU | ❌ | ❌ | ❌ | MIT |
| **Maya** (2026) | ❌ | ✅ CPU-only | ❌ | ❌ | ❌ | — |
| **XKernel** (2026) | ❌ | ❌ (Linux) | ❌ | ❌ | ❌ | MIT |
| **NeuOS** (2026) | ❌ | ❌ GPU | ❌ | ✅ | ❌ | — |
| **Loom** (2026) | ❌ | ✅ analítico | ❌ | ❌ | ❌ | MIT |
| **Live-SWE-agent** | ❌ | ❌ (Python) | ✅ (scaffold) | ❌ | ❌ | MIT |
| **SelfEvolve** | ❌ | ❌ (Python) | ✅ (TDD) | ❌ | ❌ | — |
| **symbiont.rs** | ❌ | ✅ Rust | ✅ (hot-swap) | ❌ | ❌ | MIT |
| **EVA** (2026) | ❌ | ❌ (Python) | ✅ (gated) | ❌ | ❌ | MIT |
| **MUE-X** (2026) | ❌ | ❌ (Python) | ✅ (AST) | ❌ | ❌ | MIT |
| **EvolveOS** | ❌ | ❌ (Node) | ✅ (Genesis) | ❌ | ✅ pgvector | — |
| **Aeon** (2026) | ❌ | ❌ (C++) | ❌ | ❌ | ✅ MemPalace | — |
| **leOS** (2026) | ✅ | ❌ (Python) | ✅ (reflex) | ❌ | ✅ hipersfera | MIT |
| **ZYO** (2026) | ❌ | ✅ Kernel | ✅ (eBPF) | ❌ | ❌ | — |
| **neural-os-core (alvo)** | ✅ | ✅ CPU-only | ✅ (WASM) | ✅ BitNet | ✅ MHI+Aeon | MIT |

**Diferenciais únicos:**
- Único projeto que combina LatentBus + Auto-Evolução + ISA Probe em SO bare-metal
- Único `no_std` com 3 pilares simultâneos
- Único a fazer NeuOS probe em BitNet ternário (ninguém testou)
- Único com SleepCycle + EvolveAgent integrados (dream → hardware → generate → promote)

---

*Fim do ADR-0047*
