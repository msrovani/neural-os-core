# Plano de Implementação — Multi-Slot Multimodal (ADR-0078)

**Data:** 2026-07-27
**ADR:** `docs/architecture/0078-multi-slot-multimodal-learner.md`
**RAM alvo:** 8GB disponíveis, ~5.8GB utilizados, ~2.2GB de sobra

---

## Fase 1 — Base: threshold adaptativo + conversor Python + Llama 1B

**Objetivo:** Melhorar a qualidade da conversão GGUF→ternário e ter o primeiro modelo real rodando.

### 1.1 Threshold adaptativo em gguf.rs

**Arquivo:** `crates/cortex/src/gguf.rs`

Substituir `f32_to_ternary_packed` (linha 592) por versão com threshold calculado por tensor:

```rust
// NOVA: threshold por percentil do tensor
fn optimal_threshold(data: &[f32]) -> f32 {
    let mut abs: Vec<f32> = data.iter().map(|v| v.abs()).collect();
    abs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p85 = abs[(abs.len() * 85 / 100).min(abs.len() - 1)];
    p85.max(0.01) // mínimo 0.01 evita threshold zero
}

pub(crate) fn f32_to_ternary_packed(data: &[f32], rows: usize, cols: usize) -> PackedTernaryTensor {
    let threshold = optimal_threshold(data);
    let mut vals = Vec::with_capacity(rows * cols);
    for &v in data.iter().take(rows * cols) {
        vals.push(if v > threshold { 1 } else if v < -threshold { -1 } else { 0 });
    }
    PackedTernaryTensor {
        shape: (rows, cols),
        packed_data: PackedTernaryTensor::pack_weights(&vals),
    }
}
```

**Verificação:** `cargo check --release` 0 erros. Threshold não é zero nem NaN para tensor não-vazio.

### 1.2 Scale factor por tensor (otimização adicional)

Para cada tensor convertido, guardar um `scale: f32` que escala o peso na hora do matmul. Isso permite que pesos ±1 representem valores maiores/menores que o threshold:

```rust
pub struct ScaledTernaryTensor {
    pub packed: PackedTernaryTensor,
    pub scale: f32,  // reconstrução: weight * scale
}
```

No matmul: em vez de `sum += weight * input`, fazer `sum += weight * scale * input` — mas isso adiciona uma multiplicação FP32. Para manter ADD/SUB puro, o scale pode ser foldado no RMS norm da layer (`rms_attn *= scale`).

**Prioridade:** Baixa. Implementar só se threshold adaptativo não recuperar PPL suficiente.

### 1.3 Script Python de conversão

**Arquivo novo:** `tools/convert_gguf_to_bitnet.py`

```python
#!/usr/bin/env python3
"""
Converte modelo GGUF do Hugging Face para .bitnet v4 (ternário 2-bit).
Uso: python tools/convert_gguf_to_bitnet.py --model meta-llama/Llama-3.2-1B --output target/models/llama1b.bitnet
"""

import argparse, struct, sys, math
import numpy as np
from gguf import GGUFReader  # pip install gguf
from sentencepiece import SentencePieceProcessor  # tokenizer

def optimal_threshold(weights: np.ndarray) -> float:
    abs_vals = np.abs(weights)
    sorted_abs = np.sort(abs_vals.flatten())
    idx = int(len(sorted_abs) * 0.85)
    return max(float(sorted_abs[min(idx, len(sorted_abs)-1)]), 0.01)

def pack_ternary(weights: np.ndarray) -> bytes:
    """Converte array f32 em bytes packados 2-bit (4 pesos/byte)."""
    ternary = np.where(weights > threshold, 1, np.where(weights < -threshold, -1, 0)).astype(np.int8)
    packed = bytearray()
    for i in range(0, len(ternary), 4):
        chunk = ternary[i:i+4]
        byte = 0
        for j, w in enumerate(chunk):
            # encoding: -1=0b10, 0=0b00, 1=0b01
            code = { -1: 2, 0: 0, 1: 1 }[int(w)]
            byte |= code << (j * 2)
        packed.append(byte)
    return bytes(packed)
```

