# Plano de Correções e Melhorias: k_ai + cortex

**Data:** 2026-08-23  
**Versão:** 1.0  
**Base:** Análise profunda em `docs/evidence/ANALYSIS_KAI_CORTEX_2026-08-23.md`  
**Pré-requisito:** `cargo check --release` = 0 erros (validado)

---

## Visão Geral

```
Fase 0 (P0) ──► Build Breaker Fix ──► cargo build --release OK
  │
  ├─► Fase 1 (P1) ──► Hub Auto-Detect ──► LLM selecionado automaticamente
  │     ├─ 1A: from_name aliases Falcon3
  │     ├─ 1B: fat_names_for .v6 first
  │     ├─ 1C: slot_from_bitnet_bytes por header
  │     └─ 1D: GGUF auto-integration
  │
  ├─► Fase 2 (P2) ──► Trinity MoE ──► Routing neural funcional
  │     ├─ 2A: expert_weight_source completo
  │     ├─ 2B: classify_keywords i18n
  │     └─ 2C: Router MoE treinado (futuro)
  │
  ├─► Fase 3 (P3) ──► k_ai Limpeza ──► Zero dead code
  │     ├─ 3A: Remover arch/x86_64.rs
  │     ├─ 3B: Simplificar arch/simd.rs
  │     └─ 3C: cognitive.rs — marcar/remover dead code
  │
  └─► Fase 4 (P4) ──► Auto-Select LLM ──► AIOS real
        ├─ 4A: is_complex_conversation via MoE
        └─ 4B: select_generator por ModelHeader
```

---

## Fase 0 — Build Breaker Fix (P0)

**Objetivo:** `cargo build --release -p neural-kernel --target x86_64-unknown-none` = 0 erros  
**Bloqueador:** NADA mais pode ser testado/buildado sem isso

### P0-1: Remover `k_ai/src/arch/x86_64.rs`

**Por quê:** 596 LOC de intrinsics SSE4.2/AVX2/AVX-512 que crasham rustc com `-C target-feature=-sse`.  
**Status:** Dead code — nenhuma função é chamada por nenhum caller (verificado via `code_search`).  
**Referência:** `ternary.rs:9` menciona em doc comment mas NÃO importa.

**Ação:**
```
1. Deletar crates/k_ai/src/arch/x86_64.rs
2. Atualizar crates/k_ai/src/arch/mod.rs:
   - Remover: pub mod x86_64;
   - Manter:  pub mod simd;
3. Atualizar doc comment em ternary.rs:9:
   - De: "Dispatch SIMD: k_ai::arch::x86_64"
   - Para: "Dispatch SIMD: cortex::compute::dispatch_ternary"
```

**Verificação:** `cargo check -p k_ai` = 0 erros

### P0-2: Remover `neural-sgdb` de `k_ai/Cargo.toml`

**Por quê:** neural-sgdb contém SSE2 intrinsics (`_mm_*`) que crasham rustc com soft-float.  
**Ação:**
```
1. Remover linha: neural-sgdb = { path = "../../neural-sgdb", default-features = false }
2. Verificar se algum módulo k_ai importa neural-sgdb diretamente
3. Se sim → gatear com #[cfg(feature = "nsgdb")] ou remover a dependência do módulo
```

**Nota:** O `nsgdb_bridge.rs` e `tickv_adapter.rs` foram criados na sessão ADR-0091 mas a dependência neural-sgdb quebra o build. Manter o bridge como stub até que neural-sgdb suporte no_std sem SIMD.

### P0-3: Validar build

```bash
cargo clean -p neural-kernel
cargo check -p k_ai                    # 0 erros
cargo check -p cortex                  # 0 erros
cargo check --release -p neural-kernel --target x86_64-unknown-none  # 0 erros
cargo build --release -p neural-kernel --target x86_64-unknown-none  # 0 erros, ELF gerado
```

---

## Fase 1 — Hub Auto-Detect Falcon3 (P1)

**Objetivo:** O sistema detecta automaticamente qual LLM carregar, sem nomes hardcoded

### 1A: `ModelSlot::from_name()` — aliases Falcon3

**Arquivo:** `crates/cortex/src/model_hub.rs:38-53`

