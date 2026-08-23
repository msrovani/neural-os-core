# Análise Profunda: k_ai + cortex — Estado, Gaps e Correções

**Data:** 2026-08-23  
**Escopo:** crates/k_ai (42 arquivos, ~8.000 LOC) + crates/cortex (35 arquivos, ~12.000 LOC)  
**Objetivo:** Mapear todo o estado real, identificar correções/otimizações, alinhar com o hub de modelos e Trinity MoE

---

## 1. Mapa de Responsabilidades

### k_ai (Ring 2 — Infraestrutura Cognitiva)
| Módulo | Função | Estado |
|--------|--------|--------|
| `self_heal.rs` | SelfHeal v2: checkpoint, rollback, P09 CoW CR3, blacklist WASM | ✅ Completo |
| `cognitive.rs` | Planner, SuccessEngine, Cache, ReplayBuffer, TernaryUpdate, WeightConsolidation, AutoSkillGenerator, DynamicScaling, Scheduler, Workflow, CodebookVQ, KVCache, ReAct, MCP, Finetune, DeltaBranches, WorkspaceIsolation, EpisodicMemory, BitNetTrainer, CandleSidecar, TaskSpawner, SleepCycleGuardRails | ⚠️ 1616 LOC — maioria dead code |
| `arch/x86_64.rs` | SIMD kernels SSE4.2/AVX2/AVX512 para ternário | ❌ CRASHES rustc codegen |
| `arch/simd.rs` | Dispatch estático ISA → kernel function pointer | ✅ OK (usa cortex kernels) |
| `memory_systems.rs` | BGE embedding (384d), SleepCycle, Tier memory | ✅ Funcional |
| `trust.rs` | Trust cache, tokens, verificação | ✅ OK |
| `agency.rs` | Agency specs, agent fleet | ✅ OK |
| `expert_lifecycle.rs` | Registro/prune/merge de experts | ✅ OK |
| `safety_invariants.rs` | 4 invariantes I1-I4 | ✅ OK |
| `security_detectors.rs` | 5 detectores de segurança | ✅ OK |
| `sgdb/*` | TickvStorageAdapter + NSGDB bridge | ✅ Funcional |

### cortex (Ring 2 — Motor de Inferência Neural)
| Módulo | Função | Estado |
|--------|--------|--------|
| `cortex.rs` | TransformerModel, LLM forward, tokenização, RoPE, KV-cache | ✅ Completo |
| `model_hub.rs` | ModelHub multi-slot: 8 slots, register_bytes, select_generator | ⚠️ Gaps no naming |
| `model.rs` | ModelHeader v6, load_model_v6, parse_model_header | ✅ Autônomo |
| `trinity.rs` | TrinityRouter, 7 experts, MoE neural+keyword, Efeito Matrix | ✅ Estrutura OK |
| `compute.rs` | Dispatch NPU→GPU→SMP→AMX→AVX512→AVX2→Scalar | ✅ Completo |
| `gguf.rs` | Loader GGUF Q4_0/Q4_K/Q6_K/Q2_K/Q3_K/Q5_K | ✅ Completo |
| `tensor.rs` | Tensor, PackedTernaryTensor, matmul AVX2 | ✅ OK |
| `nn.rs` | Linear, BitLinear, silu, relu2, rms_norm | ✅ OK |
| `moe.rs` | Int8Router, MoELayer, forward/forward_sequence | ✅ OK |
| `speculative.rs` | DSD: draft/verify/rejection, mesh distribuido | ⚠️ Local only |
| `structured_decode.rs` | FSM JSON/Shell/Skill, argmax_constrained | ✅ OK |
| `arena.rs` | TensorArena bump allocator (Tier 2, 2GB default) | ✅ OK |
| `global_arena.rs` | Arena global singleton, R3 traces, pending route | ✅ OK |
| `bitnet_avx2.rs` | Kernel ternário AVX2 matmul | ✅ OK |
| `bitnet_avx512.rs` | Kernel ternário AVX-512 matmul | ✅ OK |
| `amx_int8.rs` | Kernel AMX int8 (tdpbssd) | ✅ OK |
| `parallel_matmul.rs` | SMP parallel matmul | ✅ OK |

