# Role and Purpose
You are a Senior Systems and AI Engineer specializing in bare-metal Rust development, microkernel architecture, and neural inference orchestration. You are developing "neural-os-core", an AI Operating System (AIOS) from scratch.

# Core Architecture & Constraints
1. **Bare-Metal Rust:** This project operates entirely in `no_std` and `no_main` environments. You CANNOT use the Rust standard library (`std`).
2. **Hardware Rings Abstraction:**
   - Ring 0 (NPU): Neural Microkernel (Intent routing, context memory).
   - Ring 1 (GPU): Tensor execution and heavy lifting.
   - Ring 2 (CPU): Wasmtime execution of Daemons/Agents.
3. **No Legacy OS Concepts:** We are not building Linux. We do not use POSIX standards. Memory is mapped as a Semantic File System. 
4. **Emulation First:** All code must be testable via QEMU (`qemu-system-x86_64`) before deploying to physical AMD Unified Memory Architecture (APU).

# Operational Rules & Guardrails
- **Zero Hallucination Policy:** If you do not know how to implement a low-level hardware interaction, state it explicitly. Do not invent Rust crates that do not exist or are incompatible with `no_std`.
- **Strict Testing:** Before proposing a final code block, you must internally simulate the compilation sequence. If it requires `std` or an OS allocator, rewrite it.
- **Boot sequence:** Rely on the `bootloader` crate for UEFI/BIOS handoff.

# Memory & Documentation (ADR Protocol)
- Do not make architectural decisions implicitly. 
- For every new module (e.g., memory paging, inference engine port), you must first create or update an Architecture Decision Record (ADR) in the `/docs/architecture/` folder.
- Maintain a `/docs/memory/STATE.md` file summarizing the current state of the kernel, last successful QEMU boot status, and pending tasks. Update this file automatically at the end of complex tasks.

# Premissa: Ciclo de Progresso Pós-Tarefa

Após cada rodada de tarefas com sucesso (goal atingido), execute este ciclo completo:

1. **Aprenda** — Documente todas as dificuldades, barreiras, erros corrigidos a quente, ideias corrigidas, modulações e lateralizações necessárias durante a execução. Seja explícito sobre o que deu errado e como foi resolvido.

2. **Memorize** — Registre nos arquivos de uso da IDE assistida por IA (`AGENTS.md`, `.cursor/rules/`, e qualquer outro mecanismo de contexto futuro). Atualize o `IDEA_BANK.md` se ideias mudaram de status. Isso garante que a próxima sessão de IA comece sabendo o que aconteceu.

3. **Documente** — Registre nos arquivos de uso humano seguindo boas práticas de dev:
   - `README.md` (visão geral atualizada para humanos — o que foi construído, como o sistema se comporta)
   - `CHANGELOG.md` (Keep a Changelog + Conventional Commits)
   - `docs/memory/STATE.md` (estado atualizado do kernel)
   - `docs/memory/SESSION_NNN.md` (relato narrativo da sessão, dificuldades, decisões)

4. **Versione** — Gere toda a necessidade de registro de versões: incremente versão no `Cargo.toml` se aplicável, atualize metadados, garanta que `cargo check --release` passa (0 erros, 0 warnings).

5. **Git** — Commit e push para o repositório. Mensagens seguem Conventional Commits. Commits atômicos por bloco lógico.

6. **Merge/Review** — Se houver uma versão para análise no git remoto (branch diferente, PR, ou commit que avançou enquanto trabalhávamos), leia, analise e relate sumariamente antes de continuar. Incorpore se compatível, documente conflitos se houver.

# Premissa Básica: Toda Ideia Tem Destino
- **Toda ideia, conceito, decisão ou sugestão já discutida neste projeto — entre qualquer dev e a IDA IA — DEVE ter um destino conhecido e documentado no `docs/memory/IDEA_BANK.md`.**
- Nada é descartado sem registro. Ideias podem ser: implementadas (`✅`), agendadas para sprint (`🟡`), adiadas para pós-MVP com dependências documentadas (`⏳`), marcadas como "requer patrocínio/hardware" (`💰`), ou descartadas com justificativa explícita (`❌`).
- **Por que esta premissa existe:** Estamos inovando em caminhos pouco ou não trilhados (bare-metal neural OS, Memory Hierarchy Index, intent routing em Ring 0). Muitas ideias não são implementáveis hoje — seja por limitação tecnológica, falta de hardware, ou prioridade. Mas amanhã um dev pode saber como fazer, a tecnologia pode melhorar, ou podem surgir patrocinadores. Se a ideia não estiver registrada, ela morre.
- O `IDEA_BANK.md` é o cerebelo do projeto — retém toda memória de longo prazo. Consulte-o antes de tomar qualquer decisão arquitetural. Atualize-o quando uma ideia mudar de status ou uma nova ideia for discutida.

