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
## Sprint 15 — Hardware Neural Routing: IRQ 1 → Event Bus → Agent (Complete)
## Sprint 16 — A Ignição do Córtex: Ciclo de Intenção Fechado (Complete)
## Sprint 17 — Primitiva TicketLock FIFO & Refactor de Concorrência (Complete)
## Sprint 17b — Reescrita de Rota: ADR-0015 + IDEA_BANK.md (Complete)
- Análise de lacunas: 116 itens catalogados (ADR-0014 + roadmap), 0 perdidos
- Master Registry extraído para `docs/memory/IDEA_BANK.md` — documento vivo, standalone, consultável de qualquer clone
- Premissa Básica adicionada ao AGENTS.md: toda ideia tem destino conhecido
- Hierarquia técnica de dependências (8 camadas) para todos os 45 itens ⏳ Pós-MVP e 9 💰 Sponsor
- Mapa de calor: 46% no MVP, 4% agendados, 39% pós-MVP, 8% sponsor, 2% descartados
- README reescrito como manifesto
- Direção validada: 6 blocos em chain → ISO bootável x86-64 UEFI

## Sprint 18 — Block 1: PCI + ACPI + APIC (Complete)
- `crates/neural-kernel/src/pci.rs` — PCI scan via CF8/CFC, vendor/device/class/BARs, bridge support
- `crates/neural-kernel/src/acpi.rs` — RSDP discovery (EBDA + BIOS area), RSDT/XSDT walking, MADT parsing (LAPIC/IOAPIC/x2APIC)
- `crates/neural-kernel/src/apic.rs` — LAPIC init (SVR, TPR, timer disabled), IOAPIC init (redirect timer→vec32, keyboard→vec33), PIC disable, APIC-aware EOI
- `interrupts.rs` — `send_eoi()` com fallback: APIC se `USING_APIC`, senão PIC
- `main.rs` — boot flow: `pci::init_pci()` → `acpi::init_acpi()` → `apic::init_apic()` (ou fallback PIC)

### Current Status

 | Category | Status |
|---|---|---|
| Last QEMU Boot | ✅ Boot OK — VGA + serial + Breakpoint handler + EchoSkill execution |
| Compilation | ✅ `cargo check` — 0 errors, 0 warnings |
| VGA Output | ✅ Mapped via `map_physical_memory`, Writer with `print!/println!` |
| Serial Output | ✅ `uart_16550` driver, `serial_print!/serial_println!` via port `0x3F8` |
| Panic Handler | ✅ Logs to VGA and serial simultaneously |
 | IDT | ✅ Breakpoint + Double Fault + Page Fault + GPF + NP + SS + TS + AC handlers; vetor 33 → `keyboard_interrupt_handler` (porta 0x60, atomic store Release, EOI master); vetores 34-255 com `unhandled_interrupt_handler` (EOI duplo) |
| GDT + TSS | ✅ Custom GDT with TSS descriptor for Double Fault stack switching |
| Page Tables | ✅ `OffsetPageTable` via `Cr3` + `physical_memory_offset` |
| Frame Allocator | ✅ `BitmapFrameAllocator` — bitmap 128 KB, init via UEFI map, alloc/dealloc O(n), 0% leak |
| Bitmap Stress Test | ✅ 1000 alloc/dealloc estáveis — `[KERNEL] Status RAM Tensor: [0.001011, 0.0]` |
| Contiguous Alloc | ✅ `allocate_contiguous(count)` preparado para Huge Pages (Fase 4) |
| Async Neural Executor | ✅ `NeuralExecutor` — `VecDeque<AgentTask>`, cooperative poll, RawWakerVTable |
| AgentTask | ✅ `id`, `Pin<Box<dyn Future>>`, `AtomicU64` ID generation |
| DummyWaker | ✅ `RawWakerVTable` em `no_std` (clone/wake/drop no-ops) |
 | hw_bridge_daemon | ✅ Polls `interrupts::LAST_SCANCODE` (AtomicU8 swap Acquire) → publica `Event { topic: "RAW_HW_IRQ1", payload: [scancode] }` no EventBus |
 | input_daemon | ✅ Subscribe "RAW_HW_IRQ1" → buffer String → scancode→ASCII (A-Z, Space) → ENTER (0x1C) publica USER_INTENT |
 | intent_router_daemon (Córtex) | ✅ Subscribe USER_INTENT → mock inference (text contains "STATUS" → ID 1 else 0) → SkillRegistry::execute_skill("system_status") |
 | SystemStatusSkill | ✅ `Skill` impl: lê `hardware_context_tensor` via `memory::global_hardware_context()` (TicketLock), imprime `"Memoria RAM: {:.2}%. CPU: Agentes Cooperativos."` |
