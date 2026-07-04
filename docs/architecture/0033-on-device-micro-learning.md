# ADR-0033: On-Device Micro-Learning — Self-Training MoE

**Data:** 2026-07-03
**Status:** Draft
**Sprint Target:** 77

---

## 1. Visão

O Neural AIOS pode treinar seus próprios modelos neurais **na mesma máquina em que roda**.
Sem Python. Sem PyTorch. Sem máquina externa.

```
┌────────────────────────────────────────────────────────────────┐
│              SELF-TRAINING AIOS                                  │
│                                                                 │
│  Usuário: "Aprenda a programar em Rust para criar apps WASM"    │
│                                                                 │
│  ┌──────────┐    ┌──────────────────┐    ┌──────────────────┐  │
│  │ Dataset  │───▶│ Candle Trainer   │───▶│ Trinity Hub      │  │
│  │ /data/   │    │ (sidecar ELF)    │    │ /models/         │  │
│  │ 200K ex  │    │ Rust std + GPU   │    │ rust_coder.bitnet│  │
│  └──────────┘    └──────────────────┘    └──────────────────┘  │
│                                                                 │
│  Mesma máquina. Mesma GPU. Mesmo FS. Zero dependências externas.│
└────────────────────────────────────────────────────────────────┘
```

## 2. Por que BitNet Ternário?

BitNet usa pesos em {-1, 0, +1}. A arquitetura é **ADD/SUB, não float**:

| Operação | Float (PyTorch) | Ternário (BitNet) |
|---|---|---|
| Forward | f32 × f32 (FPU) | ADD/SUB i8 (1 ciclo) |
| Backward | autograd (complexo) | sign(gradient) (1 linha) |
| Quantização | Post-training (loss) | Built-in (lossless) |
| Memória | 16-32 bits/param | **2 bits/param** |
| Treino on-device | Impossível (FPU requerido) | ✅ Viável (CPU) |

Isso significa que **fine-tuning roda no próprio kernel** com ADD/SUB, e **full training** roda como sidecar Candle com GPU.

## 3. Três Modos de Treino

| Modo | Dados | Onde roda | Tempo | GPU? | Dependências |
|---|---|---|---|---|---|
| **Fine-tuning** | 100 exemplos | Kernel (no_std) | ~2 segundos | ❌ CPU | Zero (ADD/SUB puro) |
| **Transfer learning** | 1000 exemplos | Kernel (no_std) | ~20 segundos | ❌ CPU | Zero |
| **Full training** | 100K+ exemplos | Sidecar Candle (std) | horas | ✅ GPU | Candle crate |

### 3.1 Fine-Tuning (Kernel, CPU, ADD/SUB)

```rust
// ~300 LOC Rust no_std — roda no próprio kernel
struct BitNetTrainer {
    weights: PackedTernaryWeights,
    config: ModelConfig,
}

impl BitNetTrainer {
    fn train_step(&mut self, input: &[i8], target: i8) {
        // Forward: ADD/SUB matmul
        let output = self.forward(input);
        let error = (output - target).signum();
        
        // Backward: STE (Straight-Through Estimator)
        for (w, g) in self.weights.iter_mut().zip(input.iter()) {
            if error * *g != 0 {
                *w = (*w + (error * *g).signum() as i8).clamp(-1, 1);
            }
        }
    }
}
```

### 3.2 Full Training (Sidecar Candle, GPU)

O kernel spawna um **processo externo** (ELF com std) que roda Candle com acesso à GPU:

```
Kernel (no_std, Ring 0):
  1. Carrega ELF do FS: DiskAgent.read("/bin/candle_trainer.elf")
  2. Aloca stack (64 KB) + heap (512 MB) via frame allocator
  3. Mapeia segmentos ELF via page tables
  4. Passa handles: DiskAgent (phys addr), GPU ring buffer (phys addr)
  5. Configura GDT/TSS com nova stack
  6. Salta pra entry do ELF

Candle Trainer (std, Ring 0 compartilhado):
  1. Reconstrói handles dos endereços físicos
  2. Lê dataset do FS via DiskAgent handle
  3. Treina com Candle crate + GPU backend
  4. Quantiza para ternário (mesmo algoritmo do kernel)
  5. Escreve .bitnet no FS
  6. HLT → kernel detecta, coleta resultado

Kernel (no_std, Ring 0):
  1. Detecta HLT do child
  2. Trinity Hub carrega .bitnet
  3. Hermes publica MODEL_READY
  4. Desaloca stack + heap do child
```

## 4. O Candle Crate (HuggingFace, Rust puro)

Candle é um framework de ML em Rust desenvolvido pela HuggingFace:

| Candle | PyTorch (equiv) |
|---|---|
| `candle_core::Tensor` | `torch.Tensor` |
| `candle_nn::Linear` | `torch.nn.Linear` |
| `candle_nn::AdamW` | `torch.optim.AdamW` |
| `Tensor::backward()` | `loss.backward()` |
| `Device::Cuda(0)` | `device='cuda'` |
| `Device::Cpu` | `device='cpu'` |

**Vantagens sobre Python/PyTorch:**
- Zero dependências Python (sem pip, venv, conda)
- Compilado como binário standalone (único ELF)
- Mesma GPU via CUDA backend, mas em Rust
- ~15K ★ no GitHub, mantido ativamente
- Compila com `x86_64-unknown-linux-gnu` (tem std!)

