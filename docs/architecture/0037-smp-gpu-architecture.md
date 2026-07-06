# ADR-0037: SMP + GPU Architecture — Arquitetura Genérica Multiplataforma

**Data:** 2026-07-05 (v4 — arquitetura genérica multiplataforma)
**Status:** Draft — Em Análise
**Substitui:** ADR-0029 (GPU Architecture) + v3 deste documento
**Depende de:** ADR-0014 (Ideias de Hardware), ADR-0031 (AIOS Evolution)
**Sprint Target:** N (SPSC + IPI) até N+3/4 (GPU Compute)

---

## 1. Contexto

### 1.1 Visão Arquitetural

O neural-os-core deve ser um **sistema operacional AI-native multiplataforma** capaz de:
- Suportar **qualquer arquitetura de processador** (x86-64, ARM64, RISC-V)
- Suportar **qualquer GPU** (NVIDIA, AMD, Intel, Apple Silicon)
- Suportar **qualquer NPU** (AMD XDNA, Intel NPU, Apple ANE)
- Detectar e inicializar hardware automaticamente
- Abstrair diferenças de hardware através de interfaces genéricas
- Escalar performance de forma adaptativa baseada no hardware disponível
- QEMU/VBox são ambientes de **desenvolvimento e debug** apenas
- **Hardware real** é o critério de aceite para performance

### 1.2 Estado Atual

O kernel neural-os-core atualmente:
- BSP + APs via INIT-SIPI-SIPI funcional (`smp/mod.rs`)
- APs entram em `loop { hlt() }` — nunca recebem trabalho
- AgentScheduler single-threaded round-robin no core 0
- Todos os forward passes LLM rodam em 1 core
- GPU: apenas VirtIO-GPU framebuffer (sem compute)
- AVX2 sob WHPX: 2x MAIS LENTO que scalar (VEX = VM exits)

### 1.3 Problema Central

O sistema atual é **hard-coded para hardware específico** e não tem abstrações genéricas para:
- Detecção automática de processadores (CPUID, MIDR, etc.)
- Detecção automática de GPUs (PCI device IDs, capability bits)
- Detecção automática de NPUs (ACPI tables, device IDs)
- Drivers genéricos e modulares para diferentes hardwares
- Escalabilidade adaptativa (1 core → N cores → GPU → NPU)

**Objetivo:** Criar uma arquitetura genérica que suporte qualquer processador, GPU e NPU, com drivers modulares e detecção automática.

---

## 2. Pesquisa Expandida por Categoria

### 2.1 SMP / Work-Stealing Schedulers

| Fonte | Aderência | Dificuldade | Melhora | Dependências |
|---|---|---|---|---|
| **fast-steal** (crates.io 6.5.4, no_std) | ★★★★ | Baixa (~200 LOC wrapper) | Work-stealing queue pronto, testado, 27k downloads | Precisa parking_lot + portable-atomic — verificar compatibilidade no_std puro |
| **bbqueue** (elodin-sys, no_std SPSC) | ★★★★★ | Baixa (~100 LOC) | DMA-safe SPSC lockless, baseado em BipBuffer, sem CAS em alguns targets | Nenhuma. Já temos DMA pages UC. Ideal para IRQ→task data path |
| **st3** (asynchronics, no_std) | ★★★ | Média (~300 LOC adaptação) | Bounded work-stealing FIFO/LIFO, lock-free, comprovado formalmente | Precisa de allocator para Box |
| **echOS-x64 CFS/RT + SMP** | ★★★ | Alta (portar ~2000 LOC) | Referência de implementação: CFS, deadline, work-stealing + SMP AP | Arquitetura diferente (UEFI, Limine). Inspiração, não copiar |
| **moss-kernel EEVDF + IPI** | ★★★ | Média (~500 LOC) | Task migration via IPI, per-CPU slab cache. Algoritmo EEVDF testado com 105 syscalls | Foco AArch64. Conceitos portáveis |
| **veda-rs adaptive scheduling** | ★★★ | Média (~400 LOC) | Adaptive scheduling com feedback loop, telemetry, deterministic mode, GPU support | Depende de crossbeam (std). Podemos portar só o algoritmo |

**Recomendado:** bbqueue para comunicação cross-core imediata + scheduler work-stealing custom baseado no padrão Chase-Lev (200 LOC, padrão bem conhecido). fast-steal se for compatível no_std.

### 2.2 GPU Compute Bare Metal