| Ciclo Intenção Fechado | ✅ Teclado → buffer → USER_INTENT → Córtex (NPU mock) → Skill Registry (MCP) → log RAM |
 | TicketLock FIFO | ✅ `ticket-lock` crate — `TicketLock<T>` com `AtomicUsize ticket/serving`, `UnsafeCell<T>`, spin loop, Garantia FIFO, Send+Sync |
| EventBus refactor | ✅ Substituído `spin::Mutex` por `TicketLock` em `EventBus.subscribers` e `Receiver.queue`; ID counter migrado para `AtomicU64` |
| GLOBAL_ALLOCATOR | ✅ `memory::GLOBAL_ALLOCATOR: TicketLock<Option<BitmapFrameAllocator>>` — alocações físicas concorrentes sem data races |
| NeuralExecutor refactor | ✅ `frame_allocator` field removido — executor usa `memory::global_hardware_context()` via TicketLock |
| Top-Half/Bottom-Half I/O | ✅ Interrupt handler = microsecond (port read + atomic store + EOI); HW Bridge em user-context (alloc + publish + EventBus) |
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
 | `crates/neural-kernel/src/interrupts.rs` | IDT, TSS, GDT, Breakpoint + Double Fault + Page Fault + PIT Timer + PIC remap + GPF/NP/SS/TS/AC handlers + `unhandled_interrupt_handler` |
| `crates/neural-kernel/src/memory.rs` | `OffsetPageTable`, `BitmapFrameAllocator`, `init_memory()` |
| `crates/neural-kernel/src/allocator.rs` | `LockedHeap` global allocator, `init_heap()` |
| `crates/neural-kernel/src/simd.rs` | `enable_simd()` — CR0/CR4 FPU/SSE enablement |
| `crates/neural-kernel/src/tensor.rs` | `Tensor` + `TernaryTensor` + `PackedTernaryTensor` |
| `crates/neural-kernel/src/nn.rs` | `silu()`, `rms_norm()`, `Linear`, `BitLinear`, `argmax` |
| `crates/neural-kernel/src/task/mod.rs` | `DummyWaker` — `RawWakerVTable` em `no_std` |
| `crates/neural-kernel/src/task/agent.rs` | `AgentTask` — `id: u64`, `Pin<Box<dyn Future>>` |
| `crates/neural-kernel/src/task/executor.rs` | `NeuralExecutor` — `VecDeque` loop cooperativo |
 | `crates/neural-kernel/src/main.rs` (`hw_bridge_daemon`) | Polls AtomicU8 → EventBus::publish("RAW_HW_IRQ1") |
| `crates/neural-kernel/src/main.rs` (`input_daemon`) | Subscribe "RAW_HW_IRQ1" → buffer String → scancode→ASCII → ENTER flush USER_INTENT |
| `crates/neural-kernel/src/main.rs` (`intent_router_daemon`) | Subscribe USER_INTENT → mock inference → SkillRegistry::execute_skill |
| `crates/neural-kernel/src/main.rs` (`SystemStatusSkill`) | Skill MCP: lê hardware_context_tensor via memory::global_hardware_context() → log RAM |
| `crates/neural-kernel/src/sync/ticket_lock.rs` | Re-exporta `ticket_lock::*` (TicketLock FIFO) |
| `crates/neural-kernel/src/sync/mod.rs` | Module sync — `pub mod ticket_lock;` |
| `crates/ticket-lock/src/lib.rs` | `TicketLock<T>` — AtomicUsize ticket/serving + UnsafeCell<T> + spin loop justo |
| `crates/neural-kernel/src/interrupts.rs` (`keyboard_interrupt_handler`) | IDT[33]: port 0x60 → AtomicU8 Release → raw EOI |
| `crates/event-bus/src/lib.rs` | Re-exports: `EventBus`, `CapabilityToken`, `Event` |
| `crates/event-bus/src/capability.rs` | `CapabilityToken(pub u64)` — validação de permissão |
| `crates/event-bus/src/event.rs` | `Event { id, topic, payload, token }` |
 | `crates/event-bus/src/bus.rs` | `EventBus` — `TicketLock<BTreeMap<>>`, `Arc<TicketLock<VecDeque>>`, `AtomicU64` ID |