---

## 2. 🚨 Problemas Críticos

### 2.1 CRASH: `cargo build --release` — SSE intrinsics em k_ai

**Causa raiz:** `k_ai/src/arch/x86_64.rs` (596 LOC) contém intrinsics SSE4.2/AVX2/AVX-512 (`_mm_*`, `_mm256_*`, `_mm512_*`) que crasham o `rustc` durante codegen quando o target tem `-C target-feature=-sse,-sse2,...`.

**Sintoma:** `cargo build --release -p neural-kernel --target x86_64-unknown-none` → `STATUS_ILLEGAL_INSTRUCTION (0xc000001d)` no LLVM.

**Por que `cargo check` passa:** check não gera código nativo — só valida tipos e lifetimes.

**Impacto:** O binário do kernel NUNCA foi built com `cargo build --release`. O `cargo check --release` mascarava o problema.

**Fix necessário:** Gatear `k_ai::arch` com `#[cfg(feature = "simd")]` ou `#[cfg(target_feature = "sse4.2")]` em runtime, compilando o módulo apenas quando SSE estiver disponível. O `k_ai/src/arch/simd.rs` já tem dispatch correto — o `x86_64.rs` é redundante (cortex já tem `bitnet_avx2.rs`, `bitnet_avx512.rs`).

**Recomendação:** **Remover `arch/x86_64.rs`** — é dead code. O dispatch real já vive em `cortex::compute::dispatch_ternary`. O `k_ai::arch::simd.rs` pode ser mantido como interface de alta nível mas apontando para os kernels do cortex.

### 2.2 neural-sgdb: SSE2 intrinsics em crate dependente

**Causa:** `neural-sgdb` (crate externo) contém intrinsics SSE2 que crasham rustc com soft-float.

**Impacto:** Mesmo após remover o módulo `nsgdb_bridge.rs`, a dependência em `k_ai/Cargo.toml` ainda é puxada.

**Fix:** Remover `neural-sgdb` de `k_ai/Cargo.toml` completamente (ou gatear atrás de feature `nsgdb`).

### 2.3 neural-kernel bin: `lazy_static! { TRINITY }` shadow

**Problema:** O bin `neural-kernel/src/main.rs:649` cria `static ref TRINITY: TicketLock<TrinityRouter> = ...` — mas o `hermes` e `k_ai` não têm acesso a ele. O globals do hermes é vazio (como documentado em AGENTS.md lição SESSION_217).

**Estado atual:** O seam `trinity_inject.rs` no hermes foi criado para resolver isso, mas o fluxo real ainda depende do `TRINITY` do bin.

---

## 3. 🟡 Gaps no Hub de Modelos (LLM auto-seleção)

### 3.1 `ModelSlot::from_name()` usa nomes legados

```rust
// cortex/src/model_hub.rs:38-53
"generator_pro" | "pro" | "3b" | "bitnet3b" => Some(Self::GeneratorPro),
// ^^^ "bitnet3b" é BitNet legado, não Falcon3-3B
// O falcon3-3B deve mapear para Active, não GeneratorPro
```

**Fix:** Adicionar aliases Falcon3:
```rust
"falcon3" | "falcon3b" | "falcon3-3b" | "tiiuae" => Some(Self::Active),
"falcon3-7b" | "falcon7b" | "pro" => Some(Self::GeneratorPro),
```

### 3.2 `slot_from_bitnet_bytes()` — thresholds podem errar

```rust
if len < 1100 * MB {
    // Falcon3-3B .BIN legado (~771MB) → Agent
    ModelSlot::Agent  // ← ERRADO para Falcon3-3B-Instruct
}
```

Falcon3-3B-Instruct-1.58bit.bin ≈ 771MB cai em `Agent` em vez de `Active`.

