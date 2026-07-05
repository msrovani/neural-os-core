# ADR-0037: SMP + GPU Architecture — Pesquisa, Análise e Plano de Implementação

**Data:** 2026-07-05
**Status:** Draft — Em Análise
**Substitui:** ADR-0029 (GPU Architecture) — revisão completa com novas fontes e plano SMP+GPU integrado
**Depende de:** ADR-0014 (Ideias de Hardware — SMP, APIC), ADR-0031 (AIOS Evolution)
**Sprint Target:** N (SPSC + IPI) até N+3 (GPU Compute)

---

## 1. Contexto

O kernel neural-os-core atualmente:
- BSP + APs via INIT-SIPI-SIPI funcional (`smp/mod.rs`)
- APs entram em `loop { hlt() }` — nunca recebem trabalho
- AgentScheduler single-threaded round-robin no core 0
- Todos os forward passes LLM rodam em 1 core
- GPU: apenas VirtIO-GPU framebuffer (sem compute)
- AVX2 sob WHPX: 2x MAIS LENTO que scalar (VEX = VM exits)

**Problema central:** 4 cores x86-64 com AVX2 nativo + RTX 1050 4GB estão subutilizados. Forward pass BitNet b1.58 850M leva ~60s sob WHPX para 64 tokens. Em hardware real, com SMP + GPU, pode cair para <0.1s.

**Objetivo deste ADR:** Catalogar todas as ideias de pesquisa, analisar viabilidade/aderência, e produzir um sprint plano factível para transformar o kernel single-core num sistema SMP completo com aceleração GPU.

---

## 2. Fontes de Pesquisa

### 2.1 Projetos Open-Source (GitHub)

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

### 2.2 Artigos Acadêmicos (arXiv)

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

### 2.3 Crates.io (no_std compatíveis)

| Crate | Versão | Downloads | no_std | Função |
|---|---|---|---|---|
| **fast-steal** | 6.5.4 | 27.188 | ✅ | Work-stealing scheduler ultra-fino |
| **bbqueue** | 0.6 | ~10K | ✅ | SPSC lockless queue (BipBuffer) |
| **st3** | — | — | ✅ | Bounded work-stealing FIFO/LIFO |
| **crossbeam-deque** | 0.8 | — | ❌ (std) | Chase-Lev work-stealing (inspiração) |
| **scirs2-core** | 0.5 | — | ✅ (feature) | SIMD + work-stealing + NUMA + GPU |
| **avx_parallel** | — | — | ❌ (std) | Thread pool + work-stealing + SIMD |
| **kofft** | 0.1.5 | — | ✅ | SIMD FFT/DSP no_std, parallel feature |

### 2.4 Documentação Técnica

| Fonte | Tipo | Conteúdo |
|---|---|---|
| NVIDIA open-gpu-kernel-modules | C/Rust | BAR0/BAR1 management, doorbell registers, GPU MMIO |
| NVIDIA Nova driver (LKML) | Rust | Novo driver GPU NVIDIA Rust para Linux kernel |
| RuVix SMP (docs.rs) | Rust | ADR-087: Per-CPU, WFE/SEV, ticket lock, cache alignment |
| Chase-Lev deque paper | Paper | Algoritmo original de work-stealing dinâmico |

---

## 3. Análise por Categoria

### 3.1 SMP / Work-Stealing

**Ideias implementáveis:**

| Ideia | Origem | Aderência | Dificuldade | LOC | Dependências |
|---|---|---|---|---|---|
| **SPSC ring (bbqueue)** | bbqueue + monadic-hypervisor | ★★★★★ | Mínima | 100 | Nenhuma — padrão lock-free conhecido |
| **IPI vetorizado** | moss-kernel + echOS-x64 | ★★★★★ | Média | 150 | LAPIC funcional (temos) |
| **PerCpu dinâmico** | RuVix SMP + moss | ★★★★ | Média | 300 | Alocador de frames |
| **Work-stealing Chase-Lev** | crossbeam-deque + fast-steal | ★★★★ | Média | 400 | PerCpu + IPI |
| **CFS/EEVDF scheduler** | moss-kernel + echOS-x64 | ★★★ | Alta | 800 | Work-stealing base pronto |
| **RCU** | echOS-x64 + moss-kernel | ★★★ | Média | 300 | Atomics funcionais |
| **Per-CPU slab allocator** | moss-kernel | ★★★★ | Média | 300 | PerCpu pronto |

**Conexões:** SPSC ring → IPI → PerCpu → Work-stealing → CFS/EEVDF. Dependência linear: cada item requer o anterior.

**Melhoria buscada:** Forward pass passa de 1 core para 4 cores → speedup 2-3.5× no matmul.

### 3.2 GPU Compute Bare Metal

