# Roadmap — neural-os-core v0.76.1 🏆

**Última atualização:** 2026-07-03

## Blocos Completos (19 blocos, 76 sprints)

| Bloco | Sprints | v | Status |
|---|---|---|---|
| 1-15. Foundation | 1-57 | 0.1–0.57 | ✅ Kernel, PCI, Rede, Transformer, Self-Heal, Agents |
| **16. HW Real + USB** | **58** | **0.58** | **✅ Boot HW real, xHCI HID, FAT12, ATA, CAD** |
| **17. Bootloader 0.11** | **59** | **0.59** | **✅ Framebuffer UEFI 1280×720, bootloader 0.11** |
| **18. Security** | **74** | **0.74.x** | **✅ TPM TIS, Ed25519 signing, Partition mask 0x1C** |
| **19. Disk Intelligence** | **75** | **0.75.x** | **✅ DiskAgent, NVMe, SMART, ARC cache, GPT** |
| **20. Memory + Tick** | **76** | **0.76.x** | **✅ Adaptive heap, Dynamic tick, Event-driven Hermes** |

## Funcionalidades por Camada

### ✅ Kernel Base
- `no_std` Rust, `x86_64-unknown-none`, nightly
- Framebuffer UEFI 1280×720
- IDT 0-31, PIC/APIC dual EOI
- Bitmap Frame Allocator (dynamic sizing)
- **Adaptive Heap** (16 MB → resize para modelo AI, via frame allocator)
- FPU/SSE, Tensor f32, matmul
- BitNet 1.58-bit (ADD/SUB kernel)
- Transformer 4 layers, Attention, 272K params

### ✅ Storage
- **DiskIntelligenceAgent** (6 controladoras, 10+ FS probes)
- NVMe driver (Admin queue + Identify + I/O Read)
- USB-MSC bulk fix (xHCI IOC+ring+ERDP + BOT protocol)
- S.M.A.R.T. monitoring (ATA 0xB0+0xD0, health alerts)
- GPT partition table, SED/OPAL detection
- ARC cache 1MB DRAM + tier migration MHI
- FAT32-only (Fat12Writer removido, 102 LOC eliminated)

### ✅ Security
- TPM 2.0 TIS driver (SHA256 embedded, PCR[8] extend, fallback)
- Ed25519 kernel signing + auto-verification
- Partition mask 0x1C (Hidden FAT32 LBA, bootloader-compatible)
- Shutdown tracking (4 causas, FAT32 persistence)

### ✅ Agent Runtime
- **Dynamic tick** (12-192 ticks/s, calibrado por workload)
- **Hermes event-driven** (silêncio sem trabalho real)
- **AgentTier classification** (Permanent/SystemDemand/UserDemand/Periodic/Learning)
- EventDriven scheduler fix (has_event=true, has_pending early-return)
- MemoryAgent com clock calibration via rdtsc

### ✅ Input
- PS/2 keyboard (IRQ1, scancode set 1)
- **xHCI USB HID keyboard** (Boot Protocol, 68 teclas)
- Ctrl+Alt+Del (PS/2 + USB) com shutdown+FAT12 dump

### ✅ Display
- VGA text mode buffer (0xB8000)
- **UEFI framebuffer** (preparado, aguarda bootloader 0.11+)
- VirtIO-GPU (QEMU)
- Console multi-região, fonte VGA 8×16

