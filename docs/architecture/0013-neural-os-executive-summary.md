# ADR-0013: Neural OS Executive Summary — State of the Art 2026

**Status:** Accepted  
**Date:** 2026-06-22  
**Driver:** Consolidar 10 sprints de engenharia em um manifesto arquitetural alinhado com a literatura acadêmica 2025/2026, provando a ineditabilidade da combinação bare-metal Rust + microkernel + inferência ternária + roteador neural.

## Context

Pesquisa acadêmica profunda confirmou que nenhum sistema operacional existente combina os quatro pilares do neural-os-core:

1. **Bare-metal Rust** (`no_std`, `no_main`, sem Linux)
2. **Microkernel com FS semântico** (sem POSIX, sem VFS)
3. **Inferência ternária {-1, 0, +1}** como primitiva de kernel (não como aplicação)
4. **Roteador neural** substituindo o escalonador de tarefas clássico

Sistemas de fronteira (MerlionOS, FairyFuse/Bitnet.cpp, Mixture-of-Schedulers) validam partes isoladas da stack, mas nenhum as integra em um núcleo de sistema operacional monolítico-semântico.

## 1. MerlionOS — Validação da Stack Bare-metal Rust + GGUF

**Referência:** MerlionOS (2025), sistema operacional bare-metal Rust com capacidade de parsing de modelos GGUF.

### Relevância para neural-os-core

MerlionOS demonstra que é viável:
- Executar parsing de modelos neurais (GGUF) diretamente em bare-metal Rust, sem camada de SO legado.
- Mapear tensores pré-treinados diretamente na memória física sem syscalls.

### Divergência Estratégica

| Aspecto | MerlionOS | neural-os-core |
|---|---|---|
| Paradigma | SO clássico com parsing de modelos | SO neural desde o boot |
| Escalonador | Round-robin tradicional | Intent Router (MLP + argmax) |
| Memória | VFS baseada em inodes | Semantic File System (zero-copy DMA) |
| Inferência | Aplicação de usuário carregando GGUF | Primitiva de Ring 0 (Tensor + BitLinear) |

### Lições Incorporadas

- A viabilidade de parsing GGUF em bare-metal Rust confirma nossa decisão de não depender de `std`.
- Adotaremos mapeamento de páginas enormes (Huge Pages 2 MiB / 1 GiB) para sessões de inferência longas, similar ao MerlionOS.
- O formato GGUF será suportado como fonte de pesos na Fase 3 (calibração ternária a partir de modelos pré-treinados).

## 2. FairyFuse & Bitnet.cpp (TL/I2_S) — Eliminação do Branch Condicional

**Referência:** FairyFuse (2025) + Bitnet.cpp Lookup Table / I2_S kernels. Packing de 16 pesos ternários por DWORD (32 bits), acesso via lookup table sem branch.

### Impacto Arquitetural

O kernel `matmul_hybrid()` atual (Sprint 9) usa `match w { 1 => add, -1 => sub, _ => skip }` — uma operação condicional por peso. A literatura TL/I2_S demonstra que podemos:

1. **Packing hiperdenso:** 16 pesos ternários em uma única DWORD (32 bits), usando 2 bits por peso.
2. **Lookup Table (TL):** pré-calcular todas as combinações possíveis de 16 inputs × 16 pesos em uma tabela de 2^16 entradas (64 KB).
3. **Kernel branchless:** substituir `match` por `LUT[input_bits ^ packed_weights]` — uma única instrução de lookup.

### Caminho de Migração

```
Sprint 9:   match w { 1 => add, -1 => sub }    → 1 branch por peso
Sprint 10:  get_weight() + add/sub             → 1 shift + mask + branch por peso
Fase 3 TL:  LUT[input_bits ^ weights_dword]     → 1 lookup a cada 16 pesos
```

### Ganho Esperado

| Kernel | Branches por peso | Ops por 16 pesos | Cache |
|---|---|---|---|
| f32 matmul (Sprint 5) | 0 (FMA implícito) | 16 FMAs | 64 bytes (16 × f32) |
| ADD/SUB condicional (Sprint 9) | 1 | 16 ADD/SUB + 16 branches | 16 bytes (16 × i8) |
| TL I2_S (Fase 3) | 0 | 1 lookup + 1 acumulação | 2 bytes (16 pesos em DWORD) |