**Mudança:**
```rust
pub fn from_name(s: &str) -> Option<Self> {
    match s {
        // Active: Falcon3-3B-Instruct (LLM geral padrão)
        "active" | "current" | "falcon3" | "falcon3b" | "falcon3-3b" 
        | "tiiuae" | "3b" => Some(Self::Active),
        
        // GeneratorPro: Falcon3-7B/10B (LLM pesado)
        "generator_pro" | "pro" | "falcon7b" | "falcon3-7b" 
        | "falcon3-10b" | "10b" => Some(Self::GeneratorPro),
        
        // Vision (inalterado)
        "vision" | "siglip" | "vit" | "encoder" => Some(Self::Vision),
        
        // Reranker (inalterado)
        "reranker" | "rerank" | "cross_encoder" | "bge_reranker" => Some(Self::Reranker),
        
        // RustCoder (inalterado)
        "rust_coder" | "rustcoder" => Some(Self::RustCoder),
        
        // HwExpert (inalterado)
        "hw_identify" | "hwexpert" => Some(Self::HwExpert),
        
        // Learner (inalterado)
        "learner" | "qwen05" | "qwen0.5b" => Some(Self::Learner),
        
        // Agent (inalterado)
        "agent" | "qwen3b" | "agentic" | "orchestrator" => Some(Self::Agent),
        
        _ => None,
    }
}
```

### 1B: `fat_names_for()` — .v6 primeiro

**Arquivo:** `crates/cortex/src/model_hub.rs`

**Mudança em `Active`:**
```rust
ModelSlot::Active => &[
    "FALCON3B.v6",    // ← NOVO: Falcon3-3B-Instruct v6 (canonical)
    "FALCON3B.BIN",   // Falcon3-3B legado
    "FALCN3B.GGUF",   // Falcon3 GGUF
    "BITNET2B.v6",    // BitNet 2B v6 (fallback)
    "BITNET2B.BIN",   // BitNet 2B legado
    "BITNET13.BIN",
    "BITNET850.BIN",
    "BITNET3B.BIN",
    "BITNET.BIN",
    "MICRO.BITNET",
    "MICRO.BIN",
    "LLAMA8B.BIN",
],
```

**Mudança em `GeneratorPro`:**
```rust
ModelSlot::GeneratorPro => &[
    "PRO.v6",           // Falcon3-7B v6 (canonical)
    "FALCON7B.v6",      // alias alternativo
    "PRO.BIN",          // Falcon3-7B legado
    "FALCON7B.BIN",
    "BITNET3B.BIN",     // fallback legado
    "BITN3B.BIN",
    "LLAMA8B.BIN",
    "BITNET2B.BIN",     // último recurso
],
```

### 1C: `slot_from_bitnet_bytes()` — usar header v6

**Arquivo:** `crates/cortex/src/model_hub.rs`

**Mudança:**
```rust
pub fn slot_from_bitnet_bytes(data: &[u8]) -> ModelSlot {
    // Tenta parse do header v6 primeiro (autônomo — zero hardcoded)
    if let Some(h) = crate::model::parse_model_header(data) {
        let params = h.estimated_params();
        let mb = h.file_size_mb();
        k_nano::slog_cortex!("MODEL", "info", 
            "slot_from_header: params={} MB={} hidden={} layers={}", 
            params, mb, h.hidden, h.num_layers);
        
        return match params {
            0..=100_000_000 => ModelSlot::Reranker,        // <100M params
            100_000_000..=600_000_000 => ModelSlot::Learner, // ~0.5B (Qwen-0.5B)
            600_000_000..=4_000_000_000 => ModelSlot::Active, // ~3B (Falcon3-3B)
            4_000_000_000..=12_000_000_000 => ModelSlot::GeneratorPro, // ~7B (Falcon3-7B)
            _ => ModelSlot::GeneratorPro,                    // >12B
        };
    }
    
    // Fallback: tamanho bruto (legado, sem header v6)
    const MB: usize = 1024 * 1024;
    if data.len() < 20 * MB {
        ModelSlot::Reranker
    } else if data.len() < 200 * MB {
        ModelSlot::Learner
    } else if data.len() < 450 * MB {
        ModelSlot::Vision
    } else if data.len() < 1100 * MB {
        ModelSlot::Agent
    } else {
        ModelSlot::GeneratorPro
    }
}
```

### 1D: GGUF auto-integration

**Arquivo:** `crates/cortex/src/model_hub.rs`

**Mudança em `register_bytes()`:**
```rust
pub fn register_bytes(slot: ModelSlot, data: &[u8]) -> bool {
    // 1. Tenta v6 .bitnet primeiro
    if let Some(view) = crate::model::load_model_v6(data) {
        return register_model_view(slot, view);
    }
    
    // 2. Tenta GGUF
    if crate::gguf::is_gguf(data) {
        if let Some(m) = crate::gguf::load_gguf_as_transformer(data) {
            let boxed: Box<dyn Model> = Box::new(m);
            register_boxed_model(slot, boxed);
            return true;
        }
    }
    
    k_nano::slog_cortex!("MODEL", "warn", 
        "register_bytes: formato desconhecido slot={}", slot.name());
    false
}
```