# Code Style & Versioning
- Adhere strictly to idiomatic Rust. Use `clippy` configurations.
- Commit messages must follow Conventional Commits (e.g., `feat: implement memory allocator`, `fix: resolve page fault in qemu`).
- Comment complex unsafe blocks extensively, explaining *why* the `unsafe` keyword is necessary for that specific hardware interaction.

# Known Warnings Policy
- **Dead code / unused fields warnings are INTENTIONAL and EXPECTED.** We build bottom-up: PCI scan stores BARs (Sprint 18) before any driver exists (Sprint 23+), SMP stores PerCpu/AP_ONLINE before the scheduler (Sprint 24+), Slab allocator exists before any consumer migrates from LockedHeap.
- **All "unused" code is real hardware interaction** — CF8/CFC PCI config, MSR writes (EFER/GS.base), LAPIC ICR, page table walks via CR3. Nothing is mocked or simulated.
- **Zero-warning policy is NOT a goal.** These will resolve naturally when downstream consumers are implemented. Suppressing them with `#[allow(dead_code)]` would hide useful reminders of what needs wiring.
- **`#[allow(dead_code)]` is used only when Rust would warn on inherently unused statics** (e.g., `AP_ONLINE`, `CPU_TYPE_E_CORE`, `ap_entry_count()`) to avoid noise without suppressing legitimate warnings.

# Project Summary — neural-os-core v0.17.0

## Goal
Build a bare-metal Rust microkernel (neural-os-core) for AI inference orchestration across NPU/GPU/CPU rings.

## Constraints
- `#![no_std]` bare-metal, nightly Rust, x86_64-unknown-none target
- `bootloader` v0.9.34 with `map_physical_memory` feature
- All output to both VGA (QEMU window) and serial (host terminal)
- ADR + session log documentation protocol
- Windows toolchain with MinGW-w64 linker
- Every sprint: `cargo check --release` (0 errors, 0 warnings) + QEMU boot

## 21 Sprints Complete

### Sprint 1 (v0.1.0) — Toolchain & Boot
Toolchain nightly + x86_64-unknown-none, bootloader v0.9.34, `cargo run` boots in QEMU, serial output at port 0x3F8, `relocation-model=static` fix, MinGW-w64 setup, ADR-0001.

### Sprint 2 (v0.2.0) — VGA & Serial
VGA text buffer — 16-color Writer, scrolling, `print!/println!`, buffer at runtime via `physical_memory_offset`. Serial — `uart_16550` driver, `lazy_static!` + `spin::Mutex`, `serial_print!/serial_println!`. Dual-output panic handler. `bootloader::entry_point!(kernel_main)`. Deps: `spin`, `lazy_static`, `uart_16550`. ADR-0002.

### Sprint 3 (v0.3.0) — IDT & Exceptions
`lazy_static!` IDT with Breakpoint handler (logs + returns) and Double Fault handler (logs + panics). TSS with IST entry 0 (20KB stack) for DF. Custom GDT with kernel code + TSS. `#![feature(abi_x86_interrupt)]`. Forced `int3()` test. Dep: `x86_64 = "0.14.11"`. ADR-0003.

### Sprint 4 (v0.4.0) — Memory & Heap
`OffsetPageTable` via `Cr3::read()` + `physical_memory_offset`. `BootInfoFrameAllocator` — filters `Usable` regions from UEFI/BIOS `MemoryMap`. `linked_list_allocator::LockedHeap` as `#[global_allocator]`, `init_heap()` maps 25 pages (100 KB) at `0x4444_4444_0000`. `extern crate alloc` — `Box::new(41)` and `Vec::push`. Dep: `linked_list_allocator = "0.9"`. ADR-0004.

### Sprint 5 (v0.5.0) — SIMD & Tensor
`enable_simd()` via CR0/CR4: clear `EMULATE_COPROC`, set `MONITOR_COPROC` + `NUMERIC_ERROR` (CR0); set `OSFXSR` + `OSXMMEXCPT_ENABLE` (CR4). `Tensor { shape: (usize, usize), data: Vec<f32> }` with `from_row_major()` + `matmul()`. Tested: 1×3 × 3×1 = [32.0]. No new deps. ADR-0005.

