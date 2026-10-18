# ADR-0078: Multi-Slot Multimodal com Aprendizado Contínuo — conversão GGUF→ternário, 6 slots reais, visão

**Data:** 2026-07-27
**Status:** Accepted (parcial — hub wired; multimodal residual)
**Lifecycle (INDEX):** `fazendo` (SESSION_481–487: niches + LOO + validate + evict)
**Extraído de:** Sessão de design 2026-07-27 (análise de viabilidade GGUF→ternário, aprendizado contínuo, multimodal)
**Substitui:** ADR-0033 (micro-learning — absorvido e expandido), ADR-0028 (GGUF research — implementado)
**Gates:** Fase 1 ✅ → Fase 2 → Fase 3 → Fase 4 (sequencial por dependência)

---

## 1. Contexto

O Neural-OS possui hoje um pipeline de LLM baseado em **BitNet ternário 2-bit** (ADD/SUB, zero multiplicação, soft-float friendly). O modelo ativo é um **BitNet 850M** (~150MB, PPL ~12). O sistema já carrega GGUF e converte para ternário via `gguf.rs`, mas o threshold fixo `0.1` perde ~2 pontos de perplexidade (PPL).

Paralelamente, o sistema tem ambições de:
- **Auto-healing** que exige raciocínio multi-passo (stack traces, crash logs)
- **Aprendizado contínuo** on-device (SleepCycle, AutoLearnAgent)
- **Visão** (ler screenshots, PCBs, documentos, streaming)
- **Voz** (STT + TTS — já existe em jarbas)
- **Seis slots de modelo** no ModelHub, mas hoje apenas Active + Fast + Pro + TinyStories são usados de fato

### Problemas identificados

1. **Um modelo não serve para tudo** — 850M para conversa ok, mas diagnóstico complexo (stack trace, arquitetura) precisa de 3B+.
2. **Threshold fixo 0.1** — perde informação que poderia ser preservada com threshold adaptativo por tensor.
3. **Aprendizado contínuo sem modelo dedicado** — o TinyStories (135M, 4MB) é usado só para smoke test; não há um slot para o SleepCycleAgent treinar continuamente.
4. **Sem visão** — auto-healing cego (não lê screenshot de crash, não vê PCB, não entende PDF).
5. **8GB de RAM disponíveis** — o sistema atual usa menos de 1GB. Há margem para modelos muito maiores.

## 2. Decisão

Adotar uma arquitetura **multi-slot com modelos convertidos de GGUF para ternário**, complementada por **subsistemas especializados** (visão, embedding, voz) que operam fora dos slots.

### 2.1 Conversão GGUF → ternário com threshold adaptativo

Em vez de `threshold=0.1` fixo, usar threshold por tensor baseado em percentil:

```rust
fn f32_to_ternary_packed_adaptive(data: &[f32]) -> PackedTernaryTensor {
    let abs_vals: Vec<f32> = data.iter().map(|v| v.abs()).collect();
    let mut sorted = abs_vals.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let threshold = sorted[data.len() * 15 / 100]; // top 85% viram ±1
    // ... threshold por tensor, não global
}
```

Ganho estimado: **0.5-1.0 PPL** recuperado sem custo de runtime.

### 2.2 Oito slots ModelHub — equiparação TII UAE (canônico s489+)