| `crates/skill-registry/src/lib.rs` | Re-exports: `SkillRegistry`, `Skill`, `McpManifest` |
| `crates/skill-registry/src/mcp.rs` | `McpManifest { name, description, required_tokens }` |
| `crates/skill-registry/src/skill.rs` | `trait Skill: Send + Sync { manifest(), execute() }` |
| `crates/skill-registry/src/registry.rs` | `SkillRegistry` — `BTreeMap`, register, execute_skill com token validation |
| `Cargo.toml` (root) | Workspace manifest |
| `crates/neural-kernel/Cargo.toml` | Kernel package, deps, bootimage metadata |
| `crates/agent-core/Cargo.toml` | Agent abstraction crate (stub) |
| `crates/skill-registry/Cargo.toml` | WASM Skills crate (stub) |
| `crates/event-bus/Cargo.toml` | IPC EventBus crate — dep `ticket-lock` (no more `spin`) |
| `crates/ticket-lock/Cargo.toml` | Primitiva TicketLock FIFO para sincronização bare-metal |
| `.cargo/config.toml` | Target, runner, `relocation-model=static` |
| `docs/architecture/0001-*.md` to `0013-*.md` | 13 ADRs |
| `docs/memory/STATE.md` | This file |
| `docs/memory/SESSION_001.md` to `SESSION_010.md` | Sprint logs |
| `docs/roadmap.md` | Roadmap geral — fases 3–7, TL/I2_S, Padé, MatMul-free |

### Dependencies

 | Crate | Version | Purpose |
|---|---|---|---|
 | `bootloader` | 0.9.34 | Boot image, `BootInfo`, `map_physical_memory` |
| `skill-registry` | 0.1.0 | MCP layer — `Skill`, `McpManifest`, `SkillRegistry` com validação de token |
| `ticket-lock` | 0.1.0 | TicketLock FIFO — sincronização justa com AtomicUsize + UnsafeCell |
| `spin` | 0.9 | `Mutex<T>` for `no_std` sync (ainda usado no kernel, removido do event-bus) |
| `lazy_static` | 1.5 | `lazy_static!` with `spin_no_std` |
| `uart_16550` | 0.2 | 16550 UART driver |
| `x86_64` | 0.14.11 | IDT, GDT, TSS, page tables, frame allocator trait |
| `linked_list_allocator` | 0.9.1 | `LockedHeap` global allocator |
| `libm` | 0.2.16 | `expf`, `sqrtf` — funções matemáticas em `no_std` |
| `pic8259` | 0.10.4 | Driver do controlador 8259A — remap IRQ, EOI |

### Known Issues

1. **Heap 100 KB fixo** — tamanho arbitrário, precisa de budget tuning.
2. **MinGW linker required** — `bootimage` needs C linker.
3. **IDT coverage** — Vetor 33 (keyboard) tratado; vetores 34-255 têm `unhandled_interrupt_handler` (EOI duplo), seguro mas sem diagnóstico. Futuro: mascarar IRQs não usadas no PIC.

### Next Steps — Sprint 18 (Block 1: PCI + ACPI + APIC)

- [ ] **PCI scan (CF8/CFC)** — enumerar barramento 0..255, ler vendor/device/class/BARs
- [ ] **ACPI RSDP/MADT parser** — descobrir LAPICs presentes, modo PIC vs APIC
- [ ] **LAPIC init (BSP)** — SVR, spurious vector, task priority
- [ ] **IOAPIC init** — redirection entries: keyboard→IRQ1, timer→IRQ0
- [ ] **PIC disables** — mask+remap ou disable via OCW1
- [ ] **ECR (Early Concept Release)** — 6 milestones: PCI scan → ACPI MADT → LAPIC → IOAPIC → PIC disable → Timer+Keyboard via APIC

### Backlog (Sprint 19+, Block 2)

- [ ] **SMP wake** — trampoline 16→32→PAE→64, INIT-SIPI-SIPI, PerCpu struct, GS.base
- [ ] **Slab allocator** — buckets 32-4096 para heap dinâmico
- [ ] **CorePools** — P-core/E-core aware assignment