### ✅ Agentes (20 agentes)
| Código | Agente | Tipo | Função |
|---|---|---|---|
| A-001 | SystemAgent | System | Init, EchoSkill |
| A-002 | MonitorAgent | System | SYSTEM_READY |
| A-003 | HwBridgeAgent | Router | IRQ bridge |
| A-004 | NetAgent | Network | smoltcp poll |
| A-005 | InputAgent | Console | Keyboard (PS/2 + USB) |
| A-006 | CortexAgent | Inference | LLM transformer + Medusa |
| A-007 | HermesAgent | Router | Intent routing, ReAct, Council |
| A-008 | DisplayAgent | Console | Framebuffer + VGA |
| A-009 | NetDriverAgent | Driver | RTL8139 + VirtIO-net |
| A-010 | UsbDriverAgent | Driver | xHCI init |
| A-011 | BootSelfHealAgent | System | SelfHeal init |
| A-012 | BootTrustAgent | System | TrustCache init |
| A-013 | PlatformAgent | System | PCI+ACPI+APIC+SMP |
| A-014 | MemoryAgent | System | MHI + Arch |
| A-015 | GpuDriverAgent | Driver | VirtIO-GPU |
| A-016 | HwDetectAgent | System | HwIdentifySkill |
| A-017 | CronAgent | System | Cron Scheduler |
| A-018 | SecurityAgent | System | Security Pipeline |
| A-019 | SafetyAgent | System | Asimov 4 Laws |
| A-020 | OptimizerAgent | System | Self-Optimization |

### ✅ Trust & Security
- TrustCache (allow/deny/TTL/denylist)
- Ed25519 via `ed25519-compact`
- CapabilityToken enum (Legacy + Ed25519)
- 5 detectores (PortScan, ArpSpoof, etc)
- Path Confinement, Mask Secrets
- Graduated Enforcement (Observe→Warn→Contain→Enforce)
- Safety Interceptor (Asimov 4 Laws)

### ✅ Self-Healing
- FailureClass taxonomy (Memory/Execution/Resource/Logic/External)
- SelfHeal analyze + RecoveryAction
- Exception handlers (Page Fault, Double Fault, GPF)
- RESPAWN_QUEUE + corrective prompting
- CDC Rabin chunking + XOR Delta snapshot

### ✅ Hermes Cognitive
- DA Identity Layer (nome/versão/lema)
- Runtime SDD (goal/context/plan/rollback)
- ReAct 7 fases (OBSERVE→THINK→PLAN→BUILD→EXECUTE→VERIFY→LEARN)
- Council skill (3 vozes)
- Intent Transparency, Context Fencing
- Bitter Pill Engineering
- Usage Pattern Analyzer, Workflow Predictor
- Dynamic Resource Scaling, Reflex Threshold
- Self-Optimizing Scheduler

### ✅ Storage (novo em v0.58)
- **ATA PIO driver** (read/write via PCI class 0x01)
- **MBR parser** (tabela de 4 partições)
- **FAT12 filesystem** (BPB, root dir, append file)
- **patch_image.py** (cria partição FAT12 na imagem)

### ✅ Boot Hardware Real
- **primeiro boot em notebook físico** via SDHC USB
- VGA text mode funcional
- Hermes Cognitive rodando (ReAct)
- USB keyboard via xHCI
- Ctrl+Alt+Del com dump FAT12 + shutdown
- BOOT.LOG visível no Windows Explorer

## Pendências Técnicas

| Item | Esforço | Depende de |
|---|---|---|
| Prompt interativo `>` para chat | ~50 LOC | Nada |
| Framebuffer UEFI (bootloader 0.11+) | ~500 LOC | Upgrade bootloader |
| VirtIO-GPU GET_DISPLAY_INFO | Debug | QEMU TCG |
| SMP `-smp 2` sem WHPX | Debug | TCG atomicidade |
| Driver e1000/r8169 (rede real) | ~300 LOC | Teste HW |
| WASM sandbox (`wasmi`) | ~1500 LOC | Fase 5+ |
| Modelo 1.5B params (treino) | Python | GPU |
| Plugin Hub MCP Index | ~400 LOC | #236 |

## Prioridades Imediatas (v0.59)
1. **Prompt `>`** — Hermes aguarda input do usuário
2. **Completar call de funções** — Hermes executa skills via teclado
3. **Testar teclado USB no notebook** — driver já implementado
4. **Upgrade bootloader 0.11+** — framebuffer UEFI GOP