| Ideia | Origem | Aderência | Dificuldade | LOC | Dependências |
|---|---|---|---|---|---|
| **BAR0/BAR1 mapping UC** | nova-core + NVIDIA DM | ★★★★★ | Média | 300 | NVMe driver funcional |
| **GPU doorbell ring** | nova-core + gpu-nvme-direct | ★★★★ | Alta | 400 | BAR0 mapping |
| **Job submission ring (SPSC)** | monadic-hypervisor + dmaplane | ★★★★ | Média | 300 | Doorbell funcional |
| **VRAM allocator** | coconutOS + nova-core | ★★★ | Alta | 400 | BAR1 mapping |
| **GPU DMA engine** | gpu-nvme-direct + dmaplane | ★★★ | Alta | 500 | Job ring + VRAM alloc |
| **NVIDIA firmware loader** | nova-core + nouveau docs | ★★ | Muito Alta | 1000+ | Tudo acima |

**Conexões:** BAR mapping → Doorbell → Job ring → VRAM → DMA → Firmware. GPU compute só é viável depois de SMP básico funcional.

**Melhoria buscada:** Forward pass migra de CPU (4 cores ~0.5s) para GPU (RTX 1050 ~0.02s). Ganho 25× sobre CPU 4c.

### 3.3 GPU Kernel Scheduling

| Ideia | Origem | Aderência | Dificuldade | LOC | Dependências |
|---|---|---|---|---|---|
| **XQueue abstraction** | XSched (OSDI) | ★★★★ | Alta | 600 | Submissão GPU funcional |
| **TPC stealing** | LithOS | ★★★ | Muito Alta | 1000+ | GPU scheduling pronto |
| **Agent.xpu split** | Agent.xpu (arXiv) | ★★★★★ | Média | 400 | GPU decode funcional |
| **MSched memory** | MSched (arXiv) | ★★★ | Alta | 500 | VRAM allocator |
| **gpu_ext eBPF** | gpu_ext (arXiv) | ★★ | Muito Alta | 2000+ | Runtime eBPF+GPU |

**Conexões:** Job ring → XQueue → Agent.xpu split. GPU kernel scheduling é N+3 ou N+4.

**Melhoria buscada:** Múltiplos workloads GPU concorrentes (display + LLM + training) com preempção justa.

### 3.4 DMA / Data Plane

| Ideia | Origem | Aderência | Dificuldade | LOC | Dependências |
|---|---|---|---|---|---|
| **SPSC index caching** | Monadic Data Plane | ★★★★★ | Mínima | 50 | SPSC ring existente |
| **Cache line alignment** | monadic-hypervisor | ★★★★★ | Mínima | 10 | (já fazemos parcial) |
| **Credit-based flow control** | dmaplane | ★★★★ | Média | 200 | SPSC funcional |
| **DMA-BUF sharing** | dmaplane + nova-core | ★★★ | Alta | 500 | GPU funcional |
| **KV cache over DMA** | dmaplane + m-store | ★★★ | Alta | 400 | GPU + DMA funcionais |
| **GPU-initiated NVMe** | gpu-nvme-direct | ★★★ | Muito Alta | 800 | NVMe + GPU DMA |

**Conexões:** SPSC ring → Index caching → Credit flow → DMA-BUF → KV cache. DMA é infraestrutura base.

**Melhoria buscada:** Zero-copy entre CPU/GPU/NVMe. KV cache de 307 MB pode ser swapada em 200ms via PCIe.

### 3.5 Parallel Compute (no_std)

| Ideia | Origem | Aderência | Dificuldade | LOC | Dependências |
|---|---|---|---|---|---|
| **burn-flex backend** | burn-flex | ★★★★★ | Média | 800 | Heapless Vec (temos) |
| **oxionnx core** | oxionnx | ★★★★ | Alta | 1200 | ONNX parser |
| **scirs2-core scheduler** | scirs2-core | ★★★★ | Média | 300 | no_std feature |
| **gemm optimization** | burn-flex + avx_parallel | ★★★★★ | Média | 400 | AVX2 (temos) |
| **Quantization fused** | burn-flex | ★★★★ | Média | 300 | gemm pronto |

**Conexões:** gemm → quantization → burn-flex backend. Parallel compute é independente de SMP (roda em 1 core já otimizado).

**Melhoria buscada:** Elimina `bitnet_avx2.rs` manual (~800 LOC), substitui por backend testado com 2-95× speedup.

### 3.6 Sincronização Cross-Core

| Primitiva | Já temos? | Ação | LOC | Prioridade |
|---|---|---|---|---|
| TicketLock | ✅ `ticket-lock` crate | Garantir `alignas(64)` | 10 | Imediata |
| SPSC ring | ❌ | Implementar (bbqueue) | 100 | Imediata |
| IPI | Parcial (só INIT/SIPI) | `send_ipi(lapic_id, vector)` | 150 | Imediata |
| Atomic wait (UMONITOR) | ❌ | Opcional (IceLake+) | 50 | N+3 |
| Cache line padding | Parcial | `#[repr(align(64))]` padronizar | 10 | Imediata |