| Fonte | Aderência | Dificuldade | Melhora | Dependências |
|---|---|---|---|---|
| **coconutOS** (GPUs infra) | ★★★★★ | 2-3 sprints (~1500 LOC) | A referência mais direta. Supervisor ~5K LOC, shards isolados por IOMMU, syscall GPU DMA, FXSAVE/FXRSTOR para preservar FPU entre shards. Já roda transformer inference shard em QEMU | Precisa IOMMU (VT-d) que não temos. Precisamos de pelo menos 1 sprint de VT-d antes |
| **nova-core** (NVIDIA Rust driver) | ★★★★★ | 3-4 sprints (~3000 LOC) | Código oficial NVIDIA Rust para GPU. BAR1 management, GPU MMIO, user-space doorbells, BAR0 uncacheable access, sysmem flush. RTX 1050 é um exemplo — design genérico para GPUs NVIDIA Pascal+ | Kernel Linux - precisamos portar os conceitos. BAR1 mapping já temos parcialmente (cache UC) |
| **gpu-nvme-direct** | ★★★ | 2 sprints (~1000 LOC) | GPU faz NVMe READ/WRITE direto via BAR0 MMIO. 2.1 GB/s sustentado. CPU fica fora do data path | Precisa GPU BAR0 mapping funcional + NVMe driver funcionando (temos ambos parciais) |
| **monadic-hypervisor** (zero-kernel) | ★★★ | 1 sprint (~600 LOC) | Padrões de PCIe bypass, SPSC ring com alignas(64), WFE/SEV ao invés de spin. Inspiração para o ring buffer GPU-CPU | ARM64 EL2 focus. Conceitos portáveis para x86 (usar HLT/MWAIT no lugar de WFE) |

**Recomendado:** coconutOS como blueprint arquitetural + nova-core (NVIDIA), amdgpu (AMD), i915 (Intel) como referências de BAR1/MMIO. GPU compute no bare metal é viável mas exige 3-4 sprints.

### 2.3 GPU Kernel Scheduling (dentro da GPU)

| Fonte | Aderência | Dificuldade | Melhora | Dependências |
|---|---|---|---|---|
| **LithOS** (arXiv 2504.15465) | ★★★ | 4-5 sprints | TPC stealing — rouba thread blocks entre SMs igual work-stealing. Kernel atomization (quebra kernel grande em átomos). DVFS por workload. Latência de predição online | GPU-specific (NVIDIA/AMD HAL). Precisa do driver GPU primeiro. Inspiração para scheduler dentro da GPU |
| **gpu_ext** (arXiv 2512.12615) | ★★★ | 3-4 sprints | eBPF dentro da GPU — políticas de scheduling programáveis. Work-stealing thread-block scheduler. 4.8× throughput. Adaptative memory prefetch | NVIDIA open kernel modules. Verificador eBPF para GPU. Muito inovador mas imaturo |
| **XSched** (OSDI 2025) | ★★★ | 2-3 sprints | XQueue — fila de comandos preemptível. 3 níveis de preempção (pending/in-flight/running). Política agnóstica de hardware | Precisa de mecanismo de preempção GPU (Turing+ tem) |
| **MSched** (arXiv 2512.24637) | ★★★ | 1 sprint (~400 LOC) | Memory scheduling proativo — prevê working set, faz eviction ótimo (OPT/Belady) para GPU HBM. Reduz page faults em 78× | Depende de XSched para preempção. Referência para nossa gestão de VRAM 4GB |
| **Agent.xpu** (arXiv 2506.24045) | ★★★★★ | 2-3 sprints | Prefill/Decode split — NPU pra prefill, iGPU pra decode. Stage elasticity, fine-grained preemption. 1.2-4.9× throughput. Direto aplicável ao nosso BitNet | Heterogeneous SoC. Nosso caso: GPU para decode, CPU para prefill/tokenization |

**Recomendado:** XSched como mecanismo de scheduling GPU (preemptível, agnóstico). Agent.xpu como blueprint para split CPU/GPU do BitNet (CPU faz tokenization + prefill curto, GPU faz decode via KV cache).

### 2.4 DMA / Data Plane / Zero-Copy