**Fix:** Usar o header v6 (magic `0xBE11BE11`) para decidir o slot, não só o tamanho:
```rust
pub fn slot_from_bitnet_bytes(data: &[u8]) -> ModelSlot {
    // Tenta parse do header v6 primeiro
    if let Some(h) = crate::model::parse_model_header(data) {
        return match h.estimated_params() {
            0..=100_000_000 => ModelSlot::Reranker,      // <100M params
            100_000_000..=600_000_000 => ModelSlot::Learner, // ~0.5B
            600_000_000..=4_000_000_000 => ModelSlot::Active, // ~3B (Falcon3-3B)
            4_000_000_000..=12_000_000_000 => ModelSlot::GeneratorPro, // ~7B
            _ => ModelSlot::GeneratorPro,
        };
    }
    // Fallback: tamanho bruto (legado)
    // ...existing code...
}
```

### 3.3 `fat_names_for(Active)` — falta FALCON3B.v6

```rust
ModelSlot::Active => &[
    "FALCON3B.BIN",   // ← legado .BIN
    "FALCN3B.GGUF",
    "BITNET2B.v6",    // ← legado BitNet
    // Falta: "FALCON3B.v6" como preferido!
],
```

**Fix:** Colocar `.v6` primeiro:
```rust
ModelSlot::Active => &[
    "FALCON3B.v6",    // Falcon3-3B-Instruct-1.58bit v6 (canonical)
    "FALCON3B.BIN",   // fallback legado
    "FALCN3B.GGUF",
    "BITNET2B.v6",
    // ...
],
```

### 3.4 `select_generator_slot()` — não prioriza Falcon3

A função decide qual slot usar para gerar texto. Para prompts complexos, ela tenta `GeneratorPro` primeiro. Mas se o Falcon3-3B está no `Active` e o `GeneratorPro` não tem blob, cai no `Active` — que é o comportamento correto. Porém, falta a lógica de detectar se o `Active` é um modelo maior que o `GeneratorPro` (ex: Falcon3-7B no Active = usar Active em vez de GeneratorPro vazio).

### 3.5 `is_complex_conversation()` — heuristicamente limitado

```rust
pub fn is_complex_conversation(prompt: &str) -> bool {
    if prompt.len() > 160 { return true; }
    contains_ci(prompt, "detalhad") || contains_ci(prompt, "analis") || ...
}
```

**Problema:** Só usa heurísticas de comprimento e palavras-chave. Um AIOS real deveria usar o **router MoE** para decidir a complexidade, não palavras-chave.

---

## 4. 🟡 Gaps na Trinity MoE

### 4.1 Router MoE: apenas fallback determinístico

O `init_router_weights()` tenta carregar de arquivo; se falhar, gera pesos aleatórios (LCG seed=42). Na prática, **nenhum arquivo de router está na imagem FAT** — o router SEMPRE usa o fallback deterministic.

**Fix:** Gerar e incluir um `ROUTER.BITNET` treinado na imagem FAT (via `tools/train_router.py`).

### 4.2 `expert_weight_source()` — incompleto

```rust
pub fn expert_weight_source(kind: ExpertKind) -> Option<&'static str> {
    match kind {
        ExpertKind::HwIdentify => Some("HWEXPRT.V6"),
        ExpertKind::RustCoder => Some("RUSTCDR.BITNET"),
        _ => None,  // ← Generator, Security, DiskDiag, SpeechSynth sem fonte
    }
}
```

**Fix:** Adicionar fontes para experts que têm modelos treinados:
```rust
ExpertKind::Generator => Some("PRO.v6"),  // Falcon3-7B como expert generator
ExpertKind::Security => Some("SECURITY.v6"),  // se treinado
```

### 4.3 `classify_keywords()` — hardcoded PT-BR

O roteamento por keyword é funcional mas limitado. O routing deveria:
1. Usar o router MoE neural (quando treinado)
2. Fallback para keyword (hoje)
3. **Nunca** depender de palavras-chave hardcoded em um idioma específico

### 4.4 7 experts registrados mas sem pesos

Todos os 7 experts de `init_trinity()` têm `weight: None`. O Efeito Matrix (`get_or_mmap_expert`) só funciona para `HwIdentify` e `RustCoder` (os únicos com `expert_weight_source`). Os demais experts rodam sem pesos — o classify sempre cai no keyword fallback.