### Long-term (Pós-MVP)

- Ver `docs/architecture/0015-curso-correcao-mvp.md` Apêndice A para inventário completo de 116 itens.

---

## Blueprint Integrado

**Data:** 2026-06-23  
**Status:** ADR-0015 aprovado como novo plano diretor  

O plano diretor do neural-os-core é agora **ADR-0015 (Curso Correção MVP Hermes)**,
que substitui e absorve o roadmap.md e as ideias avulsas:

| Documento | Conteúdo |
|---|---|
| `docs/architecture/0015-curso-correcao-mvp.md` | ⭐ Novo plano diretor: chain de 6 blocos → MVP Hermes + Master Registry (116 itens) |
| `docs/architecture/0013-neural-os-executive-summary.md` | Manifesto SotA + Monorepo + Rust Traits (histórico) |
| `docs/architecture/0014-ideias-hardware.md` | Inventário de ideias de hardware (histórico, absorvido pelo IDEA_BANK.md) |
| `docs/roadmap.md` | Roadmap original (superseded by ADR-0015 + IDEA_BANK.md) |
| `docs/memory/IDEA_BANK.md` | 🧠 Cerebelo do projeto — 116 ideias catalogadas com status + hierarquia técnica |
| `docs/memory/STATE.md` | Estado atual + pendências |

**Ação Imediata (Concluída):** Bitmap Frame Allocator implementado — 128 KB bitmap, init UEFI, alloc/dealloc, `allocate_contiguous()` para Huge Pages, `hardware_context_tensor() -> [f32; 2]`. 1000 alloc/dealloc estáveis em QEMU. Monorepo workspace criado.

**Sprint 13 (Concluído):** Event Bus IPC — crate `event-bus` com `CapabilityToken`, `Event`, `EventBus` (publish/subscribe). `system_daemon` assina "SYSTEM_READY" e aguarda assincronamente. `hardware_monitor_daemon` publica o evento com token validado. IPC cooperativo entre agentes via `yield_now().await`.

**Sprint 14 (Concluído):** Skill Registry e MCP Layer operacionais. Crate `skill-registry` com `Skill` trait (Send+Sync), `McpManifest` (nome, descrição, tokens requeridos), e `SkillRegistry` — registro central com validação Zero-Trust de `CapabilityToken` antes da execução. `EchoSkill` de demonstração registrada no boot, executada pelo agente `system_daemon` ao receber `SYSTEM_READY`. Saída QEMU verificada: `[SKILL] EchoSkill executada. Output reverso: [3, 2, 1]`.

**Sprint 15 (Concluído):** Roteamento de Hardware Neural — Top-Half/Bottom-Half I/O implementado. Interrupt handler do teclado (IDT[33]) lê porta 0x60 via `x86_64::instructions::port::Port`, armazena scancode em `LAST_SCANCODE: AtomicU8` com `Release`, e envia EOI raw. `hw_bridge_daemon` (contexto normal) faz `swap(0, Acquire)` do atômico e publica `Event { topic: "RAW_HW_IRQ1", ... }` no EventBus. `input_daemon` (agente de I/O) assina o tópico, loga o scancode e infere tecla 'A' para scan code 0x1E. Validação em QEMU: 4 tasks spawnadas (2 completam, 2 ficam em loop de polling), 500+ ticks PIT sem Double Fault. ADR-0013 validado: kernel roteia bytes brutos, daemons interpretam semântica.

**Sprint 16 (Concluído):** A Ignição do Córtex — Ciclo de Intenção Fechado. `input_daemon` evoluído com buffer `String` heap-alocado + `scancode_to_ascii()` (A-Z, Espaço, Backspace). ENTER (0x1C) publica `USER_INTENT`. `intent_router_daemon` (Córtex) assina `USER_INTENT`, faz mock inference (contains "STATUS" → ID 1, else ID 0), e aciona `SkillRegistry::execute_skill("system_status")`. `SystemStatusSkill` lê o `hardware_context_tensor` via `memory::global_hardware_context()` e loga ocupação RAM. 5 tasks spawnadas (3 persistentes: HW Bridge, Input Daemon, Córtex). 1000+ ticks PIT estáveis. Pipeline completo validado em QEMU: Teclado → Buffer → USER_INTENT → Córtex → Skill Registry.

