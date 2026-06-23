# Project State — neural-os-core

## Sprint 1 — Chassi Básico (Complete)
## Sprint 2 — Observabilidade Ring 0 (Complete)
## Sprint 3 — Captura de Exceções da CPU (Complete)
## Sprint 4 — Alocação Dinâmica e Heap (Complete)
## Sprint 5 — Ativação de SIMD e Fundação Tensorial (Complete)
## Sprint 6 — Primitivas Neurais e dependência libm (Complete)
## Sprint 7 — Intent Router MLP e Forward Pass (Complete)
## Sprint 8 — Hardware Interrupts & Memory Safety (Complete)
## Sprint 9 — Ternary Inference Engine (Complete)
## Sprint 10 — 2-bit Packing and Ternary Quantization (Complete)
## Sprint 11 — Bitmap Frame Allocator (Complete)
## Sprint 12 — Kernel Abstraction: Async Neural Executor (Complete)
## Sprint 14 — Skill Registry & MCP Layer (Complete)

### Current Status

 | Category | Status |
|---|---|---|
| Last QEMU Boot | ✅ Boot OK — VGA + serial + Breakpoint handler + EchoSkill execution |
| Compilation | ✅ `cargo check` — 0 errors, 0 warnings |
| VGA Output | ✅ Mapped via `map_physical_memory`, Writer with `print!/println!` |
| Serial Output | ✅ `uart_16550` driver, `serial_print!/serial_println!` via port `0x3F8` |
| Panic Handler | ✅ Logs to VGA and serial simultaneously |
| IDT | ✅ Breakpoint + Double Fault handlers, IST stack switch for DF |
| GDT + TSS | ✅ Custom GDT with TSS descriptor for Double Fault stack switching |
| Page Tables | ✅ `OffsetPageTable` via `Cr3` + `physical_memory_offset` |
| Frame Allocator | ✅ `BitmapFrameAllocator` — bitmap 128 KB, init via UEFI map, alloc/dealloc O(n), 0% leak |
| Bitmap Stress Test | ✅ 1000 alloc/dealloc estáveis — `[KERNEL] Status RAM Tensor: [0.001011, 0.0]` |
| Contiguous Alloc | ✅ `allocate_contiguous(count)` preparado para Huge Pages (Fase 4) |
| Async Neural Executor | ✅ `NeuralExecutor` — `VecDeque<AgentTask>`, cooperative poll, RawWakerVTable |
| AgentTask | ✅ `id`, `Pin<Box<dyn Future>>`, `AtomicU64` ID generation |
| DummyWaker | ✅ `RawWakerVTable` em `no_std` (clone/wake/drop no-ops) |
| system_daemon | ✅ `async fn` test — spawn, executa, complete, idle loop com hardware context |
| EventBus crate | ✅ `event-bus` — `no_std`, `alloc`, publish/subscribe com `BTreeMap` + `Arc<Mutex<VecDeque>>` |
| CapabilityToken | ✅ `pub struct CapabilityToken(pub u64)` — `is_valid()` check (token > 0) |
| Event | ✅ `{ id, topic, payload, token }` — ID gerado automaticamente no publish |
| Publish/Subscribe | ✅ `subscribe(topic) -> Receiver`, `publish(Event) -> Result` com validação de token |
 | IPC Flow | ✅ `system_daemon` subscribe → yield → `hardware_monitor` publish → receive → `SkillRegistry.execute_skill` → complete |