**Limitação:** Candle **não roda no kernel** (precisa de std). Por isso usamos sidecar.

## 5. Dados de Treino — 3 Fontes

| Fonte | Disponível | Exemplos | Requer |
|---|---|---|---|
| **FS local** | ✅ Hoje | logs SMART, self-heal, agent state, EventBus dump | Nada |
| **Pré-carregado** | ✅ Hoje | `/data/rust_training/200K.json` colocado no FS | Nada |
| **Internet** | ❌ Precisa B-01 | Crawl de docs, GitHub, Stack Overflow | B-01 + HTTP |

**Fine-tuning e transfer learning** funcionam com dados locais — sem internet.

**Full training** de um modelo novo (200K+ exemplos) precisa do dataset já estar no FS ou ser baixado (B-01).

## 6. Task Spawner — Caminho 3

```rust
// kernel/exec.rs (~300 LOC)
pub struct ChildTask {
    entry: u64,
    stack_top: u64,
    heap_start: u64,
    heap_size: u64,
    disk_phys: u64,
    gpu_phys: u64,
}

pub fn spawn_elf(elf_path: &str, disk_phys: u64, gpu_phys: u64) -> Option<ChildTask> {
    let bytes = DiskAgent::read(elf_path)?;
    let elf = goblin::elf::Elf::parse(&bytes).ok()?;
    let entry = elf.header.e_entry;

    // Aloca stack (64 KB)
    let stack = allocate_contiguous_pages(16)?; // 16 × 4KB = 64KB
    
    // Aloca heap (512 MB para Candle + dataset)
    let heap = allocate_contiguous_pages(131072)?; // 131072 × 4KB = 512MB

    // Mapeia segmentos ELF nos endereços virtuais corretos
    for phdr in &elf.program_headers {
        if phdr.p_type == goblin::elf::program_header::PT_LOAD {
            let vaddr = phdr.p_vaddr;
            let memsz = phdr.p_memsz;
            map_elf_segment(vaddr, memsz, &bytes[phdr.p_offset as usize..]);
        }
    }

    // Configura stack na TSS (Ring 0, mesmo nível de privilégio)
    let stack_top = stack + stack_size - 16;

    // Salva estado atual do kernel
    save_kernel_state();

    // Salta para entry do ELF com argumentos
    unsafe {
        jump_to_elf(entry, stack_top, disk_phys, gpu_phys);
    }

    Some(ChildTask { entry, stack_top, heap_start: heap, heap_size: 512*1024*1024, disk_phys, gpu_phys })
}
```

## 7. Fluxo Completo

```
1. USUÁRIO: "Treine um modelo de código Rust com 200K exemplos"

2. HERMES → Cortex.classify("training_request")
   → TrainingAgent.tick()

3. TRAINING_AGENT:
   a. MemoryAgent: "GPU detectada. VRAM: 8 GB. Full training viável."
   b. DiskAgent: "Dataset /data/rust_training/ encontrado: 200K pares."
   c. spawn_elf("/bin/candle_trainer.elf", disk_phys, gpu_phys)

4. CANDLE TRAINER (sidecar):
   a. Reconstrói DiskAgent handle → lê 200K exemplos do FS
   b. Reconstrói GPU handle → Candle Device::Cuda(0)
   c. Treina 100 épocas, loss decresce 2.4 → 0.3
   d. Quantiza pesos ternários → PackedTernaryWeights
   e. Escreve /models/rust_coder.bitnet (125 KB, 500K params)
   f. HLT → retorna ao kernel

5. KERNEL:
   a. Detecta HLT → child terminou
   b. Trinity Hub: load_model("rust_coder.bitnet")
   c. Desaloca stack + heap (512 MB liberados)

6. HERMES:
   "[TRAIN] Modelo rust_coder pronto. 125 KB, 500K params ternários.
           Treinado em 200K exemplos por 100 épocas.
           Loss final: 0.31. Precisão: 94.2%."

7. USUÁRIO: "Crie um app jogo da velha"
   → Trinity Router → rust_coder → gera código → cargo → .wasm → agent
```

## 8. Estimativas

| Componente | LOC | Sprint |
|---|---|---|
| BitNetTrainer (fine-tuning kernel) | ~300 | 77 |
| Candle Trainer (sidecar ELF) | ~200 | 77 |
| Task Spawner (ELF loader + jump) | ~500 | 77 |
| TrainingAgent (orquestrador) | ~200 | 77 |
| Trinity Hub (registro de experts) | ~150 | 77 |
| Candle crate (dependência externa) | — | 77 |
| **Total Sprint 77** | **~1350 LOC** | |

## 9. Decisões

1. **Caminho 3 (sidecar)** — trainer roda como ELF separado, mesma máquina, mesma GPU, mesma RAM.
2. **Fine-tuning no kernel** — ADD/SUB puro, zero dependências, ~300 LOC.
3. **Candle para full training** — Rust puro, GPU, zero Python.
4. **Dataset no FS** — DiskAgent provê acesso; sem internet para fine-tuning.
5. **HTL como sinal de término** — o sidecar faz HLT, kernel captura e coleta resultado.