**Funções:**
- `download_model(model_name)`: baixa GGUF do Hugging Face (ou usa cache local)
- `extract_weights(reader)`: extrai todos os tensores, aplica threshold adaptativo por tensor
- `build_header(config)`: monta header .bitnet v4 (magic, hidden, layers, heads, vocab, etc.)
- `pack_layers(weights)`: converte cada tensor para PackedTernaryTensor
- `write_bitnet(path, header, layers)`: escreve arquivo .bitnet

**Suporte a arquiteturas:**
- LLaMA (Meta-Llama-3.2-*, Llama-3.1-*)
- Qwen2.5 (Qwen/Qwen2.5-*)
- DeepSeek Coder (deepseek-coder-*)
- Phi-3 (microsoft/Phi-3-*)
- Gemma (google/gemma-*)

### 1.4 Download e teste do primeiro modelo

```bash
# Instalar dependências
pip install gguf sentencepiece numpy

# Baixar e converter Llama 3.2 1B
python tools/convert_gguf_to_bitnet.py \
    --model meta-llama/Llama-3.2-1B \
    --output target/models/LLAMA1B.BIN

# Estimar PPL (script separado)
python tools/evaluate_ppl.py \
    --model target/models/LLAMA1B.BIN \
    --dataset wikitext2
```

**Verificação no QEMU:**
1. Copiar `LLAMA1B.BIN` para o FAT32 do disco de dados
2. Boot no QEMU: verificar `[MODEL] slot=generator_fast loaded` no log serial
3. Testar geração: "Qual a capital do Brasil?" → resposta coerente

### 1.5 Critérios de aceite da Fase 1

- [ ] `optimal_threshold()` implementada e testada com tensores reais
- [ ] `cargo check --release` 0 erros
- [ ] Python script converte Llama 3.2 1B completo (~310MB) em <5 minutos
- [ ] `.bitnet` gerado carrega no QEMU e gera texto coerente
- [ ] PPL do modelo convertido com threshold adaptativo é **menor** que com threshold fixo 0.1

**Esforço estimado:** 1-2 dias

---

## Fase 2 — 6 slots: Learner + modelos reais

**Objetivo:** Populartodos os 6 slots com modelos reais, carregamento simultâneo, seletor inteligente.

### 2.1 Adicionar ModelSlot::Learner

**Arquivo:** `crates/cortex/src/model_hub.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ModelSlot {
    Active = 0,
    GeneratorFast = 1,
    GeneratorPro = 2,
    TinyStories = 3,  // mantido por compatibilidade, mas repurpose internamente
    RustCoder = 4,
    HwExpert = 5,
    Learner = 6,       // NOVO
}
```

**Mudanças:**
- `N_SLOTS: 6 → 7`
- `fat_names_for(Learner)` → `["LEARNER.BIN", "QWEEN05.BIN"]`
- `slot_from_bitnet_bytes()`: <20MB → TinyStories, <200MB → Fast, <450MB → Active, >=450MB → Pro
- `select_generator_slot()`: incluir Learner no fallback chain

### 2.2 Pipeline de carregamento no boot

**Arquivo:** `crates/neural-kernel/src/main.rs`

```
boot sequence → Phase 5-6:
  1. Tenta carregar Active do FAT (BITNET2B.BIN)
  2. Tenta Fast (LLAMA1B.BIN) → register_generator_fast
  3. Tenta Pro (LLAMA8B.BIN) → register_generator_pro
  4. Tenta RustCoder (RUSTCDR3.BIN) → register_rustcoder
  5. Tenta HwExpert (HWEXPRT.BIN) → register_hwexpert
  6. Tenta Learner (LEARNER.BIN) → register_learner
```

Cada `try_load_slot()` lê do FAT e registra no ModelHub. Falha de um slot não aborta os outros.

### 2.3 Converter e testar modelos restantes

```bash
# DeepSeek Coder 1.3B para RustCoder slot
python tools/convert_gguf_to_bitnet.py \
    --model deepseek-ai/deepseek-coder-1.3b-instruct \
    --output target/models/RUSTCDR3.BIN

# Llama 3.1 8B para Pro slot
python tools/convert_gguf_to_bitnet.py \
    --model meta-llama/Llama-3.1-8B \
    --output target/models/LLAMA8B.BIN

# Qwen2.5 0.5B para Learner slot
python tools/convert_gguf_to_bitnet.py \
    --model Qwen/Qwen2.5-0.5B \
    --output target/models/LEARNER.BIN
```