**Arquivo:** `crates/cortex/src/gguf.rs`

**Adicionar:** `pub fn is_gguf(data: &[u8]) -> bool` e `pub fn load_gguf_as_transformer(data: &[u8]) -> Option<TransformerModel>` — wrappers que convertem GGUF→TransformerModel.

---

## Fase 2 — Trinity MoE (P2)

**Objetivo:** Router MoE funcional com routing neural real

### 2A: `expert_weight_source()` — completo

**Arquivo:** `crates/cortex/src/trinity.rs`

**Mudança:**
```rust
pub fn expert_weight_source(kind: ExpertKind) -> Option<&'static str> {
    match kind {
        ExpertKind::HwIdentify => Some("HWEXPRT.v6"),     // hwexpert treinado
        ExpertKind::RustCoder => Some("RUSTCDR.v6"),      // rustcoder treinado
        ExpertKind::Generator => Some("PRO.v6"),          // Falcon3-7B como expert
        ExpertKind::Security => Some("SECURITY.v6"),      // se treinado
        ExpertKind::DiskDiag => Some("DISKDIAG.v6"),      // se treinado
        ExpertKind::SpeechSynth => None,                   // skill pura (TTS nativo)
        ExpertKind::HwControl => None,                     // skill pura (volume/mute)
        ExpertKind::Unknown => None,
    }
}
```

### 2B: `classify_keywords()` — i18n (PT-BR + EN + ES)

**Arquivo:** `crates/cortex/src/trinity.rs`

**Mudança:** Adicionar palavras-chave em inglês e espanhol para cada expert:

```rust
// HwControl — PT-BR + EN + ES
let is_hw_control = has_word("volume") || has_word("mute") || has_word("brilho")
    || has_word("brightness") || has_word("volumen") || has_word("silencio")
    || has_word("luminosidad") || has_word("dimmer");

// Chat/Greeting — PT-BR + EN + ES
let is_chat = has_word("oi") || has_word("hello") || has_word("hola")
    || has_word("hey") || has_word("ola") || has_word("saludos")
    || lower.contains("bom dia") || lower.contains("good morning") 
    || lower.contains("buenos dias");

// Security — PT-BR + EN + ES
let is_security = has_word("security") || has_word("seguranca") 
    || has_word("seguridad") || has_word("cve") || has_word("ataque")
    || has_word("attack") || has_word("vulnerabilidade") || has_word("vulnerability");
```

### 2C: Router MoE treinado (futuro — requer `tools/train_router.py`)

**Status:** ⏳ Depende de treinar o router com dados reais de intent classification  
**Ação futura:** Gerar `ROUTER.BIN` e incluir na imagem FAT

---

## Fase 3 — k_ai Limpeza (P3)

**Objetivo:** Zero dead code, zero crash paths

### 3A: Confirmar remoção de `arch/x86_64.rs`

Já coberto pelo P0-1. Verificar que nenhum teste referencia o módulo.

### 3B: Simplificar `arch/simd.rs`

**Arquivo:** `crates/k_ai/src/arch/simd.rs`

**Mudança:** O `simd.rs` já aponta para os kernels do cortex (`bitnet_avx512`, `bitnet_avx2`). Manter como interface de alta nível mas documentar que o dispatch real é `cortex::compute::dispatch_ternary`.

```rust
//! ADR-0061: Static SIMD Kernel Dispatch
//!
//! NOTA: O dispatch principal vive em cortex::compute::dispatch_ternary.
//! Este módulo é uma interface de alta nível para callers que precisam
//! do kernel diretamente (ex.: testes, benchmarks).
```

### 3C: `cognitive.rs` — avaliar dead code

**Arquivo:** `crates/k_ai/src/cognitive.rs` (1616 LOC)

**Ação:** Verificar callers de cada struct:
- `IntentPlanner` — usado em algum lugar? → se não, marcar `#[allow(dead_code)]` ou remover
- `SuccessEngine` — usado em algum lugar? → se não, marcar
- `NeuralCache` — usado em algum lugar? → se não, marcar
- `MatMulFreeLM` — usado em algum lugar? → se não, marcar
- `TernaryUpdate`, `ReplayBuffer`, `WeightConsolidation` — verificar callers

**Decisão:** Se não há callers → marcar com `#[allow(dead_code)]` explícito (não remover, pois são "residuals" documentados no ADR-0084).

---

## Fase 4 — Auto-Select LLM (P4)

**Objetivo:** O AIOS seleciona automaticamente o LLM certo para cada tarefa

### 4A: `is_complex_conversation()` via MoE

**Arquivo:** `crates/cortex/src/model_hub.rs`

**Mudança:** Usar o router MoE (quando disponível) em vez de keywords:

