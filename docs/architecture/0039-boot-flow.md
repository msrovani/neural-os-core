# ADR-0039: Fluxo Completo de Boot — neural-os-core v1.0.0

**Data:** 2026-07-08 (última revisão: 2026-07-10)
**Status:** Accepted
**Propósito:** Documentar exaustivamente o fluxo de boot do kernel, todas as fases, pontos de decisão, fallbacks e onde cada serviço de IA entra.
**Nota:** O código atual implementa 8 fases (BootPhase: SafeHarbor, MemoryCore, SystemBringup, Diagnostics, HardwareDiscovery, DriverInit, AgentFleet, Runtime). As referencias a numeros de linha neste documento estao defasadas — consultar `main.rs` para localizacoes exatas.

---

## 1. VISÃO GERAL — MACRO FLUXO

```
INICIO (bootloader 0.11.15)
  │
  ▼
┌──────────────────────────────────────────────────────────────────┐
│ FASE 0: INIT (hardware mínimo)                                   │
│ Serial → Framebuffer/UGA → IDT → Heap → SIMD → TPM              │
│ SEM IA ainda. SEM alocação dinâmica (pré-heap).                 │
└──────────────────────────────────┬───────────────────────────────┘
                                   │
                                   ▼
┌──────────────────────────────────────────────────────────────────┐
│ FASE 1: SYSTEM BRINGUP (sistema base)                            │
│ Slab allocator → CortexAgent ACORDA (modelo carregado)           │
│ IA: modelo carregado mas SEM inferência ainda                    │
└──────────────────────────────────┬───────────────────────────────┘
                                   │
                                   ▼
┌──────────────────────────────────────────────────────────────────┐
│ FASE 2: HARDWARE DISCOVERY (drivers + FS)                        │
│ NICs → ATA → AHCI → xHCI → FAT32 → VFS → GPU → Audio            │
│ IA: Nenhuma. Drivers clássicos, sem IA.                          │
└──────────────────────────────────┬───────────────────────────────┘
                                   │
                                   ▼
┌──────────────────────────────────────────────────────────────────┐
│ FASE 3: AGENT FLEET (registro + execução)                        │
│ init_phase(): 14 boot agents executam EM ORDEM                   │
│ ├── PlatformAgent: PCI+ACPI+APIC+SMP (HW discovery real)        │
│ ├── BootSelfHealAgent: analisa shutdown anterior                 │
│ ├── memory_agent: budget adaptativo                              │
│ ├── NetDriverAgent: RTL|E1000|VirtIO                             │
│ ├── HwDetectAgent: ▶ PRIMEIRA INFERENCIA IA ◀                    │
│ │   └── PCI scan → HWExpert identifica cada device               │
│ │   └── generate_register_map() para dispositivos de rede        │
│ │   └── Publica LLM_REQUEST com device tree                      │
│ └── GpuDriverAgent + HDA + USB-Audio + UVC                      │
│                                                                   │
│ ▶ registry.run() → LOOP PRINCIPAL (eterno) ◀                     │
│ ├── Tick 1: CortexAgent ▶ PRIMEIRA GERACAO LLM ◀                 │
│ │   (processa LLM_REQUEST do HwDetectAgent)                      │
│ ├── Tick 1: MonitorAgent → SYSTEM_READY                          │
│ ├── Tick 1-10: NetAgent → DHCP/static IP                         │
│ ├── Tick 1+: HermesAgent → prompt > + ReAct                      │
│ ├── Tick 1+: DisplayAgent → JARVIS desktop                       │
│ ├── Tick 40+: DNS + HTTP                                         │
│ └── Tick 600: DHCP timeout → fallback IP estático                │
└──────────────────────────────────────────────────────────────────┘
```

---

## 2. FASE 0 — INIT (kernel_main, linha 969)

