# ADR-0043: Análise Tecnológica CubeCL — Padrões Aproveitáveis para o AIOS K³CHJ

**Status:** Finalizado — Análise Completa (código + book + exemplos)  
**Data:** 2026-07-15  
**Autores:** AI Agent (OpenCode)  
**Contexto:** Análise de viabilidade e extração de padrões do projeto [tracel-ai/cubecl](https://github.com/tracel-ai/cubecl) v0.10.0 (1.468 commits, 408 arquivos Rust, ~2.9MB). Código fonte clonado localmente, book lido, 4 exemplos analisados. **Nenhum código foi implementado — apenas documentação dos padrões.**

---

## 1. Escopo

Esta ADR documenta os resultados da análise aprofundada (~130 arquivos fonte lidos) do código do CubeCL, extraindo **18 padrões tecnológicos concretos** que podem ser adotados, adaptados ou inspirados no neural-os-core. Cada padrão inclui: descrição, localização no CubeCL, mapeamento para nossa stack, e prioridade de adoção.

---

## 2. Arquitetura Geral do CubeCL

```
┌─────────────────────────────────────────────────────────────────┐
│                      cubecl-macros (proc-macro)                 │
│  #[cube], #[cube_launch], CubeType derive, autotune macros      │
│  43 files, 338 KB                                              │
├─────────────────────────────────────────────────────────────────┤
│                      cubecl-core (frontend + IR)                │
│  Frontend: CubeType, Vector, Tensor, Comptime, branch, plane    │
│  Codegen: KernelBuilder, KernelIntegrator, settings              │
│  Post-processing: const_prop, unroll, disaggregate, predication  │
│  137 files, 1,006 KB                                           │
├─────────────────────────────────────────────────────────────────┤
│   cubecl-ir (IR types)     │   cubecl-opt (optimizer)           │
│   Operation, Scope, Value, │   CFG + SSA + GVN + Liveness       │
│   Type, AddressSpace       │   Dominance + Uniformity + PRE     │
│   30 files, 171 KB         │   30 files, 211 KB                 │
├─────────────────────────────────────────────────────────────────┤
│                      cubecl-runtime                              │
│  Client/Server, MemoryManagement (3 pool types), Stream/Scheduler│
│  Autotune (Tuner + TuneCache), MetadataCache (LRU), Graph Capture│
│  Throughput benchmarking, TimestampProfiler                       │
│  67 files, 512 KB                                               │
├───────────────────┬───────────────────┬──────────────────────────┤
│   cubecl-cpu      │   cubecl-cuda     │   cubecl-wgpu            │
│   LLVM/MLIR JIT   │   cudarc NVRTC    │   wgpu Vulkan/Metal/DX   │
│   Threadpool SIMD │   CUDA driver     │   SPIR-V / WGSL          │
│   48 files 237KB  │                   │                          │
├───────────────────┴───────────────────┴──────────────────────────┤
│                      cubecl-common (foundation)                   │
│  Arena, Cache, Bytes, DeviceHandle, Float Types (FP4/6/8), Quant  │
│  45 files, 388 KB                                               │
└─────────────────────────────────────────────────────────────────┘
```

### Crates de Suporte

| Crate | Função | Tamanho |
|-------|--------|---------|
| `cubecl-macros-internal` | Proc-macro `#[cube_impl]` para geração de IR ops | 27 KB |
| `cubecl-hip` | Backend AMD ROCm | — |
| `cubecl-metal` | Backend Apple Metal | — |
| `cubecl-spirv` | Backend SPIR-V (Vulkan via wgpu-hal) | — |
| `cubecl-zspace` | Shape/Strides computation | — |
| `cubecl-cpp` | Backend C++ codegen | — |
| `cubecl-std` | Shared test infrastructure | — |

---

## 3. Padrões Tecnológicos Identificados

### P-01: Sistema de 4 Eixos de Paralelismo (A JOIA)

**Localização:** `cubecl-ir/src/variable.rs:73-105` (enum `Builtin` com 24 variantes de topology)

**O que é:** CubeCL modela hardware como 4 eixos ortogonais lidos em comptime:

| Eixo | Significado | GPU (CUDA) | GPU (AMD) | CPU |
|------|------------|------------|-----------|-----|
| **Vector** | SIMD width (128b-512b) | 1-2 (f32x2) | 1-2 | 8 (AVX2) |
| **Plane** | Warp/Subgroup (lockstep) | 32 | 64 | 1 |
| **CubeDim** | Block/Workgroup | 256 | 256 | 1 |
| **CubeCount** | Grid | N | N | 1 (seq) |

Cada kernel lê esses valores via `CUBE_DIM_X`, `PLANE_DIM`, `UNIT_POS`, `ABSOLUTE_POS` — tudo como constantes IR, sem branching runtime.

**Código real:**
```rust
// cubecl-ir/src/variable.rs
pub enum Builtin {
    UnitPos, UnitPosX, UnitPosY, UnitPosZ,
    CubePos, CubePosX, CubePosY, CubePosZ,
    CubeDim, CubeDimX, CubeDimY, CubeDimZ,
    CubeCount, CubeCountX, CubeCountY, CubeCountZ,
    PlaneDim, PlanePos, UnitPosPlane,
    AbsolutePos, AbsolutePosX, AbsolutePosY, AbsolutePosZ,
}
```

**Nosso problema:** `jarvis/src/gpu/nvidia.rs` hardcode `warpSize=32` (quebra em AMD). `jarvis/src/gpu/intel.rs` assume SIMD=8 fixo. `backend.rs` usa if/else vendor.

**Adoção:** Refatorar `GpuAccel` para expor `PlaneDim`, `CubeDim`, `VectorWidth` como propriedades lidas em compile-time do kernel. O matmul adapta-se ao backend sem if/else.

**Arquivos-alvo:** `gpu/backend.rs`, `gpu/intel.rs`, `gpu/nvidia.rs`

---

### P-02: Comptime — Especialização sem Custo Runtime

**Localização:** `cubecl-core/src/frontend/comptime.rs` (18 linhas), `cubecl-core/src/frontend/comptime_option.rs` (1.428 linhas — implementação completa de `Option<T>` comptime)

**O que é:** O kernel pode ler `PLANE_DIM`, `CUBE_DIM`, vector width **em tempo de compilação do kernel** (não runtime). Blocos `comptime!` executam Rust normal durante a expansão do IR e os resultados são baked como constantes. `comptime_type!` wrappers permitem tipos que só existem em comptime (ex: `Rc<DeviceProperties>` no IR).

**Código real:**
```rust
// cubecl-core/src/frontend/comptime.rs
#[cube]
pub fn device_properties() -> comptime_type!(Rc<DeviceProperties>) {
    intrinsic!(|scope| scope.state().device_properties.as_ref().unwrap().clone())
}

#[cube]
pub fn hardware_properties() -> comptime_type!(HardwareProperties) {
    let props = &device_properties().comptime().hardware;
    comptime!(props.clone())  // baked as constant no IR
}
```

**O `ComptimeOption<T>`** (1.428 linhas) implementa `Option<T>` completo (is_some, unwrap, map, and_then, ok_or, etc.) que **só existe em comptime** — resolvido durante a construção do IR, zero custo no kernel final.

**Nosso cenário:** `cortex/src/bitnet_avx2.rs` tem `if has_avx2() { avx2_path } else { scalar }` — branching runtime em toda chamada. Com comptime, isso vira decisão na geração do kernel.

**Adoção:** Macro `specialize!` que gera N variantes do kernel em build time:
```rust
// Pseudocódigo do padrão que podemos adotar
specialize!(kernel = matmul_ternary,
    variants: [
        (vector=8, plane=1),   // CPU AVX2
        (vector=4, plane=1),   // CPU SSE
        (vector=1, plane=32),  // NVIDIA warp
        (vector=1, plane=64),  // AMD wave
    ]
);
```
Cada variante é pré-compilada. Dispatch é tabela de ponteiros, não if/else.

**Arquivos-alvo:** `cortex/src/bitnet_avx2.rs`, `cortex/src/tensor.rs`

---

### P-03: Autotune com Cache Persistente (Tuner + TuneCache)

**Localização:** `cubecl-runtime/src/tune/tuner.rs` (464 linhas), `cubecl-runtime/src/tune/tune_cache.rs`, `cubecl-runtime/src/tune/base.rs`

**O que é:** Sistema completo de autotuning que:
1. Define `TunableSet<K, F, Out>` — conjunto de kernels que resolvem o mesmo problema
2. `Tuner<K>::check_tune()` — busca cache, se miss → benchmarka cada variante em hardware real
3. Benchmark executa N amostras com `ProfileDuration` → `BenchmarkComputations` (median)
4. Resultado cacheado por `TuneCache<K>` — serializado em disco via `ciborium`/`serde_json`
5. `TuneCacheResult::Hit { fastest_index }` — dispatch O(1) nas chamadas seguintes

**Pipeline de tuning:**
```
check_tune(key)
  → cache lookup (hashmap + disk)
  → if miss: generate_inputs(key) → plan.next() → tune_benchmark() → resolve_bench() → cache_insert()
  → TuneCacheResult::Hit { fastest_index }
```

**Nosso cenário:** `gpu/bench.rs` roda benchmark mas não cacheia. Paga warmup toda inicialização.

**Adoção:** Implementar `AutotuneCache` serializado no NeuralFS. Na segunda inicialização, kernel já escolhido:
```rust
// Padrão adaptado
pub struct AutotuneCache<K: Hash + Serialize> {
    cache: HashMap<K, AutotuneResult>,
    disk_path: Option<&'static str>,  // "system://autotune.cache"
}
impl<K: Hash + Serialize> AutotuneCache<K> {
    fn fastest(&self, key: &K) -> Option<usize>;
    fn cache_insert(&mut self, key: K, fastest_index: usize);
    fn save(&self, fs: &mut NeuralFS);  // persistência entre boots
}
```

**Arquivos-alvo:** `gpu/bench.rs`, `neural_fs/`

---

### P-04: Memory Management com 3 Pool Types

**Localização:** `cubecl-runtime/src/memory_management/memory_manage.rs` (2.204 linhas), pools em `memory_pool/`

**O que é:** Três estratégias de alocação de memória GPU, escolhidas por tamanho da alocação:

| Pool | Tipo | Uso | Estratégia |
|------|------|-----|-----------|
| **SlicedPool** | Page-based buddy | Alocações pequenas/médias (< page_size) | Divide páginas em slices, first-fit com coalescência |
| **ExclusivePool** | 1:1 alloc | Alocações grandes (≥ page_size) | Cada alloc vira uma página exclusiva |
| **PersistentPool** | Never-free | Dados permanentes (pesos de modelo) | Aloca e nunca libera. Ideal para model weights |

**Código real (SlicedPool):**
```rust
// cubecl-runtime/src/memory_management/memory_pool/sliced_pool.rs
pub struct SlicedPool {
    pages: Vec<(MemoryPage, StorageId)>,
    page_size: u64,
    alignment: u64,
    max_alloc_size: u64,
    max_pages: Option<u16>,  // None = unbounded
}
```

**Nosso cenário:** `gpu/vram.rs` tem buddy allocator single-strategy. Pesos de modelo (BitNet 2B = 590MB) usam mesma estratégia que buffers temporários.

**Adoção:** Adicionar `PersistentPool` ao VRAM allocator. Model weights alocados como persistent, nunca fragmentam o heap. `SlicedPool` para ativações temporárias.

**Arquivos-alvo:** `gpu/vram.rs`

---

### P-05: Compute Graph Capture/Replay

**Localização:** `cubecl-runtime/src/client.rs:39-148` (structs `Graph<R>`, `GraphHandle<R>`)

**O que é:** Sistema de captura de grafo de computação: `client.start_capture()` → executa N kernels → `client.stop_capture()` → devolve um `Graph<R>` que pode ser reproduzido via `graph.replay()`. Útil para:
- Decode loops de LLM (mesma sequência de kernels, buffers diferentes)
- Warmup: popula caches na capture window, replay só executa
- Zero-overhead de scheduling em loops inferencia

**Pipeline de captura:**
```rust
let capture = client.start_capture();
for _ in 0..warmup { decode_step(&mut cache); }
let graph = client.stop_capture(capture);
// graph.replay() agora executa decode_step sem passar pelo scheduler
```

**Paralelo neural:** Nosso loop de inferência em `cortex/src/cortex.rs` executa a mesma sequência de matmul/attention para cada token. Poderíamos capturar o grafo e replayed-lo.

**Adoção:** Implementar `InferenceGraph` que grava a sequência de operações `matmul_hybrid` → `rms_norm` → `attention` → `ffn` → `matmul_hybrid` e replaya sem scheduler overhead. Especialmente útil para Medusa speculative decoding (3 heads paralelas).

**Arquivos-alvo:** `cortex/src/cortex.rs`, `cortex/src/parallel_matmul.rs`

---

### P-06: Metadata Cache LRU com Graph Pinning

**Localização:** `cubecl-runtime/src/metadata_cache.rs` (503 linhas)

**O que é:** Cache de buffers de metadata (shapes, strides, scalars) keyed por conteúdo. Duas inovações:

1. **Cache cross-kernel:** Mesma chave (`InfoCacheKey = Vec<u64>`) serve para kernels diferentes. Metadata é determinada só pelos shapes, não pelo kernel — então dois kernels com mesmas dimensões compartilham buffer.
2. **Graph pinning:** Durante captura de grafo, entradas são pinadas (refcount). Nunca evaporadas enquanto o grafo existir. Garante que `replay()` encontre todos os buffers.

**Código real:**
```rust
pub enum CacheMode {
    Normal,   // LRU, evict quando cheio
    Capture,  // Cache tudo, nunca evict
}
pub struct MetadataInfoCache {
    entries: HashMap<InfoCacheKey, Handle>,
    lru: VecDeque<InfoCacheKey>,
    pins: HashMap<GraphId, HashSet<InfoCacheKey>>,  // refcounted
}
```

**Adoção:** Implementar `ShapeCache` para nosso inference engine. Tensores de atenção (Q, K, V, O) têm shapes estáveis — cachear metadata evita recomputação de strides/offsets a cada token.

**Arquivos-alvo:** `cortex/src/tensor.rs`

---

### P-07: Multi-Stream Scheduler com Interleave/Sequential

**Localização:** `cubecl-runtime/src/stream/scheduler.rs` (283 linhas), `cubecl-runtime/src/stream/base.rs` (127 linhas)

**O que é:** Pool de streams com duas estratégias de scheduling:

| Estratégia | Comportamento | Uso |
|-----------|--------------|-----|
| **Sequential** | Stream A tasks → flush → Stream B tasks | Dependências entre streams |
| **Interleave** | A1, B1, A2, B2, ... no mesmo queue | Throughput máximo, GPU ocupada |

**Stream alignment:** Quando dois streams compartilham um binding, o scheduler flusha automaticamente o stream antigo antes de registrar no novo — garantindo consistência sem deadlock.

```rust
pub fn register(&mut self, stream_id: StreamId, task: B::Task, args_streams: &[StreamId]) {
    self.align_streams(stream_id, args_streams);
    // ...
}
```

**Adoção:** Implementar `ComputeStream` para nosso pipeline CPU/GPU. Prefill (CPU) em stream 1, decode (GPU) em stream 2, interleave para maximizar utilização. Especialmente útil quando tivermos GPU compute funcional.

**Arquivos-alvo:** `gpu/xpu.rs`, `cfs.rs`

---

### P-08: Sistema de Tipos com FP4/FP6/FP8/BF16/Flex32/TF32

**Localização:** `cubecl-ir/src/type.rs:17-38` (enum `FloatKind`), `cubecl-common/src/float/`

**O que é:** Suporte nativo a 11+ tipos de ponto flutuante, incluindo formatos experimentais:

| Tipo | Bits | Expoente | Mantissa | Uso típico |
|------|------|----------|----------|-----------|
| E2M1 | 4 | 2 | 1 | Ultra-low precision (nosso BitNet ternário) |
| E2M3 | 6 | 2 | 3 | FP6 experimental |
| E3M2 | 6 | 3 | 2 | FP6 alternativo |
| E4M3 | 8 | 4 | 3 | FP8 (H100) |
| E5M2 | 8 | 5 | 2 | FP8 (dinâmico) |
| UE8M0 | 8 | 8 | 0 | Unsigned 8-bit exponent |
| F16 | 16 | 5 | 10 | Half precision |
| BF16 | 16 | 8 | 7 | Brain float |
| Flex32 | 32 | — | — | NVIDIA FlexPoint |
| F32 | 32 | 8 | 23 | Standard float |
| TF32 | 32 | 8 | 10 | NVIDIA TensorFloat-32 |

Cada tipo tem `size()`, `size_bits()`, `epsilon()`, `max_variable()`, `min_variable()`, `from_f64()`, `to_f64()`.

**Código real:**
```rust
impl ElemType {
    pub const fn size(&self) -> usize {
        match self {
            ElemType::Float(kind) => match kind {
                FloatKind::E2M1 | FloatKind::E2M3 | FloatKind::E3M2
                | FloatKind::E4M3 | FloatKind::E5M2 | FloatKind::UE8M0 => 1, // 1 byte
                FloatKind::F16 => 2,
                FloatKind::BF16 => 2,
                FloatKind::F32 | FloatKind::Flex32 | FloatKind::TF32 => 4,
                FloatKind::F64 => 8,
            },
            // ...
        }
    }
}
```

**Nosso cenário:** Temos `PackedTernaryTensor` (2-bit packing, 4 pesos/byte) em `cortex/src/tensor.rs`. CubeCL tem FP4 E2M1 (4-bit) e packing `Packed(ElemType, factor)`.

**Adoção:** Adicionar `FloatKind::E2M1` (4-bit) como tipo nativo ao lado do nosso ternário 2-bit. O `StorageType::Packed(ElemType, usize)` permite packing genérico (ex: `Packed(F32, 4)` = 128-bit SIMD). Isso daria um sistema de tipos unificado para todos os níveis de precisão.

**Arquivos-alvo:** `cortex/src/tensor.rs`

---

### P-09: IR com 30+ Operações em 11 Categorias

**Localização:** `cubecl-ir/src/operation.rs` (enum `Operation` com 20 variantes), módulos: `arithmetic`, `bitwise`, `memory`, `branch`, `synchronization`, `atomic`, `plane`, `cmma`, `barrier`, `tma`, `tensor_indexing`

**O que é:** IR completo de GPU shader com operações de alto e baixo nível:

| Categoria | Exemplos |
|-----------|---------|
| `Arithmetic` | Add, Sub, Mul, Div, ModFloor, Rem, Abs, Exp, Log, Sqrt, Fma |
| `Memory` | Load, Store, Index, Init, Copy, AllocShared |
| `Branch` | If, Else, Break, Continue, Loop, Return, Switch |
| `Synchronization` | SyncCube, SyncPlane, SyncMemory |
| `Atomic` | Add, Sub, Max, Min, And, Or, Xor, Exchange, CompareExchange |
| `Plane` | Sum, Product, Min, Max, Broadcast, ShuffleUp, ShuffleDown, ShuffleXor |
| `CoopMma` | Load, Execute, Store (matrix multiply-accumulate cooperativo) |
| `Tma` | Async copy, descriptor load (Tensor Memory Accelerator) |
| `Barrier` | Init, Arrive, Wait |
| `Operator` | Cast, Reinterpret, Logical (And, Or, Not), Shift |
| `Metadata` | Len, Shape, Stride, Vectorized (low-level queries) |

**Inovação:** O sistema `OperationReflect` (derive macro) permite reflexão completa sobre operações — cada `Operation` expõe seus argumentos via `args()`, permitindo visitação genérica sem match.

**Adoção:** Nosso pipeline de inferência (RMS norm, RoPE, attention, FFN, SiLU, softmax) poderia ser expresso como IR próprio. Benefícios:
- Otimizações (constant folding, dead code elimination) via passe genérico
- Diferenciação automática (para fine-tuning on-device)
- Exportação do grafo para debug/visualização

**Arquivos-alvo:** `cortex/src/cortex.rs` (atuais ~1.200 linhas de operações inline)

---

### P-10: Otimizador SSA Completo (CFG + GVN + Liveness + PRE)

**Localização:** `cubecl-opt/src/lib.rs` (674 linhas), `analyses/liveness.rs` (595 linhas), `gvn/analysis.rs` (338 linhas)

**Pipeline de otimização:**
```
1. Parse IR scope → CFG (petgraph StableDiGraph)
2. Split critical edges
3. InlineCopies + PlacePhiNodes + VersionProgram → SSA
4. Post-SSA loop:
   - InlineCopies → EliminateUnusedVariables
   - ConstOperandSimplify → MergeSameExpressions
   - ConstEval → EliminateConstBranches
   - EmptyBranchToSelect → EliminateDeadBlocks → EliminateDeadPhi
5. GVN (Global Value Numbering) — PRE + dominators
6. ReduceStrength → CopyTransform
7. SharedLiveness → MergeBlocks
8. Captures (closure variable analysis)
```

**Análises disponíveis:**

| Análise | O que faz |
|---------|-----------|
| `Dominators` | Árvore de dominância (algoritmo de Lengauer-Tarjan) |
| `PostDominators` | Pós-dominância para backward analysis |
| `Liveness` | Variáveis vivas por block (dataflow forward) |
| `MemoryLiveness` | Liveness incluindo memórias não-destruturáveis (arrays) |
| `SharedLiveness` | Liveness reversa para shared memory + alocação de slices |
| `Uniformity` | Quais valores são uniformes no workgroup |
| `PointerSource` | Rastreamento de origem de ponteiros |
| `IntegerRange` | Range analysis para constant propagation |
| `Captures` | Closure captures detection |

**GVN (Global Value Numbering):**
- Forward dominator pass → available expressions + leaders
- Backward post-dominator pass → anticipated expressions (PRE)
- Phi translation para loops
- Substituição de expressões redundantes por valores já computados

**Adoção:** Implementar optimizer simplificado para nosso inference graph. Mesmo sem SSA completo, podemos usar:
- Constant folding para shapes de tensores (que são conhecidos em compile-time)
- Dead code elimination em branches de comprimento fixo (Medusa tem 3 heads, algumas sempre não-usadas)
- Liveness para saber quando liberar KV cache entries

**Arquivos-alvo:** `cortex/src/`

---

### P-11: Disaggregate + Unroll — Otimizações Pós-SSA Especializadas

**Localização:** `cubecl-core/src/post_processing/disaggregate.rs`, `cubecl-core/src/post_processing/unroll.rs` (510 linhas)

**O que é:** Duas otimizações especializadas que rodam pós-SSA:

**Disaggregate:** Quebra `ConstructAggregate`/`ExtractAggregateField` em operações escalares. Fat pointers (ptr + offset + length) viram variáveis separadas. Permite que SSA promova cada campo individualmente.

**Unroll:** Desenrola operações vetorizadas em operações escalares quando o fator de vectorização excede o suporte do hardware:
```rust
// Se hardware só suporta vector_size=4 mas kernel pede vector_size=8:
// v = vector_add(a, b)
// vira:
// for i in 0..2 { v[i] = a[i] + b[i] }
```

**TransformAction:** Padrão de transformer:
```rust
pub enum TransformAction {
    Ignore,              // Não aplica
    Replace(Vec<Instruction>),  // Substitui por N instruções
}
```

**Adoção:** `Disaggregate` é diretamente aplicável ao nosso sistema de fat pointers no VFS e NeuralFS. Podemos representar `FileHandle` como aggregate, desagregar durante compilação, e SSA-promover cada campo.

**Arquivos-alvo:** `neural_fs/`

---

### P-12: DeviceHandle + ComputeClient — Abstração de Device Segura para Threads

**Localização:** `cubecl-runtime/src/client.rs` (1.352 linhas), `cubecl-common/src/device/handle/` (9 arquivos)

**O que é:** `DeviceHandle<R::Server>` é um handle thread-safe para um `ComputeServer` rodando em sua própria thread. Usa `crossbeam` channels para comunicação assíncrona:

```rust
pub struct DeviceHandle<S: ComputeServer> {
    sender: crossbeam_channel::Sender<ServerMessage<S>>,
    receiver: crossbeam_channel::Receiver<ServerResponse>,
    // ...
}
```

**Padrão:** `submit(FnOnce)` envia closure para a server thread. `submit_and_wait(FnOnce → R)` envia e bloqueia até resposta. Isso isola o server de race conditions e permite que o server seja single-threaded.

**Três implementações de channel:**
| Feature | Channel | Uso |
|---------|---------|-----|
| `channel-mutex` | Mutex<Vec> | WASM (sem threads) |
| `channel-mpsc` | `crossbeam` MPSC | Nativo (multi-thread) |
| `channel-cell` | `Cell` | Single-thread embedded |

**Adoção:** Nosso `gpu/backend.rs` usa `spin::Mutex<Option<GpuAccel>>` — acesso síncrono com lock. Poderíamos adotar o padrão `DeviceHandle` para cada backend GPU rodar em sua própria core (nosso SMP tem 4+ cores). Mensagens via EventBus ou SPSC ring.

**Arquivos-alvo:** `gpu/backend.rs`, `event-bus/`

---

### P-13: Kernel Compilation Cache (Cross-Platform)

**Localização:** `cubecl-common/src/compilation_cache.rs`, `cubecl-common/src/cache_file.rs`

**O que é:** Cache de kernels compilados em disco. `CompilationCache` serializa o IR compilado (CUDA cubin, SPIR-V binary, MLIR module) usando `ciborium` (CBOR binário). Chave = hash do IR + device properties.

```rust
pub struct CompilationCache {
    cache_dir: PathBuf,
    entries: HashMap<u64, Vec<u8>>,  // hash → compiled binary
}
```

**Inovação:** Checksum validation — quando device properties mudam (ex: driver update), o cache é invalidado automaticamente.

**Adoção:** Implementar `KernelCache` para nossos kernels AVX2. O IR do kernel (sequência de operações) é hasheado → lookup no cache → se hit, pula compilação. Compilação de kernel AVX2 é cara (~50µs) mas paga em toda inicialização.

**Arquivos-alvo:** `cortex/src/`

---

### P-14: Throughput Benchmarker — Kernel Profiling In-Vivo

**Localização:** `cubecl-runtime/src/throughput/` (6 arquivos)

**O que é:** Sistema de benchmark que mede throughput real de kernels em produção (não apenas warmup). `ThroughputBenchmarker` coleta métricas de execução real e pode re-autotunar se throughput degradar.

```rust
pub struct ThroughputBenchmarker {
    cache: ThroughputCache,
    configs: HashMap<KernelConfig, Vec<Duration>>,
}
pub struct ThroughputKey {
    kernel_name: String,
    input_shape: Vec<usize>,
}
```

**Adoção:** Monitorar throughput do nosso pipeline de inferência token/s. Se degradar (ex: fragmentação de cache), disparar re-autotune ou GC. Especialmente útil para Medusa (3 heads com throughput variável).

**Arquivos-alvo:** `cortex/src/cortex.rs`, `agents.rs` (MonitorAgent)

---

### P-15: TimestampProfiler — Profiling Sem Overhead de Lock

**Localização:** `cubecl-runtime/src/timestamp_profiler.rs`

**O que é:** Coletor de timestamps lock-free para profiling. Cada stream tem seu próprio buffer circular de timestamps. `resolve()` coleta e calcula durações.

```rust
pub struct TimestampProfiler {
    timestamps: Vec<TimestampEntry>,
    // lock-free append via atomic increment
}
pub enum TimestampEntry {
    Start(String),
    End(String),
    Instant(String),
}
```

**Adoção:** Nossa `ConsciousnessMetrics` (10 métricas) em `cortex.rs` poderia usar `TimestampProfiler` lock-free para medir latência de cada fase da inferência (RMS norm, attention, FFN, sampling). Hoje usa `rdtsc` inline com `core::sync::atomic`.

**Arquivos-alvo:** `cortex/src/cortex.rs` (Consciousness)

---

### P-16: CPU Runtime — LLVM/MLIR JIT com Threadpool

**Localização:** `cubecl-cpu/src/` (48 arquivos, 237 KB)

**O que é:** Backend CPU completo que:
1. Lower CubeCL IR → MLIR (dialect `func` + `scf` + `llvm`)
2. MLIR passa por otimizações (CSE, DCE, loop unroll, vectorization)
3. LLVM IR gerado → JIT compilado → executado in-process via `tracel-llvm` ExecutionEngine
4. Threadpool com `crossbeam` para paralelismo entre cubes
5. CPU affinity (Linux: `pthread_setaffinity_np`, Windows: `SetThreadAffinityMask`)

**Código real (visitor pattern):**
```rust
// cubecl-cpu/src/compiler/visitor/mod.rs
pub struct Visitor<'a> {
    first_block: Option<BlockRef<'a, 'a>>,
    block: BlockRef<'a, 'a>,
    module: &'a Module<'a>,
    blocks: HashMap<NodeIndex, BlockRef<'a, 'a>>,
    context: &'a Context,
    values: Values<'a>,
    liveness: Rc<MemoryLiveness>,
    mutable_variables: Vec<Id>,
    stack_saves: HashMap<Id, StackSave<'a>>,
    needs_parallelism: &'a mut bool,
}
```

**Scheduler do threadpool:**
| `SchedulerStrategy` | Comportamento |
|-------------------|--------------|
| **Naive** | Round-robin simples |
| **Dispatcher** | Work-stealing entre threads |
| **Aside** | Thread auxiliar para async ops |

**Adoção:** Não podemos usar LLVM/MLIR (no_std + bare-metal). Mas o padrão **visitor que caminha CFG → gera código** é replicável. Nosso pipeline de inferência atual é inline (sequência fixa de operações). Poderíamos expressá-lo como IR+visitor para:
- Gerar código especializado para diferentes model sizes (850M vs 2B)
- Escolher entre AVX2/AVX512/scalar sem if/else
- Otimizar loop unrolling baseado em K, V shapes conhecidos

**Arquivos-alvo:** `cortex/src/`

---

### P-17: Arena Allocator com DropBump

**Localização:** `cubecl-common/src/arena.rs` (25.514 bytes), `cubecl-ir/src/arena.rs`

**O que é:** Arena allocator próprio (não bumpalo) com suporte a `Drop`:
```rust
pub struct DropBump {
    // Bump allocator que roda drop em ordem reversa
    // Ideal para IR nodes com vida curta
}
```

Usado extensivamente no IR para alocar `Instruction`, `Value`, `Scope` sem pagar custo de allocator global. Resetada entre compilações de kernel (`reference_arena.reset()` em `Scope::process`).

**Adoção:** Já temos `TensorArena` bump allocator em `global_arena.rs`. Poderíamos adicionar `DropBump` para alocações temporárias durante a construção do graph de inferência (antes de entrar no loop de tokens).

**Arquivos-alvo:** `arena.rs`, `global_arena.rs`

---

### P-18: MemoryLayoutPolicy — Controle de Layout de Buffer

**Localização:** `cubecl-runtime/src/server/base.rs` (trait `ComputeServer`)

**O que é:** Cada backend define como buffers são dispostos na memória:

```rust
pub trait MemoryLayoutPolicy {
    fn select(&self, binding: &Binding) -> MemoryLayoutDescriptor;
}

pub enum MemoryLayoutStrategy {
    Reorder,    // Otimiza layout para acesso coalescido
    Align,      // Alinha para SIMD
    Default,    // Layout do hardware
}
```

**Inovação:** `MemoryLayoutDescriptor` contém `strides`, `offset`, `padding` que o kernel usa para acessar dados no formato correto. O backend pode reordenar layouts sem o kernel saber.

**Adoção:** Nosso `Tensor` em `cortex/src/tensor.rs` tem layout row-major fixo (linhas × colunas). Poderíamos adicionar suporte a strides arbitrários para:
- Transposição sem cópia (mudar strides)
- Padding para alinhamento SIMD
- Layout NHWC vs NCHW para diferentes kernels

**Arquivos-alvo:** `cortex/src/tensor.rs`

---

## 4. Código-Fonte dos Exemplos (Analisado em 2026-07-15)

### Exemplo: GELU Activation

```rust
// examples/gelu/src/lib.rs
#[cube(launch_unchecked)]
fn gelu_array<F: Float, N: Size>(input: &[Vector<F, N>], output: &mut [Vector<F, N>]) {
    if ABSOLUTE_POS < input.len() {
        output[ABSOLUTE_POS] = gelu_scalar(input[ABSOLUTE_POS]);
    }
}

#[cube]
fn gelu_scalar<F: Float, N: Size>(x: Vector<F, N>) -> Vector<F, N> {
    let sqrt2 = F::new(comptime!(2.0f32.sqrt()));
    let tmp = x / Vector::new(sqrt2);
    x * (Vector::erf(tmp) + Vector::one()) / Vector::new(F::new(2.0f32))
}
```

**Padrões:** `Vector<F,N>` com comptime `N` = SIMD width determinada em compile-time (P-01, P-02).

### Exemplo: Fusing (Kernel Fusion)

```rust
// examples/fusing/src/lib.rs
#[cube(launch_unchecked)]
fn fusing<F: Float, N: Size>(
    inputs: Sequence<Box<[Vector<F, N>]>>,
    mut outputs: Sequence<Box<[Vector<F, N>]>>,
    #[comptime] ops: Sequence<Operation>,
) {
    #[unroll]
    for index in 0..ops.len() {
        let op = comptime! { ops.index(index) };
        let input = inputs.index(op.input_index);
        let output = outputs.index_mut(op.output_index);
        match op.kind {
            OperationKind::Exp => output[ABSOLUTE_POS] = input[ABSOLUTE_POS].exp(),
            OperationKind::Log => output[ABSOLUTE_POS] = input[ABSOLUTE_POS].ln(),
        }
    }
}
```

**Padrões:** `#[comptime] ops: Sequence<Operation>` + `#[unroll]` = operações conhecidas em compile-time viram código linear sem branch (P-05, P-02). Sequence de operações é resolvida antes da compilação do kernel.

### Exemplo: Normalization

```rust
// examples/normalization/src/lib.rs
#[cube(launch_unchecked)]
fn norm_test<F: Float, N: Size>(
    input: &[Vector<F, N>],
    output_a: &mut [Vector<F, N>],
    output_b: &mut [Vector<F, N>],
) {
    if ABSOLUTE_POS < input.len() {
        output_a[ABSOLUTE_POS] = Vector::cast_from(F::normalize(F::cast_from(input[ABSOLUTE_POS])));
        output_b[ABSOLUTE_POS] = input[ABSOLUTE_POS]
            / Vector::cast_from(F::magnitude(F::cast_from(input[ABSOLUTE_POS])));
    }
}
```

**Padrões:** Dois outputs do mesmo kernel = fused normalize + magnitude em uma passada (P-05). `ABSOLUTE_POS` = índice linear (P-01).

### Exemplo: Comptime com Feature Specialization

```rust
#[cube(launch)]
fn sum_plane<F: Float>(
    input: &[F],
    output: &mut [F],
    #[comptime] plane: bool,          // ← resolvido em compile-time
    #[comptime] end: Option<u32>,     // ← Option resolvido em compile-time
) {
    if plane {
        output[UNIT_POS] = plane_sum(input[UNIT_POS]);  // warp reduction
    } else {
        sum_basic(input, output, end);                    // sequential fallback
    }
    // NOTA: sem branch em runtime — compile-time gera 2 kernels separados
}
```

**Padrão P-02:** `comptime!` + `#[comptime]` params = zero branching runtime. Compilador gera N kernels, dispatch é tabela de ponteiros.

### Exemplo: FastMath por Função

```rust
#[cube(fast_math = FastMath::all())]   // ← flags por função
fn fast_rsqrt<F: Float>(x: F) -> F {
    F::inverse_sqrt(x)
}
```

**Padrão P-19 (novo):** FastMath flags por função — `#[cube(fast_math = FastMath::all())]` permite precisão reduzida em funções específicas. Útil para activation functions (GELU, SiLU) onde erro relativo < 1% é aceitável.

---

## 5. Padrões Adicionais Identificados na Documentação

### P-19: FastMath Flags por Função

| Flag | Efeito | Trades |
|------|--------|--------|
| `AllowReciprocal` | Usa `__fdividef` em vez de divisão | Precisão reduzida |
| `ReducedPrecision` | Usa `__expf`/`__logf`/`__sinf`/`__cosf` | Sem NaN/Inf handling |
| `UnsignedZero` | Ignora signed zero | Comportamento IEEE-754 relaxado |
| `NotNaN` / `NotInf` | Assume sem NaN/Inf | Mais rápido, sem verificações |

**Backends:** CUDA mapeia para intrinsics (`__expf`, `__fdividef`, `__frsqrt_rn`). Vulkan usa flags do compilador. CPU ignora.

**Adoção:** Marcar `gelu()`, `silu()`, `softmax()` em `cortex.rs` com `fast_math` equivalente. Erro tolerável (<0.1% relative) para ganho de 1.5-3× em operações transcendentes.

### P-20: FastDivmod para Indexação 2D/3D

```rust
#[cube(launch)]
pub fn some_2d_kernel<F: Float>(output: &mut [F], width: FastDivmod) {
    let (y, x) = width.div_mod(ABSOLUTE_POS);  // sem divisão real
}
```

Usa Barret Reduction com `__umulhi` (CUDA) ou `OpUMulExtended` (Vulkan) para divisão de inteiros sem custo. Fallback para `u64` cast/shift em backends sem suporte.

**Adoção:** Indexação de tensores 2D/3D no inference engine. Nosso `idx(i,j)` atual usa multiplicação+adição (barato), mas `div_mod` para `ABSOLUTE_POS` → `(y,x)` pode usar FastDivmod em kernels GPU.

---

## 6. Mapa de Adoção (Análise Apenas — Sem Implementação)

| # | Padrão | Esforço | Ganho | Quando |
|---|--------|---------|-------|--------|
| **P-01** | 4 eixos de paralelismo | ~200 LOC | GPU fallback CPU + Intel + NVIDIA uniforme | ⏳ |
| **P-02** | Comptime / specialize! macro | ~400 LOC | Zero branching no hot path | ⏳ |
| **P-03** | AutotuneCache persistente | ~150 LOC | 0 warmup em boots subsequentes | ⏳ |
| **P-04** | PersistentPool no VRAM | ~100 LOC | Zero fragmentação para model weights | ⏳ |
| **P-05** | Graph capture/replay (fusing) | ~500 LOC | Decode loop 2-3× mais rápido | ⏳ |
| **P-07** | Multi-stream scheduler | ~300 LOC | Prefill/decode paralelizados | ⏳ |
| **P-08** | StorageType::Packed + FP4/E2M1 | ~300 LOC | Sistema de tipos unificado | ⏳ |
| **P-10** | Otimizador IR simplificado | ~800 LOC | Constant folding, DCE | ⏳ |
| **P-11** | Disaggregate (fat pointers) | ~200 LOC | VFS/NeuralFS optimizado | ⏳ |
| **P-12** | DeviceHandle thread-safe | ~400 LOC | GPU backend thread-isolado | ⏳ |
| **P-15** | TimestampProfiler lock-free | ~150 LOC | Profiling zero-overhead | ⏳ |
| **P-16** | Visitor IR→codegen | ~1000+ LOC | Geração de kernel especializado | ⏳ |
| **P-18** | MemoryLayout com strides | ~200 LOC | Transposição zero-copy | ⏳ |
| **P-19** | FastMath flags por função | ~100 LOC | GELU/SiLU 1.5-3× mais rápido | ⏳ |
| **P-20** | FastDivmod para indexação | ~80 LOC | Indexação 2D sem divisão | ⏳ |

---

## 7. Tecnologias que NÃO São Aproveitáveis (e por quê)

| Tecnologia | Razão |
|-----------|-------|
| `tracel-llvm` (LLVM/MLIR JIT) | Requer std + dynamic linking + 100MB+ LLVM bundled. Inviável em bare-metal. |
| `wgpu` / `cudarc` | Dependem de GPU drivers do sistema (Vulkan ICD, CUDA driver). Não existem em bare-metal. |
| `tokio` / async channels | std-only. Nosso EventBus já faz IPC pub/sub lock-free. |
| `#[cube]` proc macro | O proc macro em si roda em build time (ok), mas o código gerado chama `cubecl-runtime` que é std. |
| `cubecl-core` frontend completo | 73 arquivos de frontend (comptime, plane, barrier, topology) que geram IR chamando runtime std. |
| `cubecl-wgpu` / `cubecl-cuda` / `cubecl-hip` | Backends GPU dependem de runtime drivers do SO. Inexistentes em bare-metal. |
| `cubecl-cpu` (LLVM/MLIR JIT) | Backend CPU usa LLVM JIT in-process. Inviável sem libffi + dynamic codegen. |

---

## 8. Conclusão

CubeCL é uma das implementações mais sofisticadas de compute language multi-target em Rust. A análise aprofundada (~130 arquivos fonte + book + exemplos) identificou **20 padrões transferíveis**, dos quais 15 foram documentados nesta ADR.

A integração como dependência (adicionar `cubecl` ao Cargo.toml) continua inviável — o projeto assume std + GPU drivers do sistema operacional. Mas **os padrões arquiteturais são diretamente transferíveis** para nosso ecossistema no_std + bare-metal.

O padrão de maior valor é **P-02 (Comptime)** combinado com **P-01 (4 eixos)**: a macro `specialize!` que gera N variantes de kernel em build time, eliminando if/else vendor no hot path de inferência. Este é o pré-requisito para os demais padrões de dispatch.

---

## 9. Análise de Pull Requests Abertos (2026-07-15)

23 PRs abertos no repositório. Os mais relevantes para nossa análise:

### #1402 (Draft): "Make everything Pliron" — Migração de IR

**O que é:** Substituição completa do IR custom (`cubecl-ir`) pelo framework [Pliron](https://github.com/pliron/pliron) — um IR framework em Rust com SSA, dominators, e passes de otimização.

**Impacto na nossa análise:**
- Confirma que **IR próprio é volátil e caro de manter**. O CubeCL, mesmo com equipe dedicada, está trocando o IR inteiro.
- Nosso P-10 (otimizador IR simplificado) deve ser **postergado** até que o ecossistema Pliron amadureça ou abandonemos a ideia de IR próprio.
- Alternativa: usar o padrão de visitor (P-16) diretamente, sem IR intermediário.

### #1189: "Improve and uniformize naming across CubeCL (axis/count/line_size)"

**O que é:** Renomeação dos 4 eixos de paralelismo para nomenclatura consistente. A nomenclatura atual (CUBE_DIM, PLANE_DIM, etc.) ainda está em fluxo.

**Impacto na nossa análise:**
- O modelo de 4 eixos (P-01) é confirmado como correto, mas a **nomenclatura ainda não é estável** nem no próprio CubeCL.
- Nossa adoção de P-01 deve usar nomes genéricos (vector_width, plane_dim, cube_dim, cube_count) sem acoplar à nomenclatura do CubeCL.

### #879: "strided (pitched) buffer I/O + centralized stride helpers"

**O que é:** Adição de suporte a strides (pitched buffers) no runtime do CubeCL. Permite que cada backend (CUDA, Vulkan, Metal) use seu layout nativo de buffer sem cópia.

**Impacto na nossa análise:**
- Confirma P-18 (MemoryLayout com strides) como padrão maduro e desejado.
- A implementação do CubeCL é complexa (~800 LOC) porque precisa conciliar 4 backends diferentes. Nossa implementação bare-metal (1 backend de cada vez) seria mais simples (~200 LOC).
- O helper `MemoryLayoutDescriptor` com strides, offset, padding é exatamente o que planejamos para nosso `Tensor`.

### #1404: "pinned-DMA bandwidth for large host-to-device uploads"

**O que é:** Otimização de DMA usando pinned host buffers para uploads grandes CPU→GPU.

**Impacto:** Relevante para quando implementarmos DMA CPU→VRAM. O padrão de pinned buffers (registrados no IOMMU) é mais rápido que bounce buffers. Nosso `cpu_to_vram()` atual usa write_volatile palavra por palavra — poderíamos adoptar pinned buffers no futuro.

### #1413: "saturate FlushingPolicyState counters"

**O que é:** Fix para #1359 — overflow em contador de pool de memória quando o flush policy é chamado muitas vezes.

**Impacto:** Edge case de memory management. Nosso PersistentPool (P-04) não tem contadores de flush (é never-free), mas o VramBuddy poderia ter overflow similar em `total_allocated`.

### #1364: "Fuzz the `#[cube]` macro"

**O que é:** Fuzzing do proc macro `#[cube]` para encontrar bugs de code generation.

**Impacto:** O proc macro ainda precisa de fuzzing — sinal de imaturidade. Reforça nossa decisão de não usar `#[cube]` ou `cubecl-core`.

### #1124: "use memory reuse marker for cpu"

**O que é:** Otimização de reuso de memória no backend CPU usando markers de ciclo de vida.

**Impacto:** Equivalente ao nosso P-04 (PersistentPool) mas para CPU. O padrão de "marcar memória como reusável" é mais flexível que nosso never-free pool. Útil para o TensorArena.

---

## 10. Repositório de Referência

- **Repositório:** [tracel-ai/cubecl](https://github.com/tracel-ai/cubecl)
- **Clone local:** `C:\Users\msrov\AppData\Local\Temp\opencode\cubecl\` (branch `main`, depth 1, 408 arquivos Rust, ~2.9MB)
- **Versão:** v0.10.0 (1.468 commits)
- **Crates analisados:** 16 crates: cubecl-core, cubecl-ir, cubecl-opt, cubecl-runtime, cubecl-common, cubecl-cpu, cubecl-macros, cubecl-macros-internal, cubecl-cuda, cubecl-wgpu, cubecl-hip, cubecl-metal, cubecl-spirv, cubecl-zspace, cubecl-cpp, cubecl-std
- **Book:** `cubecl-book/src/` — seções: Getting Started, Core Features (Constants, Comptime, Vectorization, Autotune, Hardware Features), Language Support (Trait, Enum, Struct), Advanced Usage (Config, Math Optimizations)
- **Exemplos:** `examples/gelu`, `examples/fusing`, `examples/normalization`, `examples/sum_things`, `examples/throughput`, `examples/tracing_example`, `examples/device_sharing`
- **PRs abertos (23):** #1402 (Pliron migration), #1189 (axis naming), #879 (strided I/O), #1404 (pinned DMA), #1413 (flush overflow), #1364 (fuzz proc macro), #1124 (memory reuse), #1420 (tuple destructure), #1414 (fallible sync), #1410 (C++ inf fix), #1407 (kernel profiler), #1388 (Vulkan clamp), #1377 (cleanup), #1371 (rm md5), #1334 (pinned host), #1331 (AtomicOp fix), #1328 (SPIR-V f64), #1300 (contract validation), #1252 (buffer validation), #1246 (as_arg), #1207 (Metal bf16), #1197 (exp2/log2), #1353 (distributed comm)
- **Issues abertas (126):** #1425 (SPIR-V stack overflow), #1421 (wgpu profiling), #1406 (CUDA leak), #1401 (stale pages), #1396 (cuFFT), #1375 (if/else wgpu), #1370 (UE4M3 mapping), #1365 (RDNA2 ROCm), #1359 (flush overflow), #1352 (GraphicsApi), #1336 (thread-private Array), #1318 (Atomic SPIR-V)

## 11. Referências do Projeto

- Neural OS Core: `jarvis/src/gpu/` (16 arquivos — backend, detect, vram, intel, nvidia, amd, bench, cube, ring, firmware, xqueue, kv_dma, xpu, msched, display_coex, mod)
- Neural OS Core: `cortex/src/tensor.rs` (tensor types: Tensor, TernaryTensor, PackedTernaryTensor, CodebookVQ)
- ADR-0043 (este documento): análise de ~130 arquivos fonte + book + exemplos + PRs + issues do CubeCL