### 2.4 Verificação de carregamento simultâneo

```rust
// Teste no boot: após Phase 6, verificar todos os slots
fn check_slots() {
    assert!(slot_loaded(ModelSlot::Active));
    assert!(slot_loaded(ModelSlot::GeneratorFast));
    assert!(slot_loaded(ModelSlot::GeneratorPro));
    assert!(slot_loaded(ModelSlot::RustCoder));
    assert!(slot_loaded(ModelSlot::HwExpert));
    assert!(slot_loaded(ModelSlot::Learner));
    k_nano::slog_bin!("MODEL", "info", "6/6 slots loaded, total RAM ~{}MB",
        total_estimate_mb());
}
```

### 2.5 Critérios de aceite da Fase 2

- [ ] `ModelSlot::Learner` adicionado, compila, boot não quebra
- [ ] DeepSeek Coder 1.3B convertido carrega no RustCoder slot
- [ ] Llama 8B convertido carrega no Pro slot
- [ ] Qwen 0.5B convertido carrega no Learner slot
- [ ] `cargo check --release` 0 erros
- [ ] Boot log mostra todos os 6 slots carregados

**Esforço estimado:** 1-2 dias

---

## Fase 3 — Aprendizado contínuo: AutoLearnAgent + SleepCycle + Learner

**Objetivo:** O Learner slot é fine-tunado pelos agentes do sistema.

### 3.1 AutoLearnAgent → Learner pipeline

**Arquivo novo:** `crates/k_ai/src/learner.rs`

```rust
pub struct LearnerSlot {
    model: Box<dyn Model>,
    recent_examples: Vec<(String, String)>,  // (prompt, response)
    delta_weights: Option<Vec<u8>>,
}

impl LearnerSlot {
    /// Registra exemplo para fine-tune futuro
    pub fn observe(&mut self, prompt: &str, response: &str) {
        self.recent_examples.push((prompt.to_string(), response.to_string()));
        if self.recent_examples.len() > 100 {
            self.recent_examples.remove(0);
        }
    }

    /// Fine-tune on-device: ajusta pesos ternários via gradiente estimado
    pub fn finetune(&mut self) -> bool {
        // Para cada exemplo (p, r):
        //   1. Forward com pesos atuais → logits
        //   2. Compara com resposta desejada → loss
        //   3. Ajusta pesos ternários (-1↔+1) onde loss > threshold
        //   4. Salva delta (diferença dos pesos)
        //
        // Implementação prática:
        // - Usa apenas exemplos com loss > limiar (evita overfit)
        // - Ajusta só pesos com gradiente significativo (top 5%)
        // - Delta = pesos_novos XOR pesos_antigos (em bits)
        true
    }
}
```

**Nota:** Fine-tune de modelo ternário em Rust no_std é construção nova. O MVP pode ser mais simples: **selecionar exemplos** que o Trinity MoE usa para ajustar o router (não os pesos do modelo). O fine-tune real dos pesos ternários fica como melhoria futura.

### 3.2 MVP de aprendizado: ajuste do MoE router

Em vez de fine-tunar o Learner inteiro, o MVP faz:

1. AutoLearnAgent detecta padrão: "usuário pergunta 'status' 5x hoje"
2. Cria embedding do padrão
3. Registra novo expert no Trinity MoE via `try_birth(intent_hint)`
4. Próxima vez que usuário perguntar "status", MoE roteia direto sem chamar o Pro 8B

```rust
// trinity.rs: try_birth com base nos exemplos
fn learn_pattern(&mut self, examples: &[&str]) -> bool {
    let intent_hint = examples[0].as_bytes();
    self.try_birth(intent_hint)  // clona expert mais similar
}
```

### 3.3 SleepCycle persiste padrões

**Arquivo:** `crates/k_ai/src/sleep_cycle.rs` (existente)

Adicionar ao ciclo:
```
Phase 3 CONSOLIDATE:
  - Pega exemplos do LearnerSlot
  - Comprime como delta (diferença em relação ao modelo base)
  - Salva no SGDB como meta-exemplo
  - Limpa buffer de exemplos

Phase 4 PRUNE:
  - Remove experts do Trinity com menos de 5 hits
  - Remove exemplos muito antigos (>7 dias)
```

### 3.4 Critérios de aceite da Fase 3