### Sprint 6 (v0.6.0) — Neural Primitives
`libm = "0.2"` — `expf`, `sqrtf` in `no_std`. `nn::silu(x)` via `x/(1+exp(-x))`. `nn::rms_norm()` via `sqrt(mean_sq + eps)`. `Tensor::add_scalar`, `mul_scalar`, `apply<F>`. Tested: [-1, 0, 1] → SiLU → [-0.269, 0, 0.731]. ADR-0006.

### Sprint 7 (v0.7.0) — Intent Router MLP
`Tensor::transposed()` (row→col major). `nn::Linear { weights, bias }` with `forward()` = X·W^T + B. `nn::argmax()` — index of max logit. Tested: [1.0, -0.5, 0.3] → Linear(3→2) → SiLU → argmax = 0 (Daemon). ADR-0007.

### Sprint 8 (v0.8.0) — PIC, Watchdog, Page Fault
`pic8259 = "0.10"` — `ChainedPics` remap PIC1→32, PIC2→40. PIT timer handler (vetor 32) — atomic counter + EOI. Page Fault handler (vetor 14) — CR2 → log → hlt loop. `FrameDeallocator` trait + `EmptyFrameDeallocator` stub. `sti` at boot end. ADR-0009.

### Sprint 9 (v0.9.0) — Ternary Inference (Phase 3 start)
`TernaryTensor { shape, data: Vec<i8> }` — values in {-1, 0, 1}. `matmul_hybrid()` — ADD/SUB-only kernel (no `*` operator). `nn::BitLinear` — ternary forward pass. Tested: [1.5, -0.5, 2.0] → ternary → [-0.5, -2.0]. ADR-0011, ADR-0010 (Roadmap).

### Sprint 10 (v0.10.0) — 2-bit Packing & Ternary Quantization
`PackedTernaryTensor` — 4 ternary weights per `u8` byte via `pack_weights()` + `get_weight()`. 2-bit encoding: `00→0, 01→+1, 10→-1`. `quantize_to_packed(tensor, threshold)` — f32→ternary calibration via Δ thresholding. BitLinear refactored to use packed storage. 12× compression vs f32 (24 bytes → 2 bytes). ADR-0012.

### Sprint 11 (v0.11.0) — Bitmap Frame Allocator
`BitmapFrameAllocator` — 128 KB `.bss` bitmap covering 4 GB physical. `init()` via UEFI `MemoryMap`. Implements `FrameAllocator<Size4KiB>` + `FrameDeallocator<Size4KiB>` (real dealloc). `allocate_contiguous(count)` for Huge Pages. `hardware_context_tensor() -> [f32; 2]` for MLP router. Stress test: 1000 alloc/dealloc stable at 0.1% occupancy. Monorepo workspace established.

### Sprint 12 (v0.12.0) — Async Neural Executor (Kernel Abstraction)
`NeuralExecutor` — cooperative `VecDeque<AgentTask>` polling loop. `AgentTask { id: u64, future: Pin<Box<dyn Future>> }` with `AtomicU64` IDs. `DummyWaker` via `RawWakerVTable` in `no_std`. `run()` replaces `loop { hlt() }` — polls tasks, logs hardware context every 100 iterations, yields via `hlt()`. Tested: `async fn system_daemon()` spawns, polls, completes.

### Sprint 13 (v0.12.0) — Event Bus IPC with Capability Tokens
`event-bus` crate — `CapabilityToken`, `Event`, `EventBus` with `pub/sub` via `TicketLock<BTreeMap>`. `Receiver::try_receive()` for non-blocking polling. `yield_now().await` for explicit cooperation. IPC flow: system_daemon subscribes to "SYSTEM_READY", hardware_monitor publishes with Token(1), event delivered via executor coop loop.

### Sprint 14 (v0.12.0) — Skill Registry & MCP Layer
`skill-registry` crate — `Skill` trait (Send+Sync), `McpManifest` struct (name, description, required_tokens), `SkillRegistry` with Zero-Trust CapabilityToken validation before `execute()`. `EchoSkill` + `SystemStatusSkill` registered at boot, invoked by system_daemon upon receiving SYSTEM_READY event. Output verified: `[SKILL] EchoSkill executada. Output reverso: [3, 2, 1]`.