| Fonte | Aderência | Dificuldade | Melhora | Dependências |
|---|---|---|---|---|
| **dmaplane** (arXiv 2603.10030) | ★★★ | 2 sprints (~800 LOC) | Buffer orchestration explícita: ring-based command channels, NUMA-aware allocation, credit-based flow control, GPU BAR pinning, RDMA KV cache transfer. Arquitetura de referência completa | Kernel module Linux. Portar conceitos: ring channel + credit flow control já temos base |
| **m-store** (zero-kernel RDMA) | ★★★ | 1-2 sprints (~600 LOC) | PagedAttention KV cache em NVMe — 4GB KV cache swap em 200-400ms via PCIe. Iov/Ior async API (io_uring-style) | Precisa NVMe driver (temos) + GPU BAR mapping (parcial). CXL 3.1 ignorado (não temos HW) |
| **Monadic Data Plane** (SPSC rings) | ★★★★★ | Baixa (~200 LOC) | Index caching em SPSC ring: 5.5M → 112M items/sec. alignas(64) para head/tail. DMA-BUF sharing. Já podemos usar hoje | Nenhuma. Padrão copy-paste em Rust |

**Recomendado:** Index caching + SPSC ring alignment já. dmaplane como blueprint quando tivermos GPU funcional.

### 2.5 Parallel Compute (no_std matmul/tensor)

| Fonte | Aderência | Dificuldade | Melhora | Dependências |
|---|---|---|---|---|
| **burn-flex** (tracel-ai/burn) | ★★★★★ | 1-2 sprints (~800 LOC) | no_std SIMD gemm + quantization + parallel via rayon (opcional). 2-95× speedup. Mesmas shapes do BitNet. Melhor custo-benefício | Portar só o backend CPU. Ignorar backend GPU (wgpu). Foco em avx2_ternary_matmul_impl |
| **oxionnx** (cool-japan) | ★★★ | 2 sprints (~1000 LOC) | Pure Rust ONNX inference, 165 operators, SIMD, memory pool. GPU via wgpu (opcional). Podemos carregar modelos ONNX no lugar de .bitnet | ONNX proto parser. Pesado (~2000 LOC só de parser). Alternativa futura |
| **scirs2-core** | ★★★ | 1 sprint (~300 LOC) | SIMD + work-stealing + NUMA-aware allocator. no_std feature flag. Cache-oblivious algorithms | Dependências externas (ndarray, etc). Portar só o scheduler/allocator |
| **avx_parallel** | ★★★ | Baixa (~200 LOC) | Thread pool + work stealing + SIMD ops + adaptive executor. Zero-overhead abstractions | Std-only por enquanto. Portar para no_std |
| **kofft** (FFT) | ★★★ | Baixa (~100 LOC) | no_std SIMD FFT paralelo. Não essencial agora, útil futuro para áudio/Signal | Nenhuma |

**Recomendado:** burn-flex como prioridade — portar o backend SIMD gemm + quantization elimina nossa necessidade de matmul_hybrid manual. Reduz 800 LOC de bitnet_avx2.rs.

### 2.6 Sincronização Cross-Core

| Primitiva | Onde | Aderência | Já temos? | Dif. | Ação |
|---|---|---|---|---|---|
| **TicketLock** | ticket-lock crate | ★★★★★ | Sim ✅ | — | Já funcional. Garantir alignas(64) nos dados protegidos |
| **SPSC ring (bbqueue)** | elodin-sys/bbqueue | ★★★★★ | Não | 100 LOC | Implementar agora. Ideal para IRQ→worker, GPU→CPU, core→core |
| **RCU** | echOS-x64/sync | ★★★ | Não | 300 LOC | Quando tivermos agents migrando entre cores. Evita lock em read-heavy |
| **IPI vetorizado** | apic.rs | ★★★★★ | Parcial | 100 LOC | send_ipi(lapic_id, vector) — falta implementar. Necessário para wake AP |
| **Atomic wait (UMONITOR/UMWAIT)** | x86 TSX/WAITPKG | ★★★ | Não | 50 LOC | Alternativa eficiente a HLT para idle cores. IceLake+ |
| **Cache line padding** | #[repr(align(64))] | ★★★★★ | Parcial | 10 LOC | Prevenir false sharing. Já fazemos em alguns lugares |

---

## 3. Prioridade Recomendada