**Sprint 17 (Concluído):** Primitiva TicketLock FIFO implementada. `crates/ticket-lock/` — `TicketLock<T>` com `AtomicUsize ticket/serving` + `UnsafeCell<T>` + spin loop justo. `Send` + `Sync` garantidos. EventBus refatorado: `spin::Mutex` substituído por `TicketLock` em `subscribers` e `Receiver.queue`; contador de ID migrado para `AtomicU64`. BitmapFrameAllocator encapsulado em `memory::GLOBAL_ALLOCATOR: TicketLock<Option<BitmapFrameAllocator>>`. NeuralExecutor simplificado: campo `frame_allocator` removido, executor consulta o estado global via `global_hardware_context()`. Compilação limpa (0 warnings, 0 erros). Sistema preparado para ativação SMP (ADR-0013).

**Sprint 18 (Concluído):** Block 1 — PCI Scan + ACPI MADT + LAPIC/IOAPIC. Três novos módulos criados:
- `pci.rs` — Scan via CF8/CFC, lê vendor/device/class/subclass/prog_if/BARs para cada dispositivo nos 256 busses. Suporte a PCI-PCI bridges (multi-função). `init_pci()` → loga dispositivos.
- `acpi.rs` — RSDP discovery (EBDA 0x80000-0xA0000 + BIOS 0xE0000-0x100000), RSDT/XSDT walking, MADT parsing com LAPIC entries (type 0), IOAPIC entries (type 1), x2APIC detection (type 2). `init_acpi()` → `AcpiInfo` com bases e contagens.
- `apic.rs` — LAPIC init: SVR enable (0x100), TPR=0, timer masked. IOAPIC init: redireciona IRQ0→vec32, IRQ1→vec33. PIC desabilitado (0xFF nas portas de dados). `USING_APIC: AtomicBool` + `apic_eoi()` com fallback via `LAPIC_VIRT_BASE` global.
- `interrupts.rs` — `send_eoi()`: PIC EOI se `!USING_APIC`, APIC EOI se `USING_APIC`.
- Boot flow: `pci::init_pci()` → `acpi::init_acpi()` → `apic::init_apic()` (fallback PIC se ACPI ausente).

## Sprint 19 (Block 2: SMP + Slab) — Concluído

**Data:** 2026-06-23

### Entregas

1. **`memory.rs` — `allocate_below_1mb()`** — Aloca um frame físico em endereço < 1 MiB (frames 0..255). Essencial para página de trampoline real-mode do SMP. `PHYS_MEM_OFFSET` global (AtomicU64) armazena o offset de memória física para acesso de qualquer módulo.

2. **`slab.rs` — Slab Allocator** — Alocador de pools fixos com 8 buckets (32, 64, 128, 256, 512, 1024, 2048, 4096 bytes). Cada bucket tem zona de 64 KB dentro do heap, com free list ligada via `*mut u8`. `SLAB_ALLOCATOR: Mutex<SlabAllocator>` com métricas atômicas (alloc_count, dealloc_count). `unsafe impl Send for SlabAllocator`.

3. **`allocator.rs` — Heap 4 MB** — `HEAP_SIZE` expandido de 100 KB para 4 MB (4.194.304 bytes). Os primeiros 512 KB (8 × 64 KB) são reservados para o Slab Allocator; o restante (~3,5 MB) alimenta o `LockedHeap` geral. Ambos inicializados em `init_heap()`.

4. **`smp/percpu.rs` — PerCpu struct + GS.base** — `PerCpu` repr(C) de 64 bytes: self_ptr, cpu_id, cpu_type, lapic_id, bsp_flag, online, ring. `BSP_PCPU` static inicializado. `init_bsp_percpu()` escreve self_ptr e lapic_id, depois seta GS.base via `wrmsr(0xC0000101)`. `this_cpu()` lê gs:[0] para obter o ponteiro, `cpu_id()` lê gs:[8].

5. **`smp/trampoline.rs` — Trampoline 16→32→PAE→64→Rust** — Assembly `global_asm!` com header de 48 bytes patchable: jmp32, jmp64, cr3, stack, percpu, entry_fn. 16-bit: carrega jmp32_val via CS segment (CS.base = phys_addr após SIPI), seta PE, far jump via push/retf. 32-bit: LGDT (pre-patched), PAE, CR3, EFER.LME, paging, far jump via push/retf. 64-bit: stack, GS.base, `call rax` para entry_fn. `init_trampoline()` copia blob para página < 1 MB e patcha todos os campos.