```
kernel_main(boot_info)
  │
  ├─ 1. serial::probe_port(0x3F8)          → COM1 serial OK?
  ├─ 2. outb(0x3F8, 'K')                   → alive byte
  │
  ├─ 3. display::fb::probe_uefi_framebuffer(boot_info)
  │     │
  │     ├── FB encontrado? → vga_buffer::disable_vga_plane()
  │     │                    (VGA sequencer I/O, evita page fault pre-IDT)
  │     └── SEM FB? → vga_buffer::init(pm_offset)
  │                     (VGA text mode 80x25 legacy)
  │
  ├─ 4. interrupts::init_idt()             → IDT carregada, 32 handlers
  │     ├── GDT + TSS + IST (Double Fault)
  │     ├── PIC remap (vetores 32-47)
  │     ├── PIT timer (~18.2 Hz)
  │     ├── LAPIC + IOAPIC
  │     └── Page Fault handler → retorna (antes: loop{hlt()})
  │
  ├─ 5. memory::BitmapFrameAllocator::empty()
  │     └── frame_allocator.init(memory_regions)
  │
  ├─ 6. memory::init_memory(pm_offset)     → page tables
  ├─ 7. allocator::init_heap()             → heap em 0x4444_4444_0000
  ├─ 8. simd::enable_simd()                → SSE/AVX/FMA
  │     └── has_avx2() → QEMU: false, HW real: depende
  │
  └─ 9. tpm::init_tpm(pm_offset)          → TPM ausente = fallback silencioso
```

**Nesta fase:** SEM IA. Apenas o mínimo para o kernel existir.

---

## 3. FASE 1 — SYSTEM BRINGUP (linha 1081)

```
publish_boot_phase(SystemBringup)
  │
  ├─ memory::init_global_allocator()       → frame allocator global
  ├─ publish_boot_phase(Diagnostics)
  ├─ SLAB_ALLOCATOR metrics
  │
  └─ CortexAgent::new()                    → MODELO CARREGADO
       │
        ├── modelo 2B carregado via FAT32/ramdisk
       ├── cortex::load_model(micro_data)  → TransformerModel
       ├── cortex::set_model(Box::new(model))
       └── subscriptions: TOPIC_LLM_REQUEST
```

**Nesta fase:** Modelo carregado na memória. **SEM inferência ainda.** O CortexAgent só vai gerar texto quando receber um LLM_REQUEST no EventBus (o que acontece durante init_phase, após HwDetectAgent executar).

---

## 4. FASE 2 — HARDWARE DISCOVERY (linha 1115)

```
publish_boot_phase(HardwareDiscovery)
  │
  ├── net::init_driver_rtl8139()           → PCI scan → RTL8139?
  │     └── FALHOU → net::init_driver_e1000() → e1000?
  │         └── FALHOU → (VirtIO-net no runtime)
  │
  ├── ata::AtaDriver::probe()             → ATA PIO?
  ├── ahci::AhciDriver::new()             → SATA AHCI?
  ├── xhci::init_xhci()                   → USB xHCI?
  │
  ├── usb_msc::UsbMassStorage::probe()    → USB storage?
  │
  ├── FAT32 + boot_logger::init()         → BOOT LOG PERSISTENTE
  │
  ├── verify_kernel_from_disk()           → Ed25519 signature check
  │     └── FALHOU? → boot continua (soft enforcement)
  │
  ├── VfsRegistry::new()                  → 9 mounts padrão
  │     ├── /mnt/hdd/    → AtaAgent
  │     ├── /dev/        → DevFsAgent
  │     ├── /proc/       → ProcFsAgent
  │     ├── /inference/  → InferenceFsAgent
  │     ├── /chat/       → HermesFsAgent
  │     ├── /mnt/ram/    → RamFsAgent
  │     └── /logs/       → LogFsAgent
  │
  ├── DiskIntelligenceAgent               → Storage controllers
  ├── apps::init_apps()                   → 6 desktop apps
  ├── audio::init_audio()                 → Áudio subsistema
  │
  ├── BGE.BIN loading (FAT32)             → Embedding model
  │     └── AUSENTE? → fallback silencioso
  │
  └── gpu::detect::detect_all()           → GPU detection
        ├── VirtIO-GPU (QEMU)
        ├── Intel/NVIDIA/AMD (HW real)
        ├── Unknown → CPU fallback
        └── gpu::vram::init_vram_tier()
```