### Implementação

```rust
// TL kernel sketch — Fase 3
const LUT: [f32; 65536] = precompute_all_dot_products();

fn tl_matmul(input: &[f32; 16], weights_dword: u32) -> f32 {
    let input_bits = pack_input_signs(input);  // 16 sign bits → u16
    LUT[(input_bits as usize) ^ (weights_dword as usize) & 0xFFFF]
}
```

## 3. Mixture-of-Schedulers (ASA) e Neural Kernel (eBPF) — Validação do Roteador Neural

**Referência:** 
- ASA (Adaptive Scheduling Architecture, 2025): escalonador adaptativo que usa ML leve para decisões de agendamento sub-microssegundo.
- Neural Kernel (eBPF + RL, 2026): subsistema de kernel Linux que usa reinforcement learning para otimizar políticas de I/O e rede.

### Validação do Intent Router

O nosso `Intent Router` atual (MLP 3→2 com SiLU + argmax, inferindo 0,13 µs por forward) está alinhado com ambas as abordagens:

| Sistema | Decisor | Latência | Escopo |
|---|---|---|---|
| ASA | MLP 4→2 | ~0,2 µs | Escalonamento de tarefas |
| Neural eBPF | RL Policy | ~1 µs | I/O + rede |
| neural-os-core | MLP 3→2 (SiLU) | ~0,13 µs | Intenção do usuário → ação |

### Evolução Planejada

ASA e Neural Kernel validam que **decisões de kernel sub-microssegundo via ML são viáveis e superiores a heurísticas fixas**. O roadmap evolui o Intent Router em três estágios:

1. **Primitivo (atual):** MLP (3→2) com pesos fixos, argmax monolítico.
2. **Adaptativo (Fase 4):** pesos treinados online via feedback de sucesso/fracasso das ações.
3. **Preditivo (Fase 6):** incorpora cache de decisões (Neural Cache) para atingir latência < 50 ns no hot path.

## Unicidade do neural-os-core

Nenhum sistema conhecido combina as quatro camadas:

```
                    ┌─────────────────────────────────────┐
                    │      neural-os-core v0.10.0         │
                    ├──────────┬──────────┬───────────────┤
                    │ Bare-     │ Micro-   │ Inferência    │
                    │ metal     │ kernel   │ Ternária      │
                    │ Rust      │ c/ FS    │ {-1, 0, +1}   │
                    │ no_std    │ semântico│ como          │
                    │           │          │ primitiva     │
                    ├──────────┴──────────┴───────────────┤
                    │       Roteador Neural (MLP)          │
                    │   substituindo escalonador clássico  │
                    └─────────────────────────────────────┘
```

| Projeto | Bare-metal Rust | Microkernel | Inferência Ternária | Roteador Neural |
|---|---|---|---|---|
| Linux | ✗ | ✗ | ✗ (eBPF apenas) | ✗ |
| seL4 | ✗ (C) | ✓ | ✗ | ✗ |
| MerlionOS | ✓ | ✗ | ✗ | ✗ |
| FairyFuse | ✗ | ✗ | ✓ (biblioteca) | ✗ |
| **neural-os-core** | **✓** | **✓** | **✓** | **✓** |

## Conclusão

A pesquisa acadêmica 2025/2026 confirma:
1. **MerlionOS** → nossa stack bare-metal Rust é viável e correta.
2. **FairyFuse/Bitnet.cpp** → nossa direção ternária é estado-da-arte; TL/I2_S eliminará os branches condicionais.
3. **ASA/Neural eBPF** → nosso Intent Router é academicamente sólido e escalará para políticas adaptativas.

O neural-os-core permanece inédito na integração simultânea dos quatro pilares. Esta ADR serve como manifesto arquitetural e prova de ineditabilidade para publicação acadêmica futura.

---

## Estrutura Monorepo (Cargo Workspace)

Inspirado pelo ecossistema `aios-rs`, o repositório será reorganizado como um Cargo Workspace com crates independentes, cada um responsável por um domínio do sistema:

