# ADR-0015: Neural OS Mainnet — Rota para o MVP Hermes

**Status:** Draft  
**Date:** 2026-06-23  
**Driver:** Reorganizar o neural-os-core em torno de um MVP concretamente entregável — uma imagem ISO bootável em qualquer x86-64 que se comporta como o Hermes Agent da Nous Research, mas em bare-metal Rust, sem Linux, sem Python, sem Docker.

---

## 0. A Visão: Neural OS = Hermes Agent Bare-Metal

O Hermes Agent (Nous Research) é um assistente AI que roda **sobre** um SO existente (Linux, macOS, Windows). Ele precisa de Python, uv, Node.js, um LLM, e centenas de MB de dependências.

O Neural OS "Hermes" é o **inverso**: o assistente AI **é o SO**. Não há camada abaixo. O kernel boota diretamente num terminal conversacional onde o usuário digita intents e o sistema:

1. **Detecta o hardware** — CPU, RAM, PCI, topologia
2. **Configura a si mesmo** — um MLP ternário decide heap, rings, trust, power
3. **Classifica a intenção** — o Neural Cortex (MLP 3→2) roteia o comando
4. **Executa a skill** — EchoSkill, SystemStatus, PCI scan, power control

| Camada | Hermes Agent (Nous) | Neural OS Hermes (MVP) |
|---|---|---|
| Kernel | Linux | Neural microkernel (Rust no_std) |
| Runtime | Python 3.11 + uv | MLP ternário + EventBus |
| Interface | TUI sobre terminal | VGA text + Serial (boota direto) |
| Skills | Python em ~/.hermes/skills/ | Rust traits + WASM (futuro) |
| Memória | FTS5 + LLM summarization | RAM + Trust Cache |
| Modelo | GPT-4, Claude, Llama, etc. | MLP ternário 512→256→64→9 |
| Tamanho | ~500 MB + modelo | < 2 MB (kernel + pesos) |
| Dependências | Python, uv, Node.js, ripgrep | Zero. É o SO. |

**O que torna isso radical:** O Hermes precisa de um datacenter (ou pelo menos uma GPU) para rodar um LLM. O Neural OS Hermes roda num core x86 de 2010 com 512 MB de RAM. O "cérebro" é um MLP ternário de 37 KB que classifica intents em microssegundos.

---

## 1. A Chain: Cada Bloco Depende do Anterior

Como uma blockchain, cada sprint é um bloco que:
- **Depende do hash do bloco anterior** (não pode pular)
- **Produz um entregável verificável** (teste em QEMU)
- **Tem risco documentado** (mitigação se falhar)
- **Alimenta o próximo bloco** (novas capacidades)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        NEURAL OS MAINNET                                  │
│  Block 0 (Genesis) ──→ Block 1 ──→ Block 2 ──→ Block 3 ──→ Block 4 ──→ Block 5 ──→ MVP │
│  Sprint 17          Sprint 18    Sprint 19    Sprint 20    Sprint 21    Sprint 22       │
│  "O que temos"      "PCI+ACPI"   "SMP+Heap"   "Hermes UI"  "Auto-Conf"  "Skills"        │
└─────────────────────────────────────────────────────────────────────────┘
```

Cada bloco é auto-contido: se um bloco falha, os seguintes não são construídos até ele ser resolvido. Não há "vamos fazer em paralelo" — a dependência é física.

---

## 2. Genesis Block — Onde Estamos (Sprint 17)

**Block height:** 0  
**Hash do bloco anterior:** N/A (gênesis)  
**Entregável:** Kernel funcional em QEMU com VGA, serial, IDT, heap, FPU, Tensor, EventBus, 5 agentes

### O que funciona (ativo da chain):

```
✅ VGA 80×25 + Serial 0x3F8           → output dual
✅ IDT (8 exceptions + PIT + keyboard) → captura de erros
✅ BitmapFrameAllocator (128 KB)       → 4 GB cobertura, alloc+dealloc
✅ LockedHeap 100 KB                   → alloc crate + Vec/Box/String
✅ FPU/SSE + Tensor f32                → matmul, SiLU, RMSNorm
✅ TernaryTensor + PackedTernaryTensor → 2-bit, 12× compressão
✅ PIC 8259A + PIT timer               → ~18 Hz watchdog
✅ TicketLock FIFO                      → sync SMP-safe
✅ EventBus + CapabilityToken          → IPC pub/sub
✅ Skill Registry + MCP Layer          → EchoSkill, SystemStatusSkill
✅ NeuralExecutor + 5 agentes           → HW bridge, input, cortex
```

### O que falta (dívida técnica):

```
❌ PCI config space        → sem descoberta de hardware
❌ APIC (LAPIC + IOAPIC)   → preso em PIC single-core
❌ ACPI (RSDP → MADT)      → sem topologia de cores
❌ PerCpu + GS.base        → core não sabe quem é
❌ alloc_below_1mb()       → trampoline não pode ser alocado
❌ Slab allocator          → heap fixo 100 KB
❌ scancode completo       → só A-Z, Space, Backspace
❌ Chat loop persistente   → input_daemon termina
❌ MLP de arquitetura      → nenhum inventário coletado
❌ Skills de hardware      → não leem PCI, não controlam power
```

### Riscos do bloco atual:

| Risco | Impacto |
|---|---|
| LockedHeap 100 KB fragmenta com uso contínuo | Médio — EventBus + skills alocam strings |
| PIC 8259A não escala além de 1 core | Crítico — bloqueia SMP |
| Sem PCI, todo hardware é "adivinhado" | Crítico — bloqueia descoberta |

---

## 3. Block 1 — PCI Scan + ACPI + APIC BSP (Sprint 18)

**Block height:** 1  
**Hash anterior:** `genesis(Sprint17)`  
**Objetivo:** Substituir PIC cego por descoberta real de hardware via PCI + ACPI

### O que este bloco entrega:

```
PCI config space (CF8/CFC port I/O)
  ├── scan completo: vendor, device, class, BARs
  ├── loga todos os dispositivos encontrados
  └── alimenta o HardwareInventory (Block 5)

ACPI RSDP → RSDT → MADT
  ├── descobre LAPIC IDs (quantos cores existem)
  ├── descobre IOAPIC (endereço base)
  └── fallback: hardcode QEMU se ACPI ausente

APIC Local (LAPIC) init no BSP
  ├── enable via MSR (IA32_APIC_BASE)
  ├── spurious interrupt vector
  ├── LAPIC timer (substitui PIT)
  └── EOI via write ao LAPIC (não mais ao PIC)

interrupts.rs migrado:
  ├── PIT handler removido (substituído por LAPIC timer)
  ├── PIC EOI removido (substituído por LAPIC EOI)
  ├── PIC 8259A desligado (mask all IRQs)
  └── keyboard handler migrado para LAPIC mode
```

### Arquivos criados/modificados:

| Arquivo | Ação |
|---|---|
| `crates/neural-kernel/src/pci.rs` | **Novo** — PCI config access + scan |
| `crates/neural-kernel/src/acpi.rs` | **Novo** — RSDP find → RSDT → MADT parser |
| `crates/neural-kernel/src/apic.rs` | **Novo** — LAPIC init, timer, EOI, IPI |
| `crates/neural-kernel/src/interrupts.rs` | Modificado — handlers usam LAPIC EOI, PIC removido |
| `crates/neural-kernel/src/main.rs` | Modificado — boot flow: PCI → ACPI → APIC |

### Teste de aceite (QEMU):

```
[PCI] Scanning config space...
  Bus 0, Dev 0, Func 0: 8086:1237 (Host bridge)
  Bus 0, Dev 1, Func 0: 8086:7000 (ISA bridge)
  Bus 0, Dev 1, Func 1: 8086:7010 (IDE controller)
  Bus 0, Dev 2, Func 0: 1234:1111 (VGA controller)
  Bus 0, Dev 3, Func 0: 1AF4:1041 (VirtIO net)
  ... 12 devices found

[ACPI] MADT: 2 LAPICs found (BSP + 1 AP)
[APIC] Local APIC enabled (ID: 0, version: 0x14)
[APIC] LAPIC timer at 1 kHz
[TIMER] Interrupts via LAPIC, PIC9 disabled
```

### Riscos e mitigações:

| Risco | P | I | Mitigação |
|---|---|---|---|
| CF8/CFC não funciona em hardware real (porta bloqueada pelo firmware) | M | A | Fallback para tabela hardcoded QEMU; MSR-based PCI na Intel |
| ACPI RSDP não encontrado (UEFI sem tabelas legacy) | B | A | Fallback: `-no-acpi` no QEMU; hardcode MADT para QEMU |
| LAPIC init causa Triple Fault (MSR protegido) | M | C | Manter PIC como fallback via flag `--use-pic` |
| PCI scan lê BARs mas não mapeia MMIO corretamente | M | M | Ignorar BARs no MVP; apenas logar descoberta |

### Depende de:
- Nada novo (port I/O + memory scan são puros em no_std)

### Bloqueia:
- Block 2 (SMP precisa de LAPIC + MADT)
- Block 5 (HardwareInventory precisa de PCI scan)

---

## 4. Block 2 — PerCpu + SMP + Slab Allocator (Sprint 19)

**Block height:** 2  
**Hash anterior:** `block1(PCI+ACPI+APIC)`  
**Objetivo:** Múltiplos cores cooperando + heap dinâmico

### O que este bloco entrega:

```
PerCpu struct + GS.base segment register
  ├── cada core sabe seu ID, tipo (P/E), ring, fila
  ├── acesso via core_local!() macro
  └── stack separada por core (64 KB cada)

alloc_below_1mb() no BitmapFrameAllocator
  ├── aloca página física em endereço < 1 MB
  └── necessário para trampoline real-mode

Trampoline assembly (trampoline.asm)
  ├── 16-bit → 32-bit (GDT temporária)
  ├── 32-bit → 64-bit (PAE + CR3 do BSP)
  └── salta para ap_entry_point() Rust