**Nesta fase:** SEM IA. Apenas drivers clássicos.

---

## 5. FASE 3 — AGENT FLEET (linha 1381)

### 5.1 Registro dos Agentes

```
AgentRegistry::new()
  │
  ├── 14 boot agents registrados (Oneshot → executam no init_phase)
  ├── 20+ runtime agents (Continuous → executam no registry.run())
  └── 30+ Agency agents (The Agency, 12 divisões)
```

### 5.2 init_phase — Boot Agents EXECUTAM EM ORDEM

```
registry.init_phase()
  │
  ├── 1. PlatformAgent (Oneshot)
  │     ├── PCI scan                              → dispositivos enumerados
  │     ├── ACPI (RSDP/MADT)                      → LAPIC/IOAPIC list
  │     ├── APIC init                             → LAPIC+IOAPIC+PIT+SMP
  │     └── SMP (INIT-SIPI-SIPI)                  → APs acordados
  │
  ├── 2. MemoryAgent (Oneshot)
  │     └── scan_pci() + SystemArchitecture::infer() → MHI tiers
  │
  ├── 3. BootSelfHealAgent (Oneshot)              ← TRATAMENTO DE ERROS
  │     ├── read_last_shutdown_from_boot_log()
  │     ├── ├── Unexpected → analisa boot log anterior
  │     │   │               → procura PANIC/GPU_HUNG
  │     │   │               → SelfHeal::analyze()
  │     │   │               → RecoveryAction
  │     │   └── log "self-heal applied"
  │     ├── ├── Expected/Triggered → log "ok"
  │     └── └── None (primeiro boot) → log "first boot"
  │
  ├── 4. BootTrustAgent (Oneshot)
  │     └── TRUST_CACHE init                      → Ed25519 keys
  │
  ├── 5. memory_agent::MemoryAgent (Oneshot)
  │     └── Budget adaptativo                     → heap/cache/KV split
  │
  ├── 6. NetDriverAgent (Oneshot)
  │     ├── virtio_net::init_driver_virtio()      → VirtIO-net?
  │     ├── FALHOU? → (já tentou RTL/e1000 antes)
  │     └── log "offline" se tudo falhou
  │
  ├── 7-9. HDA + USB-Audio + UVC (Oneshot)
  │     └── Drivers de áudio/vídeo (stub)
  │
  ├── 10. GpuDriverAgent (Oneshot)
  │      └── virtio_gpu::init_driver_virtio_gpu() → GPU init
  │
  ├── 11. FsBridgeAgent (Oneshot)
  │      └── VFS/MHI bridge → idle inicialmente
  │
  ├── 12. HwDetectAgent (Oneshot)                  ← PRIMEIRA IA
  │     ├── PCI scan (6 dispositivos QEMU / N+ HW real)
  │     ├── Para CADA dispositivo:
  │     │   ├── generate_via_hwexpert(              ← SINCRONO, HWExpert
  │     │   │   "identifique PCI\\VEN_X&DEV_X")
  │     │   │   └── FALHOU? → nome genérico
  │     │   └── SE classe 0x02/0x0D (rede):
  │     │       └── generate_register_map(vid, did) ← MAPA REGISTRADORES
  │     │           ├── Nível 1: mapa fixo (40+ HWIDs)
  │     │           ├── Nível 2: IA classifica família
  │     │           └── Nível 3: heurística vendor
  │     └── Publica LLM_REQUEST com device tree
  │
  ├── 13. AutoLearnAgent (PollEvery 200)
  │      └── Subscrive TRINITY_UNMATCHED
  │
  ├── 14. SleepCycleAgent (PollEvery 1000)
  │      └── Fase IDLE (aguarda 5000 ticks)
  │
  └── 15. DiskIntelligenceAgent (Oneshot)
         └── Scan discos + partições + SMART
```