Fonte de descoberta: [huggingface.co/tiiuae/collections](https://huggingface.co/tiiuae/collections).
Política: **geradores ternários / visão TII** ocupam Active·Pro·Learner·Vision;
nichos **não-TII** (HwExpert, RustCoder, Reranker, Agent) ficam de outros vendors
ou vazios até artefato coerente. H1/SSM/Mamba **não** entram nestas gavetas até
loader híbrido (ver SESSION_489 discussão — custo ≠ conversão BitNet).

| Slot | Função no AIOS | Fonte TII (HF) preferida | Artefato FAT / lab | Footprint |
|---|---|---|---|---|
| **Active** | Chat default, CURRENT_MODEL | **Falcon3-3B** Instruct **1.58bit** (`tiiuae/Falcon3-3B-Instruct-1.58bit`); alt. Falcon-E-3B | `FALCON3.V7` ← `FALCON3_BASE.V7` (L=22 h=3072); **não** L=28 misname | ~1.21GB |
| **GeneratorPro** | Diagnóstico profundo, heal | **Falcon3-7B / 10B** Instruct 1.58bit | `PRO.V7` / `FALCON10.V7` (`materialize_hub_v7.py`) | ~2.2–2.9GB |
| **Learner** | Fine-tune / replay on-device | **Falcon3-1B** 1.58bit; alt. **Falcon-E-1B** | `LEARNER.V7` ← `FALCON3_1B.V7` | ~693MB |
| **Vision** | Encoder visual → pipelines VisionQA | **SigLino** (SigLIP2+DINOv3); consumidores **Falcon-OCR / Falcon-Perception** | Hoje: `VISION.BIN` v5 legado; upgrade = SigLino convertido | ~33MB (legado) |
| **Agent** | Orquestração agentic | *(sem análogo TII bitnet)* — H1-3B só após loader H1 | Ausente (`AGENT.v6` size_mismatch) → LOO miss → Active | — |
| **RustCoder** | Patches / código | *(não-TII)* DeepSeek Coder BitNet | `RUSTCDR3.BIN` v5 | ~321MB |
| **HwExpert** | HWID → família/fw | *(não-TII)* treino SDIO próprio | `hw_expert_v6.bitnet` / `HWEXPRT*` | ~0.25MB AlwaysTiny |
| **Reranker** | Cross-encoder RAG | *(não-TII)* BGE Reranker | `RERANKER.BIN` v5 | ~165MB |

**Fora destas gavetas (explícito):**

| Item TII / externo | Destino Neural |
|---|---|
| Falcon-H1 / H1-Tiny / H1R / FalconMamba | **Fora** até motor SSM — não empacotar como Active |
| Falcon-Arabic / Emirati | Active **só se** 1.58bit conversível; senão skill/persona |
| Falcon ASR | Path **STT** (fora hub), não Active |
| Benchmarks / datasets TII (3LM, NativeQA, E2LM…) | Host/eval — não ModelSlot |
| Laya / Jev / AgentJev | **Intent/router** (`decide_intent`, ROUTER) — não slot gerador |
| BGE embed, Piper, STT CTC, ROUTER.BITNET | Subsistemas §2.3 |

> Alias Pro (`mark_pro_alias`) quando Active tem `embed_dim ≥ 2048` — presença real =
> `slot_real`. Slot Fast/TinyStories absorvido por Active.

### 2.3 Subsistemas (fora dos slots geradores)

| Subsistema | Modelo | Função |
|---|---|---|
| **BGE** | BAAI Embedding (`BGE.BIN`) | RAG semântico (fora do hub gerador) |
| **STT** | CTC LSTM tiny *(futuro: Falcon ASR)* | Voz → texto |
| **Piper** | VITS TTS | Texto → voz |
| **OCR** | Layout mínimo *(futuro: Falcon-OCR via Vision)* | PDF/imagem → texto |

### 2.4 Fluxo revisado Cortex → Trinity → Hub (s488) — ADR-0088 AIOS-first

Premissa: **IA desde o boot**; toda geração é caminho cognitivo (classify → resolve →
act → telemetria). **Proibido** atalho “saudação → CURRENT_MODEL skip MoE” ou
AirLLM antes de classificar. AirLLM = degradação **adaptativa** só após
`resolve_route` dizer gerador.

```
Boot → Trinity init + ModelHub loaders (AlwaysTiny + LOO seam)
Usuário / EventBus
  └─ Hermes classify (pending_route) ──┐
Cortex generate_via_model              │
  ├─ take pending OR classify_intent ◄─┘  (sempre decide)
  ├─ resolve_route(name) → RouteTarget
  │     ├─ HwControl          → skill HAL (HITL caps)
  │     ├─ Pipeline(id)       → moe_compose (auto-composição)
  │     ├─ Slot{slot,wrap}    → generate_from_slot + LOO
  │     ├─ GeneratorSelect    → select_generator_slot → Active|Pro
  │     │                      (+ AirLLM se stream e rota geradora)
  │     ├─ SkillBackedActive  → Active + warn (DiskDiag/Security)
  │     ├─ OutsidePiper       → texto → TTS
  │     └─ FallbackActive     → CURRENT_MODEL (honesto)
  └─ note_route / pipe telemetria (n/a ≠ 0) → SGDB/HUD
```

**Contratos:** `slot_for_expert_kind` / `slot_for_expert_name` / `resolve_route`.
Pipelines = keyword sintético (`vision_qa`/`rag`/`heal`/`agentic_pipe`) — auto-composição
de funções pequenas; não incham `ModelSlot`.

**Residência:** AlwaysTiny = HwExpert (boot); LooMid/Heavy = LOO + evict.
`ExpertKind::Agent` first-class. Fallback ausente = warn + Active, nunca silêncio.

**Materialize (s489 HW-first):** LOO = `validate_for_slot` → `register_bytes` (nunca
`load_model` LLM-only no demand path). QEMU = smoke (`PIO_QEMU` 8MB; mid = miss
honesto). Aceite VERIFIED de RustCoder/Agent/Vision = **metal** (`pio_hw`/footprint
+ `usb_hw.img` com nichos empacotados).

### 2.5 Ciclo de aprendizado contínuo

```
DIA:
  Usuário conversa → Active/Pro responde
  AutoLearnAgent detecta padrão
    → coleta exemplos
    → fine-tune Learner (Falcon3-1B / Falcon-E-1B — §2.2)
    → registra novo expert no Trinity MoE

NOITE (SleepCycle):
  Phase 1 REPLAY  → Learner replaya padrões do dia
  Phase 2 DREAM   → gera variações sintéticas
  Phase 3 CONSOLIDATE → delta compression dos novos pesos
  Phase 4 PRUNE   → remove experts sem uso
  Phase 5 REFLECT → "aprendi N novos padrões hoje"

AUTO-HEALING:
  Crash → Vision (screenshot) → Pro (8B) analisa
    → RustCoder gera patch
    → compila → aplica → reinicia driver
```

### 2.6 Pipeline de visão

```
Imagem/Frame → SigLIP ViT-B → embedding 768d
                 ↓
Pro LLM (8B) recebe: "O que tem nessa imagem?" + embedding
                 ↓
Resposta multimodal embeddada no contexto textual

OCR:
  PDF → layout parser → texto estruturado
         → Pro analisa (datasheet, manual, log)
```

## 3. Consequências

### Positivas

- **Qualidade muito superior**: PPL ~7.8 (Llama 8B convertido) vs ~12 (BitNet 850M atual)
- **Auto-healing real**: 8B consegue analisar stack trace e planejar correção
- **Aprendizado contínuo**: Learner (125MB) fine-tunável em tempo real
- **Visão desbloqueada**: screenshots, PCBs, PDFs, streaming
- **6 slots de redundância**: fallback automático se um slot falhar
- **Cabe em 8GB**: ~5.8GB total, sobra 2.2GB

### Negativas / Riscos

- **Perda por threshold**: converter GGUF→ternário perde ~1-2 PPL vs modelo original. Threshold adaptativo recupera parte.
- **Modelos grandes no soft-float**: Llama 8B convertido deve rodar a ~15 tok/s (vs 80 do 850M). Aceitável para diagnóstico, não para conversa.
- **Manutenção de 6 slots**: mais código, mais RAM mapeada, mais pontos de falha no boot.
- **Visão depende de encoder separado**: SigLIP ViT-B (~350MB) é pesado para um subsistema. Alternativas menores (ViT-Tiny, 50MB) podem substituir se necessário.
- **Learner fine-tune**: o mecanismo de fine-tune on-device ainda não existe no código atual. É construção nova.

### Alternativas consideradas

| Alternativa | Motivo da rejeição |
|---|---|
| GGUF nativo (FP32) | Inviável — soft-float torna cada forward pass minutos |
| Apenas BitNet treinado maior (3B) | Precisaria treinar do zero. Converter GGUF existente é mais rápido |
| Um modelo multimodal único (Qwen2-VL) | Muito pesado (7B+). Separação Vision encoder + Pro LLM é mais modular |
| TinyStories como Learner (135M) | 135M é pequeno demais para aprender padrões significativos. Qwen 0.5B é 4× maior por 125MB |
| Sem visão (defer) | Auto-healing continuaria cego. Prioridade alta |

## 4. Critérios de aceite

### Fase 1 — Base
- [ ] `f32_to_ternary_packed_adaptive()` implementada e testada
- [ ] Python `convert_gguf_to_bitnet.py` funcional
- [ ] Llama 3.2 1B convertido carrega e gera texto no QEMU
- [ ] PPL do modelo convertido < PPL threshold fixo + 0.5

### Fase 2 — 6 slots
- [ ] `ModelSlot::Learner` adicionado ao enum
- [ ] Llama 8B + DeepSeek Coder + Qwen 0.5B convertidos e carregando
- [ ] Boot carrega todos os slots em paralelo
- [ ] Trinity MoE `select_generator_slot()` usa Fast/Active/Pro corretamente

### Fase 3 — Aprendizado
- [ ] AutoLearnAgent grava exemplos → fine-tuna Learner
- [ ] SleepCycle persiste delta do Learner no SGDB
- [ ] Trinity registra novo expert a partir de padrão detectado
- [ ] Boot recarrega expert aprendido

### Fase 4 — Visão
- [ ] SigLIP ViT-B codifica imagem → embedding 768d
- [ ] Pro LLM recebe embedding e responde sobre conteúdo visual
- [ ] OCR extrai texto de PDF/documento
- [ ] Pipeline visão→diagnóstico funcional em auto-healing

## 5. Recursos

- `crates/cortex/src/gguf.rs` — threshold adaptativo
- `crates/cortex/src/model_hub.rs` — novo slot Learner
- `crates/cortex/src/trinity.rs` — registro dinâmico de experts
- `tools/convert_gguf_to_bitnet.py` — script Python de conversão (NOVO)
- `crates/k_ai/src/learner.rs` — slot de aprendizado (NOVO)
- `crates/k_ai/src/vision/` — encoder SigLIP (NOVO)
- `tools/download_models.py` — download de modelos (NOVO)

## 6. Dependências externas

- Acesso a Hugging Face para download de GGUF (offline, no PC de desenvolvimento)
- Python 3.10+ com `torch`, `sentencepiece`, `gguf` (pip)
- Modelos: meta-llama/Llama-3.2-1B, meta-llama/Llama-3.1-8B, deepseek-coder-1.3b, Qwen/Qwen2.5-0.5B, google/siglip-vit-base-patch16-384
- Todos os modelos são Apache 2.0 ou MIT — compatíveis com o projeto