6. **`smp/mod.rs` — SMP Orchestrator** — `init_smp()`: verifica APIC, lê CR3, obtém BSP LAPIC ID, init PerCpu, aloca trampoline page < 1 MB via `allocate_below_1mb()`, aloca stack (16 KB), identity-mapping da trampoline page via OffsetPageTable, patcha trampoline, envia INIT-SIPI-SIPI. `ap_entry()` — função extern "C" chamada pelo AP ao entrar em 64-bit.

7. **`apic.rs` — IPI functions** — `send_init_ipi()`: ICR low = (5<<8)|(1<<14)|(1<<15)|(3<<18). `send_sipi(vec)`: ICR low = (6<<8)|(3<<18)|vec. `wait_for_ipi_delivery()`: spin até bit 12 (delivery status) clear. `lapic_id()`: lê LAPIC ID register (offset 0x20 >> 24).

8. **`main.rs` — Boot flow atualizado** — `mapper` scoped para evitar aliasing. `mod smp; mod slab;`. Slab metrics exibidas após init. `init_smp()` chamado antes do executor.

### Arquivos criados/modificados neste sprint

| Arquivo | Ação | Linhas |
|---|---|---|
| `src/slab.rs` | Criado | 152 |
| `src/smp/mod.rs` | Criado | 112 |
| `src/smp/percpu.rs` | Criado | 74 |
| `src/smp/trampoline.rs` | Criado | 187 |
| `src/memory.rs` | Modificado (+25) | +`allocate_below_1mb()`, +`PHYS_MEM_OFFSET` |
| `src/allocator.rs` | Modificado | HEAP_SIZE 100KB→4MB, slab init |
| `src/apic.rs` | Modificado (+55) | +IPI functions, +`lapic_id()` |
| `src/main.rs` | Modificado | +mod smp, +mod slab, +init_smp(), mapper scoped |

### Dependências novas
Nenhuma (tudo com crates existentes + `core::arch::asm!` + `global_asm!`).

## Sprint 19 SMP Multi-Core Fix (v0.14.1)

**Data:** 2026-06-23

O Sprint 19 (Block 2) foi completado com a correção do boot multi-core. A causa raiz era que o bootloader identity-mapa apenas as páginas 0-7 (phys 0x0-0x7FFF), mas o AP precisa da página 0x40000 (VA 0x400A4) para executar o trampoline_64. PT[64] estava zero → #PF → triple fault.

### Correções Aplicadas

1. **Identity-map PTE** — `write_volatile` em `phys_offset + 0x4200` escreve PTE `0x40000 | 0x003` (Present|Write). Uma única instrução.

2. **Race condition CPU_COUNT** — `spin::Mutex` protege `fetch_add` porque QEMU TCG não garante atomicidade cross-vCPU. Todos os APs liam o mesmo valor.

3. **50ms busy-wait** após segundo SIPI para contagem precisa.

4. **Slab Allocator** — `SLAB_CHUNK_SIZE` = bucket_size (não alinhado para 8). Corrigido o `put()` que corrompia a free list.

5. **asm! memcpy** — `copy_nonoverlapping` → `asm!("rep movsb")` para evitar `native_memcpy`.

### Resultados

- `-smp 2`: ✅ `APs acordados: 1`
- `-smp 4`: ✅ `APs acordados: 3`
- `qemu_trace.log`: zero `check_exception`
- `cargo check --release`: 0 erros, 0 warnings
- `cargo bootimage --release`: 0 erros

## Sprint 20 (Block 3: Hermes Chat) — Concluído (v0.15.0)

**Data:** 2026-06-23

### Entregas

1. **`hermes.rs` — Hermes Chat console module** — `IntentMlp` com classificação MLP real: bag-of-words (16 palavras do vocabulário) → Linear(16→8) → SiLU → Linear(8→3) → argmax. Pesos artesanais para 3 intenções: chat (0), status (1), echo (2). `parse_command()` — analisador multi-palavras com suporte a comandos `/status`, `/echo <texto>`, `/help`, `/stats`, `/mem`.