INIT-SIPI-SIPI via LAPIC ICR
  ├── ICR Low (0x300) + ICR High (0x310)
  ├── vetor = trampoline_page / 0x1000
  ├── delay 10ms → SIPI → delay 200µs → SIPI
  └── AP acorda → executa trampoline → ap_entry_point()

Slab allocator
  ├── buckets: 32, 64, 128, 256, 512, 1024, 2048, 4096
  ├── aloca do slab se tamanho <= 4096
  ├── fallback para LockedHeap se > 4096
  └── reduz fragmentação do heap

Heap expandido de 100 KB para 4 MB
  ├── mapeia 1024 páginas em 0x4444_4444_0000
  └── slab usa este heap como pool
```

### Arquivos criados/modificados:

| Arquivo | Ação |
|---|---|
| `crates/neural-kernel/src/smp/percpu.rs` | **Novo** — PerCpu struct, core_local!, init_percpu() |
| `crates/neural-kernel/src/smp/trampoline.asm` | **Novo** — assembly 16→64-bit |
| `crates/neural-kernel/src/smp/mod.rs` | **Novo** — SMP init, wake_aps(), ap_entry_point() |
| `crates/neural-kernel/src/apic.rs` | Modificado — send_ipi(), wake_up_ap() |
| `crates/neural-kernel/src/memory.rs` | Modificado — alloc_below_1mb() |
| `crates/neural-kernel/src/allocator.rs` | Modificado — SlabAllocator, heap 4 MB |
| `crates/neural-kernel/src/main.rs` | Modificado — init_smp() + init_slab() no boot |

### Teste de aceite (QEMU):

```
$ qemu-system-x86_64 -smp 2 -serial stdio

