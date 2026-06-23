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

MLP 512→256→64→9 ternário (pesos embutidos no kernel)
  ├── Embedding do brand name: [f32; 32]
  ├── Core counts, freqs, cache: normalizados
  ├── Features one-hot (AVX-512, AMX, VNNI, x2APIC, hybrid...)
  ├── RAM total, PCI count, accelerators
  └── 9 saídas categóricas (argmax por grupo):
        ring0: {software, NPU, P_core}
        ring1: {P_cores, GPU, hybrid}
        ring2: {E_cores, idle_P, mixed}
        p_cores_for_ring1: u8
        sfs_policy: {NVMe_only, tiered, all_ram}
        heap_size_mb: u16
        trust_default: {deny, allow_known, allow}
        power_policy: {performance, balanced, low}
        heap_tier: {VRAM, RAM, NVMe} (prepara Memory Hierarchy)

SystemArchitecture → Config dinâmica
  ├── init_heap(arch.heap_size_mb) — tamanho do heap decidido pelo MLP
  ├── init_smp(arch.ring0, ring1, ring2, arch.p_cores_for_ring1)
  ├── init_npu(arch.ring0_target) — se NPU, tenta init; fallback software
  ├── init_trust(arch.trust_default) — política de confiança
  └── init_power(arch.power_policy) — decisão de energia

Boot flow adaptativo
  ├── HardwareInventory::collect() antes de qualquer configuração
  ├── infer_architecture() → SystemArchitecture
  ├── init_*() na ordem determinada pela arch
  └── Boot log exibe a decisão do MLP

Hardware query skill
  ├── "status hardware" → exibe inventário + decisão do MLP
  └── "status arch" → exibe SystemArchitecture ativa
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
  heap           → 64 MB
  trust          → allow_known
  power          → balanced

[SYSTEM] Neural OS Hermes ready.
Type 'help' for available commands.

> 

> help
[CORTEX] Skills: echo, system_status, system_info, pci_scan, power, trust, arch, help
[CORTEX] Digite '<skill> help' para detalhes.
[CORTEX] Hardware detectado: VGA text, serial, PS/2 keyboard, APIC timer, PCI bus

> status
[SKILL] CPU: i5-7200U @ 2.5 GHz (boost 3.1), 2 P-cores/4 threads
[SKILL] RAM: 16384 MB total, 0.2% ocupada
[SKILL] PCI: 15 dispositivos (GPU GTX 1050, NVMe, xHCI...)
[SKILL] Trust: 3 entradas ativas

> power off
[SKILL] Shutting down...
```

### Critérios de aceite:

| Requisito | Obrigatório | Desejável |
|---|---|---|
| Boota em QEMU `-m 512M` | ✅ | — |
| Boota em hardware real UEFI | ✅ | ✅ |
| PCI scan detecta dispositivos | ✅ | ✅ |
| MLP de arquitetura decide config | ✅ | — |
| Chat loop funcional (VGA + serial) | ✅ | ✅ |
| Intent Router classifica 3+ intents | ✅ | — |
| Skills: status, pci, power, help | ✅ | ✅ |
| Trust Cache funcional | ✅ | — |
| APIC timer (sem PIC) | ✅ | — |
| SMP (mínimo 2 cores) | — | ✅ |
| Shutdown via ACPI | ✅ | — |
| Slab allocator + heap 4 MB | ✅ | — |

### O que NÃO é o MVP:

| Excluído | Motivo | Previsão |
|---|---|---|
| USB driver | Complexidade alta, PS/2 legacy suficiente | Sprint 23+ |
| NVMe driver | MVP é stateless, sem SFS | Sprint 23+ |
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