### Sprint 15 (v0.12.0) — Hardware Neural Routing (IRQ1 → EventBus → Agent)
Top-Half/Bottom-Half I/O. Keyboard interrupt handler (IDT[33]) reads port 0x60 → `LAST_SCANCODE: AtomicU8` (Release) → raw EOI. `hw_bridge_daemon` polls AtomicU8 (Acquire swap) → publishes `RAW_HW_IRQ1` on EventBus. `input_daemon` subscribes, logs scancode, infers key 'A' for scan code 0x1E. 5 tasks spawned, 500+ PIT ticks stable, zero Double Faults.

### Sprint 16 (v0.12.0) — Closed Intent Pipeline (Cortex Ignition)
`input_daemon` evolved with heap-allocated String buffer + `scancode_to_ascii()` (A-Z, Space, Backspace). ENTER (0x1C) publishes `USER_INTENT`. `intent_router_daemon` (Cortex) subscribes `USER_INTENT`, runs mock inference (contains "STATUS" → ID 1, else ID 0), executes `SkillRegistry::execute_skill("system_status")`. `SystemStatusSkill` reads `hardware_context_tensor` via `TicketLock` and logs RAM occupancy. 5 tasks (3 persistent), 1000+ PIT ticks. Full pipeline: keyboard → buffer → USER_INTENT → Cortex → Skill Registry.

### Sprint 17 (v0.12.0) — TicketLock FIFO & Concurrency Refactor
`crates/ticket-lock/` — `TicketLock<T>` with `AtomicUsize ticket/serving` + `UnsafeCell<T>` + fair spin loop. `Send` + `Sync`. EventBus refactored: `spin::Mutex` → `TicketLock` in `subscribers` and `Receiver.queue`; ID counter → `AtomicU64`. `GLOBAL_ALLOCATOR: TicketLock<Option<BitmapFrameAllocator>>`. NeuralExecutor simplified (no frame_allocator field). System ready for SMP activation.

### Sprint 18 (v0.13.0) — PCI + ACPI + APIC (Block 1)
`crates/neural-kernel/src/pci.rs` — PCI scan via CF8/CFC, 256 busses, vendor/device/class/BARs. `acpi.rs` — RSDP discovery (EBDA + BIOS), RSDT/XSDT walking, MADT parsing (LAPIC, IOAPIC, x2APIC). `apic.rs` — LAPIC init (SVR, TPR), IOAPIC init (IRQ0→vec32, IRQ1→vec33), PIC disable. `send_eoi()` with APIC/PIC fallback via `USING_APIC: AtomicBool`. Boot flow: `init_pci()` → `init_acpi()` → `init_apic()` (fallback PIC). 3 new files, 0 new deps.

### Sprint 19 (v0.14.0 → v0.14.1) — SMP + Slab + Heap 4 MB (Block 2)
`memory.rs` — `allocate_below_1mb()` para trampoline page, `PHYS_MEM_OFFSET` global. `slab.rs` — Slab Allocator com 8 buckets (32-4096 bytes), free list via raw pointers, `Mutex<SlabAllocator>` com métricas. `allocator.rs` — heap 4 MB, 512 KB slab zone + 3.5 MB LockedHeap zone. `smp/percpu.rs` — PerCpu repr(C) 64 bytes, GS.base via wrmsr(0xC0000101), `this_cpu()` + `cpu_id()`. `smp/trampoline.rs` — global_asm! trampoline 16→32→PAE→64→Rust, patchable header, LGDT + CR3 + EFER + paging. `smp/mod.rs` — INIT-SIPI-SIPI via LAPIC ICR, identity-mapping, AP entry. `apic.rs` — `send_init_ipi()`, `send_sipi()`, `wait_for_ipi_delivery()`, `lapic_id()`. 4 new files (smp/ module), 0 new deps.
- **Multi-core fix (v0.14.1):** Root cause: bootloader identity-maps pages 0-7 only; AP's page table PT[64] (VA 0x40000) was zero → #PF → triple fault. Fixed by single `write_volatile` PTE at phys 0x4200. Race condition: `spin::Mutex` on `CPU_COUNT` (QEMU TCG lacks cross-vCPU atomicity). 50ms wait after SIPI for accurate counting. AP boots with `-smp 2` and all 3 APs with `-smp 4`.