2. **`scancode_to_ascii()` expandida** — agora reconhece dígitos 0-9 (scancodes 0x02-0x0B) e pontuações (`- = [ ] ; ' ` \ , . /`). Necessário para digitar comandos como `/status`.

3. **`intent_router_daemon` atualizado** — substitui o mock `text.contains("STATUS")` pelo pipeline real:
   - `parse_command()` tenta encaixar comando explícito primeiro
   - Se não for comando, `INTENT_MLP.classify()` via MLP real
   - Intent 1 (status) → `SystemStatusSkill` via SkillRegistry
   - Intent 2 (echo) → `EchoSkill` via SkillRegistry
   - Intent 0 (chat) → resposta padrão
   - Respostas publicadas no tópico `HERMES_RESPONSE` do EventBus

4. **`hermes_console_daemon`** — nova task que assina `HERMES_RESPONSE` e exibe `[Hermes] <resposta>` no VGA e serial. 6 tasks no executor.

### Arquivos criados/modificados neste sprint

| Arquivo | Ação |
|---|---|
| `src/hermes.rs` | Criado (165 linhas) |
| `src/main.rs` | Modificado — +mod hermes, scancodes expandidas, intent_router upgrade, console daemon |

### Dependências novas
Nenhuma (Tensor + Linear + SiLU já existentes no kernel).

## Sprint 21 (Block 4: MLP + MHI + Auto-detecção) — Concluído (v0.16.0)

**Data:** 2026-06-23

### Entregas

1. **`mhi.rs` — Memory Hierarchy Index** — `AllocTier` enum (`Dram`/`Vram`/`Nvme`/`Hdd`), `MemoryTier` struct com `kind`, `capacity_bytes`, `bandwidth_mbs`, `latency_ns`, `name`. `MemoryHierarchy::new()` cria um tier DRAM a partir do bitmap frame allocator. `alloc_by_tier(Dram)` — aloca frames físicos contíguos via `GLOBAL_ALLOCATOR` e retorna `PhysAddr`. Outros tiers retornam `None` com log (drivers não implementados).

2. **`inventory.rs` — HardwareInventory + SystemArchitecture** — `HardwareInventory::collect(pci_devices, acpi_info)` reúne: CPU count, RAM total (via `usable_memory_bytes()`), PCI devices, detecção de VirtIO-net/GPU, NVMe, XHCI. `SystemArchitecture::infer(inv)` com heurísticas baseadas em PCI class (GPU → ring1_mode=1), RAM (heap 2048/512/64 MB), CPU count (power_mode=1 se >4 cores). Ambos estruturas puras — sem pesos treinados (item #51 ⏳).

3. **Boot flow adaptativo** — `main.rs` agora executa: PCI scan → HardwareInventory::collect() → SystemArchitecture::infer() → log em VGA+serial → MHI init com tiers → NeuralExecutor. Exemplo de saída serial: `[ARCH] System architecture: ring0=0 ring1=0 heap=2048MB trust=1 power=0 tensor=0`; `[MHI] 1 tier(s). Best: Dram (X bytes avail)`.

4. **`memory.rs` — `usable_memory_bytes()`** — método público em `BitmapFrameAllocator` retorna `usable_frames * 4096` para a MHI e inventory.

### Arquivos criados/modificados neste sprint

| Arquivo | Ação |
|---|---|
| `src/mhi.rs` | Criado (80 linhas) |
| `src/inventory.rs` | Criado (80 linhas) |
| `src/memory.rs` | Modificado — +`usable_memory_bytes()` |
| `src/main.rs` | Modificado — +mod mhi, +mod inventory, boot flow adaptativo |
| `Cargo.toml` | v0.15.0 → v0.16.0 |

### Bug Fix — IOAPIC redirect_irq mask bit

**Descoberto durante teste QEMU:** O executor parava após 1 ciclo de polling porque `hlt()` nunca recebia interrupção de timer.

**Causa raiz:** `apic.rs:87` — `redirect_irq()` montava o redirection entry com `(1u32 << 16)`, que é o bit MASK no IOAPIC IOREDTBL. Todos os 24 redirecionamentos (IRQ0-23) ficavam mascarados. Confirmado por `IOAPIC redirection[0]: low=0x00010000` no log serial.

**Correção:** Removido `| (1u32 << 16)` — redirecionamentos agora ficam com bit 16 = 0 (unmasked). Timer IRQ0 → vetor 32, teclado IRQ1 → vetor 33. Pipeline executor roda completo.

### Dependências novas

Nenhuma (tudo com crates existentes + PCI scan + bitmap allocator já implementados).

## Sprint 22 (Block 5: Skills + Trust Cache) — Concluído (v0.17.0)

**Data:** 2026-06-23

### Entregas

1. **TrustCache** — `crates/skill-registry/src/trust_cache.rs`: `TrustEntry` com token, granted_at, ttl_ticks; `TrustCache::grant()` (com TTL override), `revoke()`, `is_trusted()`. `DEFAULT_TTL_TICKS = 1800` (~100s). `TRUST_CACHE` global no kernel.

2. **SystemStatusSkill upgrade** — agora consome MHI: `MemoryHierarchy::new()` exibe RAM por tier (nome, capacidade). `hardware_context_tensor()` retorna `[ratio, allocated_count]`. Saída: `[SYSSTATUS] RAM: 0.29% used | MHI: 1 tier(s), ~2042 MB`.

3. **HardwareInfoSkill** — nova skill registrada no `SKILL_REGISTRY`. Lê `GLOBAL_ARCH` (SystemArchitecture) e `MemoryHierarchy::new()` para expor CPU cores, GPU, heap, MHI tiers, bandwidth. Saída: `[HWINFO] CPU: 2 core(s) | GPU: ativo | Heap: 512 MB`.

4. **GLOBAL_ARCH** — `spin::Mutex<Option<SystemArchitecture>>` armazena a arquitetura detectada para consumo por skills e daemons.

5. **Boot flow** — `[ARCH]` agora inclui `PCI devices: {len}`: mostra contagem de dispositivos PCI detectados.

### Arquivos criados/modificados

| Arquivo | Ação |
|---|---|
| `crates/skill-registry/src/trust_cache.rs` | Criado (55 linhas) |
| `crates/skill-registry/src/lib.rs` | Modificado — +mod trust_cache, +exports |
| `crates/neural-kernel/src/main.rs` | Modificado — HardwareInfoSkill, GLOBAL_ARCH, TRUST_CACHE, SystemStatusSkill upgrade |
| `crates/neural-kernel/src/memory.rs` | Modificado — +allocated_frame_count(), hardware_context_tensor() retorna allocated_count |
| `Cargo.toml` | v0.16.0 → v0.17.0 |

### Pendências (Sprint 23 — Network Sprint, pós-MVP)

Ver ADR-0016 para detalhes completos.

### Pendências (Sprint 23 — Network Sprint, pós-MVP)

Ver ADR-0016 para detalhes completos.

---

## Roadmap — Chain de 8 Blocos (ADR-0015 + ADR-0016)

A rota atual é a **chain de 6 blocos** (ADR-0015) + **Network Sprint** (ADR-0016).

| Bloco | Nome | Sprints | Pré-requisito | Entrega |
|---|---|---|---|---|---|
| 0 | Genesis (concluído) | 1–17 | — | Kernel bootável, EventBus, Skills, Executor, PIC, keyboard |
| 1 | PCI + ACPI + APIC | 18 (concluído) | Block 0 | PCI scan → MADT → LAPIC → IOAPIC → PIC disable → APIC I/O |
| 2 | SMP + Slab Allocator | 19 (concluído) | Block 1 | PerCpu, trampoline, INIT-SIPI-SIPI, slab heap 4 MB |
| 3 | Hermes Chat | 20 (concluído) | Block 2 | MLP intent router, commands, console daemon |
| 4 | MLP + MHI + Auto-detecção | 21 (concluído) | Block 3 | MemoryHierarchyIndex, alloc_by_tier, SystemArchitecture MLP |
| 5 | Skills + Trust Cache | 22 | Block 4 | system_status, hardware_info, trust_cache |
| MVP | **Neural OS Hermes ISO** | 22 | Block 5 | ISO bootável x86-64 UEFI com chat neural |
| 6 | **Network Sprint** | 23 | MVP | VirtIO-net + smoltcp + DNS + HTTP |
| 7 | NVMe + SFS persistente | 24 | Network | Armazenamento durável |
| 8+ | WASM + TLS + multi-agent | 25+ | SFS | Skills WASM, HTTPS, agentes de rede |

Para inventário completo de 116 itens com status individual: ver `docs/memory/IDEA_BANK.md` (documento vivo, standalone).
