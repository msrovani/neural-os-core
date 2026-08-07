# ADR-0047-GPU: GPU Compute Pipeline — Matmul Ternário, Persistent Kernel, VRAM Unificada

**Data:** 2026-07-16  
**Status:** Accepted (MVP parcial) — SESSION_126–127; G1–G5 PoC (G5 50μs aspiracional); DP4A defer  

**Complemento de:** ADR-0047 (LatentBus + EvolveAgent + NeuOS Probe)  
**Depende de:** ADR-0029 (GPU Architecture), ADR-0037 (SMP+GPU), ADR-0041 Cap P5 (DMA pin), ADR-0043 (CubeCL 20 patterns)  
**Sprint:** 109+ (paralelo com ADR-0042 N2→N5 e ADR-0047 pilares 1-3)  
**Hardware alvo:** Intel Gen9+ iGPU, NVIDIA GTX 1050 (Pascal, sm_61), VirtIO-GPU (QEMU)

---

## 0. MVP PoC (SESSION_126–127)

| Item | Status |
|------|--------|
| G1 persistent work-queue | ✅ `gpu/work_queue.rs` |
| G2 matmul path + gate HW/CPU_FALLBACK | ✅ `backend::adr0047_compute_gate` |
| G3 SASOS-lite unified map | ✅ `gpu/sasos.rs` |
| G4 H2O + pages (CPU) | ✅ `cortex/kv_h2o.rs` |
| G5 pipeline stages (CPU timing) | ✅ `gpu/pipeline_g5.rs` — 50μs/token aspiracional até shader HW |
| N-gram DP4A verify | ⏳ defer (precisa G2 HW real) |

## Índice