### 5.3 registry.run() — LOOP PRINCIPAL

```
registry.run() → LOOP ETERNO
  │
  ├── tick += 1
  ├── RESPAWN queue → recria agents crashed
  │
  ├── StateGraph ativo? → graph.advance()
  │
  └── Round-robin sobre agentes ativos:
        │
        ├── Rate-limit: >50 Pending consecutivos → skip 4/5 ticks
        ├── FlowTrigger check (Schedule/Start/Listen/Router)
        ├── agent.tick(tick, counter)
        │
        ├── tick=1:
        │   ├── CortexAgent: ▶ PRIMEIRA GERACAO LLM ◀
        │   │   └── processa LLM_REQUEST (device tree)
        │   │   └── generate_via_model()
        │   │       ├── TrinityRouter.classify() → expert?
        │   │       ├── HWExpert carregado? → usa HWExpert
        │   │       ├── RustCoder carregado? → usa RustCoder
        │   │       └── FALLBACK → BitNet LLM principal
        │   │
        │   ├── MonitorAgent: SYSTEM_READY → EventBus
        │   ├── SystemAgent: recebe SYSTEM_READY → conclui
        │   ├── NetAgent: init netstack → DHCP
        │   ├── HermesAgent: aguarda input (sem prompt ainda)
        │   └── DisplayAgent: JARVIS desktop → framebuffer
        │
        ├── tick=10+:
        │   └── NetAgent: DHCP poll ou static IP
        │
        ├── tick=40+:
        │   └── DNS + HTTP GET (se DHCP funcionou)
        │
        ├── tick=200:
        │   └── AutoLearnAgent: verifica intents não classificados
        │
        ├── tick=600:
        │   └── DHCP timeout → static IP 10.0.2.15/24
        │
        └── tick=5000:
            └── SleepCycleAgent: inicia ciclo REPLAY→DREAM→...
```

---

## 6. FLUXOGRAMA DE DECISÃO — NETWORK

```
init_driver_rtl8139()
  ├── PCI scan → VID 0x10EC, class 0x02?
  │   ├── SIM → init RTL8139 (PIO) → OK
  │   └── NÃO →
  │         └── init_driver_e1000()
  │               ├── PCI scan → VID 0x8086, class 0x02?
  │               │   ├── SIM → init e1000 (MMIO) → OK
  │               │   └── NÃO → (fallback para VirtIO-net no Agent)
  │               │
  │               ▼ (durante NetDriverAgent.init_phase)
  │         virtio_net::init_driver_virtio()
  │               ├── PCI 1AF4:1000/1041?
  │               │   ├── SIM → init VirtIO-net → OK
  │               │   └── NÃO → "offline"
  │               │
  │               ▼ (runtime, NetAgent.ticks)
  │         smoltcp::Interface com phy unificado
  │               ├── DHCP → poll até tick 600
  │               │   ├── OK → IP dinâmico + DNS + HTTP
  │               │   └── TIMEOUT → IP estático 10.0.2.15/24
  │               └── smoltcp TCP/UDP/ARP/DNS
```

---

## 7. FLUXOGRAMA DE DECISÃO — GPU