```
neural-os-core/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── neural-kernel/            # Ring 0 — microkernel bare-metal
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs           # entry_point, panic handler, boot flow
│   │       ├── vga_buffer.rs
│   │       ├── serial.rs
│   │       ├── interrupts.rs     # IDT, GDT, TSS, PIC, PIT
│   │       ├── memory.rs         # OffsetPageTable, allocators
│   │       ├── simd.rs
│   │       ├── tensor.rs         # Tensor, TernaryTensor, PackedTernaryTensor
│   │       └── nn.rs             # silu, Linear, BitLinear, argmax
│   │
│   ├── agent-core/               # Ring 1 — abstração de agente (no_std)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── process.rs        # AgentProcess trait + struct
│   │       ├── scheduler.rs      # Agent Scheduler (MLP-driven)
│   │       └── context.rs        # Context memory, KV-cache
│   │
│   ├── skill-registry/           # Ring 2 — WASM Skills + MCP (no_std ou std)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── registry.rs       # Skill trait, lookup, lifecycle
│   │       ├── wasm.rs           # wasmi embedder / WASM runtime
│   │       └── mcp.rs            # Model Context Protocol messages
│   │
│   └── event-bus/                # IPC — publish/subscribe interno (no_std)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── bus.rs            # EventBus core, channels
│           ├── capability.rs     # CapabilityToken, permission model
│           └── message.rs        # Event, Message, Priority
│
├── docs/                         # ADRs, roadmap, session logs
└── Cargo.lock
```

### Regras do Workspace

1. `neural-kernel` é o único crate que depende do `bootloader` e roda em bare-metal (`#![no_std]`, `x86_64-unknown-none`).
2. `agent-core` e `event-bus` são `no_std` — podem ser compilados para host (testes unitários) ou target (kernel).
3. `skill-registry` pode optar por `std` quando compilado para ferramentas de host (ex: carregar .wasm do filesystem para testes). No kernel, usa `no_std` + alocador personalizado.
4. Dependências entre crates seguem a seta de isolamento: `neural-kernel ← agent-core ← skill-registry ← event-bus` (nenhuma dependência cíclica).

---

## Design System (Rust Traits)

### AgentProcess

Define a unidade mínima de execução no sistema — um agente que possui identidade, contexto e uma fila de habilidades a executar.

```rust
/// Identificador único de 64 bits para um agente no sistema.
/// Gerado pelo Agent Scheduler no momento da criação.
pub type AgentId = u64;

/// Prioridade de execução do agente. Decidida pelo Intent Router.
#[repr(u8)]
pub enum Priority {
    Critical = 0,
    High     = 1,
    Normal   = 2,
    Low      = 3,
    Idle     = 4,
}

/// Contexto de execução de um agente.
/// Mantido em Ring 0 (context memory) e consultado a cada forward pass.
pub struct AgentContext {
    pub id: AgentId,
    pub priority: Priority,
    pub embedding: [f32; 3],       // embedding de intenção (input do MLP)
    pub skill_queue: Vec<SkillId>, // fila de skills a executar
}

/// Trait que todo agente deve implementar.
pub trait AgentProcess {
    fn id(&self) -> AgentId;
    fn context(&self) -> &AgentContext;
    fn context_mut(&mut self) -> &mut AgentContext;
    fn tick(&mut self) -> Option<SkillCommand>;  // chamado a cada ciclo do scheduler
}
```

### Skill (Trait)

Habilidade executável — opera em Ring 2 (WASM) no futuro, mas o trait é agnóstico ao runtime.

```rust
/// Identificador de skill (UUID simplificado para 64 bits).
pub type SkillId = u64;

/// Resultado da execução de uma skill.
pub struct SkillOutput {
    pub tensor: Option<Tensor>,       // tensor de saída (ex: logits)
    pub tokens_consumed: usize,       // budget consumido
    pub success: bool,
}

/// Conjunto de capacidades que uma skill declara necessitar.
pub struct CapabilitySet {
    pub requires_network: bool,
    pub requires_persist: bool,
    pub requires_mmio: bool,
    pub max_memory_pages: usize,
}

/// Trait que toda skill deve implementar.
pub trait Skill {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> CapabilitySet;
    fn execute(&mut self, input: &Tensor) -> SkillOutput;
    fn drop(&mut self);  // cleanup forçado pelo scheduler
}
```