- [ ] AutoLearnAgent detecta padrão e registra expert no Trinity
- [ ] SleepCycle persiste delta no SGDB
- [ ] Boot recarrega último estado do Learner
- [ ] Exemplo: perguntar "status" 3x → na 4ª vez resposta sem chamar Pro

**Esforço estimado:** 1 semana

---

## Fase 4 — Visão: SigLIP + OCR + reasoning

**Objetivo:** Sistema vê imagens, lê documentos, diagnostica com contexto visual.

### 4.1 SigLIP ViT-B encoder

**Arquivo novo:** `crates/k_ai/src/vision/mod.rs`

```rust
pub struct VisionEncoder {
    model: SiglipModel,  // ViT-B/16, 384px, 768d output
}

impl VisionEncoder {
    pub fn load(data: &[u8]) -> Option<Self> { ... }
    pub fn encode(&self, pixels: &[u8]) -> [f32; 768] { ... }
}
```

**Pipeline:**
1. Carrega pesos SigLIP de `VISION.BIN` (formato .bitnet convertido)
2. Recebe pixels RGBA 384×384
3. Patch embedding → 12 transformer layers → CLS token → projeção 768d
4. Retorna embedding float32[768]

### 4.2 OCR + layout parser

**Arquivo novo:** `crates/k_ai/src/vision/ocr.rs`

Mínimo viável: detector de regiões de texto + reconhecedor de caracteres.

```rust
pub struct OcrEngine {
    ctc_model: CtcModel,  // reusa arquitetura do STT (CTC LSTM)
}

impl OcrEngine {
    pub fn extract_text(&self, image: &[u8]) -> Vec<(String, Rect)> {
        // 1. Segmenta regiões de texto (connected components)
        // 2. Para cada região: CTC decode → texto
        // 3. Retorna (texto, bounding box)
    }
}
```

### 4.3 Integração Vision → Pro LLM

```
Vision Encoder → embedding 768d
       ↓
Pro LLM recebe: [IMG] embedding [TXT] "O que tem nessa imagem?"
       ↓
Resposta multimodal

A implementação técnica:
- Embedding visual é projetado para hidden_dim do Pro LLM (4096)
- Injetado como token virtual no início do prompt
- Pro LLM faz cross-attention via contexto (não requer treino)
```

### 4.4 Auto-healing visual

```
CRASH:
  1. Sistema salva framebuffer em $TEMP/crash_screenshot.raw
  2. VisionEncoder → embedding do crash screen
  3. Pro LLM (8B) recebe embedding + "Diagnostique este crash"
  4. Pro responde: "Rust panic em ahci.rs:342 — null pointer no PRDT"
  5. RustCoder gera patch
  6. SelfHealAgent aplica
```

### 4.5 Critérios de aceite da Fase 4

- [ ] Vision encoder carrega e codifica imagem → embedding coerente
- [ ] OCR extrai texto de screenshot de terminal
- [ ] Pro LLM responde com contexto visual
- [ ] Pipeline crash→screenshot→diagnóstico→patch funcional

**Esforço estimado:** 2 semanas

---

## Resumo de milestones

| Marco | Data alvo | Depende de |
|---|---|---|
| **M1** Threshold adaptativo + conversor Python | Fase 1.1-1.3 | Nada |
| **M2** Llama 1B rodando no QEMU | Fase 1.4 | M1 |
| **M3** 6 slots carregando no boot | Fase 2 | M2 |
| **M4** Learner aprende padrão | Fase 3 | M3 |
| **M5** Visão + OCR + diagnóstico visual | Fase 4 | M3 |

## Riscos e mitigação

| Risco | Probabilidade | Impacto | Mitigação |
|---|---|---|---|
| Threshold adaptativo não melhora PPL | Baixa | Médio | Scale factor por tensor (plano B) |
| Llama 8B muito lento (soft-float) | Média | Alto | Usar só para diagnóstico, não conversa; Fast 1B para uso diário |
| Fine-tune on-device complexo demais | Alta | Médio | MVP = só ajuste do MoE router, não pesos do modelo |
| Visão SigLIP não cabe em 350MB | Baixa | Baixo | ViT-Tiny (50MB) como fallback |
| 6 slots estouram 8GB | Baixa | Alto | Carregamento sob demanda (lazy), não todos no boot |