[PERCPU] Core 0: BSP, type=Performance, ring=0
[SMP] Waking AP (LAPIC ID: 1, trampoline at 0x8000)
[SMP] AP (Core 1) online e cooperando.
[PERCPU] Core 1: AP, type=Performance, ring=2
[SLAB] Buckets: 32/64/128/256/512/1024/2048/4096
[SLAB] Heap: 4 MB at 0x4444_4444_0000
[NEURAL] 2 cores disponiveis
```

### Riscos e mitigações:

| Risco | P | I | Mitigação |
|---|---|---|---|
| Trampoline assembly não entra em 64-bit | A | C | Testar em QEMU + GDB step-by-step. Fallback single-core. |
| AP não recebe SIPI (ICR ignorado) | M | C | Verificar LAPIC version; tentar INIT sozinho. |
| Slab fragmenta mais que LockedHeap | M | M | Benchmark. Se pior, remover slab, manter LockedHeap com 4 MB. |
| Heap 4 MB esgota frames do BitmapAllocator | B | M | 4 MB = 1024 frames. Bitmap de 128 KB cobre 1M frames. Folga enorme. |

### Depende de:
- Block 1 (APIC BSP + MADT para LAPIC IDs)

### Bloqueia:
- Block 3 (chat loop precisa de heap >100 KB para histórico)
- Block 5 (SMP alimenta HardwareInventory)

---

## 5. Block 3 — Chat Loop + Intent Router (Sprint 20)

**Block height:** 3  
**Hash anterior:** `block2(SMP+Slab)`  
**Objetivo:** Terminal conversacional funcional — o "Hermes" começa a existir

### O que este bloco entrega:

```
scancode_to_ascii() completo
  ├── A-Z, a-z, 0-9, símbolos (!@#$%...)
  ├── Shift, Caps Lock, Ctrl (prefixos)
  ├── Backspace, Enter, Tab
  └── buffer circular de 256 chars

Chat loop persistente
  ├── input_daemon não termina nunca
  ├── prompt "> " no VGA + serial
  ├── echoa o que o usuário digita
  ├── histórico em buffer circular (últimas 16 linhas)
  ├── Enter → submete intent para o Cortex
  └── scrolling no VGA quando enche a tela

Serial input line read
  ├── lê linha completa do serial (não só tecla)
  ├── ideal para SSH via qemu -serial stdio
  └── trata backspace, enter, caracteres especiais

Intent Router real (MLP 3→2 atual → 3-class)
  ├── classe 0: "system_status" → SystemStatusSkill
  ├── classe 1: "skill_request" → lista/comando de skills
  ├── classe 2: "unknown" → "Não entendi. Digite help."
  └── treinado offline com frases curtas em português/inglês

Skill dispatch integrado ao chat
  ├── intent_router_daemon classifica → executa → retorna
  ├── output formatado no VGA + serial
  └── feedback imediato ("[SKILL] RAM: 2.0% ocupada")

Help skill
  ├── "help" ou "?" → lista skills disponíveis
  ├── "help system_status" → descrição da skill
  └── "help hardware" → hardware detectado no boot
```

### Arquivos criados/modificados:

| Arquivo | Ação |
|---|---|
| `crates/neural-kernel/src/keyboard.rs` | **Novo** — scancode_to_ascii() completo, shift states |
| `crates/neural-kernel/src/console.rs` | **Novo** — chat loop, prompt, history buffer |
| `crates/neural-kernel/src/cortex.rs` | **Novo** — IntentRouter 3-class, dispatch |
| `crates/neural-kernel/src/serial.rs` | Modificado — read_line() |
| `crates/neural-kernel/src/vga_buffer.rs` | Modificado — scroll up, cursor management |
| `crates/neural-kernel/src/nn.rs` | Modificado — IntentRouter struct (MLP 3→2 wrapper) |
| `crates/neural-kernel/src/main.rs` | Modificado — spawn console_daemon no lugar de input_daemon |

### Experiência do usuário:

```
[ARCH] Neural OS ready. Type your intent.
> status
[SKILL] RAM: 2.1% ocupada. CPU: 2 cores cooperando.
> help
[CORTEX] Skills disponiveis: echo, system_status, help
> echo 1 2 3
[SKILL] echo: 3 2 1
> pci list
[CORTEX] Skill nao encontrada. Digite 'help' para lista.
> 
```

### Riscos e mitigações:

| Risco | P | I | Mitigação |
|---|---|---|---|
| scancode_to_ascii() faltando teclas (setas, F1-F12) | B | B | Apenas essenciais no MVP. Setas/F-key = pós-MVP. |
| Chat loop deadlock com EventBus | B | M | Receiver::try_receive() non-blocking; poll nunca bloqueia. |
| Buffer de histórico overflow | B | B | Buffer circular fixo de 16 linhas (~4 KB). |
| MLP 3-class confunde comandos similares | M | M | Aceitar no MVP. Melhorar dataset de treino no pós-MVP. |

### Depende de:
- Block 2 (heap 4 MB para buffer de histórico + strings)

### Bloqueia:
- Block 5 (console é a UI que expõe as skills)
- Block 6 (skills precisam do dispatch do chat)

---

## 6. Block 4 — MLP de Arquitetura + Auto-Config (Sprint 21)

**Block height:** 4  
**Hash anterior:** `block3(Hermes-Chat)`  
**Objetivo:** O sistema vê o hardware e se configura sozinho — é aqui que a "AI" do OS realmente aparece

### O que este bloco entrega:

```
HardwareInventory::collect(boot_info)
  ├── CPU: brand, cores (P/E/HT), freq, cache, features (CPUID leafs)
  ├── RAM: total, speed, ECC (do MemoryMap do bootloader)
  ├── PCI: devices (do scan do Block 1)
  ├── Accelerators: NPU/GPU detectados via PCI class + DID
  └── Storage: NVMe/VirtIO detectados via PCI class

────────── MEMORY HIERARCHY INDEX (MHI) ──────────
  ├── struct MemoryTier { device, kind, capacity, bandwidth, latency, is_unified, numa_node }
  ├── struct MemoryHierarchy { tiers: Vec<MemoryTier> }  ← ordenado do mais rápido ao mais lento
  ├── enum AllocTier { Dram, Vram, Nvme, Hdd }
  ├── fn alloc_by_tier(tier: AllocTier, size) -> Option<PhysAddr>
  │     └── MVP: só AllocTier::Dram implementado (BitmapAllocator)
  │     └── Vram → None (disponível em Sprint 23+)
  │     └── Nvme → None (disponível em Sprint 24+)
  │
  ├── MemoryHierarchy populado no boot com tiers detectados:
  │     Ex: { DRAM: 16384 MB @ 19.2 GB/s } (só DRAM no MVP)
  │     Ex: { VRAM: 4 GB @ 112 GB/s, DRAM: 16 GB @ 19.2 GB/s, NVMe: 256 GB @ 3.5 GB/s }
  │
  └── Visível ao usuário via "status memory":
        [CORTEX] Memory Hierarchy:
          tier[0] DRAM: 16384 MB @ 19.2 GB/s ← ativo
          tier[1] VRAM: GTX 1050 (4 GB) ← sem driver
          tier[2] NVMe: 256 GB ← sem driver
────────── FIM MHI ──────────

MLP 512→256→64→9 ternário (pesos embutidos no kernel)
  ├── Embedding do brand name: [f32; 32]
  ├── Core counts, freqs, cache: normalizados
  ├── Features one-hot (AVX-512, AMX, VNNI, x2APIC, hybrid...)
  ├── RAM total, PCI count, accelerators
  └── 12 saídas categóricas (argmax por grupo):
        ring0: {software, NPU, P_core}
        ring1: {P_cores, GPU, hybrid}
        ring2: {E_cores, idle_P, mixed}
        p_cores_for_ring1: u8
        sfs_policy: {NVMe_only, tiered, all_ram}
        heap_size_mb: u16
        heap_tier: {Dram, Vram, Nvme}          ← guia MHI
        tensor_tier: {Dram, Vram, Nvme}        ← guia MHI
        kv_cache_tier: {Dram, Vram, Nvme}      ← guia MHI
        trust_default: {deny, allow_known, allow}
        power_policy: {performance, balanced, low}
        sfs_active_tier: {Dram, Nvme}          ← guia MHI

SystemArchitecture → Config dinâmica
  ├── init_heap(arch.heap_size_mb, arch.heap_tier)
  │     └── No MVP: heap_tier ignorado, aloca na DRAM
  │     └── Pós-MVP: aloca no tier decidido
  ├── init_smp(arch.ring0, ring1, ring2, arch.p_cores_for_ring1)
  ├── init_npu(arch.ring0_target) — se NPU, tenta init; fallback software
  ├── init_trust(arch.trust_default)
  ├── init_power(arch.power_policy)
  └── init_mhi(arch.heap_tier, arch.tensor_tier, ...)
        └── Configura MemoryHierarchy com decisões do MLP

Boot flow adaptativo
  ├── HardwareInventory::collect() antes de qualquer configuração
  ├── infer_architecture() → SystemArchitecture
  ├── init_mhi() — MHI populado com tiers detectados + decisões do MLP
  ├── init_*() na ordem determinada pela arch
  └── Boot log exibe a decisão do MLP + MHI

Hardware query skills
  ├── "status" → info geral + skills disponíveis
  ├── "status hardware" → inventário completo
  ├── "status arch" → SystemArchitecture ativa
  └── "status memory" → MHI completo (tiers, decisões, drivers ausentes)
```

### Arquivos criados/modificados:

| Arquivo | Ação |
|---|---|
| `crates/neural-kernel/src/arch/inventory.rs` | **Novo** — HardwareInventory::collect() |
| `crates/neural-kernel/src/arch/mlp.rs` | **Novo** — MLP 512→256→64→9 ternário + forward |
| `crates/neural-kernel/src/arch/mod.rs` | **Novo** — infer_architecture(), SystemArchitecture |
| `crates/neural-kernel/src/trust.rs` | **Novo** — TrustPolicy, TrustCache |
| `crates/neural-kernel/src/main.rs` | Modificado — boot flow adaptativo |
| `crates/neural-kernel/src/cortex.rs` | Modificado — nova skill "status hardware" |

### Teste de aceite:

```
[ARCH] MLP Inference (512→256→64→9): 142 µs
[ARCH] Decision:
  ring0          → software (no NPU detected)
  ring1          → P_cores (2 cores available, no GPU)
  ring2          → E_cores (no E-cores, using P_cores idle)
  p_cores_ring1  → 0 (single core)
  heap_size      → 64 MB
  sfs_policy     → NVMe_only (no NVMe detected → RAM fallback)
  trust_default  → allow_known (desktop)
  power_policy   → balanced (QEMU default)
[HEAP] Init: 64 MB at 0x4444_4444_0000
[NEURAL] Cortex: System Architecture loaded.

> status hardware
[CORTEX] Hardware: QEMU Virtual CPU, 2048 MB RAM, 12 PCI devices
[CORTEX] Decisao do MLP: ring0=software, ring1=P_cores, heap=64 MB
```

### Riscos e mitigações:

| Risco | P | I | Mitigação |
|---|---|---|---|
| MLP 512→256→64→9 não cabe no kernel (~150k pesos ternários = 37 KB) | B | M | 37 KB cabe no .rodata. Verificar com `size` no link. |
| MLP produz decisão catastrófica (heap 0, ring0 = NPU sem NPU) | B | C | Clamps de segurança: heap mínimo 64 KB, ring0 sempre fallback software se sem NPU. |
| Inventário lento (~1 ms para PCI scan completo) | B | B | PCI scan é ~1 µs por device. 12 devices = 12 µs. Insignificante. |
| Embedding do brand name da CPU não normalizado corretamente | M | M | Hash simples + lookup table para CPUs conhecidas. Fallback: vetor zero. |

### Depende de:
- Block 1 (PCI scan para inventário)
- Block 2 (SMP + heap expansível)

### Bloqueia:
- Block 6 (trust cache, power skills)

---

## 7. Block 5 — Skills de Hardware + Trust (Sprint 22)

**Block height:** 5  
**Hash anterior:** `block4(Auto-Conf)`  
**Objetivo:** O chat Hermes agora age sobre o hardware real

### O que este bloco entrega:

```
Skills de hardware:
  ├── system_info  → CPU brand, freq, cores, RAM, PCI count
  ├── pci_scan     → lista detalhada de dispositivos PCI
  ├── power        → shutdown (via ACPI S5) ou reboot (via 0x64)
  ├── trust        → lista/gerencia TrustCache
  └── arch_status  → exibe decisão do MLP de arquitetura

TrustCache operacional:
  ├── TrustEntry { vid, pid, skill_name, trusted, first_seen }
  ├── TrustTable (64 slots fixos, sem alocação dinâmica)
  ├── trust allow/policy → skills reconhecidas rodam sem confirmação
  ├── trust deny → skill bloqueada
  └── "trust list" → exibe entradas do cache

Skill de shutdown real:
  ├── ACPI S5 via PM1a_CNT (port 0x2000 ou via FADT)
  ├── Fallback: 0x64 + 0xFE (PS/2 reset)
  └── "power off" → desliga QEMU / hardware real

Hardware query expandida:
  ├── "status" → info geral
  ├── "status cpu" → detalhes do processador
  ├── "status ram" → ocupação do BitmapFrameAllocator
  ├── "status pci" → dispositivos PCI
  └── "status trust" → trust cache
```

### Arquivos criados/modificados:

| Arquivo | Ação |
|---|---|
| `crates/neural-kernel/src/trust.rs` | Modificado — TrustCache persistente, trust list/allow/deny |
| `crates/neural-kernel/src/cortex.rs` | Modificado — novas skills: pci, power, trust, arch |
| `crates/neural-kernel/src/smp/mod.rs` | Modificado — power management (ACPI S5) |
| `crates/neural-kernel/src/main.rs` | Modificado — register novas skills |
| `skill-registry/src/registry.rs` | Modificado — execute_skill com trust check |

### Experiência do usuário:

```
> status
[SKILL] CPU: QEMU Virtual (1 core, 1 thread, ~2.5 GHz)
[SKILL] RAM: 2048 MB total, 0.5% ocupada
[SKILL] PCI: 12 dispositivos encontrados
[SKILL] Trust: 2 entradas (echo, system_status)

> pci list
[SKILL] Dispositivos PCI:
  0000:00:00.0 8086:1237 Host bridge
  0000:00:01.0 8086:7000 ISA bridge
  0000:00:02.0 1234:1111 VGA controller
  0000:00:03.0 1AF4:1041 VirtIO net
  ... (12 devices)

> trust list
[SKILL] Trust Cache:
  echo              → trusted (first: boot)
  system_status     → trusted (first: boot)

> power off
[SKILL] Shutting down via ACPI S5...
```

### Riscos e mitigações:

| Risco | P | I | Mitigação |
|---|---|---|---|
| ACPI S5 não funciona (FADT não parseado) | M | M | Fallback: port 0x64 + 0xFE (8042 reset). Último caso: hlt loop. |
| TrustCache sem persistência (reset limpa) | A | B | MVP não tem SFS. Trust é volátil. Pós-MVP: salvar no SFS. |
| PCI scan skill lenta se muitos dispositivos | B | B | Scan é < 100 µs. Cache de scan no boot. |

### Depende de:
- Block 1 (PCI scan)
- Block 4 (TrustPolicy + inventário)
- Block 3 (chat loop para interface)

---

## 8. MVP Terminal Block — Neural OS Hermes

**Block height:** 6  
**Hash anterior:** `block5(Skills+Trust)`  
**Entregável:** `neural-os-core.iso` bootável em qualquer x86-64 UEFI

### O que é o MVP:

Uma imagem ISO (~2 MB) que você:
- Boota no QEMU: `cargo run`
- Grava num USB: `dd if=target/x86_64-unknown-none/release/neural-os-core.iso of=/dev/sdb`
- Boota num notebook x86 real (UEFI, 512 MB RAM mínimo)

E recebe:

```
[neural-os-core v1.0.0 MVP]

[ARCH] Detecting hardware...
  CPU: Intel(R) Core(TM) i5-7200U (2 P-cores, 4 threads)
  RAM: 16384 MB DDR4-2400
  PCI: 15 devices (VGA, xHCI, SATA, NVMe, HDA, USB 3.0...)
  NPU: not detected

[ARCH] MLP decision (142 µs):
  ring0          → software (no NPU)
  ring1          → P_cores (AVX-512 not present)
  ring2          → P_cores idle (no E-cores on this CPU)
  heap           → 64 MB (tier: DRAM)
  tensor         → DRAM (Vram detected but driver absent)
  kv_cache       → DRAM (Vram would be better, 4 GB available)
  sfs_active     → DRAM (Nvme detected but driver absent)
  trust          → allow_known
  power          → balanced

[MHI] Memory Hierarchy Index:
  tier[0] DRAM: 16384 MB @ 19.2 GB/s ← heap, tensor, kv_cache, sfs
  tier[1] VRAM: GTX 1050 4 GB @ 112 GB/s ← detected, driver missing
  tier[2] NVMe: 256 GB @ 3.5 GB/s ← detected, driver missing

[SYSTEM] Neural OS Hermes ready.
Type 'help' for available commands.

> 

> help
[CORTEX] Skills: echo, system_status, system_info, pci_scan, power, trust, arch, memory, help
[CORTEX] Digite '<skill> help' para detalhes.
[CORTEX] Hardware detectado: VGA text, serial, PS/2 keyboard, APIC timer, PCI bus, DRAM

> status
[SKILL] CPU: i5-7200U @ 2.5 GHz (boost 3.1), 2 P-cores/4 threads
[SKILL] RAM: 16384 MB total, 0.2% ocupada
[SKILL] PCI: 15 dispositivos (GPU GTX 1050, NVMe, xHCI...)
[SKILL] Trust: 3 entradas ativas

> status memory
[CORTEX] Memory Hierarchy Index:
  tier[0] DRAM: 16384 MB @ 19.2 GB/s ← ativo
  tier[1] VRAM: GTX 1050 4 GB @ 112 GB/s ← instale skill gpu_bar
  tier[2] NVMe: 256 GB @ 3.5 GB/s ← instale skill nvme_driver

[CORTEX] MLP decidiu:
  heap_tier     → Dram (padrao)
  tensor_tier   → Dram (Vram recomendado, 112 GB/s > 19 GB/s)
  kv_cache_tier → Dram (Vram seria 4x mais rapido)

> power off
[SKILL] Shutting down...
```

### Critérios de aceite:

| Requisito | Obrigatório | Desejável |
|---|---|---|
| Boota em QEMU `-m 512M` | ✅ | — |
| Boota em hardware real UEFI | ✅ | ✅ |
| PCI scan detecta dispositivos | ✅ | ✅ |
| Memory Hierarchy Index populado | ✅ | ✅ |
| `alloc_by_tier(Dram)` funcional | ✅ | — |
| `alloc_by_tier(Vram/Nvme)` retorna `None` com diagnóstico | ✅ | — |
| `status memory` exibe tiers + drivers ausentes | ✅ | ✅ |
| MLP de arquitetura decide config (incl. heap/tensor/kv tiers) | ✅ | — |
| Chat loop funcional (VGA + serial) | ✅ | ✅ |
| Intent Router classifica 3+ intents | ✅ | — |
| Skills: status, memory, pci, power, help | ✅ | ✅ |
| Trust Cache funcional | ✅ | — |
| APIC timer (sem PIC) | ✅ | — |
| SMP (mínimo 2 cores) | — | ✅ |
| Shutdown via ACPI | ✅ | — |
| Slab allocator + heap 4 MB | ✅ | — |

### O que NÃO é o MVP:

| Excluído | Motivo | Previsão |
|---|---|---|
| `alloc_by_tier(Vram)` | Requer BAR de GPU mapeado | Sprint 23+ |
| `alloc_by_tier(Nvme)` | Requer driver NVMe | Sprint 24+ |
| `alloc_by_tier(Hdd)` | Requer SFS + driver ATA/NVMe | Sprint 24+ |
| USB driver | Complexidade alta, PS/2 legacy suficiente | Sprint 23+ |
| NVMe driver (full) | MVP é stateless, sem SFS | Sprint 24+ |
| UEFI framebuffer | VGA text funciona | Sprint 23+ |
| WASM embedder | Skills Rust traits bastam | Sprint 24+ |
| NPU real | Requer hardware AMD APU + firmware | Sprint 25+ |
| Agent scheduler | Executor cooperativo segura 1-4 cores | Sprint 24+ |
| Cognitive planner | Planejamento multi-etapa | Fase 6 |
| Rede/network stack | Sem VirtIO-net no MVP | Sprint 24+ |
| Audio/Video | Nenhuma skill de mídia no MVP | Fase 5+ |
| Memória persistente (SFS) | MVP volátil, sem storage | Sprint 23+ |

---

## 9. Timeline Consolidada

```
Sprint 18 (Block 1) ─── PCI scan + ACPI + APIC BSP
   │   Entrega: kernel descobre hardware real
   │   Teste: "12 PCI devices, LAPIC timer at 1 kHz"
   ▼
Sprint 19 (Block 2) ─── PerCpu + SMP + Slab allocator
   │   Entrega: 2 cores cooperando, heap 4 MB
   │   Teste: "AP (Core 1) online e cooperando"
   ▼
Sprint 20 (Block 3) ─── Chat loop + Intent Router
   │   Entrega: Hermes terminal funcional
   │   Teste: "> status → RAM: 2.0% ocupada"
   ▼
Sprint 21 (Block 4) ─── MLP de Arquitetura + Auto-Config
   │   Entrega: sistema se configura sozinho
   │   Teste: "MLP decision: ring0=software, heap=64 MB"
   ▼
Sprint 22 (Block 5) ─── Skills de hardware + Trust
   │   Entrega: Hermes age no hardware
   │   Teste: "> power off → shutdown via ACPI"
   ▼
╔═══════════════════════════════════════════════════════╗
║            MVP: Neural OS Hermes v1.0.0               ║
║  ISO bootável, chat neural, auto-config, skills reais  ║
╚═══════════════════════════════════════════════════════╝
   │
   ▼
Pós-MVP (fases seguintes):
  Sprint 23 ─── Framebuffer UEFI + USB keyboard
  Sprint 24 ─── VirtIO-net + NVMe + SFS persistente
  Sprint 25 ─── WASM embedder + linear memory pool
  Sprint 26 ─── NPU XDNA suporte + hardware real
```

Cada sprint = 1-2 semanas. MVP em ~10 semanas de trabalho focada.

---

## 10. Impacto nos Documentos Existentes

| Documento | Ação |
|---|---|
| `docs/roadmap.md` | Substituir conteúdo por resumo + redirecionamento para ADR-0015 |
| `docs/architecture/0014-ideias-hardware.md` | Manter como visão de longo prazo. Marcar itens como "Pós-MVP: Block 5+" e "Pós-MVP: Block 6+" |
| `docs/memory/STATE.md` | Atualizar Next Steps: "Sprint 18 — PCI scan + ACPI + APIC BSP" |
| `AGENTS.md` | Atualizar Next Sprint, adicionar ADR-0015, atualizar referências |
| `.plans/` (se existir) | Adicionar diretório de planning |

---

## 11. Resumo da Chain

```
Block 0 (Genesis):      Sprint 17 ─── O que temos (VGA, serial, heap, EventBus, 5 agentes)
     ↓ depende de: nada (já implementado)
Block 1 (PCI+ACPI):     Sprint 18 ─── Descoberta de hardware real
     ↓ depende de: Block 0
Block 2 (SMP+Heap):     Sprint 19 ─── Multi-core + memória dinâmica
     ↓ depende de: Block 1 (APIC, PCI)
Block 3 (Hermes Chat):  Sprint 20 ─── Terminal conversacional com intent router
     ↓ depende de: Block 2 (heap, PerCpu)
Block 4 (Auto-Conf):    Sprint 21 ─── MLP decide configuração do sistema
     ↓ depende de: Block 1 (PCI), Block 2 (SMP)
Block 5 (Skills):       Sprint 22 ─── Hermes age no hardware (shutdown, pci, trust)
     ↓ depende de: Block 3 (chat), Block 4 (trust policy)
Block 6 (MVP):          Terminal ─── neural-os-core.iso bootável em qualquer x86
```

Cada bloco é verificado em QEMU antes de avançar. Se um bloco falha, os seguintes não são construídos. Isso é o que torna a chain segura — como cripto, mas com hardware real.

---

## 12. O Nome "Hermes" no Contexto do Neural OS

O Hermes mitológico:
- **Mensageiro** — leva intenções do usuário para o kernel
- **Intérprete** — traduz linguagem natural em ações de hardware
- **Psicopompo** — guia transições entre estados (boot → config → chat → ação)
- **Fronteiriço** — opera na fronteira entre hardware e software, entre usuário e máquina
- **Ladrão** — questiona autoridade de dispositivos (zero-trust, trust cache)

O Hermes Agent (Nous Research):
- Terminal TUI com skills → nosso VGA/serial chat loop
- MCP integration → nosso EventBus + CapabilityToken
- Skill system → nosso SkillRegistry
- Memory → nosso TrustCache
- Multi-platform → nosso x86-64 qualquer

**O Neural OS Hermes é o Hermes Agent que não precisa de Linux porque ele É o Linux.** Se o Nous Hermes é um agente que roda em qualquer SO, o Neural Hermes é um SO que É um agente.

---

## Apêndice A — Master Registry: Inventário Completo de Ideias

> **⚠️ Documento vivo:** O registro mestre agora vive em `docs/memory/IDEA_BANK.md`.
> Este apêndice é o **seed histórico** (congelado na criação da ADR-0015).
> Para status atualizado, consulte o IDEA_BANK.md.

Este apêndice cataloga **toda ideia, decisão e item** dos documentos precursores (ADR-0014, roadmap.md, ADR-0010, discussões) e seu destino na nova rota. Nada é esquecido — cada item tem status, justificativa e target.

**Legenda:**

| Marca | Significado |
|---|---|
| ✅ Block N | Implementado ou em andamento no bloco N |
| 🟡 Sprint N | Agendado para sprint específica |
| ⏳ Pós-MVP | Adiado para depois do MVP, com motivo |
| 💰 Sponsor | Requer hardware/parceria/financiamento |
| ❌ Descartado | Não será feito, com motivação |
| 🔄 Fundido | Absorvido por item maior |

### A.1. ADR-0014 — USB

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 1 | xHCI controller mínimo (<500 LOC, BAR0, port status) | ⏳ Pós-MVP | Sprint 23+ | MVP usa PS/2 legacy. xHCI requer PCI (Block 1) + driver USB (~500 LOC). |
| 2 | `identify_device()` → VID/PID/class | ⏳ Pós-MVP | Sprint 23+ | Bloqueado pelo xHCI driver. |
| 3 | Neural Cortex classify (MLP 7→5: allow/deny/learn/no_intent/suspect) | ⏳ Pós-MVP | Sprint 23+ | MLP arquitetura (Block 4) pode ser estendido. |
| 4 | Trust Cache (TrustEntry, TrustTable, trust-once-use-always) | 🔄 Fundido no Block 5 | Sprint 22 | TrustCache do MVP (Block 5) é versão simplificada. Serial/last_seen adicionados no Block 5 atualizado. |
| 5 | Trust Cache: regra de 5 situações (auto-ON, conhecido, novo, rejeitado, desconhecido) | 🔄 Fundido no Block 5 | Sprint 22 | Incorporado à TrustTable do MVP. |
| 6 | Trust Cache: persistência no SFS (`/system/trust/usb.tbl`) | ⏳ Pós-MVP | Sprint 24+ | Requer SFS (Sprint 24). MVP sem persistência. |
| 7 | Trust Cache: revogação ("não confio mais") | 🔄 Fundido no Block 5 | Sprint 22 | `trust deny <skill>` no MVP. |
| 8 | WASM skill dispatch para protocolos USB (hid_mouse.wasm, uvc_capture.wasm) | ⏳ Pós-MVP | Sprint 25+ | Requer WASM embedder (Sprint 25). |
| 9 | Nível 1 — HW Detection (xHCI mínimo, sem IA) | ⏳ Pós-MVP | Sprint 23+ | Bloqueado pelo xHCI driver. |
| 10 | Nível 2 — Device Classification (MLP 7→5) | ⏳ Pós-MVP | Sprint 23+ | MLP arquitetura (Block 4) é primeiro passo. |
| 11 | Nível 3 — Dynamic Interface Creation (WASM) | ⏳ Pós-MVP | Sprint 25+ | Requer WASM embedder. |
| 12 | USB flow: dispositivo desconhecido → porta desabilitada | ⏳ Pós-MVP | Sprint 23+ | Mesma dependência do xHCI. |
| 13 | USB flow: trust-once → segunda conexão auto-ON | ⏳ Pós-MVP | Sprint 23+ | TrustCache existe (Block 5), falta xHCI. |
| 14 | USB flow: usuário precisa inferir intenção (nada automático) | ⏳ Pós-MVP | Sprint 23+ | Princípio arquitetural. Válido, mas implementação depende de xHCI. |
| 15 | "Zero autorun, zero superfície de ataque USB" | ⏳ Pós-MVP | Sprint 23+ | Princípio. Adotado como diretriz de design. |

### A.2. ADR-0014 — SMP / APIC / Multicore

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 16 | APIC Local (LAPIC) init no BSP | ✅ Block 1 | Sprint 18 | Essencial para substituir PIC e permitir SMP. |
| 17 | IOAPIC init (roteamento IRQ externo) | 🟡 Block 1 | Sprint 18 | Necessário para keyboard + timer após desligar PIC. |
| 18 | x2APIC mode (MSR-based, sem MMIO) | 🟡 Block 1 | Sprint 18 | Adicionar após LAPIC básico funcionar. Fallback: MMIO. |
| 19 | MADT parsing (ACPI → LAPIC list) | ✅ Block 1 | Sprint 18 | Essencial para descobrir quantos cores existem. |
| 20 | CPUID leaf 0x1A (P-core / E-core detection — Intel Hybrid) | ✅ Block 2 | Sprint 19 | Essencial para CorePools inteligente. |
| 21 | CPUID leaf 0x0B (Extended Topology: thread/core/package) | ✅ Block 2 | Sprint 19 | Necessário para distinguir HT de cores físicos. |
| 22 | CorePools / ComputePools (P→Ring0/1, E→Ring2) | ✅ Block 2 | Sprint 19 | Atribuição por tipo de core + fallback homogêneo. |
| 23 | Algoritmo `assign_cores()` — P/E-aware + N+1 + fallback | ✅ Block 2 | Sprint 19 | Adicionado ao Block 2 após cross-ref. |
| 24 | PerCpu struct (core_id, lapic_id, core_type, ring, stack, queue) | ✅ Block 2 | Sprint 19 | Essencial para APs saberem quem são. |
| 25 | GS.base segment register per-core | ✅ Block 2 | Sprint 19 | Mecanismo de acesso ao PerCpu. |
| 26 | INIT-SIPI-SIPI via LAPIC ICR | ✅ Block 2 | Sprint 19 | Protocolo Intel de wake. |
| 27 | Trampoline assembly (16→32→PAE→64→Rust) | ✅ Block 2 | Sprint 19 | Ponte entre modo real e long mode. |
| 28 | AP startup IPI (BSP → INIT → SIPI → SIPI) | ✅ Block 2 | Sprint 19 | Depende do trampoline + alloc_below_1mb. |
| 29 | Stack separada por core (64 KB cada) | ✅ Block 2 | Sprint 19 | Essencial para APs não compartilharem stack. |
| 30 | Regras de escalonamento por pool (tabela: qual trabalho → qual pool) | ✅ Block 2 | Sprint 19 | Adicionado ao Block 2. |
| 31 | "Se só E-cores, tudo roda em E-cores mais lentos" | ✅ Block 2 | Sprint 19 | Caso de borda documentado. |
| 32 | "Se 1 core apenas (QEMU -smp 1), tudo no mesmo core" | ✅ Block 2 | Sprint 19 | Caso de borda documentado. |
| 33 | "HT: 1 thread por core físico no Ring 0/1, HT restante no Ring 2" | ✅ Block 2 | Sprint 19 | Regra de atribuição incluída. |
| 34 | `acpi` crate para parser MADT/PPTT | 🟡 Block 1 | Sprint 18 | Dependência futura. No MVP, hardcode QEMU + parser ACPI mínimo. |
| 35 | `raw-cpuid` crate para detecção de features | 🟡 Block 2 | Sprint 19 | Dependência futura. No MVP, CPUID inline assembly. |

### A.3. ADR-0014 — NPU (AMD XDNA)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 36 | `Npu` struct + `try_init()` via PCI scan | 💰 Sponsor | Sprint 25+ | Requer AMD APU real (XDNA) ou QEMU com NPU virtual. Sem hardware, sem teste. |
| 37 | `Accelerator::XDNA(Npu)` / `Accelerator::Software` enum | 💰 Sponsor | Sprint 25+ | Depende de #36. |
| 38 | Command queue circular + doorbell write | 💰 Sponsor | Sprint 25+ | Requer documentação do XDNA ou engenharia reversa do driver amdxdna.ko. |
| 39 | Overlay loading via MMIO | 💰 Sponsor | Sprint 25+ | Vendor-specific. AMD Vitis AI compiler gera overlay. |
| 40 | MSI-X interrupt registration | 💰 Sponsor | Sprint 25+ | Depende de #36 + IOAPIC/MSI. |
| 41 | Fallback automático: init_npu() → se falha → Software | ✅ Block 4 | Sprint 21 | `ring0: {software, NPU, P_core}` — se NPU ausente, cai para software. |
| 42 | 3 cenários: QEMU (Software) / APU real sem driver (Software) / APU real com driver (XDNA) | 🟡 Block 4 | Sprint 21 | Lógica de fallback documentada e implementada no Block 4. |
| 43 | Cadeia de programação: Modelo → Vendor Compiler → Overlay → DRAM | 💰 Sponsor | Sprint 25+ | Fora do escopo do kernel. Requer toolchain AMD Vitis. |
| 44 | NPU: "Ring 0 (Intent Router MLP) NÃO precisa do NPU — 20 pesos rodam em 1 core" | ✅ Block 4 | Sprint 21 | Premissa arquitetural adotada: MLP minúsculo roda em software sempre. |
| 45 | Caminho de migração: QEMU → APU fase 1 (fallback) → APU fase 2 (driver) → APU fase 3 (overlay compilado) | 💰 Sponsor | Sprint 25+ | Roteiro documentado. Depende de patrocínio/hardware. |

### A.4. ADR-0014 — AI-Driven Hardware Detection

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 46 | `HardwareInventory::collect()` — CPU, RAM, PCI, aceleradores, storage | ✅ Block 4 | Sprint 21 | Coração do Block 4. |
| 47 | `cortex::infer_architecture(&inventory)` | ✅ Block 4 | Sprint 21 | MLP 512→256→64→9 ternário. |
| 48 | MLP 512→256→64→9 ternário (~37 KB, pesos embutidos) | ✅ Block 4 | Sprint 21 | ~150k pesos ternários em .rodata. |
| 49 | `SystemArchitecture` struct (ring0, ring1, ring2, heap, sfs, trust, power, tiers) | ✅ Block 4 | Sprint 21 | 12 saídas categóricas. |
| 50 | Boot flow adaptativo: collect → infer → init | ✅ Block 4 | Sprint 21 | Substitui boot sequence fixo atual. |
| 51 | Treinamento offline do MLP (10k hardware profiles sintéticos) | ⏳ Pós-MVP | Sprint 21+ | Pesos iniciais podem ser heuristic-based. Treinamento real depois. |
| 52 | Atualização do MLP via skill WASM (baixar pesos novos) | ⏳ Pós-MVP | Sprint 25+ | Requer WASM embedder. |
| 53 | Fallback seguro: se MLP produz absurdo, valores default clamped | ✅ Block 4 | Sprint 21 | Implementado: heap mínimo 64 KB, ring0 sempre fallback software. |
| 54 | "O modelo de arquitetura é pequeno (37 KB). Cabe no kernel." | ✅ Block 4 | Sprint 21 | Premissa verificada. |
| 55 | "A inferência é rápida (µs)" | ✅ Block 4 | Sprint 21 | MLP ternário em 1 core = microssegundos. |

### A.5. ADR-0014 — Memory Hierarchy Index

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 56 | `struct MemoryTier { device, kind, capacity, bandwidth, latency, is_unified, numa_node }` | ✅ Block 4 | Sprint 21 | Adicionado ao MVP (cross-ref). |
| 57 | `struct MemoryHierarchy { tiers: Vec<MemoryTier> }` ordenado por velocidade | ✅ Block 4 | Sprint 21 | Adicionado ao MVP. |
| 58 | `enum AllocTier { Dram, Vram, Nvme, Hdd }` | ✅ Block 4 | Sprint 21 | Adicionado ao MVP. |
| 59 | `fn alloc_by_tier(tier: AllocTier, size) -> Option<PhysAddr>` | ✅ Block 4 | Sprint 21 | Dram implementado. Vram/Nvme → None com diagnóstico. |
| 60 | `AllocTier::Vram` → alocar no BAR da GPU | ⏳ Pós-MVP | Sprint 23+ | Requer driver GPU + BAR mapeado. |
| 61 | `AllocTier::Nvme` → alocar páginas no NVMe via SFS | ⏳ Pós-MVP | Sprint 24+ | Requer NVMe driver + SFS. |
| 62 | `AllocTier::Hdd` → cold storage | ⏳ Pós-MVP | Sprint 24+ | Requer SFS + driver ATA/NVMe. |
| 63 | MLP saídas expandidas: `heap_tier`, `tensor_tier`, `kv_cache_tier`, `sfs_active_tier` | ✅ Block 4 | Sprint 21 | 4 tiers de saída no MLP do MVP. |
| 64 | MLP saídas expandidas: `sfs_cold_tier`, `tensor_swap_tier`, `skill_heap_tier` | 🟡 Block 4 | Sprint 21 | Adicionar como campos opcionais no SystemArchitecture. |
| 65 | Exemplo real: notebook i5 + GTX 1050 + NVMe + HDD | ✅ Documentation | README + ADR-0015 | Adicionado como caso de uso no README. |
| 66 | Exemplo real: Xeon 6900 (1 TB RAM, NVMe RAID) | ✅ Documentation | ADR-0015 | Adicionado como caso de uso. |
| 67 | Exemplo real: AMD APU Strix Point (memória unificada) | ✅ Documentation | ADR-0015 | Adicionado como caso de uso. |

### A.6. ADR-0014 — Periféricos (PCI, NVMe, VirtIO)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 68 | PCI config space access (CF8/CFC) | ✅ Block 1 | Sprint 18 | Fundação de toda descoberta de hardware. |
| 69 | PCI scan: vendor, device, class, subclass, BARs | ✅ Block 1 | Sprint 18 | Essencial para inventário. |
| 70 | PCI bridges (hierarquia de barramento) | ⏳ Pós-MVP | Sprint 18+ | Scan cego bus 0..255 funciona para MVP. Hierarquia real depois. |
| 71 | NVMe driver (PCI Class 01.08) | ⏳ Pós-MVP | Sprint 24+ | MVP é stateless. Sem SFS, NVMe é desnecessário. |
| 72 | VirtIO-blk (PCI 1AF4:1001) | ⏳ Pós-MVP | Sprint 24+ | Alternativa QEMU ao NVMe. Mesma dependência de SFS. |
| 73 | VirtIO-net (PCI 1AF4:1041) | ⏳ Pós-MVP | Sprint 24+ | MVP sem rede. |
| 74 | VirtIO-gpu (PCI 1AF4:1050) | ⏳ Pós-MVP | Sprint 24+ | MVP usa VGA text. |
| 75 | Intel HDA (PCI 04.03) | ⏳ Pós-MVP | Fase 5+ | Nenhuma skill de áudio no MVP. |
| 76 | "Sem kernel thread de hotplug" | ✅ Princípio adotado | — | Diretriz: sem hotplug, sem sysfs, sem device tree. |
| 77 | "Sem sysfs genérico" | ✅ Princípio adotado | — | Diretriz adotada. |
| 78 | "Cada driver é um módulo autocontido sem trait Device universal" | ✅ Princípio adotado | — | Diretriz adotada. |

### A.7. ADR-0014 — Áudio/Vídeo

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 79 | UEFI framebuffer (`BootInfo::framebuffer` + BGRA32 writer) | ⏳ Pós-MVP | Sprint 23+ | VGA text funciona para MVP. Framebuffer é upgrade visual. |
| 80 | Font rendering bitmaps para alta resolução | ⏳ Pós-MVP | Sprint 23+ | Depende de #79. |
| 81 | VirtIO-GPU (2D/3D acelerado) | ⏳ Pós-MVP | Sprint 24+ | Requer VirtIO. |
| 82 | Tensor visualization (renderizar ativações no framebuffer) | ⏳ Pós-MVP | Fase 5+ | Depende de #79 + #81. |
| 83 | Intel HDA audio driver | ❌ Descartado | — | Nenhuma skill de áudio no roadmap atual. Se surgir skill de voz, reavaliar. |
| 84 | Áudio via USB (UAC) | ❌ Descartado | — | USB é pós-MVP. Áudio pós-MVP. USB audio = duplo post-MVP. |

### A.8. Princípios Arquiteturais (ADR-0014)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 85 | Mínimo viável: só implementar driver se requisito para skill WASM ou boot flow | ✅ Princípio adotado | — | Guia todas as decisões do MVP. |
| 86 | VirtIO first: em QEMU, usar VirtIO. Hardware real só após protótipo QEMU validado | ✅ Princípio adotado | — | Adotado como diretriz. |
| 87 | Polling > Interrupção: polling para dispositivos de baixa taxa | ✅ Princípio adotado | — | Adotado. Interrupções só para latência crítica. |
| 88 | Sem HAL genérica: cada driver é módulo autocontido | ✅ Princípio adotado | — | Adotado. |
| 89 | "O usuário precisa inferir" — nenhum dispositivo tem autoridade implícita | ✅ Princípio adotado | — | Fundamento do zero-trust. |
| 90 | Trust-once-use-always usabilidade | ✅ Block 5 | Sprint 22 | TrustCache implementa isso. |

### A.9. Roadmap Original — Memória

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 91 | Bitmap Frame Allocator | ✅ Genesis | Sprint 11 | Já implementado (Sprint 11). |
| 92 | Huge Pages (2 MiB) — mapper + TLB miss reduction | ⏳ Pós-MVP | Sprint 23+ | Otimização de performance. MVP não tem inferência pesada o suficiente para justificar. |
| 93 | Huge Pages (1 GiB) — CPUID check + contiguous alloc | ⏳ Pós-MVP | Sprint 24+ | Depende de #92 + hardware real. |
| 94 | Slab Allocator — buckets 32-4096 | ✅ Block 2 | Sprint 19 | Essencial para heap dinâmico. |

### A.10. Roadmap Original — Kernel Abstraction

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 95 | Async Neural Executor | ✅ Genesis | Sprint 12 | Já implementado (Sprint 12). |
| 96 | Agent Scheduler (round-robin com prioridade) | ⏳ Pós-MVP | Sprint 24+ | Executor cooperativo atual segura 1-4 cores. Scheduler é upgrade para >4 cores. |
| 97 | Budget de execução (tokens_consumed por agente) | ⏳ Pós-MVP | Sprint 24+ | Depende do Agent Scheduler (#96). |
| 98 | MLP decide prioridade no scheduler | ⏳ Pós-MVP | Sprint 24+ | Depende do Agent Scheduler (#96). |

### A.11. Roadmap Original — EventBus

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 99 | EventBus + CapabilityToken | ✅ Genesis | Sprint 13 | Já implementado (Sprint 13). |
| 100 | Topic enum completo (AgentCreated, SkillRequest, MemoryPressure, etc.) | ⏳ Pós-MVP | Sprint 23+ | MVP usa strings soltas. Tipar enums melhora segurança, mas não é crítico para MVP. |
| 101 | ML-based routing (EventBus consulta Intent Router para filtrar assinantes) | ⏳ Pós-MVP | Sprint 23+ | Inovação futura. MVP usa BTreeMap direto. |

### A.12. Roadmap Original — Skill Registry

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 102 | Skill trait + MCP + Registry | ✅ Genesis | Sprint 14 | Já implementado (Sprint 14). |
| 103 | WASM embedder (wasmi) | ⏳ Pós-MVP | Sprint 25+ | Skills Rust traits funcionam para MVP. WASM é upgrade de portabilidade. |
| 104 | Linear memory pool (256 KB por skill) | ⏳ Pós-MVP | Sprint 25+ | Depende de #103. |

### A.13. Roadmap Original — Cognitive Runtime

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 105 | Intent Planner (sequência de SkillCommands) | ⏳ Pós-MVP | Fase 6 | MVP classifica intent única. Planner é multi-etapa. |
| 106 | Success Engine (feedback loop, ajuste online de pesos) | ⏳ Pós-MVP | Fase 6 | Depende de #105. Pesquisa acadêmica futura. |
| 107 | Neural Cache (lookup table 50 ns em Huge Pages) | ⏳ Pós-MVP | Fase 6 | Depende de #92 (Huge Pages) + #105. |
| 108 | MatMul-free LM (RWKV/Mamba/ternary pooling) | ⏳ Pós-MVP | Fase 7 | Meta futura já identificada como distante no roadmap original. |

### A.14. Roadmap Original — Timeline

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 109 | Sprint 16: Slab Allocator | 🔄 Remapeado | Sprint 19 (Block 2) | Movido para depois de PCI+APIC (dependência física). |
| 110 | Sprint 17: Agent Scheduler | 🔄 Remapeado | Sprint 24+ (Pós-MVP) | Adiado. Executor cooperativo é suficiente. |
| 111 | Sprint 18+: Cognitive Runtime | 🔄 Remapeado | Fase 6 | Adiado. MVP primeiro. |

### A.15. Outras Ideias (discussões de sessão)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 112 | Bootável em hardware x86 real (não só QEMU) | ✅ MVP | Sprint 22 | Critério de aceite do MVP. |
| 113 | Nome "Hermes" como identidade do MVP | ✅ Adotado | — | README + ADR-0015 usam. |
| 114 | Chat loop estilo Hermes Agent (Nous Research) | ✅ Block 3 | Sprint 20 | Inspiração direta para o MVP. |
| 115 | Sponsor: NPU AMD XDNA requer parceria | 💰 Sponsor | Sprint 25+ | Sem hardware, sem implementação. Documentado como oportunidade. |
| 116 | Sponsor: port para ARM/RISC-V | 💰 Sponsor | Futuro | Fora do escopo x86-64. Requer nova arquitetura. |

---

## Apêndice B — Mapa de Calor: Cobertura do ADR-0015 vs Precursores

| Fonte | Itens totais | ✅ No MVP | 🟡 Sprint | ⏳ Pós-MVP | 💰 Sponsor | ❌ Descarte |
|---|---|---|---|---|---|---|
| ADR-0014 USB | 15 | 0 | 0 | 15 | 0 | 0 |
| ADR-0014 SMP | 20 | 17 | 3 | 0 | 0 | 0 |
| ADR-0014 NPU | 10 | 1 | 1 | 0 | 8 | 0 |
| ADR-0014 AI Detection | 10 | 9 | 0 | 1 | 0 | 0 |
| ADR-0014 MHI | 12 | 8 | 1 | 3 | 0 | 0 |
| ADR-0014 Periféricos | 11 | 3 | 0 | 6 | 0 | 0 |
| ADR-0014 Áudio/Vídeo | 6 | 0 | 0 | 4 | 0 | 2 |
| ADR-0014 Princípios | 6 | 6 | 0 | 0 | 0 | 0 |
| Roadmap Memória | 4 | 2 | 0 | 2 | 0 | 0 |
| Roadmap Kernel | 4 | 1 | 0 | 3 | 0 | 0 |
| Roadmap EventBus | 3 | 1 | 0 | 2 | 0 | 0 |
| Roadmap Skills | 3 | 1 | 0 | 2 | 0 | 0 |
| Roadmap Cognitive | 4 | 0 | 0 | 4 | 0 | 0 |
| Roadmap Timeline | 3 | 0 | 0 | 3 | 0 | 0 |
| Outras | 5 | 4 | 0 | 0 | 1 | 0 |
| **Total** | **116** | **53 (46%)** | **5 (4%)** | **45 (39%)** | **9 (8%)** | **2 (2%)** |

**Nenhum item perdido.** 116 itens originais catalogados (ADRs 0014-0015). O inventário vivo cresceu para **249 itens** via análises de ecossistema (Tiers 0-4, ADRs 0020-0024). Consulte o `IDEA_BANK.md` para o estado atual. Distribuição original: 53 (46%) dentro do MVP, 5 (4%) agendados para sprints específicas, 45 (39%) pós-MVP com motivo documentado, 9 (8%) aguardam patrocínio/hardware, 2 (2%) descartados com justificativa.

---

## Apêndice C — Hierarquia Técnica de Dependências (Pós-MVP)

> **⚠️ Documento vivo:** A hierarquia atualizada vive em `docs/memory/IDEA_BANK.md` (Seção 3).
> Este apêndice é o **seed histórico**. Para versão mais recente, consulte o IDEA_BANK.md.

Este apêndice organiza todos os itens diferidos (Pós-MVP, Sponsor) em **camadas de dependência técnica**. Nada foi "jogado para depois sem plano" — cada item tem pré-requisitos explícitos e uma cadeia de dependências que determina quando pode ser implementado.

### Notação

```
Item [ID] ─── nome do item
  ├── Pré: IDs dos pré-requisitos
  ├── → Bloqueia: IDs dos itens que dependem deste
  └── Por que está aqui: justificativa técnica
```

### C.1 — Camada 0: Já Existe (MVP Genesis, Block 0)

```
[46-55] HardwareInventory::collect() + MLP 512→256→64→9
[56-67] MemoryHierarchy + AllocTier enum + alloc_by_tier(Dram)
[68-69] PCI scan (CF8/CFC)
[16-19] LAPIC/IOAPIC init + MADT parsing
[24-33] PerCpu + trampoline + SMP wake
[94] Slab Allocator
[91] Bitmap Frame Allocator
[95] Async Neural Executor
[99] EventBus + CapabilityToken
[102] Skill trait + MCP + Registry
[59] alloc_by_tier(Dram) ← funcional no MVP
```

Nada nesta camada depende de itens pós-MVP. São a base.

### C.2 — Camada 1: Drivers de Dispositivo (Sprint 23+)

Depende de PCI scan (Block 1), APIC timer (Block 1), heap (Block 2).

```
[1] xHCI controller mínimo
  ├── Pré: [68] PCI scan (BAR0 mapeado), [17] IOAPIC (MSI)
  ├── → Bloqueia: [2, 3, 6, 8, 9, 10, 11, 12, 13, 14, 84]
  └── Razão: PS/2 legacy funciona. USB é centenas de LOC de driver, sem skill no MVP que precise.

[2] identify_device() → VID/PID/class
  ├── Pré: [1] xHCI mínimo
  └── Razão: sem xHCI, sem dispositivo USB para identificar.

[9] Nível 1 — HW Detection (xHCI sem IA)
  ├── Pré: [1] xHCI mínimo, [2] identify_device()
  └── Razão: nível 1 depende de xHCI funcionando.

[10] Nível 2 — Device Classification (MLP 7→5)
  ├── Pré: [9] Nível 1 HW Detection, [47] MLP 512→256→64→9 (estendido)
  └── Razão: primeiro hardware real para classificar.

[11] Nível 3 — Dynamic Interface Creation (WASM)
  ├── Pré: [9] Nível 1, [103] WASM embedder
  └── Razão: requer WASM + xHCI.

[12] USB flow: desconhecido → porta desabilitada
  ├── Pré: [1] xHCI mínimo, [89] princípio zero-autorun
  └── Razão: política, mas precisa de xHCI.

[13] USB flow: trust-once → auto-ON
  ├── Pré: [1] xHCI, [4] TrustCache (Block 5)
  └── Razão: TrustCache existe, falta xHCI.

[14] USB flow: usuário precisa inferir intenção
  ├── Pré: [12] flow desconhecido
  └── Razão: princípio arquitetural + xHCI.

[15] "Zero autorun, zero superfície de ataque USB"
  ├── Pré: [12, 13, 14] fluxos USB completos
  └── Razão: é o princípio final, implementado pelos fluxos.

[79] UEFI framebuffer (BGRA32 writer)
  ├── Pré: BootInfo::framebuffer (já disponível via bootloader crate)
  ├── → Bloqueia: [80, 81, 82]
  └── Razão: VGA text serve perfeitamente para MVP. Framebuffer é upgrade visual, 0 impacto funcional.

[80] Font rendering para alta resolução
  ├── Pré: [79] framebuffer
  └── Razão: sem framebuffer, sem render.

[60] AllocTier::Vram (alocar no BAR da GPU)
  ├── Pré: [68] PCI scan + BAR mapeado, [79] ou driver GPU específico
  └── Razão: BAR existe, mas driver GPU não. MVP aloca tudo em DRAM.
```

### C.3 — Camada 2: Armazenamento e Persistência (Sprint 24+)

Depende de PCI scan (Block 1), driver infrastructure (Camada 1).

```
[71] NVMe driver (PCI Class 01.08)
  ├── Pré: [68] PCI scan, [17] IOAPIC/MSI-X, [25] PerCpu (IRQ affinity)
  ├── → Bloqueia: [61, 62, 72]
  └── Razão: MVP é stateless. Sem SFS, NVMe é peso morto.

[72] VirtIO-blk (PCI 1AF4:1001)
  ├── Pré: [68] PCI scan, [17] IOAPIC, VirtIO transport (MMIO/PIO)
  ├── → Bloqueia: [61, 62]
  └── Razão: alternativa NVMe para QEMU. Mesma dependência SFS.

[73] VirtIO-net (PCI 1AF4:1041)
  ├── Pré: [68] PCI scan, [17] IOAPIC/MSI, VirtIO transport
  └── Razão: MVP sem rede. Nenhuma skill precisa de rede.

[74] VirtIO-gpu (PCI 1AF4:1050)
  ├── Pré: [68] PCI scan, [79] framebuffer (ou substitui)
  └── Razão: VGA text é suficiente.

[61] AllocTier::Nvme (alocar páginas no NVMe via SFS)
  ├── Pré: [71] NVMe driver OU [72] VirtIO-blk, [SFS] (sem ID, pós-MVP)
  → Bloqueia: [62]
  └── Razão: requer NVMe + SFS. Ambos pós-MVP.

[62] AllocTier::Hdd (cold storage)
  ├── Pré: [61] Nvme (ou driver ATA), [SFS]
  └── Razão: cold storage = SFS sobre HDD. Cadeia longa.

[70] PCI bridges (hierarquia de barramento)
  ├── Pré: [68] PCI scan (funciona cegamente sem bridges)
  └── Razão: scan bus 0..255 funciona. Bridges são refinamento para casos com >256 devices.

[6] Trust Cache: persistência no SFS (`/system/trust/usb.tbl`)
  ├── Pré: [4] TrustCache (Block 5), [SFS]
  └── Razão: TrustCache existe (Block 5), mas sem SFS é volátil.

[52] Atualização do MLP via skill WASM (baixar pesos novos)
  ├── Pré: [103] WASM embedder, [73] VirtIO-net (ou rede)
  └── Razão: requer WASM + rede. Duplo pós-MVP.
```

### C.4 — Camada 3: VirtIO e Aceleração Gráfica (Sprint 24+)

Depende de PCI (Block 1), driver infra (Camada 1-2).

```
[81] VirtIO-GPU 2D/3D acelerado
  ├── Pré: [74] VirtIO-gpu básico (já é o mesmo)
  └── Razão: VGA text é suficiente. VirtIO-GPU é upgrade.

[82] Tensor visualization (renderizar ativações no framebuffer)
  ├── Pré: [79] framebuffer, [81] VirtIO-GPU
  └── Razão: depende de framebuffer E GPU.

[75] Intel HDA audio driver
  ├── Pré: [68] PCI scan, BAR mapeado
  └── Razão: ❌ Descartado (A.7). Nenhuma skill de áudio no roadmap.
```

### C.5 — Camada 4: Scheduler e Runtime Avançado (Sprint 24+)

Depende de SMP (Block 2), heap expansível (Block 2).

```
[96] Agent Scheduler (round-robin com prioridade)
  ├── Pré: [95] Async Neural Executor (já existe), [24-33] SMP + PerCpu (>1 core)
  ├── → Bloqueia: [97, 98, 105]
  └── Razão: Executor cooperativo atual funciona para 1-4 cores. Scheduler completo é upgrade para servidores multi-core reais.

[97] Budget de execução (tokens_consumed por agente)
  ├── Pré: [96] Agent Scheduler
  └── Razão: sem scheduler, budget não tem onde atuar.

[98] MLP decide prioridade no scheduler
  ├── Pré: [96] Agent Scheduler, [47] MLP 512→256→64→9 (estendido para prioridade)
  └── Razão: scheduler precisa existir antes de ser MLP-orientado.

[100] Topic enum completo (AgentCreated, MemoryPressure, etc.)
  ├── Pré: [99] EventBus (já existe), design review
  └── Razão: strings funcionam. Enum é segurança de tipo, não funcionalidade.

[101] ML-based routing (EventBus consulta Intent Router)
  ├── Pré: [99] EventBus, [47] MLP, [100] Topic enum
  └── Razão: inovação futura. BTreeMap resolve para o MVP.

[85] Princípio: mínimo viável
  └── (Princípio, não implementação) — já guia todas as decisões.
```

### C.6 — Camada 5: WASM Embedder (Sprint 25+)

Depende de heap (Block 2), storage (Camada 2), scheduler (Camada 4).

```
[103] WASM embedder (wasmi em no_std)
  ├── Pré: [94] Slab allocator (heap para módulos WASM), [96] Agent Scheduler (tempo de execução)
  ├── → Bloqueia: [8, 11, 52, 104]
  └── Razão: wasmi é crate existente mas requer port para no_std. Skills Rust traits (atuais) são suficientes para MVP. WASM é upgrade de portabilidade e isolamento.

[104] Linear memory pool (256 KB por skill)
  ├── Pré: [103] WASM embedder
  └── Razão: sem WASM, sem pool.

[8] WASM skill dispatch para protocolos USB
  ├── Pré: [1] xHCI, [103] WASM embedder
  └── Razão: USB + WASM. Duplo pós-MVP.
```

### C.7 — Camada 6: Memória Avançada (Sprint 23-24+)

Depende de BitmapAllocator (Genesis), Slab (Block 2), PCI (Block 1).

```
[92] Huge Pages 2 MiB (mapper + TLB miss reduction)
  ├── Pré: [91] BitmapFrameAllocator (existe), mapeador de páginas 2 MiB no page table
  ├── → Bloqueia: [93, 107]
  └── Razão: MVP não tem inferência pesada. MLP de arquitetura (37 KB) cabe em 1 página 4 KiB. Huge Pages são para modelos grandes (pós-MVP).

[93] Huge Pages 1 GiB (CPUID check + contiguous alloc)
  ├── Pré: [92] Huge Pages 2 MiB funcionando, CPUID leaf para 1 GiB pages
  ├── → Bloqueia: [107]
  └── Razão: 1 GiB depende de 2 MiB primeiro. Requer hardware real com 1 GiB pages suportadas.

[107] Neural Cache (lookup table 50 ns em Huge Pages)
  ├── Pré: [92] Huge Pages, [105] Intent Planner
  └── Razão: cache de decisões só faz sentido quando o planner existe.
```

### C.8 — Camada 7: Cognitive Runtime (Fase 6)

Depende de WASM (Camada 5), Scheduler (Camada 4), Huge Pages (Camada 6).

```
[105] Intent Planner (sequência de SkillCommands)
  ├── Pré: [96] Agent Scheduler, [47] MLP, [103] WASM embedder (skills complexas)
  ├── → Bloqueia: [106, 107]
  └── Razão: MVP classifica intent única (SystemStatus vs SkillRequest vs Unknown). Planner multi-etapa requer scheduler + WASM.

[106] Success Engine (feedback loop, ajuste online de pesos)
  ├── Pré: [105] Intent Planner, [47] MLP (pesos ajustáveis)
  └── Razão: pesquisa acadêmica. Ajuste online de pesos em no_std é problema aberto.

[51] Treinamento offline do MLP (10k hardware profiles)
  ├── Pré: [47] MLP arquitetura (Block 4), dataset sintético
  └── Razão: pesos iniciais heurísticos funcionam. Treinamento real é refinamento.

[109] Sprint 16: Slab Allocator → 🔄 Block 2 Sprint 19
  └── Já incorporado.

[110] Sprint 17: Agent Scheduler → 🔄 Sprint 24+
  └── Já remapeado.

[111] Sprint 18+: Cognitive Runtime → 🔄 Fase 6
  └── Já remapeado.
```

### C.9 — Camada 8: Meta / MatMul-Free (Fase 7)

Depende de toda a stack abaixo.

```
[108] MatMul-free LM (RWKV/Mamba/ternary pooling)
  ├── Pré: [107] Neural Cache, [92] Huge Pages, [103] WASM embedder
  ├── → Bloqueia: (nada — é meta final)
  └── Razão: futuro distante. Roadmap original já marcava como Fase 7.
```

### C.10 — Camada S: Sponsor / Hardware Real

Depende de patrocínio ou hardware físico. Sem data.

```
[36] Npu struct + try_init() via PCI scan
  ├── Pré: [68] PCI scan, AMD XDNA hardware (APU real) ou QEMU com NPU virtual
  └── Razão: sem documentação pública do XDNA, sem QEMU com NPU, sem implementação testável.

[37] Accelerator::XDNA(Npu) enum
  ├── Pré: [36] Npu struct
  └── Razão: sem struct, sem enum.

[38] Command queue circular + doorbell write
  ├── Pré: [36] Npu struct, documentação XDNA
  └── Razão: vendor-specific.

[39] Overlay loading via MMIO
  ├── Pré: [36] Npu struct, AMD Vitis toolchain
  └── Razão: overlay é compilado pelo Vitis AI, não pelo kernel.

[40] MSI-X interrupt registration para NPU
  ├── Pré: [36] Npu, [17] IOAPIC/MSI support
  └── Razão: depende do resto da stack NPU.

[43] Cadeia de programação: Modelo → Overlay → DRAM
  ├── Pré: [36-40] toda stack NPU
  └── Razão: cadeia completa vendor-dependent.

[45] Caminho de migração NPU (QEMU → APU fase 1/2/3)
  ├── Pré: [36-43] toda stack NPU
  └── Razão: é o roteiro, não a implementação.

[116] Port para ARM/RISC-V
  ├── Pré: nova arquitetura-target (aarch64-unknown-none, riscv64-unknown-none)
  └── Razão: x86-64 é o target do MVP. ARM/RISC-V seria novo projeto.
```

### C.11 — Dependências Cruzadas (Grafo Resumido)

```
MVPs ─── Block 1 (PCI) ─── Block 2 (SMP) ─── Block 3 (Chat) ─── Block 4 (MLP) ─── Block 5 (Skills) ─── MVP
  │           │                                                        │
  │           ▼                                                        ▼
  │     ┌──────────────┐                                      ┌────────────────┐
  │     │ Layer 1      │                                      │ Layer 4        │
  │     │ Sprint 23+   │                                      │ Sprint 24+     │
  │     │ xHCI         │                                      │ Agent Scheduler│
  │     │ Framebuffer  │                                      │ Budget         │
  │     │ GPU BAR      │                                      │ Topic Enum     │
  │     └──────┬───────┘                                      └───────┬────────┘
  │            ▼                                                      ▼
  │     ┌──────────────┐                                      ┌────────────────┐
  │     │ Layer 2      │                                      │ Layer 5        │
  │     │ Sprint 24+   │                                      │ Sprint 25+     │
  │     │ NVMe/VirtIO  │                                      │ WASM Embedder  │
  │     │ SFS          │                                      │ Linear Pool    │
  │     │ Storage      │                                      └───────┬────────┘
  │     └──────┬───────┘                                              ▼
  │            ▼                                              ┌────────────────┐
  │     ┌──────────────┐                                      │ Layer 7        │
  │     │ Layer 3      │                                      │ Fase 6         │
  │     │ VirtIO-GPU   │◄───── [107] Neural Cache ◄──── [105] │ Intent Planner │
  │     │ Áudio/Vídeo  │                                      │ Success Engine │
  │     └──────────────┘                                      └───────┬────────┘
  │            ▼                                                      ▼
  │     ┌──────────────┐                                      ┌────────────────┐
  │     │ Layer 6      │                                      │ Layer 8        │
  │     │ Huge Pages   │◄──────────────────────────────── [108]│ MatMul-Free LM │
  │     │ 2MiB → 1GiB  │                                      │ (Fase 7)       │
  │     └──────────────┘                                      └────────────────┘
  │
  └── Layer S (Sponsor): NPU XDNA, ARM/RISC-V ── sem data, sem hardware
```

### C.12 — Regras de Engenharia (derivadas da hierarquia)

1. **Um item na camada N só pode começar quando todos os pré-requisitos das camadas < N estão estáveis.** Ex: NVMe (Layer 2) não começa antes de PCI (Layer 0 / Block 1) estar compilando e testado.

2. **Cada sprint tem um "teto de camada".** Sprint 23 só mexe em Layer 1. Sprint 24 só mexe em Layer 2. Isso evita dispersão.

3. **Itens sponsor têm data = "quando tivermos hardware + funding".** A stack de software (PCI, APIC, SMP) estará pronta antes. NPU XDNA poderá ser integrada assim que o hardware chegar.

4. **Nada desta hierarquia bloqueia o MVP.** Todo item pós-MVP tem um caminho claro de volta para a chain principal (Block 1 → 5). Se o MVP termina em Sprint 22, o Layer 1 começa limpo em Sprint 23.

5. **Se um pré-requisito muda de camada (ex: Huge Pages se torna essencial para MLP), o item sobe de camada.** A hierarquia é revisada a cada sprint review.