### EventBus (Publish/Subscribe com Capability Tokens)

Único mecanismo de IPC do sistema. Não há syscalls diretos — toda comunicação entre Ring 0, Ring 1 e Ring 2 passa por mensagens no barramento.

```rust
/// Token de capacidade — acompanha toda mensagem no barramento.
/// Gerado pelo Agent Scheduler e verificado pelo EventBus antes de entregar.
pub struct CapabilityToken {
    pub agent_id: AgentId,
    pub permissions: u64,  // bitmap de permissões
}

/// Mensagem trafegada no barramento.
pub struct Message {
    pub topic: Topic,
    pub payload: &'static [u8],  // fat pointer para dado serializado
    pub token: CapabilityToken,
    pub priority: Priority,
}

/// Tópicos do barramento de eventos.
pub enum Topic {
    AgentCreated(AgentId),
    AgentDestroyed(AgentId),
    SkillRequest(SkillId),
    SkillOutput { skill: SkillId, output: SkillOutput },
    CortexDecision { action: u8, confidence: f32 },
    WatchdogTick(u64),
    MemoryPressure { free_pages: usize, threshold: usize },
}

/// Trait do barramento de eventos.
pub trait EventBus {
    /// Inscreve um agente em um tópico.
    fn subscribe(&mut self, agent: AgentId, topic: Topic) -> Result<(), ()>;

    /// Cancela inscrição.
    fn unsubscribe(&mut self, agent: AgentId, topic: &Topic);

    /// Publica uma mensagem no barramento.
    /// A mensagem só é entregue se o token do remetente tiver permissão para o tópico.
    fn publish(&mut self, msg: Message) -> Result<(), ()>;

    /// Um agente consome a próxima mensagem de sua fila (non-blocking).
    fn poll(&mut self, agent: AgentId) -> Option<Message>;

    /// Verifica se um token tem permissão para publicar/consumir em um tópico.
    fn authorize(&self, token: &CapabilityToken, topic: &Topic) -> bool;
}
```

### Fluxo de Execução com Traits

```
boot → AgentScheduler::new()
         ├─ EventBus::subscribe(cortex, Topic::SkillRequest)
         ├─ EventBus::subscribe(cortex, Topic::AgentCreated)
         └─ loop {
              for agent in agents {
                  if let Some(cmd) = agent.tick() {
                      let msg = Message { topic: SkillRequest(cmd.skill_id), ... };
                      event_bus.publish(msg);  // EventBus verifica token
                  }
              }
              if let Some(msg) = event_bus.poll(cortex_id) {
                  // Cortex processa e publica decisão
                  event_bus.publish(Message { topic: CortexDecision {...}, ... });
              }
            }
```

### Regras de Design

1. **AgentProcess** é a única abstração de usuário no sistema — não há `Process` ou `Thread`.
2. **Skill** substitui o conceito de "syscall": skills são funções puras (input → output) que executam em sandbox.
3. **EventBus** substitui IPC clássico (sockets, pipes, signals). Todo estado compartilhado passa por mensagens com tokens de capacidade verificados.
4. Nenhum trait depende de `std`. Todos usam `core` + `alloc`.
5. A implementação concreta de `EventBus` usa uma `Vec<Vec<Message>>` indexada por `AgentId` no kernel, evoluindo para um ring-buffer lock-free em Huge Pages na Fase 4.

## References

- MerlionOS: A Bare-Metal Rust OS with GGUF Model Parsing (2025)
- FairyFuse: Lookup Table Kernels for Ternary Neural Networks (2025)
- Bitnet.cpp: Efficient CPU Inference for 1.58-bit Models, I2_S Kernel (2025)
- ASA: Adaptive Scheduling Architecture with ML-based Task Routing (2025)
- Neural Kernel: eBPF-based Reinforcement Learning for OS Policies (2026)
- ADR-0010: Strategic Roadmap and Architectural Innovations
- ADR-0011: BitLinear and Hybrid Ternary MatMul
- ADR-0012: 2-bit Packing and Ternary Quantization