| # | Ação | Sprint | LOC | Impacto |
|---|---|---|---|---|
| 1 | SPSC ring (bbqueue) + cache alignment | Atual | 100 | Base para tudo (IRQ, cross-core, GPU) |
| 2 | IPI vetorizado | Atual | 100 | Acordar APs sob demanda |
| 3 | PerCpu por AP + alocação dinâmica | N+1 | 300 | Cada core com dados próprios |
| 4 | Work-stealing scheduler (Chase-Lev) | N+1 | 400 | Distribuir agents entre 4 cores |
| 5 | Parallel-for no matmul (AVX2 chunking) | N+1 | 300 | 2-3× speedup inferência |
| 6 | GPU BAR0/BAR1 mapping + doorbell | N+2 | 500 | GPU como compute device (NVIDIA/AMD/Intel) |
| 7 | GPU job ring buffer (SPSC) | N+2 | 300 | CPU enfileira, GPU executa |
| 8 | XSched-style preemptible GPU queue | N+3 | 600 | Múltiplos workloads GPU |
| 9 | burn-flex backend port | N+3 | 800 | Gemm+SIMD pronto, elimina bitnet_avx2 manual |

---

## 4. Conclusão da Pesquisa

### 4.1 Descobertas Críticas

1. **coconutOS** (github.com/coconut-os/coconutOS) já prova que GPU-isolated AI inference em Rust no_std é viável hoje — supervisor ~5K LOC, shards com IOMMU, transformer rodando. É nosso blueprint arquitetural.

2. **nova-core** (NVIDIA Rust, código oficial) mostra como mapear BAR0/BAR1, gerenciar doorbells e submeter jobs para GPUs NVIDIA.
   **amdgpu** (Linux) documenta PM4 packet ring buffer para AMD RDNA.
   **i915** (Linux) descreve GuC/HuC firmware + ring buffer protocol para Intel Gen6+.
   A combinação destas 3 fontes cobre todo o espectro GPU.

3. **LithOS + gpu_ext** (arXiv) apontam para a fronteira real — scheduling dentro da GPU com TPC stealing e eBPF no device.

### 4.2 Plano de 9 Passos

O plano recomendado acima começa pelo SPSC ring + IPI (pode fazer agora, 200 LOC) e culmina no burn-flex backend (~3000 LOC totais distribuídos em 3 sprints).

### 4.3 HW Real como Critério de Aceite

- **Hardware real** é o critério de aceite para performance
- QEMU/VBox são ambientes de desenvolvimento e debug apenas
- Toda otimização SIMD/AVX2 deve ser avaliada em hardware real
- Emulação distorce métricas: WHPX emula VEX como VM exits, TCG não tem AVX2
- Drivers GPU validados apenas em HW real (QEMU não emula NVIDIA/AMD/Intel compute)

---

## 5. Referências

### 5.1 Projetos Open-Source (GitHub)

| Projeto | Estrelas | Linguagem | Relevância | URL |
|---|---|---|---|---|
| **coconutOS** | ~50 | Rust | ★★★★★ GPU-isolated AI inference microkernel | github.com/coconut-os/coconutOS |
| **nova-core** (NVIDIA) | — | Rust | ★★★★★ Driver NVIDIA GPU oficial em Rust | github.com/NVIDIA/open-gpu-kernel-modules |
| **echOS-x64** | ~20 | Rust | ★★★★ SMP + CFS/RT/deadline/work-stealing | github.com/asosyal04440/echOS-x64 |
| **moss-kernel** | ~500 | Rust | ★★★★ EEVDF SMP + async kernel + per-CPU slab | github.com/hexagonal-sun/moss-kernel |
| **pepita** | — | Rust | ★★★ Work-stealing Blumofe-Leiserson + scheduler | github.com/paiml/pepita |
| **Rugo** | ~100 | Rust+Go | ★★★ SMP IPI + work-stealing + tiered HW | github.com/Maxencejules/Rugo |
| **polymorph_os** | ~100 | Rust | ★★ Topological O(1) allocator, lock-free executor | github.com/joreag/polymorph_os |
| **hollow-asm** | ~50 | Rust+ASM | ★★ Topological scheduling, SIMD I/O <3ns | github.com/teerthsharma/hollow-asm |
| **monadic-hypervisor** | — | Rust | ★★★★ Zero-kernel Type-1, PCIe bypass, SPSC rings | github.com/SiliconLanguage/monadic-hypervisor |
| **m-store** | — | Rust | ★★★ Zero-kernel RDMA, PagedAttention KV swap | github.com/SiliconLanguage/m-store |
| **gpu-nvme-direct** | — | CUDA+Rust | ★★★★ GPU-initiated NVMe via BAR MMIO | github.com/xaskasdf/gpu-nvme-direct |
| **burn-flex** | (burn 20K+) | Rust | ★★★★★ no_std SIMD gemm + quantization | github.com/antimora/burn-flex |
| **oxionnx** | — | Rust | ★★★★ ONNX inference puro Rust, GPU via wgpu | github.com/cool-japan/oxionnx |
| **veda-rs** | — | Rust | ★★★★ Work-stealing + adaptive + GPU compute | github.com/TIVerse/veda-rs |