```
gpu::detect::detect_all()
  ├── PCI scan → class 0x03 (VGA controller)
  │   ├── VirtIO-GPU (1AF4:1050)
  │   │   ├── MMIO BAR mapping
  │   │   │   ├── map_mmio_page() OK → init control queue
  │   │   │   └── Page Fault → GPU desabilitada (boot continua)
  │   │   └── GET_DISPLAY_INFO
  │   │       ├── OK → framebuffer GPU
  │   │       └── FALHOU → fallback UEFI framebuffer
  │   │
  │   ├── Intel HD (8086:XXXX, class 03)
  │   │   ├── BAR0 UC mapping
  │   │   └── Gen ring buffer init
  │   │
  │   ├── NVIDIA (10DE:XXXX)
  │   │   ├── BAR0 UC mapping
  │   │   ├── PFIFO + FALCON init (stub)
  │   │   └── ACR firmware (secure boot)
  │   │
  │   ├── AMD (1002:XXXX)
  │   │   ├── BAR0 UC mapping
  │   │   ├── PM4 ring (stub)
  │   │   └── PSP firmware (secure boot)
  │   │
  │   └── DESCONHECIDA → CPU fallback
  │
  └── gpu::backend::init_backend()
        └── Seleciona: NVIDIA > AMD > Intel > CPU
```

---

## 8. FLUXOGRAMA — SELF-HEAL NO BOOT

```
BootSelfHealAgent.init_phase()
  │
  ├── SELF_HEAL.lock()
  ├── read_last_shutdown_from_boot_log()
  │
  ├── CAUSA = Unexpected?
  │   ├── SIM → Analisa boot log anterior:
  │   │   ├── Contém "PANIC"?        → RecoveryAction::Restart
  │   │   ├── Contém "GPU_HUNG"?     → RecoveryAction::ResetGpu
  │   │   ├── Contém "OOM"?          → RecoveryAction::ExpandHeap
  │   │   └── Genérico               → RecoveryAction::LogAnalyze
  │   │
  │   ├── SelfHeal::analyze()
  │   │   ├── → RecoveryAction
  │   │   ├── → Publica KERNEL_ERROR
  │   │   └── → log "self-heal applied"
  │   │
  │   └── boot continua (sistema tenta de novo)
  │
  ├── CAUSA = Expected/Triggered?
  │   └── log "ok" (desligamento normal)
  │
  └── CAUSA = None?
      └── log "first boot" (sem log anterior)
```

---

## 9. FLUXOGRAMA — IA NO BOOT

```
FASE 0-2: SEM IA (drivers clássicos, hardware discovery)
  │
  ▼
HwDetectAgent (init_phase, linha 1407)
  ├── PCI scan → N dispositivos
  ├── Para cada device:
  │   ├── HWExpert.generate(                              ← 1a INFERÊNCIA
  │   │   "identifique PCI\\VEN_X&DEV_X")                 ← modelo 72KB
  │   │   ├── HWExpert carregado? → resposta direta
  │   │   └── FALHOU? → nome genérico "PCI XXXX:XXXX"
  │   │
  │   └── SE network/wireless:
  │       └── generate_register_map(vid, did)             ← 2a INFERÊNCIA
  │           ├── Nível 1: mapa fixo (match HWID)
  │           ├── Nível 2: HWExpert classifica família
  │           └── Nível 3: heurística por vendor ID
  │
  └── Publica LLM_REQUEST no EventBus (device tree)
      │
      ▼
CortexAgent (tick 1 do registry.run())
  ├── Recebe LLM_REQUEST
  ├── generate_via_model(device_tree)                     ← 1a GERACAO LLM
  │   ├── TrinityRouter.classify()
  │   │   ├── router_weight treinado? → ML → softmax
  │   │   └── FALHOU? → keyword matching
  │   │
  │   ├── É "hw_identify"? → HWExpert.generate()
  │   ├── É "rust_coder"? → RustCoder.generate()
  │   └── NENHUM expert? → BitNet LLM.generate()          ← FALLBACK
  │
  └── Publica LLM_RESPONSE no EventBus
      │
      ▼
  HermesAgent recebe → exibe no console/desktop
```

---

## 10. FLUXOGRAMA — PANIC HANDLER

