# Roadmap — neural-os-core v0.59.0 🏆

**Última atualização:** 2026-06-29

## 🏆 Marco Conquistado: Bootloader 0.11 + Framebuffer UEFI!

Bootloader 0.9.34 → 0.11.15. Framebuffer 1280×720 com Hermes grafico.
DisplayAgent renderiza NeuralConsole no framebuffer do bootloader.
20 agentes rodando, Hermes Cognitive completo.

## Blocos Completos (17 blocos, 59 sprints)

| Bloco | Sprints | v | Status |
|---|---|---|---|
| 1. Chassi | 1-17 | 0.1–0.12 | ✅ VGA, heap, EventBus, IPC, SMP, APIC |
| 2. Discovery | 18-22 | 0.13–0.17 | ✅ PCI, ACPI, MHI, Trust, LAPIC |
| 3. Rede | 23-24 | 0.23–0.24 | ✅ RTL8139, smoltcp |
| 4. Transformer | 26-27 | 0.26–0.27 | ✅ Attention BitNet 272K |
| 5. HW-Aware LLM | 28-30 | 0.28–0.30 | ✅ PCI+USB training (66K pairs) |
| 6. Capabilities | 31 | 0.31 | ✅ HW→skill mapping |
| 7. Self-Healing | 32-37 | 0.32–0.37 | ✅ Failure taxonomy, checkpoint |
| 8. Agent/Skill-First | 39-42 | 0.39–0.40 | ✅ Agent trait, 18 agentes |
| 9. Network Evolution | 43-44 | 0.41–0.42 | ✅ DHCP, VirtIO-net, NetPhy |
| 10. Display+Bugfix | 45 | 0.43–0.45 | ✅ Framebuffer, VirtIO-GPU, 5 bugs |
| 11. CDC+Delta+Locks | 46-47 | 0.46–0.47 | ✅ IrqSafeLock, DmaBuf, Rabin |
| 12. Network+Platform | 48 | 0.48 | ✅ x2APIC, Huge Pages, PCI bridges, Cron |
| 13. Trust & Security | 49-50 | 0.49–0.50 | ✅ Ed25519, Security Pipeline |
| 14. Hermes Cognitive | 51-55 | 0.51–0.55 | ✅ SDD, ReAct, Council, Self-Opt |
| 15. Memory+Ecosystem | 56-57 | 0.56–0.57 | ✅ Medusa, Pipeline, MemTree, KG |
| **16. HW Real + USB** | **58** | **0.58** | **✅ Boot HW real, xHCI HID, FAT12, ATA, CAD** |
| **17. Bootloader 0.11** | **59** | **0.59** | **✅ Framebuffer UEFI 1280×720, bootloader 0.11, Hermes grafico** |

## Funcionalidades por Camada

### ✅ Kernel Base
- `no_std` Rust, `x86_64-unknown-none`, nightly
- VGA text mode (80×25, scroll, cores)
- Serial (COM1 115200 baud)
- IDT 0-31, PIC/APIC dual EOI
- Bitmap Frame Allocator (4GB)
- Heap 16MB (LockedHeap + Slab)
- FPU/SSE, Tensor f32, matmul
- BitNet 1.58-bit (ADD/SUB kernel)
- Transformer 4 layers, Attention, 272K params

### ✅ Hardware Discovery
- PCI scan (CF8/CFC, 256 bus, capabilities, bridges)
- ACPI (RSDP/RSDT/XSDT/MADT)
- APIC (LAPIC, IOAPIC, x2APIC)
- SMP (INIT-SIPI-SIPI, PerCpu, stacks)
- WHPX acceleration
- Memory Hierarchy Index (Dram/Vram/Nvme/Hdd)

### ✅ Rede
- RTL8139 driver (I/O ports)
- smoltcp 0.13 (TCP/IP, DHCP, HTTP)
- VirtIO-net manual
- NetPhy unificada

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