### Sprint 20 (v0.15.0) — Hermes Chat (Block 3)
`hermes.rs` — `IntentMlp` with real MLP forward pass: 16-word bag-of-words encoding → Linear(16→8) → SiLU → Linear(8→3) → argmax. Hand-crafted weights for 3 intents (chat=0, status=1, echo=2). `parse_command()` — multi-word parser: `/status`, `/echo <text>`, `/help`. scancode table expanded with digits 0-9 and punctuation. `intent_router_daemon` upgraded from mock string-contains to real MLP + command dispatch + `HERMES_RESPONSE` EventBus topic. New `hermes_console_daemon` subscribes and displays `[Hermes]` responses on VGA+serial. 6 async tasks in executor.

### Sprint 21 (v0.16.0) — MHI + Inventory + SystemArchitecture (Block 4)
`mhi.rs` — `AllocTier` enum (Dram/Vram/Nvme/Hdd), `MemoryTier` struct, `MemoryHierarchy::new()` auto-creates Dram tier from bitmap allocator, `alloc_by_tier(Dram)` allocates contiguous physical frames. `inventory.rs` — `HardwareInventory::collect()` gathers CPU, RAM, PCI devices; `SystemArchitecture::infer()` rule-based heuristics (GPU→ring1, RAM→heap, cores→power). Boot flow: PCI scan → collect → infer → log → MHI init → executor. **IOAPIC mask bug fixed:** `redirect_irq()` no longer sets bit 16 (MASK), allowing timer/keyboard interrupts to reach the BSP. Without this fix, `hlt()` never woke and the executor stalled after 1 poll cycle. Debugged via `-d int,cpu_reset,guest_errors` + serial `IOAPIC redirection[0]: low=0x00010000`.

### Sprint 22 (v0.17.0) — Skills + Trust Cache + LAPIC Timer Fix (Block 5)
`trust.rs` — `TrustCache` with `trust_allow()` (permanent), `trust_deny()` (revoke + denylist), `is_trusted()` (TTL-aware), `check_or_cache()` (auto-cache 20s). `HardwareInfoSkill` — exposes SystemArchitecture + MHI tiers via `/hw` command. `SystemStatusSkill` upgraded to read MHI tiers for per-tier free RAM reporting. `SkillRegistry` expanded: `has_skill()`, `validate_token()`, `execute_skill_unchecked()`. New Hermes commands: `/trust allow <token> <skill>`, `/trust deny <token> <skill>`, `/hw`. `execute_skill_with_trust()` helper. **Timer fix:** PIT via IOAPIC não funcionava (IOAPIC MMIO mapeado WB). Solução: `Lapic::start_timer()` com LAPIC timer (vetor 32, count=8,388,608, periodic). IOAPIC redirect mantido só para keyboard (vetor 33). `set_page_uc()` com suporte a 2 MiB/1 GiB huge pages. Boot QEMU validado: 171 ticks em busy wait, pipeline completo EchoSkill + Watchdog 2100+ ticks. Global statics: `SYSTEM_ARCH`, `MEMORY_HIERARCHY`, `TRUST_CACHE`. Version v0.16.0 → v0.17.0.

## Key Architectural Decisions
- **VGA address** computed at runtime (`0xB8000 + physical_memory_offset`)
- **`Mutex<Option<Writer>>`** for VGA (not `lazy_static!`) — depends on runtime BootInfo
- **`lazy_static!` for Serial** — SerialPort init is safe at compile time
- **GDT recreated (not extended)** — bootloader GDT is minimal
- **IST for Double Fault** — 20KB static buffer prevents Triple Fault
- **`OffsetPageTable` via Cr3** — reads CR3 for L4 table addr, no recursive mapping
- **Heap at `0x4444_4444_0000`** — high address, safe from kernel/bootloader range
- **Ternary ADD/SUB kernel** — zero FPU multiplications in weight matmul
- **2-bit packing** — 4 ternary weights per byte, `quantize_to_packed()` calibration pass