1. [Executive Summary](#1-executive-summary)
2. [Estado Atual da GPU](#2-estado-atual-da-gpu)
3. [Análise dos Projetos de Referência](#3-análise-dos-projetos-de-referência)
4. [Matmul Ternário na GPU: O Pulo do Gato](#4-matmul-ternário-na-gpu-o-pulo-do-gato)
5. [Pilar G1 — Persistent Kernel GPU-Side](#5-pilar-g1--persistent-kernel-gpu-side)
6. [Pilar G2 — Tensor Ops na GPU](#6-pilar-g2--tensor-ops-na-gpu)
7. [Pilar G3 — SASOS: Espaço de Endereço Unificado RAM+VRAM](#7-pilar-g3--sasos-espaço-de-endereço-unificado-ramvram)
8. [Pilar G4 — H2O KV Cache Folding + PagedAttention](#8-pilar-g4--h2o-kv-cache-folding--pagedattention)
9. [Pilar G5 — GPU Inference Pipeline Completo](#9-pilar-g5--gpu-inference-pipeline-completo)
10. [Roteiro de Implementação](#10-roteiro-de-implementação)
11. [Referências](#11-referências)
12. [Riscos e Mitigações](#12-riscos-e-mitigações)

---

## 0. MVP PoC (SESSION_126+)

| Item | Status |
|------|--------|
| G1 persistent work-queue | ✅ `gpu/work_queue.rs` |
| G2 matmul path + gate HW/CPU_FALLBACK | ✅ `backend::adr0047_compute_gate` |
| G3 SASOS-lite unified map | ✅ `gpu/sasos.rs` |
| G4 H2O + pages (CPU) | ✅ `cortex/kv_h2o.rs` |
| G5 pipeline stages (CPU timing) | ✅ `gpu/pipeline_g5.rs` — target 50μs/token = aspiracional até shader HW |
| N-gram DP4A verify | ⏳ defer (precisa G2 HW real) |

## 1. Executive Summary

A GPU do neural-os-core está num estado de "esqueleto funcional sem músculos": detecta, aloca VRAM, carrega firmware, faz DMA de KV cache — mas **todo tensor op real roda em CPU**. O compute shader está bloqueado desde a Sprint 85 sob alegação de "NDA da ISA NVIDIA".

Este ADR mostra que:

1. **Intel GEN é documentação pública** — podemos gerar shaders para iGPU hoje
2. **GPUOS (arXiv 2025)** demonstrou kernel persistente com 15.3× speedup para ops pequenas
3. **Matmul ternário (±1,0) é ideal para GPU INT8 DPAS** — só ADD/SUB, sem multiplicação
4. **NMOS (2026)** roda 70B em 4GB VRAM com H2O folding + double-buffered prefetch
5. **TensorOS (2026)** mostrou SASOS — mapear VRAM no mesmo AS da RAM → zero-copy

Cinco pilares de implementação concreta:

| Pilar | O quê | Ganho estimado | Esforço |
|-------|-------|---------------|---------|
| **G1** | Persistent kernel GPU-side (work queue + op table) | 15.3× launch overhead | ~500 LOC |
| **G2** | Tensor ops na GPU (matmul, rms_norm, softmax) | 10-100× sobre CPU | ~1000 LOC |
| **G3** | SASOS: RAM+VRAM no mesmo address space | Zero-copy, sem DMA | ~100 LOC |
| **G4** | H2O folding + PagedAttention | 4-8× compressão KV cache | ~500 LOC |
| **G5** | Pipeline completo de inferência GPU | ~50μs/token (vs ~5ms CPU) | ~2000 LOC total |

---

## 2. Estado Atual da GPU

### 2.1 Código existente

```
crates/neural-kernel/src/gpu/
├── mod.rs              ✅ GPU Module entry (detect, VRAM tier, ring, firmware, backend)
├── detect.rs           ✅ GPU Detection — PCI class 0x03, vendor, modelo, VRAM
├── backend.rs          ⚠️ GPU Backend — conecta GPU ao pipeline de inferência
├── vram.rs             ✅ VRAM Tier — buddy allocator (VramBuddy { base, size, free })
├── nvidia.rs           ⚠️ PFIFO PUSH_BUFFER via BAR2 — compute via stub
├── intel.rs            ⚠️ iGPU Ring Buffer — "compute stub — usando fallback CPU"
├── amd.rs              ⚠️ VRAM mapeada — "PM4 compute futuro"
├── ring.rs             ✅ SPSC job ring — CPU enfileira jobs, GPU consome (doorbell)
├── xqueue.rs           ✅ XQueue — 3 níveis de prioridade preemptível
├── xpu.rs              ⚠️ CPU prefill + GPU decode split (só KV DMA, não compute)
├── kv_dma.rs           ✅ CPU↔GPU KV cache transfer (DMA direction enum)
├── msched.rs           ✅ MSched — Belady/OPT eviction predictor (só monitora)
├── firmware.rs         ✅ NVIDIA ACR, AMD PSP, Intel GuC — VRAM WPR + boot
├── display_coex.rs     ✅ iGPU display + dGPU compute coexistence
├── cube.rs             ❌ CubeCL integration — "Fallback CPU"
├── bench.rs            ❌ GPU Benchmark — "CPU fallback"
└── mod.rs              ✅ Module file
```

### 2.2 Pipeline atual

```
Input → CPU embed → CPU matmul (ternário) ×24 layers → CPU softmax → CPU sample
                                                                        │
                                                                  KV cache (RAM)
                                                                        │
                                                                  DMA → VRAM (kv_dma.rs)
                                                                        │
                                                            GPU só armazena KV cache
                                                            compute: ❌ CPU fallback
```

**Problemas fundamentais:**
1. **Compute zero**: matmul, softmax, rms_norm, attention — tudo CPU
2. **Overhead de launch**: cada "kernel GPU" que não existe seria 5-10μs de launch se existisse
3. **VRAM subutilizada**: só armazena KV cache. Tensor de peso do modelo (202MB) está na RAM
4. **Sem unificação**: ponteiro CPU não alcança VRAM. Toda transferência é cópia explícita
5. **Pipeline split caro**: prefill (CPU) → DMA KV → decode (CPU) — GPU só armazena

### 2.3 Hardware disponível

| GPU | VRAM | Compute | Status driver | Oportunidade |
|-----|------|---------|---------------|-------------|
| Intel HD 620 (Kaby Lake, Gen9) | 0-512MB (UMA) | 24 EUs, DPAS? → Gen9 não. Gen12+ tem | GUC loaded | Shader via ring buffer + MEDIA_OBJECT |
| NVIDIA GTX 1050 (Pascal, sm_61) | 2GB GDDR5 | 640 CUDA cores, INT8 DP4A | PFIFO channel, BAR2 UC | CUDA compute via PFIFO |
| VirtIO-GPU (QEMU) | 0MB (hóspede) | VirGL? | Stub | Só teste display |
| NVIDIA RTX 3060 (se disponível) | 12GB GDDR6 | Ampere, INT8 tensor core | PFIFO channel | Tensor cores reais |

**Realidade**: GTX 1050 é o alvo principal. sm_61 não tem tensor core verdadeiro, mas tem **DP4A** (Dot Product 4-element Accumulate) via `__dp4a` intrinsic — ideal para matmul ternário.

### 2.4 Por que o compute shader nunca saiu do papel

O bloqueio histórico é "NDA da ISA NVIDIA". A ISA do PFIFO (a fila de comandos) é documentada no open-gpu-doc e no envydiss. O que **não** é público é a ISA dos shaders (SASS). Mas:

1. **Intel GEN ISA é 100% pública** — documentação de 500+ páginas. Podemos começar pela iGPU
2. **CUDA driver API é pública** — `cuModuleLoad`, `cuLaunchKernel`, `cuStreamCreate` — não precisa da ISA
3. **Nós já usamos PFIFO** — o PUSH_BUFFER funciona (`nvidia.rs` linha 150+). Só não tem compute
4. **DP4A intrinsic é CUDA** — não ISA nativa. Podemos usar CUDA assembly (`asm("{ dp4a ... }")`) ou PTX

---

## 3. Análise dos Projetos de Referência

### 3.1 GPUOS (Yang et al., arXiv:2604.17861, 2026)

```
Título: GPUOS: A GPU Operating System Primitive for Transparent Operation Fusion
15.3× speedup para operações dominadas por launch overhead
```

**Ideia central**: um kernel GPU permanente que nunca termina. CPU escreve tasks numa fila circular atômica. GPU lê da fila, executa via jump table de function pointers.

```cuda
// Kernel persistente — GPUOS pattern
__global__ void persistent_worker(WorkQueue* q) {
    while (true) {
        uint idx = atomicAdd(&q->head, 1) & (q->size - 1);
        Task t = q->tasks[idx];
        if (t.op == OP_EXIT) break;
        g_op_table[t.op](t.args);  // device function pointer table
        __threadfence();
        atomicInc(&q->processed);
    }
}

// CPU side — submission
q->tasks[tail] = Task { op: MATMUL_T, ptrA, ptrB, ptrC, M, N, K };
atomicStoreRelease(&q->commit, tail + 1);
```

**Dual-slot aliasing**: a jump table tem 2 slots por operação. Quando um novo operador é compilado (NVRTC), ele preenche o slot inativo, faz um atomic flip de versão, e o slot ativo/inativo troca — sem nunca interromper o kernel.

**Integração PyTorch**: TorchDispatch hook intercepta ops pequenas → enfileira no GPUOS → executa batch → retorna. Transparente pro usuário.

**O que pegar**: o kernel persistente + work queue + op table. Nosso `ring.rs` já tem SPSC. Falta o GPU-side loop.

### 3.2 neurOS (Price, 2026)

```
Todo SO como tensor ops na GPU: MMU, TLB, scheduler, filesystem, compiler — tudo neural.
Zero CPU-GPU sync durante operação normal.
```

**Neural TLB**: fully-associative GPU-tensor cache: lookup é broadcast `(vpn_tags == vpn) & (asid_tags == asid)` em todos os 64 entries. MLP de eviction treinado.

**Neural Cache**: LSTM replacement policy + LSTM prefetch predictor. Treinado com gradiente único.

**Neural Scheduler**: Transformer self-attention sobre fila de processos. Considera interações inter-processo.

**Online Adaptation**: TLB, cache, scheduler aprendem em tempo real. Um gradiente por decisão.

**O que pegar**: conceito de "subsistema neural na GPU". Não pra copiar (PyTorch, GPU), mas pra inspirar: nosso MSched podia usar modelo MLP em vez de Belady heurístico.

### 3.3 NMOS — Neural Memory OS (2026)

```
70B em 4GB VRAM. Predictive partial execution engine.
Double-buffered prefetcher + H2O + PagedAttention.
```

**Double-buffered prefetcher**: SSD→RAM→VRAM pipeline. Enquanto layer N processa na GPU, layer N+1 está sendo carregada do SSD pra RAM, e layer N+2 do SSD.

**H2O (Heavy Hitter Oracle)**: identifica páginas KV importantes por score de atenção. Popa (folda) as menos importantes. Reduz KV cache em 4-8× sem perda significativa.

**PagedAttention**: KV cache em páginas de 16MB. Swappáveis entre RAM e VRAM. VRAM = working set ativo, RAM = cold storage completo.

**O que pegar**: H2O folding + double-buffered prefetch. Já temos `kv_dma.rs`. Falta a política de evicção baseada em importância.

### 3.4 TensorOS / NeuroLang (Inphinie, 2026)

```
Exokernel SASOS: RAM + VRAM + NVMe no mesmo espaço de endereço 64-bit.
Boot <2s, zero syscalls, zero-copy NVMe→VRAM, Tile-oriented language.
```

**Single Address Space (SAS)**: memória unificada. Um ponteiro serve pra CPU e GPU. Zero-copy por construção.

**Direct Storage**: DMA NVMe→VRAM sem passar pela CPU. Alinhamento de página entre SSD e GPU.

**Tile Language**: `Tile<128, 128, f16>` como primitiva de linguagem. Compilado via MLIR para PTX.

**O que pegar**: SASOS. Nós já mapeamos BAR2 como UC. Só falta mapear no mesmo espaço virtual do heap e tratar VRAM como tier de memória no MHI.

### 3.5 LithOS (arXiv:2504.15465, 2025)

```
GPU Operating System em Rust (~5000 LOC). TPC scheduler, kernel atomization.
Transparente para CUDA apps — não modifica código.
```

**TPC Scheduler**: escalonamento no nível de TPC (Texture Processor Cluster). Não kernel inteiro.

**Kernel Atomization**: quebra kernels longos em "átomos" (chunks de thread block) para escalonamento fino.

**TPC Stealing**: TPCs ociosas são emprestadas para outras tarefas.

**Hardware Right-Sizing**: modelo leve prevê quantos TPCs alocar pra cada kernel.

**O que pegar**: o conceito de atomização é relevante para latência. Nosso XQueue de 3 níveis podia atomizar kernels longos.

### 3.6 CubeCL (ADR-0043, 2026)

```
20 padrões GPU compilados do cubical tech. Comptime, autotune, graph capture.
```

| Padrão | Descrição | Aplicabilidade |
|--------|-----------|---------------|
| P-01 | Kernel Launch — submissão de shader | Já temos ring buffer + doorbell |
| P-02 | Compute Shader — kernel genérico | Precisa de shader compiler |
| P-03 | Comptime — constantes em tempo de compilação | Ideal para ternário (pesos fixos) |
| P-04 | Autotune — busca automática de parâmetros | Tile size para matmul |
| P-05 | Graph Capture — grafo de operações fusionadas | Attention fusion |
| P-06 | Memory Pool — reuso de alocações GPU | VramBuddy já faz |
| P-07 | Tensor Core Dispatch — uso de hardware especializado | DPAS/DP4A para matmul |
| P-08 | Warp-Level Ops — shuffle, ballot dentro do warp | Reduce em softmax |
| P-09 | Cooperative Groups — sincronização entre blocos | Atenção com contexto longo |
| P-10 | Stream/CUDA Stream — paralelismo entre kernels | Prefetch + compute overlap |

### 3.7 NeuroOS (3scud3r0, 2026)

```
Single-file Python: GGUF inference + MCTS + Dynamic LoRA + Graph Memory.
PagedAttention, INT8/FP16 KV cache, HydraMoE.
```

**ProteusNet & HydraMoE**: intercepta feedback do usuário → hot-swap LoRA adapters na VRAM. Aprendizado contínuo sem catastrophic forgetting.

**O que pegar**: o conceito de hot-swap de adapters LoRA na VRAM. Se tivermos múltiplos experts no MoE, podemos ter os pesos de todos em VRAM e trocar o ativo conforme o routing.

### 3.8 N-gram Speculative Decoding (llama.cpp, Alok 2026)

```
Tweet: https://x.com/analogalok/status/2077718647905333549
Técnica: rolling LCG hash → O(1) lookup → draft M tokens → GPU verify
```

Aceleração de inferência **sem draft model, sem VRAM extra**:

- Rolling hash da janela de N tokens → O(1) lookup no KV cache
- Draft de M tokens copiados da ocorrência anterior
- GPU verifica todo o batch draft em paralelo
- Resultado: **2× speed** no Gemma 4 26B MoE com T4, zero overhead

**Sinergia com GPU pipeline**: N-gram gera drafts que o pipeline GPU verifica via DP4A. É o mecanismo de speculative decoding mais leve que existe — ideal para nosso orçamento de VRAM (2GB) onde não cabe um draft model separado. Implementação trivial (~150 LOC) que já acelera a inferência enquanto os pilares G1-G5 são construídos.

**Status (SESSION_125):** ✅ lógica OK em CPU/`KvCache` (`cortex/ngram_spec.rs`). Verify paralelo na GPU (DP4A) permanece aberto neste ADR.

---

## 4. Matmul Ternário na GPU: O Pulo do Gato

### 4.1 Por que ternário é ideal para GPU

Matmul ternário: `y_i = Σ_j w_ij × x_j` onde `w ∈ {-1, 0, +1}`.

Propriedade fundamental: **não precisa de multiplicadores**. O matmul ternário é APENAS ADD/SUB condicionais:

```
y_i = Σ_{j: w=+1} x_j  —  Σ_{j: w=-1} x_j
```

### 4.2 Implementação com DP4A (NVIDIA)

DP4A é uma instrução INT8 que faz Dot Product de 4 elementos acumulando em INT32:

```
// CUDA intrinsic: __dp4a(a, b, c) → c += a.b0*b.b0 + a.b1*b.b1 + a.b2*b.b2 + a.b3*b.b3
// a, b são int32 que contêm 4 × int8 cada

// Nosso caso: a = peso ternário empacotado como int8 em {-1, 0, 1}
// b = ativação fp16 convertida para int8 (escalado)
// dp4a processa 4 elementos de cada vez

int dp4a(int a, int b, int c) {
    asm("{ dp4a.s32.s32 %0, %1, %2, %3; }" : "+r"(c) : "r"(a), "r"(b), "r"(c));
    return c;
}
```

**GTX 1050 (sm_61)**: 640 CUDA cores, cada core faz 1 DP4A/ciclo → 640 operações/ciclo. A 1.455 MHz → **~930 GOPS INT8**. Hidden=1024, FFN=4096 → matmul 1024×4096 = 4M ops → **~4.3μs**.

### 4.3 Implementação com DPAS (Intel Xe)

Intel Xe (Gen12+) tem DPAS: Dot Product Accumulate Systolic. Processa matrizes 8×8×8:

```
// DPAS: C[8][8] += A[8][8] × B[8][8]
// Cada elemento: int8. Total: 512 MACs por instrução

// Para matmul 1024×1024:
//   tiles 8×8: (1024/8) × (1024/8) = 128 × 128 = 16384 DPAS calls
//   16384 × 4 ciclos (latência DPAS) ≈ 65536 ciclos
//   GPU 1100 MHz → ~60μs
```

**Intel HD 620 (Gen9)**: **não** tem DPAS (só Gen12+). Mas tem MAD (multiply-add) INT8 em cada EU. 24 EUs × 8 threads × 4 MADs → 768 MADs/ciclo → 768 ops/ciclo. À 1050 MHz → ~800 GOPS. Matmul 1024×4096 → ~5μs. Mais lento que NVIDIA mas ainda **100× melhor que CPU**.

### 4.4 Empacotamento ternário para GPU

Nosso formato atual: 2-bit packing (4 pesos/byte). Para GPU, melhor desempaquetar para INT8 (1 peso/byte) — quadruplica o tamanho mas permite DP4A/DPAS:

```rust
// Em pack.rs: formato GPU-optimized
pub fn pack_for_gpu(weights: &[i8]) -> Vec<i8> {
    // weights contém {-1, 0, +1}
    // Para GPU: manter como int8 (1 byte/peso)
    // Perde compressão (4× maior) mas ganha DP4A
    weights.to_vec()  // já é int8
}

// Alternativa: manter 2-bit packing e desempacotar no shader
// Cada thread carrega 1 byte (4 pesos), extrai com shift, acumula
// Mais trabalho por thread mas 4× menos bandwidth de VRAM
```

### 4.5 Kernel matmul ternário (CUDA)

```cuda
// Matmul ternário GPU-optimized
// Pesos em int8 (±1,0), ativação em float16
// Usa DP4A para acumular 4 elementos de cada vez

__global__ void ternary_matmul_dp4a(
    const int8_t* __restrict__ W,  // pesos ternários [M, K]
    const half* __restrict__ X,    // ativação [K, N]
    float* __restrict__ Y,         // saída [M, N]
    int M, int N, int K
) {
    // Cada thread processa 1 elemento de Y
    int row = blockIdx.x * blockDim.x + threadIdx.x;
    int col = blockIdx.y * blockDim.y + threadIdx.y;
    if (row >= M || col >= N) return;

    float sum = 0.0f;
    int k = 0;

    // DP4A: 4 elementos por iteração
    for (; k + 4 <= K; k += 4) {
        int4 w_vec = *(int4*)&W[row * K + k];  // 4 × int8 de peso
        half4 x_vec = *(half4*)&X[col * K + k]; // 4 × float16 de ativação

        // Converter half4 → int4 (com escala)
        int x_int = halfs2int8(x_vec);  // helper: converte 4 halfs para 1 int32

        // DP4A: sum += w0*x0 + w1*x1 + w2*x2 + w3*x3
        asm("{ dp4a.s32.s32 %0, %1, %2, %3; }"
            : "+r"(sum) : "r"(w_vec.x), "r"(x_int), "r"(sum));
    }

    // Remainder (K % 4) — processar sequencialmente
    for (; k < K; k++) {
        float w = (float)W[row * K + k];
        float x = (float)X[col * K + k];
        sum += w * x;
    }

    Y[row * N + col] = sum;
}
```

---

## 5. Pilar G1 — Persistent Kernel GPU-Side

### 5.1 Arquitetura

```
┌────────────────────── CPU ──────────────────────┐
│                                                   │
│  publish(OP_MATMUL, ptrA, ptrB, ptrC, M, N, K)   │
│    → SPSC queue (ring.rs)                         │
│    → doorbell (store-release no doorbell reg)     │
│                                                   │
│  publish(OP_RMS_NORM, ptrX, ptrY, N, eps)         │
│    → mesma fila                                    │
│                                                   │
│  publish(OP_SOFTMAX, ptrX, ptrY, N)               │
│    → mesma fila                                    │
└───────────────────────────────────────────────────┘
                        │
                        ▼
┌────────────────────── GPU ───────────────────────┐
│                                                    │
│  persistent_worker():                              │
│    loop:                                           │
│      task = queue[atomicInc(head) % QSIZE]         │
│      if task.op == EXIT: break                     │
│      op_table[task.op](task.args)                  │
│      atomicInc(&processed)                         │
│                                                    │
│  op_table:                                         │
│    [0] = ternary_matmul                            │
│    [1] = rms_norm                                  │
│    [2] = softmax                                   │
│    [3] = silu_mul                                  │
│    [4] = rotary_embedding                          │
│    [5] = add                                       │
│    [6] = copy                                      │
│    [7] = OP_EXIT (kill worker)                     │
└────────────────────────────────────────────────────┘
```

### 5.2 Estruturas de dados

```rust
// gpu/persistent.rs — NOVO

/// Task para o kernel persistente GPU
/// Cabe em 64 bytes (1 cache line) — sem alocações
#[repr(C)]
pub struct GpuTask {
    pub op: u8,              // índice na op_table
    pub flags: u8,           // bit 0: sync após exec
    pub priority: u8,        // 0=urgente, 1=normal, 2=background
    pub _pad: u8,            // alinhamento
    pub ptr_a: PhysPtr,      // 8 bytes — endereço físico (VRAM ou RAM UC)
    pub ptr_b: PhysPtr,      // 8 bytes
    pub ptr_c: PhysPtr,      // 8 bytes
    pub dim_m: u32,          // 4 bytes
    pub dim_n: u32,          // 4 bytes
    pub dim_k: u32,          // 4 bytes
    pub extra: u32,          // 4 bytes (ex: eps pra rms_norm)
    // Total: 48 bytes — cabe em 1 cache line com sobra
}

/// Fila SPSC para tasks GPU — baseada em ring.rs existente
pub struct PersistentQueue {
    pub tasks: &'static mut [GpuTask; QUEUE_SIZE],  // em VRAM ou memória compartilhada
    pub head: AtomicU32,          // CPU escreve, GPU lê
    pub commit: AtomicU32,        // CPU: último task escrito (store-release)
    pub processed: AtomicU32,     // GPU: tasks completados
    pub doorbell: PhysPtr,        // endereço do doorbell register
}

impl PersistentQueue {
    /// CPU side: publicar task
    pub fn enqueue(&self, task: GpuTask) -> Result<(), &'static str> {
        let c = self.commit.load(Ordering::Relaxed);
        let p = self.processed.load(Ordering::Acquire);
        if c - p >= QUEUE_SIZE as u32 {
            return Err("GPU queue full");  // backpressure
        }
        self.tasks[c as usize % QUEUE_SIZE] = task;
        self.commit.store(c + 1, Ordering::Release);
        // Doorbell: GPU acorda
        write_volatile(self.doorbell, 1);
        Ok(())
    }
}
```

### 5.3 GPU-side worker (CUDA)

```cuda
// Emitido como PTX via cuModuleLoadData — não precisa de compilação offline
extern "C" __global__ void persistent_worker(
    PersistentQueue* queue,
    OpFn* op_table
) {
    while (true) {
        unsigned int h = atomicAdd(&queue->head, 1) & (QUEUE_SIZE - 1);
        GpuTask t = queue->tasks[h];
        if (t.op == OP_EXIT) break;

        // Executar operação via jump table
        op_table[t.op](t);

        __threadfence();
        atomicAdd(&queue->processed, 1);
    }
}
```

### 5.4 GPU-side worker (Intel GEN)

Para Intel iGPU, o worker loop é implementado como MEDIA_OBJECT que roda no ring buffer existente:

```rust
// gpu/intel.rs — modificação
// Em vez de stub, gerar GEN binary com o worker loop

pub fn launch_persistent_worker(queue: &PersistentQueue) -> Result<(), &'static str> {
    // 1. Alocar 2 páginas em UMA (memória compartilhada CPU-GPU)
    // 2. Copiar kernel worker (GEN ISA) para a memória
    // 3. Submeter MEDIA_OBJECT via ring buffer (já implementado)
    // 4. Worker fica rodando — CPU enfileira tasks via SPSC
    self.submit_media_object(worker_gpr0, worker_isa_addr);
    Ok(())
}
```

---

## 6. Pilar G2 — Tensor Ops na GPU

### 6.1 Catálogo de operações

| Op | Parâmetros | Kernel dim | Ciclos estimados (GTX 1050) | Prioridade |
|----|-----------|-----------|---------------------------|-----------|
| `ternary_matmul` | M, N, K (interleave) | 16×16 threads, tile 64×64 | 4.3μs (1024×4096) | **P0** |
| `rms_norm` | N, eps | 256 threads, 1 warp | 0.5μs (1024) | **P0** |
| `softmax` | N | 256 threads, warp reduce | 0.8μs (1024) | **P0** |
| `silu_mul` | N | 256 threads elementwise | 0.2μs (4096) | **P0** |
| `rotary_embedding` | N, pos, theta | 256 threads | 0.3μs (1024) | **P0** |
| `add` | N | 256 threads | 0.1μs (1024) | P1 |
| `copy` | N | 256 threads | 0.05μs (1024) | P1 |
| `attention` | N, num_heads | 16×16 threads + shared mem | ~3μs (1024, 8 heads) | P2 |
| `fused_qkv` | N, hidden, n_heads | 3 matmuls em 1 kernel | ~8μs | P2 |
| `fused_ffn` | N, hidden, ffn_hidden | silu(gate×up)×down em 1 kernel | ~10μs | P2 |

### 6.2 Kernel rms_norm

```cuda
// RMS Normalization: y = x / sqrt(mean(x^2) + eps) * weight
__global__ void rms_norm_kernel(
    const float* __restrict__ x,
    const float* __restrict__ weight,
    float* __restrict__ y,
    int N, float eps
) {
    // Warp-level reduce para soma de quadrados
    float sum = 0.0f;
    int tid = threadIdx.x;
    for (int i = tid; i < N; i += blockDim.x) {
        sum += x[i] * x[i];
    }

    // Warp shuffle reduce (__shfl_xor_sync)
    for (int offset = warpSize / 2; offset > 0; offset /= 2) {
        sum += __shfl_xor_sync(0xFFFFFFFF, sum, offset);
    }

    if (tid == 0) {
        sum = rsqrtf(sum / N + eps);  // inverso da RMS
    }
    float rms = __shfl_sync(0xFFFFFFFF, sum, 0);

    for (int i = tid; i < N; i += blockDim.x) {
        y[i] = x[i] * rms * weight[i];
    }
}
```

### 6.3 Kernel softmax

```cuda
// Softmax: y_i = exp(x_i - max) / sum(exp(x_j - max))
__global__ void softmax_kernel(float* __restrict__ x, int N) {
    // 1. Find max
    float max_val = -INFINITY;
    for (int i = threadIdx.x; i < N; i += blockDim.x) {
        max_val = fmaxf(max_val, x[i]);
    }
    // Warp reduce max
    for (int offset = warpSize / 2; offset > 0; offset /= 2) {
        max_val = fmaxf(max_val, __shfl_xor_sync(0xFFFFFFFF, max_val, offset));
    }
    max_val = __shfl_sync(0xFFFFFFFF, max_val, 0);

    // 2. Sum exp(x - max)
    float sum = 0.0f;
    for (int i = threadIdx.x; i < N; i += blockDim.x) {
        sum += expf(x[i] - max_val);
    }
    // Warp reduce sum
    for (int offset = warpSize / 2; offset > 0; offset /= 2) {
        sum += __shfl_xor_sync(0xFFFFFFFF, sum, offset);
    }

    // 3. Normalize
    sum = __shfl_sync(0xFFFFFFFF, sum, 0);
    for (int i = threadIdx.x; i < N; i += blockDim.x) {
        x[i] = expf(x[i] - max_val) / sum;
    }
}
```

### 6.4 Integração com persistent kernel

```rust
// gpu/ops.rs — NOVO
// Implementação das operações como callbacks do persistent kernel

impl PersistentQueue {
    pub fn matmul(&self, a: PhysPtr, b: PhysPtr, c: PhysPtr, m: u32, n: u32, k: u32) {
        self.enqueue(GpuTask {
            op: OP_MATMUL,
            ptr_a: a, ptr_b: b, ptr_c: c,
            dim_m: m, dim_n: n, dim_k: k,
            ..GpuTask::default()
        }).ok();
    }

    pub fn rms_norm(&self, x: PhysPtr, w: PhysPtr, y: PhysPtr, n: u32, eps: f32) {
        self.enqueue(GpuTask {
            op: OP_RMS_NORM,
            ptr_a: x, ptr_b: w, ptr_c: y,
            dim_m: n, extra: eps.to_bits(),
            ..GpuTask::default()
        }).ok();
    }

    pub fn softmax(&self, x: PhysPtr, n: u32) {
        self.enqueue(GpuTask {
            op: OP_SOFTMAX,
            ptr_a: x, dim_m: n,
            ..GpuTask::default()
        }).ok();
    }
}

// Aguardar conclusão (sync point)
pub fn sync(&self) {
    while self.processed.load(Ordering::Acquire) < self.commit.load(Ordering::Relaxed) {
        core::hint::spin_loop();
    }
}
```

---

## 7. Pilar G3 — SASOS: Espaço de Endereço Unificado RAM+VRAM

### 7.1 Estado atual

```
RAM:     0x_0000_0000_0000 — 0x_003F_FFFF_FFFF  (256GB virtual)
Heap:    0x_4000_0000_0000 — 0x_4020_0000_0000  (128GB, só RAM)
VRAM:    0x_C000_0000      — 0xC800_0000        (BAR2, mapeado UC em addr separado)
NVMe:    sem mapeamento direto
```

### 7.2 Proposta SASOS

```
RAM:      0x_0000_0000_0000 — 0x_003F_FFFF_FFFF  (256GB, cacheable)
Heap RAM: 0x_4000_0000_0000 — 0x_4020_0000_0000  (128GB, páginas WB)
VRAM:     0x_4020_0000_0000 — 0x_4040_0000_0000  (128GB, páginas UC → BAR2)
NVMe:     0x_4040_0000_0000 — 0x_4060_0000_0000  (128GB, páginas UC → NVMe BAR)

Heap total: 384GB — RAM + VRAM + NVMe no mesmo range contíguo
```

> **Reconciliação com ADR-0087 (2026-08-06):** o SASOS aqui (VRAM no heap, UC) e o
> Copy Engine da ADR-0087 §2.0.1 **não são concorrentes — são complementares**.
> SASOS = acesso pontual/aleatório por ponteiro (KV pages, tensores < 1MB);
> CE/SDMA/BCS = transfers bulk via engine (pesos 792MB, migração de tier).
> O dono do Tier 0 é a **ADR-0087** (Fase 4a = SASOS, 4b = CE); este ADR define
> o layout SASOS e o consumo (`Tensor::location = MemTier::Vram`).

### 7.3 Modificações no page table

```rust
// memory.rs — modificação

/// Inicializar SASOS: mapear VRAM e NVMe no espaço do heap
pub fn init_sasos(vram_base: PhysAddr, vram_size: usize) -> Result<(), &'static str> {
    let heap_vram_base = VirtAddr::new(0x_4020_0000_0000);  // logo após heap RAM

    // Mapear VRAM BAR2 como UC (uncacheable — coerência DMA)
    let num_pages = (vram_size + 0xFFF) / 0x1000;
    for i in 0..num_pages {
        let vaddr = heap_vram_base + i * 0x1000;
        let paddr = vram_base + i * 0x1000;
        // UC = PAT bit 0 (no cache)
        map_page(vaddr, paddr, PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE)?;
    }

    // VramBuddy agora opera no espaço virtual do heap
    VRAM_BUDDY.lock().init_virtual(heap_vram_base, vram_size);

    serial_println!("[SASOS] VRAM mapeada: {:?} → {:?} ({} MB)",
        vram_base, heap_vram_base, vram_size / 1024 / 1024);
    Ok(())
}
```

### 7.4 Impacto no Tensor

```rust
// cortex/src/tensor.rs — modificação

pub struct Tensor {
    pub shape: (usize, usize),
    pub data: Vec<f32>,     // ← PODE ESTAR NA VRAM!
    pub location: MemTier,  // NOVO: Ram | Vram | Nvme
}

impl Tensor {
    // Alocar tensor na VRAM se couber
    pub fn new_vram(shape: (usize, usize)) -> Option<Self> {
        let bytes = shape.0 * shape.1 * 4;  // f32
        let ptr = vram_alloc(bytes)?;        // retorna ponteiro no espaço SASOS
        Some(Tensor {
            shape,
            data: unsafe { Vec::from_raw_parts(ptr as *mut f32, len, len) },
            location: MemTier::Vram,
        })
    }

    // Matmul: se ambos os operandos estão na VRAM, executa na GPU
    pub fn matmul(&self, other: &Tensor) -> Option<Tensor> {
        if self.location == MemTier::Vram && other.location == MemTier::Vram {
            // GPU matmul via persistent kernel
            let result = Tensor::new_vram((self.shape.0, other.shape.1))?;
            GPU_QUEUE.matmul(
                self.phys_ptr(), other.phys_ptr(), result.phys_ptr(),
                self.shape.0 as u32, other.shape.1 as u32, self.shape.1 as u32
            );
            return Some(result);
        }
        // Fallback CPU
        self.matmul_cpu(other)
    }
}
```

**Consequência**: `Tensor { data: Vec<f32> }` funciona igual, mas se o ponteiro cai na VRAM, o matmul executa na GPU. **Transparente para o código existente.** Zero mudanças nos callers.

---

## 8. Pilar G4 — H2O KV Cache Folding + PagedAttention

### 8.1 Arquitetura

```
KV Cache (completo, em RAM)
    │
    ▼
H2O Scoring: para cada página KV, calcular importance score
    │ score = Σ attention_weights daquela página
    │
    ▼
Ranking: ordenar páginas por score
    │
    ├─ Top K (working set) → VRAM (via kv_dma ou SASOS)
    │
    └─ Bottom N-K → RAM (disponível para evicção)
        │
        ▼
        Se precisa de mais VRAM → fold (descartar) bottom páginas
```

### 8.2 H2O Score

```rust
// gpu/h2o.rs — NOVO

/// Heavy Hitter Oracle: identifica páginas KV importantes
pub struct H2O {
    num_pages: usize,
    page_size: usize,       // 16KB (256 tokens × 2 layers × 32 dimensões)
    scores: Vec<f32>,       // scores por página
    threshold: f32,         // mínimo para manter em VRAM
}

impl H2O {
    /// Calcular score de importância para cada página
    /// Score = soma cumulativa dos pesos de atenção que referenciam esta página
    pub fn score_pages(&mut self, attention_weights: &[f32], seq_len: usize) {
        for i in 0..seq_len {
            let page = i * 4 / self.page_size;  // cada token ≈ 4 layers
            let score = attention_weights[i];    // peso de atenção acumulado
            self.scores[page] += score;
        }
    }

    /// Decidir quais páginas ficam na VRAM
    pub fn eviction_plan(&self) -> EvictionPlan {
        let mut ranked: Vec<(usize, f32)> = self.scores.iter().copied()
            .enumerate().collect();
        ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let vram_budget = VRAM_CACHE_SIZE / self.page_size;
        let keep: Vec<usize> = ranked.iter()
            .rev()
            .take(vram_budget)
            .map(|(i, _)| *i)
            .collect();

        EvictionPlan {
            keep_in_vram: keep,
            fold_from_vram: ranked.iter()
                .take(self.num_pages - vram_budget)
                .map(|(i, _)| *i)
                .collect(),
        }
    }
}
```

### 8.3 PagedAttention

```rust
pub struct PagedCache {
    pages: Vec<KvPage>,        // todas as páginas (RAM)
    vram_pages: Vec<PhysPtr>,  // working set (VRAM)
    page_table: BTreeMap<usize, usize>, // token_position → page_id
    h2o: H2O,
}

impl PagedCache {
    pub fn access(&mut self, token_pos: usize) -> &[f32] {
        let page_id = self.page_table[&token_pos];
        let page = &self.pages[page_id];

        // Se está na VRAM, ler de lá (SASOS: ponteiro unificado)
        if self.vram_pages.contains(&PhysPtr::new(page.vram_addr())) {
            return page.vram_slice();
        }

        // Se não está na VRAM, trazer (page fault style)
        self.promote_to_vram(page_id);
        page.vram_slice()
    }

    /// Promover página da RAM para VRAM
    /// Se VRAM cheia, evict usando H2O scoring
    fn promote_to_vram(&mut self, page_id: usize) {
        if self.vram_pages.len() >= VRAM_CACHE_SIZE / PAGE_SIZE {
            let plan = self.h2o.eviction_plan();
            for evict in &plan.fold_from_vram {
                self.evict_from_vram(*evict);
            }
        }
        let page = &self.pages[page_id];
        kv_dma_transfer(page.ram_addr(), page.vram_addr(), PAGE_SIZE, CpuToGpu);
        self.vram_pages.push(PhysPtr::new(page.vram_addr()));
    }
}
```

### 8.4 Integração com MSched existente

O `msched.rs` já monitora padrões de acesso com Belady/OPT predictor. Podemos usar o score do H2O como feature extra:

```rust
// gpu/msched.rs — modificação

impl Msched {
    pub fn predict_working_set(&self, attention_weights: &[f32]) -> WorkingSet {
        let h2o_scores = H2O::score_from_attention(attention_weights);
        let belady_scores = self.belady_predict();

        // Fusão dos dois predictores
        let combined: Vec<f32> = h2o_scores.iter()
            .zip(belady_scores.iter())
            .map(|(h, b)| h * 0.7 + b * 0.3)
            .collect();

        WorkingSet::from_scores(&combined, VRAM_BUDGET)
    }
}
```

---

## 9. Pilar G5 — GPU Inference Pipeline Completo

### 9.1 Pipeline antes × depois

```
ANTES (CPU-only):
  token → embed → matmul×24 (+ softmax + rms_norm + silu) → sample
  Cada step: ~5ms
  KV cache: DMA para VRAM (só armazenamento)

DEPOIS (GPU-accelerated):
  token → CPU embed (emb_lookup é barato, fica na CPU)
           │
           ▼
         persistent queue: publish(OP_FUSED_QKV, h, Wq, Wk, Wv)
         persistent queue: publish(OP_ATTENTION, q, k, v, cache)
         persistent queue: publish(OP_FUSED_FFN, h, Wgate, Wup, Wdown)
         persistent queue: publish(OP_RMS_NORM, h, weight, h_norm)
         persistent queue: publish(OP_ROTARY, qk, pos, theta)
           │
         sync()  →  ~50μs total
           │
           ▼
  CPU sample (argmax/sampling)
```

### 9.2 Modificação no CortexAgent

```rust
// cortex/src/cortex.rs — modificação

impl BitNetModel {
    /// Forward com GPU acceleration
    pub fn forward_gpu(&self, tokens: &[u16], cache: &mut KVCache) -> (Tensor, Tensor) {
        if !GPU_AVAILABLE {
            return self.forward_cpu(tokens, cache);
        }

        let mut h = self.embed_lookup_gpu(tokens);  // embed → VRAM

        for (i, layer) in self.layers.iter().enumerate() {
            // QKV projection na GPU
            let q = GPU_QUEUE.matmul(&h, &layer.wq);
            let k = GPU_QUEUE.matmul(&h, &layer.wk);
            let v = GPU_QUEUE.matmul(&h, &layer.wv);

            // Rotary embedding
            let q_rot = GPU_QUEUE.rotary(&q, cache.seq_len, self.rope_theta);
            let k_rot = GPU_QUEUE.rotary(&k, cache.seq_len, self.rope_theta);

            // Atenção na GPU
            let attn_out = GPU_QUEUE.attention(&q_rot, &k_rot, &v, &cache.pages[i]);

            // FFN na GPU (gate + up + silu + down fusionado)
            let ffn_out = GPU_QUEUE.fused_ffn(&attn_out, &layer.wgate, &layer.wup, &layer.wdown);

            // Residual + rms_norm na GPU
            h = GPU_QUEUE.add(&h, &ffn_out);
            h = GPU_QUEUE.rms_norm(&h, &self.rms_layers[i]);
        }

        // Final norm + lm_head
        h = GPU_QUEUE.rms_norm(&h, &self.rms_final);
        let logits = GPU_QUEUE.matmul(&h, &self.lm_head);

        GPU_QUEUE.sync();  // aguarda tudo

        (h.to_cpu(), logits.to_cpu())
    }
}
```

### 9.3 Double-buffered prefetch (NMOS pattern)

Para modelos maiores que a VRAM (ex: BitNet 2B = 202MB, VRAM GTX 1050 = 2GB → cabe inteiro):

```rust
pub struct PrefetchPipeline {
    current_layer: usize,
    next_layer_load: Option<PhysPtr>,  // próxima layer já carregando
}

impl PrefetchPipeline {
    /// Iniciar prefetch da próxima layer enquanto a atual processa
    pub fn prefetch_next(&mut self, layer_idx: usize, weights_addr: PhysPtr) {
        if let Some(pending) = self.next_layer_load.take() {
            // Aguardar loading anterior (já deve ter terminado)
            pending.wait();
        }
        // Iniciar DMA da próxima layer: SSD→RAM→VRAM
        self.next_layer_load = Some(dma_chain(
            weights_addr,              // VRAM destino
            layer_weights_ssd(layer_idx + 1),  // SSD origem
            LAYER_SIZE,                // ~8MB por layer
        ));
    }
}
```

### 9.4 Latência estimada

| Operação | CPU (FP32, 1 core @ 2.5GHz) | GPU (GTX 1050, DP4A) | GPU (Intel Gen9, MAD) |
|----------|----------------------------|---------------------|----------------------|
| Embed lookup | 0.1μs | 0.1μs (CPU) | 0.1μs (CPU) |
| QKV matmul (3× 1024×1024) | 150μs | 4μs (3× DP4A) | 12μs |
| Attention (8 heads, S=1024) | 200μs | 3μs (shared mem) | 15μs |
| FFN (silu(gate×up)×down) | 300μs | 8μs (fused) | 25μs |
| RMS norm (2×) | 10μs | 0.5μs | 1μs |
| Rotary embedding | 5μs | 0.3μs | 0.5μs |
| **Total por layer** | **~665μs** | **~16μs** | **~54μs** |
| **24 layers** | **~16ms** | **~384μs** | **~1.3ms** |
| **Token (incl. lm_head)** | **~5ms** | **~50μs** | **~200μs** |

**Ganho**: **100×** (NVIDIA) ou **25×** (Intel iGPU) sobre CPU.

---

## 10. Roteiro de Implementação

### 10.1 Fases

```
Fase 0 — Preparação (Sprint 109, ~200 LOC)
├── gpu/persistent.rs      — Estruturas de dados (GpuTask, PersistentQueue)
├── gpu/ops.rs             — Catálogo de operações (enum GpuOp)
├── gpu/h2o.rs             — H2O scorer + eviction plan
└── memory.rs              — init_sasos() mapeamento VRAM

Fase 1 — SASOS + Persistent Kernel (Sprint 109-110, ~700 LOC)
├── memory.rs              — SASOS: VRAM no heap (100 LOC)
├── vram.rs                — VramBuddy usa espaço virtual SASOS (50 LOC)
├── tensor.rs              — Tensor::new_vram(), Tensor::location (100 LOC)
├── persistent.rs          — enqueue() + sync() (200 LOC)
├── nvidia.rs              — launch_persistent_worker() via PFIFO (150 LOC)
└── intel.rs               — launch_persistent_worker() via GEN ring (100 LOC)

Fase 2 — Tensor Ops Core (Sprint 110-111, ~800 LOC)
├── kernels/matmul.cu       — ternary_matmul_dp4a (150 LOC PTX)
├── kernels/rms_norm.cu     — rms_norm_kernel (80 LOC PTX)
├── kernels/softmax.cu      — softmax_kernel (80 LOC PTX)
├── kernels/silu_mul.cu     — silu_mul_kernel (50 LOC PTX)
├── kernels/rotary.cu       — rotary_embedding (100 LOC PTX)
├── kernels/add.cu          — add_kernel (30 LOC PTX)
├── gpu/loader.rs           — cuModuleLoad dos kernels (200 LOC)
└── gpu/op_table.rs         — Jump table GPU-side (100 LOC)

Fase 3 — Attention + FFN Fusion (Sprint 111-112, ~500 LOC)
├── kernels/fused_qkv.cu    — QKV num kernel só (200 LOC PTX)
├── kernels/attention.cu    — Attention com shared memory (200 LOC PTX)
├── kernels/fused_ffn.cu    — gate+up+silu+down fusionado (150 LOC PTX)
└── gpu/fusion.rs           — Pipeline builder (100 LOC)

Fase 4 — KV Cache + Prefetch (Sprint 112-113, ~500 LOC)
├── gpu/paged_cache.rs      — PagedAttention + H2O (300 LOC)
├── gpu/prefetch.rs         — Double-buffered SSD→RAM→VRAM (150 LOC)
└── gpu/msched.rs           — MSched neural + H2O integrados (100 LOC)

Fase 5 — Inference Pipeline (Sprint 113-114, ~500 LOC)
├── cortex/cortex.rs        — forward_gpu() pipeline (200 LOC)
├── gpu/backend.rs          — GPU backend completo (200 LOC)
└── gpu/bench.rs            — Benchmark funcional (100 LOC)
```

### 10.2 Total: ~3.200 LOC

### 10.3 Marcos

| Marco | Sprint | O quê | Critério de aceite |
|-------|--------|-------|-------------------|
| M-G1 | 109 | SASOS: VRAM no heap | `vram_alloc(1MB)` retorna ptr virtual, CPU lê |
| M-G2 | 109 | H2O scoring | Score de atenção identifica páginas importantes |
| M-G3 | 110 | Persistent kernel launch | GPU processa 1 task (add) e retorna |
| M-G4 | 110 | Matmul ternário GPU | `matmul(1024,1024)` GPU = resultado idêntico CPU |
| M-G5 | 111 | RMS norm + softmax GPU | Forward parcial de 1 layer na GPU |
| M-G6 | 111 | Attention GPU | Attention match CPU |
| M-G7 | 112 | Fused FFN GPU | FFN match CPU |
| M-G8 | 112 | PagedAttention + H2O | KV cache 4× compressão sem perda |
| M-G9 | 113 | Forward 24 layers GPU | Geração de token >100× mais rápida que CPU |
| M-G10 | 114 | Pipeline completo | Teste clima e2e com GPU acceleration |

---

## 11. Referências

### 11.1 Projetos-analisados

1. **GPUOS** — Yang, Y. et al. (2026). *GPUOS: A GPU Operating System Primitive for Transparent Operation Fusion*. arXiv:2604.17861. https://github.com/Multi-V-VM/GPUOS/
   - Persistent kernel, JIT operator injection, 15.3× speedup

2. **neurOS** — Price, R. (2026). *neurOS: GPU-Native Neural Operating System*. https://github.com/robertcprice/nCPU
   - Todos subsistemas OS como redes neurais na GPU

3. **NMOS** — Pankaj, A. (2026). *Neural Memory Operating System*. https://github.com/AlfaPankaj/Neural_Memory_Operating_system
   - 70B em 4GB VRAM, H2O folding, double-buffered prefetch

4. **TensorOS / NeuroLang** — Inphinie (2026). *NeuroLang_TensorOS*. https://github.com/Inphinie/NeuroLang_TensorOS
   - SASOS exokernel, espaço unificado RAM+VRAM+NVMe

5. **LithOS** (2025). *LithOS: An Operating System for Efficient Machine Learning on GPUs*. arXiv:2504.15465
   - TPC scheduler, kernel atomization, TPC stealing

6. **CubeCL** — cubical tech (2026). 20 GPU patterns. ADR-0043.
   - Comptime, autotune, graph capture, warp-level ops

7. **NeuroOS** — 3scud3r0 (2026). *NeuroOS*. https://github.com/3scud3r0/NeuroOS
   - Single-file: GGUF, MCTS, Dynamic LoRA, HydraMoE

8. **Yantra / Sutra** — Leonhart, E. (2026). *Yantra: A Neuro-Symbolic, GPU-Native OS*. clawRxiv:2605.02611. https://github.com/EmmaLeonhart/Sutra
   - Axon-based IPC na GPU, tudo tensor-op graph

9. **ZYO** (2026). https://github.com/thesnmc/ZYO
   - RL scheduler + LLM hot-swap via eBPF (Linux, mas conceito aplicável)

10. **Maya** — Kolaparthi, J. S. (2026). *Maya: An AI-Native OS Kernel*. Zenodo:10.5281/zenodo.19218503
    - Scheduler PPO, anomaly detector em I/O, 109ns IPC

11. **Alok** (2026). *N-gram speculative decoding in llama.cpp*. https://x.com/analogalok/status/2077718647905333549
    - Rolling LCG hash, O(1) lookup, draft M tokens, GPU verify. 2× speedup, zero VRAM extra

### 11.2 Documentação interna

11. ADR-0029 — GPU Architecture (detecção, ring buffer, PFIFO)
12. ADR-0037 — SMP+GPU Architecture (XPU, KV DMA, MSched)
13. ADR-0041 — Capability Rings P5 (DMA pin)
14. ADR-0043 — CubeCL 20 GPU Patterns
15. ADR-0047 — LatentBus + EvolveAgent + NeuOS Probe

### 11.3 Referências técnicas

16. Intel Gen9+ GPGPU documentation — https://01.org/linuxgraphics/documentation
17. envydiss / open-gpu-doc — NVIDIA PFIFO + PUSH_BUFFER ISA
18. NVIDIA CUDA DP4A intrinsic — `__dp4a` in CUDA Math API
19. FlashAttention — Dao et al. (2022). *FlashAttention: Fast and Memory-Efficient Exact Attention*
20. vLLM — Kwon et al. (2023). *Efficient Memory Management for Large Language Model Serving with PagedAttention*
21. H2O — Zhang et al. (2023). *H2O: Heavy-Hitter Oracle for Efficient Generative Inference of Large Language Models*

---

## 12. Riscos e Mitigações

### 12.1 Técnicos

| Risco | Probabilidade | Impacto | Mitigação |
|-------|--------------|---------|-----------|
| **DP4A não disponível na GTX 1050**: sm_61 pode não suportar | Baixa | Alto | GTX 1050 suporta DP4A via `__dp4a` (compute capability 6.1). Verificado no CUDA Programming Guide. Alternativa: INT8 mul+add manual |
| **PFIFO channel não permite compute arbitrário**: só PUSH_BUFFER, não kernel launch | Média | Crítico | PFIFO pode submeter métodos de objeto (0x80XX). O método 0x8060 (COMPUTE) está documentado no envydiss. CUDA driver API é caminho alternativo |
| **Intel Gen9 sem DPAS**: só MAD | Alta | Médio | MAD INT8 ainda dá ~800 GOPS. Suficiente para matmul ternário |
| **Overhead de sincronização GPU-CPU**: sync() espera GPU terminar | Média | Baixo | Pipeline batches minimiza syncs. Uma sync por token (não por layer) |
| **VRAM insuficiente (2GB)**: modelo 202MB + KV cache 1GB + tensores temporários | Média | Médio | H2O folding reduz KV cache. Matmul ternário empacotado reduz peso. Se exceder, fallback CPU parcial |

### 12.2 Arquiteturais

| Risco | Descrição | Mitigação |
|-------|-----------|-----------|
| **GPU não presente**: QEMU/VirtIO sem GPU real | QEMU sem GPU real rodando | Fallback CPU mantido. `GPU_AVAILABLE = false` → pipeline existente |
| **NDA NVIDIA para compute completo**: PFIFO pode não cobrir todos os casos | Usar CUDA driver API (`cudaLaunchKernel`) como alternativa. PFIFO = método direto, CUDA = mais compatível |
| **Manutenção de dois backends (NVIDIA + Intel)** | Abstrair atrás de trait `GpuBackend` com impls NVIDIA e Intel. ~100 LOC de trait, ~500 LOC cada impl |
| **Determinismo GPU vs CPU**: float32 GPU pode ter pequenas diferenças | Matmul ternário é determinístico (só ADD/SUB, sem rounding). RMS norm e softmax têm diferenças <1e-5. Aceitável para inferência |

### 12.3 Non-goals (para esta fase)

- CUDA graphs / graph capture (otimização futura)
- AMD ROCm / PM4 compute (nenhuma AMD dGPU disponível para teste)
- Multi-GPU (apenas 1 GPU ativa por vez)
- VirtIO-GPU compute (QEMU VirGL não implementa compute shader)
- GPU-native training (só inferência)
- Hot-swap de operadores GPU (GPUOS dual-slot — fase futura)

---

## Apêndice A: Código GPU vs CPU — Comparação de Formatos

| Aspecto | CPU (atual) | GPU (proposto) | Razão |
|---------|-------------|----------------|-------|
| **Packing ternário** | 2-bit (4 pesos/byte) | INT8 (1 peso/byte) | DP4A requer INT8 |
| **Ativação** | Vec<f32> | half (FP16) | 2× bandwidth, DP4A input |
| **Matmul** | ADD/SUB loop | DP4A intrinsic | 100× ganho |
| **Alocação** | heap (Vec<f32>) | VramBuddy + SASOS ptr | Zero-copy |
| **Sincronização** | N/A (inline) | PersistentQueue sync() | Pipeline |
| **KV cache** | Vec<f32> em RAM | Paged pages em VRAM | H2O compression |

## Apêndice B: Experimento de Viabilidade (Pré-Implementação)

Antes de implementar o pipeline completo, este experimento de ~100 LOC valida o caminho crítico:

```rust
// gpu/viability_test.rs

/// Teste de viabilidade: matmul ternário na GPU
/// 
/// 1. Alocar dois tensores na VRAM (SASOS)
/// 2. Submeter task de matmul via persistent queue
/// 3. Sync
/// 4. Comparar resultado com CPU matmul
pub fn test_gpu_matmul() -> Result<(), &'static str> {
    let m = 256; let n = 256; let k = 256;

    // Alocar na VRAM via SASOS
    let a = Tensor::new_vram((m, k))?;
    let b = Tensor::new_vram((k, n))?;
    let c = Tensor::new_vram((m, n))?;

    // Preencher com dados de teste
    // a: pesos ternários aleatórios {-1, 0, +1}
    // b: ativações f32 aleatórias

    // Submeter matmul na GPU
    GPU_QUEUE.matmul(a.gpu_ptr(), b.gpu_ptr(), c.gpu_ptr(), m, n, k);
    GPU_QUEUE.sync();

    // Comparar com CPU
    let c_cpu = a.matmul_cpu(&b).unwrap();
    let max_diff = c.data.iter()
        .zip(c_cpu.data.iter())
        .map(|(g, c)| (g - c).abs())
        .fold(0.0f32, f32::max);

    if max_diff > 1e-3 {
        Err("GPU matmul divergiu do CPU")
    } else {
        serial_println!("[GPU_TEST] Matmul OK: max_diff = {}", max_diff);
        Ok(())
    }
}
```

**Critério de aceite**: `max_diff < 1e-3` para matmul ternário com entradas aleatórias. Se falhar, a abordagem DP4A precisa de revisão.

---

*Fim do ADR-0047-GPU*