---

## 4. Ideias Descartadas / Deferidas

| Ideia | Motivo | Talvez Futuro |
|---|---|---|
| **Topological scheduling (hollow-asm)** | Requer AVX-512 para <3ns. Nosso HW alvo (i5-6xxx) não tem AVX-512. Interessante mas impraticável | Se HW com AVX-512 |
| **WASM no scheduler** | Overhead alto para scheduling em tempo real. WASM é para skills, não para core scheduler | Já existe (WasmSkill) |
| **NuMA-aware stealing** | Nosso HW é single-socket. Complexidade desnecessária | Se HW multi-socket |
| **eBPF GPU (gpu_ext)** | Requer runtime eBPF + verifier. ~2000 LOC. IMATURO | Após GPU funcional |
| **CXL 3.1 (m-store)** | Sem HW CXL. Conceito futuro | Pós-MVP |
| **Complete firmware NVIDIA** | ~10000+ LOC para driver completo. Inviável agora | Se houver equipe |
| **Multi-GPU cluster** | Sem HW. Apenas 1 RTX 1050 | Se HW permitir |
| **RDMA** | Sem HW RDMA (InfiniBand/RoCE). dmaplane conceitual | Se HW disponível |
| **GPU-initiated NVMe** | Depende de patches NVIDIA DKMS. Barreira alta | Se comunidade evoluir |
| **Machine check (MCE) handling** | Importante para HW real mas não relacionado a SMP/GPU | Sprint separado |
| **Dynamic voltage/freq scaling** | Precisa de MSR específicos (IA32_PERF_CTL), válido mas não bloqueante | N+4 |

---

## 5. Plano de Implementação (Sprints Ajustados)

### Premissas:
- Hardware real (i5 4c, DDR4, RTX 1050) é o alvo de benchmark
- QEMU+WHPX é ambiente de desenvolvimento (AVX2 desligado)
- Cada sprint = ~300-800 LOC
- Blocos rearranjados por dependência técnica não por cronograma original

### Sprint N (Atual + 1) — Foundation: SPSC + IPI + PerCpu

| Item | LOC | Origem | Depende de |
|---|---|---|---|
| SPSC ring lockless (bbqueue) | 100 | bbqueue | Nenhuma |
| `#[repr(align(64))]` em todos atomics cross-core | 10 | monadic-hypervisor | Nenhuma |
| `send_ipi(lapic_id, vector)` | 100 | moss-kernel | LAPIC (✅) |
| IPI handler registrável | 50 | echOS-x64 | send_ipi |
| PerCpu dinâmico (alocar + GS.base por AP) | 300 | RuVix SMP | Nenhuma |
| **Total** | **~560** | | |

**Resultado:** APs podem receber trabalho. Cada core tem dados próprios. Base para tudo.

### Sprint N+1 — Work-Stealing + Parallel Matmul

| Item | LOC | Origem | Depende de |
|---|---|---|---|
| Work-stealing Chase-Lev scheduler | 400 | crossbeam-deque + fast-steal | PerCpu + SPSC |
| Parallel-for no matmul (chunk AVX2) | 300 | avx_parallel | Work-stealing |
| AgentScheduler multicore (4 run queues) | 200 | moss-kernel | Work-stealing |
| Per-CPU slab allocator | 300 | moss-kernel | PerCpu |
| **Total** | **~1200** | | |

**Resultado:** Forward pass 2-3× mais rápido. Agents distribuídos entre 4 cores.

### Sprint N+2 — GPU Foundations

| Item | LOC | Origem | Depende de |
|---|---|---|---|
| GPU BAR0/BAR1 mapping UC | 300 | nova-core | NVMe (✅) |
| PCIe doorbell register setup | 100 | nova-core | BAR0 mapping |
| GPU SPSC job ring | 300 | monadic-hypervisor | Doorbell |
| VRAM allocator (buddy) | 400 | coconutOS | BAR1 mapping |
| **Total** | **~1100** | | |

**Resultado:** GPU reconhecida como compute device. Primeiro job submetido via ring.

### Sprint N+3 — GPU Decode (BitNet offload)

| Item | LOC | Origem | Depende de |
|---|---|---|---|
| Agent.xpu prefill/decode split | 400 | Agent.xpu (arXiv) | GPU job ring |
| GPU matmul kernel (ternary) | 300 | nova-core patterns | GPU ring |
| CPU→GPU KV cache transfer | 200 | dmaplane | GPU DMA |
| XQeue preemptível | 600 | XSched (OSDI) | GPU ring |
| **Total** | **~1500** | | |