```rust
pub fn is_complex_conversation(prompt: &str) -> bool {
    // Heurística rápida (comprimento)
    if prompt.len() > 160 { return true; }
    
    // Se o router MoE está treinado, usar ele
    // (o router classifica a complexidade implicitamente via expert selection)
    // Por agora, manter heurísticas + adicionar mais keywords
    contains_ci(prompt, "detalhad")
        || contains_ci(prompt, "analis")
        || contains_ci(prompt, "explain")
        || contains_ci(prompt, "compare")
        || contains_ci(prompt, "porque")
        || contains_ci(prompt, "why ")
        || contains_ci(prompt, "architect")
        || contains_ci(prompt, "desenhe")    // PT-BR
        || contains_ci(prompt, "design")     // EN
        || contains_ci(prompt, "diseño")     // ES
        || (prompt.len() > 80 && (
            contains_ci(prompt, "como ") 
            || contains_ci(prompt, "how ")
            || contains_ci(prompt, "cómo ")
        ))
}
```

### 4B: `select_generator_slot()` por ModelHeader

**Arquivo:** `crates/cortex/src/model_hub.rs`

**Mudança:** Usar o header do modelo carregado para decidir:

```rust
pub fn select_generator_slot(prompt: &str) -> ModelSlot {
    let complex = is_complex_conversation(prompt);
    
    if complex {
        // Para prompts complexos: preferir modelo maior
        if hub_has_blob(ModelSlot::GeneratorPro) {
            return maybe_fit(ModelSlot::GeneratorPro);
        }
        // Se só temos Active (Falcon3-3B), usar ele
        if slot_loaded(ModelSlot::Active) {
            return maybe_fit(ModelSlot::Active);
        }
    }
    
    // Para prompts simples: modelo rápido
    if slot_loaded(ModelSlot::Active) {
        return maybe_fit(ModelSlot::Active);
    }
    
    // Fallback: qualquer modelo disponível
    if hub_has_blob(ModelSlot::GeneratorPro) {
        return maybe_fit(ModelSlot::GeneratorPro);
    }
    
    ModelSlot::Active
}
```

---

## Ordem de Execução

| # | Fase | Dependências | Esforço | Validação |
|---|------|-------------|---------|-----------|
| P0-1 | Remover arch/x86_64.rs | Nenhuma | 10 min | `cargo check -p k_ai` |
| P0-2 | Remover neural-sgdb dep | Nenhuma | 5 min | `cargo check -p k_ai` |
| P0-3 | Validar build | P0-1 + P0-2 | 15 min | `cargo build --release` |
| 1A | from_name aliases | P0 | 15 min | Teste unitário |
| 1B | fat_names_for .v6 | P0 | 10 min | Teste unitário |
| 1C | slot_from_bitnet_bytes | P0 | 30 min | Teste unitário |
| 1D | GGUF auto-integration | P0 | 45 min | Teste host |
| 2A | expert_weight_source | P0 | 10 min | Teste unitário |
| 2B | classify_keywords i18n | P0 | 30 min | Teste unitário |
| 3A | Confirmar remoção | P0-1 | 5 min | grep |
| 3B | Simplificar simd.rs | P0-1 | 10 min | `cargo check -p k_ai` |
| 3C | cognitive dead code | P0 | 30 min | `cargo check -p k_ai` |
| 4A | is_complex via MoE | 1A | 30 min | Teste |
| 4B | select_generator | 1C | 30 min | Teste |

**Total estimado:** ~5h de trabalho

---

## Critérios de Aceite

| Critério | Como validar |
|----------|-------------|
| Build limpo | `cargo build --release -p neural-kernel --target x86_64-unknown-none` = 0 erros |
| Testes verdes | `cargo test --workspace --exclude neural-kernel --exclude boot` = 118+ PASS |
| Falcon3 detectado | Log do boot mostra "MODEL: hub slot=active loaded" com Falcon3-3B |
| GGUF aceito | Arquivo .gguf no FAT é carregado automaticamente |
| Trinity MoE | Log mostra "TRINITY: MoE router (R3): expert X" em vez de keyword fallback |
| Zero dead code crash | Nenhum `_mm_*` intrinsic em módulo compilado com soft-float |

---

## Riscos

| Risco | Mitigação |
|-------|-----------|
| Remover neural-sgdb quebra bridge | Manter stubs em k_ai/sgdb/* com `cfg(feature = "nsgdb")` |
| GGUF→TransformerModel conversion incorreta | Teste host com GGUF sintético |
| from_name aliases conflitam | Teste unitário que verifica todos os aliases |
| classify_keywords i18n incompleto | Manter PT-BR como primário, EN/ES como secundário |