### 5.2 Artigos Acadêmicos (arXiv)

| Artigo | Ano | Relevância | Tese Central |
|---|---|---|---|
| **LithOS** (2504.15465) | 2025 | ★★★★ GPU OS: TPC stealing, kernel atomization, fine-grained scheduling |
| **gpu_ext** (2512.12615) | 2025 | ★★★★ eBPF para GPU: work-stealing thread-block, 4.8x throughput |
| **XSched** (OSDI 2025) | 2025 | ★★★★ Preemptive scheduling XPUs via XQueue, 3-level HW model |
| **MSched** (2512.24637) | 2025 | ★★★ Proactive GPU memory scheduling, OPT eviction |
| **Agent.xpu** (2506.24045) | 2025 | ★★★★★ Prefill/decode split NPU/iGPU, stage elasticity |
| **dmaplane** (2603.10030) | 2026 | ★★★★ Kernel DMA buffer orchestration, NUMA-aware, credit flow |
| **HARP** (2509.24859) | 2025 | ★★ Heterogeneous GPU cluster parallel training |
| **Orion** (EuroSys 2024) | 2024 | ★★★ Fine-grained GPU sharing, interference-aware |
| **hetGPU** (2506.15993) | 2025 | ★★★ Binary compatibility entre GPUs via IR |

### 5.3 Crates.io (no_std compatíveis)

| Crate | Versão | Downloads | no_std | Função |
|---|---|---|---|---|
| **fast-steal** | 6.5.4 | 27.188 | ✅ | Work-stealing scheduler ultra-fino |
| **bbqueue** | 0.6 | ~10K | ✅ | SPSC lockless queue (BipBuffer) |
| **st3** | — | — | ✅ | Bounded work-stealing FIFO/LIFO |
| **crossbeam-deque** | 0.8 | — | ❌ (std) | Chase-Lev work-stealing (inspiração) |
| **scirs2-core** | 0.5 | — | ✅ (feature) | SIMD + work-stealing + NUMA + GPU |
| **avx_parallel** | — | — | ❌ (std) | Thread pool + work-stealing + SIMD |
| **kofft** | 0.1.5 | — | ✅ | SIMD FFT/DSP no_std, parallel feature |

### 5.4 Documentação Técnica

| Fonte | Tipo | Conteúdo |
|---|---|---|
| NVIDIA open-gpu-kernel-modules | C/Rust | BAR0/BAR1 management, doorbell registers, GPU MMIO |
| NVIDIA Nova driver (LKML) | Rust | Novo driver GPU NVIDIA Rust para Linux kernel |
| RuVix SMP (docs.rs) | Rust | ADR-087: Per-CPU, WFE/SEV, ticket lock, cache alignment |
| Chase-Lev deque paper | Paper | Algoritmo original de work-stealing dinâmico |

---

## 6. Conclusão Final

A grande sacada: coconutOS (5K LOC) prova que GPU-isolated AI inference em Rust no_std já funciona. LithOS + gpu_ext provam que scheduling dentro da GPU é a fronteira. Nosso diferencial: unificar agentes + inferência + GPU num kernel single-address-space, sem syscall overhead, com work-stealing entre cores e dentro da GPU via adaptação de TPC stealing.

**Plano de 9 passos recomendado:**
1. SPSC ring (bbqueue) + cache alignment (100 LOC)
2. IPI vetorizado (100 LOC)
3. PerCpu por AP + alocação dinâmica (300 LOC)
4. Work-stealing scheduler (Chase-Lev) (400 LOC)
5. Parallel-for no matmul (AVX2 chunking) (300 LOC)
6. GPU BAR0/BAR1 mapping + doorbell (500 LOC)
7. GPU job ring buffer (SPSC) (300 LOC)
8. XSched-style preemptible GPU queue (600 LOC)
9. burn-flex backend port (800 LOC)

**Total: ~3000 LOC distribuídos em 3 sprints.**