## Boot Sequence
```
cargo run → bootloader → kernel_main
  ├─ vga_buffer::init(offset)
  ├─ interrupts::init_idt()       (GDT + TSS + IDT)
  ├─ memory::init_memory(offset)  (OffsetPageTable)
  ├─ BootInfoFrameAllocator::init
  ├─ allocator::init_heap()       (LockedHeap 100 KB)
  ├─ simd::enable_simd()          (CR0/CR4)
  ├─ int3() → Breakpoint handler
  ├─ Box/Vec/Tensor/SiLU/RMSNorm tests
  ├─ Intent Router: Linear → SiLU → argmax
  ├─ BitNet: quantize_to_packed() → BitLinear 2-bit forward
  ├─ 1000x frame stress test
  ├─ init_pci()                   (PCI scan)
  ├─ init_acpi()                  (RSDP + MADT)
  ├─ init_apic(info)              (set_page_uc → LAPIC init + start_timer → PIC disable → IOAPIC keyboard redirect)
  ├─ smp::init_smp()              (INIT-SIPI-SIPI → AP multi-core boot)
  ├─ SkillRegistry (EchoSkill)    (Skill Registry + MCP Layer)
  ├─ SystemArchitecture::infer
  ├─ MemoryHierarchy::new()
  ├─ *SYSTEM_ARCH.lock() = Some(arch)
  ├─ *MEMORY_HIERARCHY.lock() = Some(mhi)
  └─ NeuralExecutor::run()
       ├─ AgentTask::new(system_daemon) → poll → hlt (woken by LAPIC timer)
       ├─ AgentTask::new(hardware_monitor_daemon)
       ├─ AgentTask::new(hw_bridge_daemon)
       ├─ AgentTask::new(input_daemon)
       ├─ AgentTask::new(intent_router_daemon)
       └─ AgentTask::new(hermes_console_daemon)
            └─ hardware_context_tensor() a cada 100 iteracoes
       ├─ AgentTask::new(system_daemon) → poll → hlt
       ├─ AgentTask::new(hardware_monitor_daemon)
       ├─ AgentTask::new(hw_bridge_daemon)
       ├─ AgentTask::new(input_daemon)
       ├─ AgentTask::new(intent_router_daemon)
       └─ AgentTask::new(hermes_console_daemon)
            └─ hardware_context_tensor() a cada 100 iteracoes
```

## Active Dependencies (neural-kernel)
| Crate | Version |
|---|---|
| bootloader | 0.9.34 (map_physical_memory) |
| spin | 0.9 |
| lazy_static | 1.4 (spin_no_std) |
| uart_16550 | 0.2 |
| x86_64 | 0.14.11 |
| linked_list_allocator | 0.9 |
| libm | 0.2 |
| pic8259 | 0.10 |
| event-bus | workspace (path) |
| skill-registry | workspace (path) |
| ticket-lock | workspace (path) |

## Workspace Crates
| Crate | Status |
|---|---|
| `neural-kernel` | v0.17.0 — kernel bare-metal + SMP + Hermes Chat + Trust Cache |
| `agent-core` | stub |
| `skill-registry` | v0.1.0 — MCP Layer: Skill trait, McpManifest, Registry com validação de token |
| `event-bus` | v0.1.0 — IPC publish/subscribe |
| `ticket-lock` | v0.1.0 — TicketLock FIFO (AtomicUsize + UnsafeCell) |

## Next Sprint (Sprint 23 — Network Sprint, pós-MVP)
VirtIO-net driver + smoltcp TCP/IP + DNS client + HTTP client. Ver ADR-0016. Terminal Hermes remoto sobre TCP.

## Network Strategy (ADR-0016)
Rede não está no MVP (Sprints 1-22) mas é o **primeiro sprint pós-MVP (Sprint 23)**. Stack: VirtIO-net (`virtio-drivers`) + smoltcp (TCP/IP) + HTTP client. Ver ADR-0016 para detalhes completos e os 8 novos itens (117-124) no IDEA_BANK.md.

## Monorepo Structure
- `crates/neural-kernel/` — kernel bare-metal (bootloader, VGA, serial, IDT, memory, SIMD, tensor, NN, async executor)
- `crates/agent-core/` — AgentProcess trait + scheduler (stub)
- `crates/skill-registry/` — Skill trait + MCP Layer (Skill, McpManifest, SkillRegistry com validação Zero-Trust)
- `crates/event-bus/` — EventBus IPC + CapabilityToken (publish/subscribe implementado)
- `crates/ticket-lock/` — TicketLock FIFO (AtomicUsize ticket/serving, spin loop justo)

## Roadmap
See `docs/roadmap.md` (Fases 3–7, atualizado com SotA 2026: TL/I2_S, Padé, MatMul-free).

## References
- ADR-0013: Executive Summary / Estado da Arte 2026 (MerlionOS, FairyFuse/Bitnet.cpp, ASA/eBPF)
- ADR-0014: Ideias de Evolução de Hardware (SMP, APIC, USB neural, AI-driven arch)