| Heap | ✅ `LockedHeap` global allocator (linked_list_allocator v0.9.1) |
| `alloc` crate | ✅ `Box`, `Vec` testados no boot flow |
| FPU/SSE (SIMD) | ✅ CR0: clear EMULATE_COPROC, set MONITOR + NUMERIC_ERROR |
| | ✅ CR4: set OSFXSR + OSXMMEXCPT_ENABLE |
| Tensor Engine | ✅ `Tensor` struct with f32 matmul (1×3 × 3×1 = 1×1) |
| Tensor API | ✅ `add_scalar`, `mul_scalar`, `apply<F>` |
| SiLU Activation | ✅ `nn::silu(x)` via `libm::expf` — `[-0.269, 0, 0.731]` |
| RMSNorm | ✅ `nn::rms_norm(tensor, weight, eps)` via `libm::sqrtf` |
| Tensor transpose | ✅ `transposed()` — row-major → column-major, usado em Linear |
| Linear Layer | ✅ `Linear { weights, bias }` com `forward()` = X·W^T + B |
| argmax | ✅ `nn::argmax(tensor)` — índice do maior logit |
| Intent Router | ✅ MLP 1×3 → 2, SiLU → argmax → decisão (0=Daemon, 1=Halt) |
| PIC 8259A Remap | ✅ PIC1 → vetor 32, PIC2 → vetor 40 via `ChainedPics` |
| PIT Watchdog | ✅ Timer a ~18.2 Hz, contador atômico, EOI |
| Page Fault Handler | ✅ CR2 → log → `hlt` loop (barreira Ring 2) |
| Frame Deallocator | ✅ `BitmapFrameAllocator` implementa `FrameDeallocator<Size4KiB>` — reuso real |
| TernaryTensor | ✅ `i8` storage, shape (in, out), `from_row_major()` |
| Hybrid MatMul | ✅ ADD/SUB-only — zero multiplicações, `match w {1 => add, -1 => sub, _ => skip}` |
| BitLinear (i8) | ✅ Camada densa ternária com `forward()` |
| PackedTernaryTensor | ✅ 2-bit packing — 4 weights/byte via `pack_weights()` + `get_weight()` |
| Quantization | ✅ `quantize_to_packed(f32_tensor, threshold)` — calibração ternária |
| Compressed BitLinear | ✅ `PackedTernaryTensor` no lugar de `i8` — 12× vs f32, 3× vs i8 |
| 2-bit Inference | ✅ `[1.5, -1.8, 0.2, ...]` → threshold 0.5 → 2 bytes → `[-0.5, -2.0]` |
| `libm` crate | ✅ v0.2.16 — `expf`, `sqrtf` em `no_std` |
| Toolchain | ✅ nightly, bootimage v0.10.4, MinGW-w64 |
| Monorepo Workspace | ✅ Cargo workspace em `crates/` — `neural-kernel`, `agent-core`, `skill-registry`, `event-bus` |

### Files

| File | Purpose |
|---|---|---|
| `crates/neural-kernel/src/main.rs` | Entry point, panic handler, boot flow |
| `crates/neural-kernel/src/vga_buffer.rs` | VGA Writer, `print!/println!` |
| `crates/neural-kernel/src/serial.rs` | 16550 UART, `serial_print!/serial_println!` |
| `crates/neural-kernel/src/interrupts.rs` | IDT, TSS, GDT, Breakpoint + Double Fault + Page Fault + PIT Timer + PIC remap |
| `crates/neural-kernel/src/memory.rs` | `OffsetPageTable`, `BitmapFrameAllocator`, `init_memory()` |
| `crates/neural-kernel/src/allocator.rs` | `LockedHeap` global allocator, `init_heap()` |
| `crates/neural-kernel/src/simd.rs` | `enable_simd()` — CR0/CR4 FPU/SSE enablement |
| `crates/neural-kernel/src/tensor.rs` | `Tensor` + `TernaryTensor` + `PackedTernaryTensor` |
| `crates/neural-kernel/src/nn.rs` | `silu()`, `rms_norm()`, `Linear`, `BitLinear`, `argmax` |
| `crates/neural-kernel/src/task/mod.rs` | `DummyWaker` — `RawWakerVTable` em `no_std` |
| `crates/neural-kernel/src/task/agent.rs` | `AgentTask` — `id: u64`, `Pin<Box<dyn Future>>` |
| `crates/neural-kernel/src/task/executor.rs` | `NeuralExecutor` — `VecDeque` loop cooperativo |
| `crates/event-bus/src/lib.rs` | Re-exports: `EventBus`, `CapabilityToken`, `Event` |
| `crates/event-bus/src/capability.rs` | `CapabilityToken(pub u64)` — validação de permissão |
| `crates/event-bus/src/event.rs` | `Event { id, topic, payload, token }` |
 | `crates/event-bus/src/bus.rs` | `EventBus` — `BTreeMap<String, Vec<Arc<Mutex<VecDeque>>>>` |
| `crates/skill-registry/src/lib.rs` | Re-exports: `SkillRegistry`, `Skill`, `McpManifest` |
| `crates/skill-registry/src/mcp.rs` | `McpManifest { name, description, required_tokens }` |
| `crates/skill-registry/src/skill.rs` | `trait Skill: Send + Sync { manifest(), execute() }` |
| `crates/skill-registry/src/registry.rs` | `SkillRegistry` — `BTreeMap`, register, execute_skill com token validation |
| `Cargo.toml` (root) | Workspace manifest |
| `crates/neural-kernel/Cargo.toml` | Kernel package, deps, bootimage metadata |
| `crates/agent-core/Cargo.toml` | Agent abstraction crate (stub) |
| `crates/skill-registry/Cargo.toml` | WASM Skills crate (stub) |
| `crates/event-bus/Cargo.toml` | IPC EventBus crate (stub) |
| `.cargo/config.toml` | Target, runner, `relocation-model=static` |
| `docs/architecture/0001-*.md` to `0013-*.md` | 13 ADRs |
| `docs/memory/STATE.md` | This file |
| `docs/memory/SESSION_001.md` to `SESSION_010.md` | Sprint logs |
| `docs/roadmap.md` | Roadmap geral — fases 3–7, TL/I2_S, Padé, MatMul-free |