```
kernel panic (qualquer causa)
  │
  ├── serial_println! panic info (sem alloc)
  ├── VGA text/framebuffer panic info
  │
  ├── shutdown::set_cause(Unexpected)
  ├── shutdown::write_persistent_shutdown_log()
  │   └── Escreve "SHUTDOWN: unexpected/panic tick=N" na FAT32
  │
  ├── HEAP DISPONÍVEL?
  │   ├── SIM:
  │   │   ├── Classifica falha (FailureClass):
  │   │   │   ├── "PageFault"/"OOM"/"memory" → MemoryFault
  │   │   │   ├── "GPF"/"#DF" → ExecutionFault
  │   │   │   └── Genérico → LogicFault
  │   │   ├── SelfHeal::analyze() → RecoveryAction
  │   │   └── Publica KERNEL_ERROR no EventBus
  │   └── NÃO:
  │       └── loop { hlt() } (morre em silêncio)
  │
  └── loop { hlt() }
```

---

## 11. MAPA DE ARQUIVOS POR FASE

| Arquivo | Fase | O que faz |
|---------|------|-----------|
| `main.rs:969-1053` | 0 | Init: serial, fb, IDT, heap, SIMD |
| `main.rs:1081-1115` | 1 | SystemBringup: CortexAgent criado |
| `main.rs:1115-1371` | 2 | Hardware: NICs, ATA, USB, FS, GPU |
| `main.rs:1381-1427` | 3a | AgentRegistry: registro |
| `main.rs:1427-1435` | 3b | init_phase: boot agents executam |
| `main.rs:1435-1559` | 3c | Runtime agents registrados |
| `main.rs:1559-1826` | — | Model loading (FAT32/QEMU) |
| `main.rs:1874-1880` | — | AgentFleet → registry.run() |
| `interrupts.rs:118-123` | 0 | Page Fault handler |
| `interrupts.rs:125-260` | 0 | IDT, GDT, TSS, PIC, PIT, APIC |
| `boot_logger.rs` | 0→2 | Pre-FAT buffer → FAT32 persist |
| `self_heal.rs` | 3b | Análise de shutdown anterior |
| `hw_agents.rs` | 3b | PCI scan → HwAgents (init_phase) |
| `cortex.rs:1166-1216` | 3b | generate_via_model() → MoE routing |
| `trinity.rs` | 3b | MoE router classify_intent() |
| `agents.rs` | 3b | HwDetectAgent + AutoLearnAgent |
| `agent-core/src/lib.rs:285` | 3c | registry.run() → main loop |
| `shutdown.rs` | — | Shutdown tracking + persist |
| `cortex.rs` | 3c | generate_via_model() — **síncrono, bloqueante** |

## 12. STUTTERING DA INFERÊNCIA — Problema Conhecido

### O Problema
O scheduler é **cooperativo round-robin**. `generate_via_model()` executa a inferência completa de forma **síncrona e bloqueante**. No QEMU, uma geração de 64 tokens leva ~60s — o sistema inteiro congela.

### Impacto
| Serviço | Efeito |
|---------|--------|
| NetAgent | Pacotes IP perdidos |
| DisplayAgent | Tela congela |
| HermesAgent | Sem resposta ao usuário |
| InputAgent | Teclado não responde |
| WifiAgent | Beacons 802.11 perdidos |

### Solução Planejada: Inferência Fatiada (Tick-Sliced)
```
CortexAgent.tick():
├── Estado "idle": aguarda LLM_REQUEST
├── Estado "processing":
│   ├── Executa 1 forward pass (N tokens) por tick
│   ├── Retorna Pending → scheduler continua
│   ├── Próximo tick: continua de onde parou
│   └── Complete → publica LLM_RESPONSE
└── Retorna Pending se idle
```

### Status
Atualmente `generate_via_model()` é síncrono/bloqueante. A versão tick-sliced está pendente de implementação. Pós B-01, com rede funcional, o stuttering se torna mais crítico (perda de pacotes).