---

## 5. 🟡 Gaps no Compute Dispatch

### 5.1 `dispatch_ternary()` — GPU/NPU nunca registrados

```rust
static GPU_TERNARY: AtomicUsize = AtomicUsize::new(0);  // sempre 0
static NPU_TERNARY: AtomicUsize = AtomicUsize::new(0);  // sempre 0
```

Em QEMU sem GPU/NPU real, o dispatch sempre cai no caminho CPU. Isso é **correto** para QEMU, mas falta o registration path para HW real.

### 5.2 `parallel_matmul` gated por `ap_pollable`

```rust
if big && k_nano::platform_probe::allow_smp()
    && k_nano::smp::ap_pollable()  // ← sempre false!
    && k_nano::smp::ap_entry_count() > 0
```

APs nunca são workers porque `AP_POLLABLE` default é OFF. O SMP paralelo para matmul nunca executa.

---

## 6. 🟡 Gaps no GGUF Loader

### 6.1 GGUF não é integrado no model_hub

O `gguf.rs` tem loader completo (Q4_K, Q6_K, Q2_K, Q3_K, Q5_K) mas o `model_hub::register_bytes()` só aceita formato v6 (`.bitnet`). Um arquivo `.gguf` no FAT não é carregado automaticamente.

**Fix:** Adicionar path GGUF em `register_bytes` ou em `try_hub_slot_fat`:
```rust
pub fn register_bytes(slot: ModelSlot, data: &[u8]) -> bool {
    // Tenta v6 primeiro
    if let Some(v) = crate::model::load_model_v6(data) { ... }
    // Fallback: GGUF
    if crate::gguf::is_gguf(data) {
        if let Some(m) = crate::gguf::load_gguf_as_transformer(data) {
            // ...registra no slot
        }
    }
}
```

### 6.2 `fat_lookup_size()` e `read_fat_into()` — dependem de globals ATA

O loader GGUF FAT depende do ATA driver global. Em host/teste, retorna None. Isso é correto (gate `#[cfg(target_os = "none")]`), mas significa que testes host não exercem o path real.

---

## 7. 🔧 Correções Recomendadas (por prioridade)

### P0 — Build Breaker (IMEDIATO)
| # | Fix | Arquivos | Esforço |
|---|-----|----------|---------|
| P0-1 | **Remover `arch/x86_64.rs`** (dead code SSE que crasha rustc) | `k_ai/src/arch/x86_64.rs`, `k_ai/src/arch/mod.rs` | 10 min |
| P0-2 | **Remover `neural-sgdb` de `k_ai/Cargo.toml`** (SSE2 crash) | `k_ai/Cargo.toml` | 5 min |
| P0-3 | **Validar `cargo build --release`** passa após P0-1+P0-2 | — | 15 min |

### P1 — Hub de Modelos (AUTO-SELEÇÃO)
| # | Fix | Arquivos | Esforço |
|---|-----|----------|---------|
| P1-1 | **`from_name()`: aliases Falcon3** (`falcon3`, `falcon3b`, `tiiuae`) | `cortex/src/model_hub.rs` | 15 min |
| P1-2 | **`fat_names_for(Active)`: FALCON3B.v6 primeiro** | `cortex/src/model_hub.rs` | 5 min |
| P1-3 | **`slot_from_bitnet_bytes()`: usar header v6** em vez de tamanho bruto | `cortex/src/model_hub.rs` | 30 min |
| P1-4 | **`fat_names_for(GeneratorPro)`: PRO.v6 primeiro**, remover BITNET legado | `cortex/src/model_hub.rs` | 10 min |
| P1-5 | **`register_bytes()`: path GGUF** para .gguf files | `cortex/src/model_hub.rs`, `cortex/src/gguf.rs` | 45 min |