### Dependencies

| Crate | Version | Purpose |
|---|---|---|
 | `bootloader` | 0.9.34 | Boot image, `BootInfo`, `map_physical_memory` |
| `skill-registry` | 0.1.0 | MCP layer — `Skill`, `McpManifest`, `SkillRegistry` com validação de token |
| `spin` | 0.9 | `Mutex<T>` for `no_std` sync |
| `lazy_static` | 1.5 | `lazy_static!` with `spin_no_std` |
| `uart_16550` | 0.2 | 16550 UART driver |
| `x86_64` | 0.14.11 | IDT, GDT, TSS, page tables, frame allocator trait |
| `linked_list_allocator` | 0.9.1 | `LockedHeap` global allocator |
| `libm` | 0.2.16 | `expf`, `sqrtf` — funções matemáticas em `no_std` |
| `pic8259` | 0.10.4 | Driver do controlador 8259A — remap IRQ, EOI |

### Known Issues

1. **`spin::Mutex` single-core** — deadlock if exception fires while VGA/heap lock is held.
2. **Heap 100 KB fixo** — tamanho arbitrário, precisa de budget tuning.
3. **MinGW linker required** — `bootimage` needs C linker.

### Next Steps (Sprint 11 — Phase 3 close + SotA integration)

- [x] ADR-0013: Neural OS Executive Summary — Estado da Arte 2026 (MerlionOS, TL/I2_S, ASA/eBPF)
- [x] `docs/roadmap.md` — Roadmap atualizado com Fases 3–7, Padé, MatMul-free LM
- [x] ADR-0013: Estrutura Monorepo (`crates/`) + Design System (AgentProcess, Skill, EventBus traits)
- [x] `docs/roadmap.md` — Ordem de engenharia bare-metal correta (Memória → Scheduler → EventBus → Skills → Planner)

---

## Blueprint Integrado

**Data:** 2026-06-22  
**Status:** Aprovado  

O blueprint de código do neural-os-core está consolidado em:

| Documento | Conteúdo |
|---|---|
| `docs/architecture/0013-neural-os-executive-summary.md` | Manifesto SotA + Monorepo + Rust Traits |
| `docs/roadmap.md` | Ordem de engenharia bare-metal (5 fases) |
| `docs/memory/STATE.md` | Estado atual + pendências |

**Ação Imediata (Concluída):** Bitmap Frame Allocator implementado — 128 KB bitmap, init UEFI, alloc/dealloc, `allocate_contiguous()` para Huge Pages, `hardware_context_tensor() -> [f32; 2]`. 1000 alloc/dealloc estáveis em QEMU. Monorepo workspace criado.

**Sprint 13 (Concluído):** Event Bus IPC — crate `event-bus` com `CapabilityToken`, `Event`, `EventBus` (publish/subscribe). `system_daemon` assina "SYSTEM_READY" e aguarda assincronamente. `hardware_monitor_daemon` publica o evento com token validado. IPC cooperativo entre agentes via `yield_now().await`.

**Sprint 14 (Concluído):** Skill Registry e MCP Layer operacionais. Crate `skill-registry` com `Skill` trait (Send+Sync), `McpManifest` (nome, descrição, tokens requeridos), e `SkillRegistry` — registro central com validação Zero-Trust de `CapabilityToken` antes da execução. `EchoSkill` de demonstração registrada no boot, executada pelo agente `system_daemon` ao receber `SYSTEM_READY`. Saída QEMU verificada: `[SKILL] EchoSkill executada. Output reverso: [3, 2, 1]`.

### Pendências (Sprint 15)

- [ ] Slab allocator — reduzir fragmentação do heap

---

## Roadmap Consolidado

Ver `docs/roadmap.md` para a ordem de engenharia bare-metal completa:

1. **Memória** — Bitmap Allocator + Huge Pages (Sprints 11–12)
2. **Kernel** — Agent Scheduler (Sprint 13)
3. **IPC** — EventBus + Capability Tokens (Sprint 13)
4. **Skills** — Skill Registry + MCP (Sprint 14) ✅
5. **Cognitivo** — Intent Planner + Success Engine (Sprints 15+)
6. **Meta** — Slab Allocator, MatMul-free LM (Sprints 15+)