**Resultado:** decode do BitNet roda na GPU (RTX 1050 ~0.02s/step vs CPU ~0.5s).

### Sprint N+4 — Polimento

| Item | LOC | Origem | Depende de |
|---|---|---|---|
| burn-flex backend port | 800 | burn-flex | gemm existente |
| MSched memory scheduling | 500 | MSched (arXiv) | VRAM allocator |
| CFS scheduler completo | 500 | echOS-x64 | Work-stealing |
| GPU + Display co-existência | 300 | coconutOS | GPU funcional |
| **Total** | **~2100** | | |

**Resultado:** Kernel SMP completo, GPU compute funcional, backend matmul profissional.

---

## 6. Mapa de Conexões

```
Sprint N (Foundation)
  SPSC ring ──────────► IPI vetorizado ──────► PerCpu dinâmico
    │                                                │
    ▼                                                ▼
Sprint N+1 (Parallel)
  Work-stealing ◄─── Chase-Lev ─────────────── Parallel-for matmul
    │                                                │
    ├──► AgentScheduler multicore                    │
    └──► Per-CPU slab allocator                      │
                                                     ▼
Sprint N+2 (GPU)                               [2-3× speedup CPU]
  BAR0/BAR1 mapping ──► Doorbell ──► SPSC job ring ──► VRAM alloc
                                                     │
                                                     ▼
Sprint N+3 (GPU Decode)                         [25× speedup GPU]
  Agent.xpu split ◄── GPU matmul ◄── KV cache DMA ◄── XQueue
                                                     │
                                                     ▼
Sprint N+4 (Polimento)                          [50-100× total]
  burn-flex ◄── MSched ◄── CFS ◄── GPU+Display
```

---

## 7. Riscos e Mitigação

| Risco | Probabilidade | Impacto | Mitigação |
|---|---|---|---|
| Data race em scheduler multicore com no_std | Média | Crítico | Testar primeiro com 2 cores no QEMU. Usar TicketLock existente |
| GPU BAR mapping falha (RTX 1050 PCIe conf) | Baixa | Alto | Fallback: CPU-only continua funcional. GPU é aceleração, não requisito |
| WHPX inconsistente com SMP | Alta | Médio | Hardware real como critério de aceite. WHPX só para smoke test |
| AVX2+parallel não escala linearmente | Média | Baixo | Speedup 2× já é vitória. Gargalo é banda DDR4, não núcleos |
| coconutOS patterns não portáveis ao nosso kernel | Baixa | Médio | coconutOS é inspiração arquitetural, não copiar código |
| RTX 1050 sem suporte NVIDIA open module | Baixa | Alto | Pascal (GP108) é suportado pelo nova-core. Verificar compatibilidade |

---

## 8. Conclusão

O caminho crítico é: **SPSC → IPI → PerCpu → Work-stealing → GPU BAR → GPU ring → Agent.xpu split.**

As fontes mais valiosas identificadas foram:
- **coconutOS** (prova de conceito funcional de GPU AI inference em Rust no_std)
- **nova-core** (documentação oficial NVIDIA para GPU bare metal)
- **LithOS + gpu_ext** (fronteira de pesquisa em scheduling GPU intra-device)
- **burn-flex** (porta de entrada para parallel compute profissional)

O SMP+GPU não é feature cosmética — é o **multiplicador de performance** que torna a inferência local viável. Sem SMP, forward pass = ~60s sob WHPX. Com SMP+GPU, estimativa <0.1s. Diferença entre demo e produto real.

---

## 9. Referências

1. coconutOS — github.com/coconut-os/coconutOS
2. nova-core — NVIDIA open-gpu-kernel-modules, LKML 2026
3. echOS-x64 — github.com/asosyal04440/echOS-x64
4. moss-kernel — github.com/hexagonal-sun/moss-kernel
5. LithOS — arXiv 2504.15465 (2025)
6. gpu_ext — arXiv 2512.12615 (2025)
7. XSched — OSDI 2025, USENIX
8. MSched — arXiv 2512.24637 (2025)
9. Agent.xpu — arXiv 2506.24045 (2025)
10. dmaplane — arXiv 2603.10030 (2026)
11. monadic-hypervisor — github.com/SiliconLanguage/monadic-hypervisor
12. m-store — github.com/SiliconLanguage/m-store
13. gpu-nvme-direct — github.com/xaskasdf/gpu-nvme-direct
14. burn-flex — github.com/antimora/burn-flex
15. fast-steal — crates.io 6.5.4
16. bbqueue — github.com/elodin-sys/bbqueue
17. chase-lev deque — "Dynamic Circular Work-Stealing Deque" (1994)
18. RuVix SMP — docs.rs/ruvix-smp 0.1.0
19. Monadic Data Plane — 0kernel.ai/research