### P2 — Trinity MoE
| # | Fix | Arquivos | Esforço |
|---|-----|----------|---------|
| P2-1 | **`expert_weight_source()`: adicionar Generator/Security** | `cortex/src/trinity.rs` | 10 min |
| P2-2 | **Gerar `ROUTER.BITNET`** treinado e incluir na FAT | `tools/train_router.py` + FAT | 2h |
| P2-3 | **`classify_keywords()`: tornar idiom-agnostic** (inglês+português+espanhol) | `cortex/src/trinity.rs` | 30 min |

### P3 — k_ai Limpeza
| # | Fix | Arquivos | Esforço |
|---|-----|----------|---------|
| P3-1 | **`cognitive.rs`: marcar módulos dead code** com `#[allow(dead_code)]` explícito ou remover se sem callers | `k_ai/src/cognitive.rs` | 1h |
| P3-2 | **`arch/simd.rs`: simplificar** para apontar direto para `cortex::compute::dispatch_ternary` | `k_ai/src/arch/simd.rs` | 30 min |

### P4 — Integração LLM (AUTO-AI)
| # | Fix | Arquivos | Esforço |
|---|-----|----------|---------|
| P4-1 | **`is_complex_conversation()`: usar router MoE** em vez de keywords | `cortex/src/model_hub.rs` | 30 min |
| P4-2 | **`select_generator_slot()`: detecção automática** por ModelHeader (params/size) | `cortex/src/model_hub.rs` | 30 min |
| P4-3 | **Provisioning automático**: Falcon3-3B como default, Falcon3-7B como pro | `neural-kernel/src/model_provisioner.rs` | 1h |

---

## 8. Como um AIOS deve Ser: Auto-Seleção de LLM

O fluxo ideal (e o que falta):

```
Boot → FAT scan → para cada arquivo:
  1. parse_model_header() → extrai hidden/layers/vocab/params
  2. slot_from_bitnet_bytes() → decide slot por header (não tamanho)
  3. register_bytes() → carrega no ModelHub
  4. select_generator_slot(prompt) → router MoE decide:
     - Simple greeting → Active (Falcon3-3B, rápido)
     - Complex analysis → GeneratorPro (Falcon3-7B, potente)
     - Code generation → RustCoder (se carregado)
     - HW identification → HwExpert (se carregado)
```

**Estado atual vs ideal:**
| Passo | Atual | Ideal |
|-------|-------|-------|
| Header parse | ✅ `parse_model_header` | ✅ OK |
| Slot assignment | ❌ Por tamanho bruto | ✅ Por header params |
| FAT names | ⚠️ Legado primeiro | ✅ .v6 primeiro |
| GGUF support | ⚠️ Loader existe mas não integrado | ✅ Auto-detect |
| Router MoE | ⚠️ Fallback determinístico | ✅ Treinado |
| Auto-select | ⚠️ Keywords hardcoded | ✅ MoE neural |
| Provisioning | ⚠️ Hardcoded ORDER | ✅ Auto-detect por header |

---

## 9. Resumo Executivo

### O que está PODEROSO (manter):
- **Compute dispatch** NPU→GPU→SMP→AMX→AVX512→AVX2→Scalar: arquitetura perfeita
- **ModelHub multi-slot**: 8 slots, register_bytes v6, auto-fit
- **Trinity MoE**: 7 experts, router neural + keyword, Efeito Matrix (get_or_mmap_expert)
- **GGUF loader**: Q4_K/Q6_K/Q2_K completo
- **SelfHeal v2**: checkpoint CoW, rollback P09, blacklist
- **TensorArena 2GB**: bump allocator zero-fragmentação

### O que PRECISA de fix (correção):
- **P0**: `arch/x86_64.rs` crasha rustc → remover (dead code)
- **P0**: `neural-sgdb` crasha rustc → remover dep
- **P1**: Hub não detecta Falcon3 automaticamente → fix naming/headers
- **P1**: GGUF não integrado no hub → adicionar path
- **P2**: Router MoE sempre determinístico → treinar e incluir

### O que é OPTIMIZAÇÃO (futuro):
- **P3**: cognitive.rs tem 1616 LOC de dead code
- **P4**: is_complex_conversation deve usar MoE, não keywords
