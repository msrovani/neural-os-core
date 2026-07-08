# CATÁLOGO DE TECNOLOGIAS — neural-os-core

**~19.000 LOC, 165+ arquivos Rust, 247+ agentes, 0 erros de compilação**
**Primeiro commit:** `8ac5ac7` — Bare-metal Rust microkernel chassis
**Último:** `f94dc48` — SmileyOS patterns completos (~v0.103.0)

---

## 1. KERNEL CORE — Fundação do Sistema

| Tecnologia | Descrição | ADR | IDEA | Arquivo | Sprint |
|---|---|---|---|---|---|
| Bootloader 0.11.15 | UEFI/BIOS handoff, framebuffer, 512KB stack | ADR-0001 | — | `main.rs` | v0.59 |
| VGA Text Mode 80×25 | 16 cores, scroll, VGA sequencer screen-off | ADR-0002 | #316 | `vga_buffer.rs` | v0.02 |
| Serial 16550 | 4-port probe, `serial_println!` | ADR-0002 | — | `serial.rs` | v0.02 |
| IDT 32 handlers | Double Fault IST, Breakpoint, GPF, Page Fault | ADR-0003 | — | `interrupts.rs` | v0.03 |
| GDT/TSS | Kernel segments, IST stacks | ADR-0003 | — | `interrupts.rs` | v0.03 |
| OffsetPageTable | CR3 page table walker | ADR-0004 | #91, #94 | `memory.rs` | v0.04 |
| Bitmap Frame Allocator | 128KB bitmap 4GB | ADR-0004 | #91, #94 | `memory.rs` | v0.11 |
| LockedHeap 16MB | linked_list_allocator, 0x4444_4444_0000 | ADR-0004 | #91, #94 | `allocator.rs` | v0.04 |
| Slab Allocator | 8 buckets, per-CPU, free list | ADR-0004 | — | `slab.rs` | v0.14 |
| Adaptive Heap | resize_heap_to_mb(), AI budget | ADR-0004 | — | `memory_agent.rs` | v0.76 |
| SIMD/FPU SSE/AVX/FMA | CR0/CR4, `#[deny(fpu)]` | ADR-0005 | — | `simd.rs` | v0.05 |
| PIC 8259A | Remap vec32/40, dual EOI | ADR-0009 | #367 | `interrupts.rs` | v0.08 |
| PIT Timer | IRQ0 watchdog, TIMER_TICKS | ADR-0009 | #367 | `interrupts.rs` | v0.08 |
| LAPIC Timer | Tick 12-192 t/s, IPI, init_count | ADR-0037 | #16-42 | `apic.rs` | v0.16 |
| IOAPIC | IRQ routing, RTE masking | ADR-0037 | #16-42 | `apic.rs` | v0.16 |
| x2APIC | MSR-based, IA32_APIC_BASE | ADR-0037 | #16-42 | `apic.rs` | v0.48 |
| SMP Multi-Core | INIT-SIPI-SIPI, trampoline | ADR-0037 | #16-42 | `smp/mod.rs` | v0.14 |
| PerCpu | GS.base, cpu_id, lapic_id, core_type | ADR-0037 | #36 | `smp/percpu.rs` | v0.81 |
| SPSC Lock-Free Ring | Atomic head/tail | ADR-0037 | #36, #319 | `smp/spsc.rs` | v0.81 |
| IPI Handlers | 3 vetores (reschedule/halt/call) | ADR-0037 | #16-42 | `interrupts.rs` | v0.81 |
| Work-Stealing | Chase-Lev deques | ADR-0037 | #39, #322 | `smp/work_stealing.rs` | v0.82 |
| Parallel AVX2 Matmul | Chunk dim per core | ADR-0037 | #323 | `smp/parallel_matmul.rs` | v0.82 |
| Huge Pages 2MiB/1GiB | allocate_huge_2mb() | ADR-0037 | — | `memory.rs` | v0.48 |
| PCI CF8/CFC | 256 bus, BAR0-5, bridges | ADR-0014 | #68-70 | `pci.rs` | v0.13 |
| ACPI Parser | RSDP, RSDT/XSDT, MADT | ADR-0037 | #19, #34 | `acpi.rs` | v0.13 |
| MMIO Typed Registers | Register\<T\> (Tock port) | ADR-0026 | #280 | `mmio.rs` | v0.59 |
| TicketLock FIFO | AtomicUsize ticket/serving | — | — | `ticket-lock/` | v0.13 |
| IrqSafeLock | cli/RFLAGS.IF, deadlock-free ISR | — | — | `sync/irq_lock.rs` | v0.47 |
| DmaBuf | dma_alloc() → UC pages | — | — | `dma.rs` | v0.47 |
| Async Executor | Cooperative AgentTask, DummyWaker | — | — | `task/` | v0.12 |
| CFS Scheduler | vruntime-based fairness | ADR-0037 | — | `cfs.rs` | v0.82 |
| Dynamic Tick | LAPIC init_count calibrado | — | — | `memory_agent.rs` | v0.76 |
| EventBus IPC | Publish/subscribe, CapabilityToken | ADR-0024 | #99-101 | `event-bus/` | v0.12 |

---

## 2. AGENTES — Tudo é um Agente (247+)

### 2.1 Agentes Nativos (A-001 a A-020)

| Tecnologia | Tipo | Função | Sprint |
|---|---|---|---|
| **SystemAgent** A-001 | System/Oneshot | Init, SYSTEM_READY, EchoSkill | v0.40 |
| **MonitorAgent** A-002 | System/Oneshot | Publica SYSTEM_READY | v0.40 |
| **HwBridgeAgent** A-003 | Router/Continuous | Scancode IRQ bridge | v0.40 |
| **NetAgent** A-004 | Network/Continuous | smoltcp poll + HTTP | v0.40 |
| **InputAgent** A-005 | Console/Continuous | Keyboard (PS/2 + USB xHCI) | v0.40 |
| **CortexAgent** A-006 | Inference/Continuous | LLM generate_text + Medusa + Trinity | v0.40 |
| **HermesAgent** A-007 | Router/Continuous | Intent routing + ReAct + Council + Skills | v0.40 |
| **DisplayAgent** A-008 | Console/Continuous | Framebuffer BGRA32 + compositor | v0.43 |
| **NetDriverAgent** A-009 | Driver/Oneshot | RTL8139 + VirtIO-net | v0.40 |
| **UsbDriverAgent** A-010 | Driver/Oneshot | xHCI port scan | v0.40 |
| **BootSelfHealAgent** A-011 | System/Oneshot | SelfHeal init | v0.40 |
| **BootTrustAgent** A-012 | System/Oneshot | TrustCache init, Ed25519 | v0.40 |
| **PlatformAgent** A-013 | System/Oneshot | PCI+ACPI+APIC+SMP+x2APIC | v0.40 |
| **MemoryAgent** A-014 | System/Oneshot | MHI + SystemArchitecture + Adaptive Heap | v0.40 |
| **GpuDriverAgent** A-015 | Driver/Oneshot | VirtIO-GPU + GPU backend detect | v0.45 |
| **HwDetectAgent** A-016 | System/Oneshot | HwIdentifySkill + IA device tree | v0.40 |
| **CronAgent** A-017 | System/Continuous | Cron Scheduler | v0.48 |
| **SecurityAgent** A-018 | System/Continuous | 5 detectores (PortScan, ARP, etc) | v0.50 |
| **SafetyAgent** A-019 | System/Continuous | Asimov 4 Leis Interceptor | v0.51 |
| **OptimizerAgent** A-020 | System/Continuous | Self-Optimization + scaling | v0.54 |

### 2.2 Storage & Filesystem

| Tecnologia | Descrição | Sprint |
|---|---|---|
| **DiskIntelligenceAgent** | StorageController (ATA/USB/NVMe/AHCI), 10+ FS, SMART, ARC cache | v0.75 |
| **AtaAgent** | `/mnt/hdd/sda` — ATA block device | v0.62 |
| **DevFsAgent** | `/dev/pci/`, `/dev/rtl8139`, `/dev/xhci`, `/dev/mem` | v0.62 |
| **ProcFsAgent** | `/proc/agents`, `/proc/meminfo`, `/proc/uptime` | v0.62 |
| **HermesFsAgent** | `/chat/` — send, last_response, history | v0.62 |
| **InferenceFsAgent** | `/inference/` — arquivos gerados por LLM | v0.62 |
| **RamFsAgent** | `/mnt/ram/` — DRAM cache 1MB, LRU | v0.62 |
| **MouseAgent** | PS/2, IRQ12, 3-byte packets, 5 skills | v0.61 |

### 2.3 The Agency (147 agentes especialistas)

| Divisão | Agentes | Função |
|---|---|---|
| Engineering | ~12 | Code review, architecture, testing |
| Design | ~12 | UI/UX, visual design, acessibilidade |
| Product | ~12 | Requirements, roadmaps, priorização |
| QA | ~12 | Testing, bug reporting, quality |
| Support | ~12 | User assistance, troubleshooting |
| Marketing | ~12 | Documentação, comunicação |
| Infra | ~12 | Infrastructure, deployment, monitoring |
| Data Science | ~12 | Analytics, metrics, pipelines |
| Research | ~10 | Literature review, experiment design |

### 2.4 Desktop Apps

| App | Descrição | Sprint |
|---|---|---|
| Hermes Chat | Console + shell (F1) | v0.90 |
| Settings | Theme/sound/memory/avatar/network (F2) | v0.90 |
| Power | Shutdown/reboot/hibernate (F3) | v0.90 |
| BitNet IDE | Geração de skills WASM (F4) | v0.93 |
| Camera App | Preview USB camera (F10) | v0.94 |
| AudioViz App | Espectroscópio de áudio (F11) | v0.94 |

---

## 3. HARDWARE DRIVERS — 25+ Drivers

| Tecnologia | Descrição | ADR | IDEA | Arquivo | Sprint |
|---|---|---|---|---|---|
| PCI CF8/CFC | 256 bus, BAR0-5, bridges | ADR-0014 | #68-70 | `pci.rs` | v0.13 |
| ACPI | RSDP/MADT/RSDT | ADR-0037 | #19, #34 | `acpi.rs` | v0.13 |
| LAPIC/IOAPIC | Timer, IPI, IRQ, x2APIC | ADR-0037 | #16-42 | `apic.rs` | v0.13 |
| RTL8139 | I/O ports, 4 TX desc, RX ring | ADR-0016 | #124 | `rtl8139.rs` | v0.23 |
| E1000 (82540EM) | MMIO, TDT, 48 RX desc | ADR-0016 | #250-255 | `e1000.rs` | v0.20 |
| VirtIO-net | PCI legacy, desc, MSI-X | ADR-0016 | #73, #117 | `virtio_net.rs` | v0.42 |
| VirtIO-GPU | PCI caps, MMIO, 2D | ADR-0029 | #74, #273 | `virtio_gpu.rs` | v0.45 |
| xHCI USB | Port scan, HID boot, BOT | ADR-0026 | #1-15 | `xhci.rs` | v0.29 |
| USB HID Keyboard | 68 keys scancode | — | — | `xhci.rs` | v0.58 |
| USB Mass Storage BOT | SCSI READ/WRITE | ADR-0030 | — | `usb_msc.rs` | v0.68 |
| ATA PIO | read_sectors, wait_bsy | ADR-0030 | #282b | `ata.rs` | v0.58 |
| AHCI SATA NCQ | MMIO, PRDT, DMA | ADR-0030 | — | `ahci.rs` | v0.87 |
| NVMe | Admin queue, SQ/CQ | ADR-0030 | #71, #303e | `disk_agent/nvme.rs` | v0.75 |
| Intel GPU (Gen Ring) | MI_BATCH_BUFFER, BCS | ADR-0029 | #326-332 | `gpu/intel.rs` | v0.66 |
| NVIDIA GPU (Pascal) | Push Buffer 0x002000 | ADR-0037 | #326-332 | `gpu/nvidia.rs` | v0.84 |
| AMD GPU (RDNA) | PM4 packets, PSP | ADR-0037 | #326-332 | `gpu/amd.rs` | v0.84 |
| SPSC GPU Job Ring | Doorbell generico | ADR-0037 | #327 | `gpu/ring.rs` | v0.84 |
| GPU Secure Boot | ACR/PSP/GuC | ADR-0037 | #352 | `gpu/firmware.rs` | v0.84 |
| VRAM Buddy Allocator | Power-of-2, split/merge | ADR-0037 | #328 | `gpu/vram.rs` | v0.84 |
| GPU Backend | NVIDIA→AMD→Intel→CPU | ADR-0037 | #353 | `gpu/backend.rs` | v0.66 |
| Intel HDA Audio | PCI DMA, codec | ADR-0014 | #83 | `audio/hda.rs` | Sound |
| USB Audio (UAC) | USB Audio Class | ADR-0014 | #84 | `audio/usb.rs` | Sound |
| UVC Camera | USB Video, YUYV→RGB | — | — | `uvc_driver.rs` | v0.94 |
| WiFi Generic | WifiChipset trait, union | ADR-0016 | #124 | `generic_wifi.rs` | v0.97 |
| TPM 2.0 TIS | MMIO 0xFED40000, SHA256 | ADR-0025 | #305 | `tpm.rs` | v0.74 |

---

## 4. AI / ML — Inteligência Artificial

| Tecnologia | Descrição | ADR | IDEA | Arquivo | Sprint |
|---|---|---|---|---|---|
| Tensor Engine | Tensor shape+data, matmul, apply | ADR-0006 | — | `tensor.rs` | v0.05 |
| PackedTernaryTensor | 2-bit/weight, matmul_hybrid | ADR-0011 | — | `tensor.rs` | v0.11 |
| BitNet AVX2 Kernel | _mm256_cvtepi8_epi32, FMA | ADR-0019 | #323 | `bitnet_avx2.rs` | v0.79 |
| BitNet-b1.58 850M | Microsoft GQA, BitFFN, 30L | ADR-0019 | #126-156 | `cortex.rs` | v0.79 |
| Transformer Engine | QKV causal, softmax, RMSNorm | ADR-0019 | #126-148 | `cortex.rs` | v0.26 |
| Cortex LLM | generate_text(), autoregressive | ADR-0019 | #126-148 | `cortex.rs` | v0.27 |
| KV Cache | 200× speedup (6h→84s) | ADR-0019 | #170 | `cortex.rs` | v0.80 |
| BPE Tokenizer | HuggingFace tokenizer.json | ADR-0019 | #127 | `bpe.rs` | v0.79 |
| Medusa Speculative | 3 heads, draft→verify | ADR-0019 | #140 | `cortex.rs` | v0.56 |
| Trinity MoE Router | 6 experts, keyword + ML router | ADR-0033 | #311 | `trinity.rs` | v0.79 |
| RustCoder Expert | hidden=128, 6L, 1.6M | — | #311c | `tools/rust_coder.bitnet` | v0.97 |
| HWExpert SDIO MoE | 213K HWIDs, 2.794 INF | ADR-0033 | #311b | `tools/hw_expert.bitnet` | v0.97 |
| BGE Embedding | 33.4M, 384-dim, ONNX→bitnet | ADR-0023 | — | `tools/convert_onnx` | v0.89 |
| GGUF Loader | Q4_0 dequant, Model trait | ADR-0028 | #278 | `gguf.rs` | v0.78 |
| Codebook VQ | 256×64, nearest-neighbor | — | #169 | `cognitive.rs` | v0.89 |
| MatMul-Free LM | RWKV-style WKV forward | — | #108 | `cognitive.rs` | v0.95 |
| PTRM | Gaussian + Q-head + 3 trajetórias | — | — | `cortex.rs` | v0.63 |
| Kanerva Memory | Sparse distributed hamming | — | — | `kanerva.rs` | v0.63 |
| BitNetTrainer | On-device ADD/SUB, STE | ADR-0033 | #312 | `cognitive.rs` | v0.95 |
| Trinity AutoLearn | Detecta→treina→registra expert | ADR-0033 | #311f | `agents.rs` | v0.102 |
| HW Register Map IA | 3 niveis HWID→IA→Heuristica | — | — | `cortex.rs` | v0.100 |

---

## 5. REDE — Conectividade

| Tecnologia | Descrição | ADR | IDEA | Arquivo | Sprint |
|---|---|---|---|---|---|
| smoltcp Integration | Device trait, TCP/UDP/ARP/DNS | ADR-0016 | #117-124 | `netstack.rs` | v0.24 |
| DHCP (smoltcp) | socket-dhcpv4, auto IP | ADR-0016 | #251 | `dhcp.rs` | v0.42 |
| ARP | smoltcp + gateway hardcoded | ADR-0016 | — | `netstack.rs` | v0.42 |
| HTTP Client | Connecting→Sending→Done | ADR-0016 | — | `netstack.rs` | v0.24 |
| ICMP Ping | Echo Request/Reply | ADR-0016 | — | `net.rs` | v0.20 |
| IP Static Fallback | 10.0.2.15/24 | ADR-0016 | — | `network_agent.rs` | v0.60 |
| DNS | Via smoltcp | ADR-0016 | — | `netstack.rs` | v0.24 |
| NetPhy Unified | RTL8139→VirtIO fallback | ADR-0016 | — | `netstack.rs` | v0.42 |
| WiFi Agent | Scan, select, dual-network | ADR-0016 | #124 | `wifi_agent.rs` | v0.97 |
| Link Watcher | Ethernet+WiFi failover | — | — | `link_watcher.rs` | v0.97 |

---

## 6. DISPLAY / UI — Interface Visual

| Tecnologia | Descrição | ADR | IDEA | Arquivo | Sprint |
|---|---|---|---|---|---|
| UEFI Framebuffer | BGRA32, 1280×720 | ADR-0014 | #79-82 | `display/fb.rs` | v0.59 |
| NeuralConsole | Texto, scroll, cores | ADR-0014 | #79-82 | `display/console.rs` | v0.59 |
| JarvisDesktop | Multi-window, dock, drag, close | ADR-0036 | #315 | `display/compositor.rs` | v0.61 |
| Theme Engine | 5 temas hot-swap | — | #279b | `display/theme.rs` | v0.61 |
| Font Engine | VGA 8×16 + TTF | — | — | `display/font.rs` | v0.94 |
| JARVIS Avatar | Particulas 4 estados | ADR-0036 | #315 | `display/avatar.rs` | v0.86 |
| Tensor Viz | Heatmap + attention | — | — | `display/compositor.rs` | v0.94 |
| Desktop Cube | 3D rotation crossfade | ADR-0029 | #283, #286 | `gpu/cube.rs` | v0.66 |
| WASM Icons | Skills no desktop | ADR-0032 | #309 | `display/compositor.rs` | v0.93 |
| LLM Icons | Bitmap 8×8 via HWEXPERT | — | — | `display/compositor.rs` | v0.103 |

---

## 7. MEMÓRIA E ARMAZENAMENTO

| Tecnologia | Descrição | ADR | IDEA | Arquivo | Sprint |
|---|---|---|---|---|---|
| MHI | MemoryTier, AllocTier | ADR-0014 | #63-67 | `mhi.rs` | v0.21 |
| ARC Cache | 1MB DRAM, MFU/MRU | ADR-0030 | — | `disk_agent/cache.rs` | v0.75 |
| MemoryTree v2 | MemNode, TTL, Ebbinghaus | ADR-0023 | #214-227 | `event-bus/memory_tree.rs` | v0.56 |
| Knowledge Graph | KNode+KEdge | ADR-0023 | #221 | `event-bus/kgraph.rs` | v0.56 |
| SHA-256 Dedup | Sliding window 5min | ADR-0023 | #214 | `event-bus/dedup.rs` | v0.89 |
| Hybrid Search | BM25+MLP, RRF | ADR-0023 | #217 | `event-bus/hybrid_search.rs` | v0.89 |
| 4-Tier Consolidation | Working→Episodic→Semantic→Procedural | ADR-0023 | #218 | `event-bus/atkinson.rs` | v0.89 |
| Atkinson-Shiffrin | Sensory→STM→LTM | ADR-0023 | #224 | `event-bus/atkinson.rs` | v0.89 |
| Ebbinghaus Decay | strength = I×e^(-λ·days) | ADR-0023 | #219 | `event-bus/metacognitive.rs` | v0.89 |
| VFS Layer | Mount, resolve, lookup | ADR-0030 | #281 | `vfs/mod.rs` | v0.62 |
| FAT32 | Read/write, clusters | ADR-0030 | — | `fat32.rs` | v0.75 |
| OverlayFS | Multi-layer, CoW | — | — | `vfs/` | v0.96 |
| Zero-Copy SFS | Slice references | — | — | `self_heal.rs` | v0.96 |

---

## 8. SEGURANÇA

| Tecnologia | Descrição | ADR | IDEA | Arquivo | Sprint |
|---|---|---|---|---|---|
| Ed25519 Identity | verify_signature() bare-metal | ADR-0020 | #166, #176 | `identity.rs` | v0.50 |
| TPM 2.0 | SHA256, PCR[8], FIFO | ADR-0025 | #305 | `tpm.rs` | v0.74 |
| TrustCache | Token→skill, deny, TTL | ADR-0025 | — | `trust.rs` | v0.17 |
| Security Agent | 5 detectores | ADR-0025 | #256-264 | `security.rs` | v0.50 |
| Safety Interceptor | 4 Asimov Leis, halt | ADR-0025 | — | `safety.rs` | v0.51 |
| Merkle Audit Trail | SHA-256 chain, Ed25519 | ADR-0025 | — | `audit.rs` | v0.87 |
| Fail-Closed | 4 invariantes SMT-proof | ADR-0025 | — | `safety.rs` | v0.87 |
| Path Confinement | PathRule + check_path() | ADR-0025 | — | `trust.rs` | v0.49 |
| Mask Secrets | 12 padrões→REDACTED | ADR-0025 | — | `trust.rs` | v0.49 |
| SelfHeal | FailureClass, RecoveryAction | ADR-0027 | #366-374 | `self_heal.rs` | v0.32 |
| Failure Taxonomy | 5 classes | — | — | `self_heal.rs` | v0.96 |
| Corrective Prompting | Error→LLM→recovery | — | — | `self_heal.rs` | v0.96 |

---

## 9. J.A.R.V.I.S. PERSONA

| Tecnologia | Descrição | Sprint |
|---|---|---|
| SOUL.md Engine | Name/tone/humor/formality/empathy | v0.86 |
| Emotion Analysis | 7 emoções + sarcasmo | v0.88 |
| Ego Layer | Confidence per domain, can_answer() | v0.90 |
| Proactive Heartbeats | Alertas (disk/mem/net) | v0.90 |
| Dream Engine | Insights sintéticos, clustering | v0.90 |
| Auto-Skill Gen | ≥3 repetições → skill | v0.90 |
| Fluid Persona | Coach/Tutor/Tool adaptativo | v0.87 |
| Session Compression | 4 estratégias (summarize/merge/segment) | v0.86 |
| Consciousness Metrics | 10 métricas | v0.73 |
| SleepCycle | REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT | v0.89 |

---

## 10. AUDIO

| Tecnologia | Descrição | Sprint |
|---|---|---|
| Intel HDA | PCI DMA, codec discovery | Sprint Sound |
| USB Audio (UAC) | USB Audio Class | Sprint Sound |
| Neural TTS (PocketTTS) | 100M params, ~200ms latência | Sprint Sound |
| Formant TTS | Síntese por formantes | Sprint Sound |
| VAD | Voice Activity Detection | Sprint Sound |
| SER | Speech Emotion Recognition | Sprint Sound |
| Wake Word | "Jarvis" via Rustpotter | Sprint Sound |
| Audio Ring Buffer | PCM circular lockless | Sprint Sound |
| Audio Mixer | Mixagem PCM | Sprint Sound |

---

## 11. TREINO E FERRAMENTAS (30+ Scripts Python)

| Ferramenta | Função | Dados |
|---|---|---|
| `train_hw_model.py` | Treina modelo HW ID (66K+ pares) | PCI + USB + SDI |
| `sdio_moe_pipeline.py` | Pipeline SDIO .inf/.sys → JSONL | 305K HWIDs |
| `sdio_daily.py` | Watcher SDIO (18 packs, 2.794 entradas) | DP_* .7z |
| `samdrivers_full.py` | Pipeline SamDrivers (7 packs validados) | soft.samlab.ws |
| `finetune_rust_llm.py` | Fine-tuning RustCoder (41.2K samples) | HuggingFace |
| `extract_sdi_bin.py` | Extrai SDI .bin (305K HWIDs) | Windows SDI |
| `publish_hf_dataset.py` | Publica dataset no HuggingFace | Sanitizado |
| `build_image.py` | Cria imagem FAT12+FAT32 | Kernel + modelos |

---

## 12. ECOSSISTEMA — Código Aberto Portado

| Fonte | Padrão | Arquivo |
|---|---|---|
| redox-os (16.4K★) | SchemeHandler trait | `scheme.rs` |
| theseus-os (3.2K★) | TypedAgent<Boot\|Running\|Faulted> | `state.rs` |
| embassy (9.5K★) | TimerWheel 64-slot | `timer_wheel.rs` |
| openai/swarm (21.8K★) | Handoff enum | `hermes.rs` |
| tock/tock (5.3K★) | Register<T> + RegisterField | `mmio.rs` |

---

## 13. SMLEYOS PATTERNS (Implementados nativamente)

| Padrão | Descrição | Status |
|---|---|---|
| Shell 55+ comandos | ls, cat, ps, top, fetch, netstat, mkdir, touch... | ✅ v0.103 |
| Temas (5) | Hot-swap, hermes-dark/dracula/matrix/solarized/light | ✅ v0.61 |
| Compositor multi-window | Drag, resize, [X] close, dock bar | ✅ v0.103 |
| WASM Executor | VM stack-based, 20+ opcodes, fuel metering | ✅ v0.103 |
| Ícones LLM | Bitmap 8x8 via HWEXPERT_MODEL | ✅ v0.103 |
| App SDK via trait | Agent trait + AppRegistry | ✅ v0.61 |
| v86 browser | Emulador x86 WASM | ❌ Pendente |

---

## 14. SPRINT 84-103 — INOVAÇÕES RECENTES

| Tecnologia | Sprint | Descrição |
|---|---|---|
| GPU Foundations | 84 | BAR UC, SPSC ring, VRAM buddy, secure boot |
| GPU Decode | 85 | Prefill/decode split, KV DMA, XQueue |
| JARVIS Persona | 86 | Avatar, SOUL.md, IPW, Session Compression |
| Emotion + Cache | 88 | EmotionEngine, SleepCycle, NeuralCache |
| JARVIS Cognitive | 89 | DreamEngine, AutoSkillGen, BabelIndex |
| Desktop UI | 90 | Hermes Chat, Settings, Power apps |
| WASM Runtime | 93 | MemoryPool, 15 WASI→Skill, BitNet IDE |
| Vision | 94 | UVC camera, YOLO, TTF engine |
| Cognitive Engine | 95 | 25+ itens: IntentPlanner, SuccessEngine, CodebookVQ |
| Self-Healing | 96 | ZeroCopySfs, FailureTaxonomy, CorrectivePrompting |
| RustCoder Expert | 97 | hidden=128, 41.2K amostras, loss 0.34 |
| Trinity MoE no LLM | 98 | generate_via_model() roteia internamente |
| SDIO MoE Pipeline | 97-99 | 2.794 entradas, 18 packs, .inf+.sys+pefile |
| AutoLearn | 102 | Detecta necessidade → BitNetTrainer → Expert |
| HW Register Map IA | 100-101 | Síntese de registradores por classificação |
| SmileyOS Nativo | 103 | 55+ cmd, drag, resize, wasm exec, llm icons |

---

## 15. WORKSPACE CRATES

| Crate | Versão | LOC | Descrição |
|---|---|---|---|
| neural-kernel | v0.103 | ~16.500 | Kernel + drivers + agentes + IA + display |
| agent-core | v0.1 | ~1.200 | Agent trait, scheduler, pipeline, DAG |
| skill-registry | v0.1 | ~800 | Skill trait, MCP, registry, FanOut |
| event-bus | v0.1 | ~1.000 | IPC, MemoryTree, KG, Ecosystem |
| ticket-lock | v0.1 | ~100 | TicketLock FIFO |

---

## 16. SPRINTS — Linha do Tempo (v0.01 a v0.103)

| Sprint | v | Foco | LOC | Commit |
|---|---|---|---|---|
| 1-2 | 0.01-0.02 | Chassis + VGA + Serial | ~500 | `8ac5ac7` |
| 3-5 | 0.03-0.05 | IDT, Memory, SIMD, Tensor | ~1.000 | `19bbd0e` |
| 9-11 | 0.09-0.11 | BitNet ternário, 2-bit packing | ~800 | `cb2c04a` |
| 13-16 | 0.13-0.16 | PCI, ACPI, APIC, SMP | ~1.500 | `t6u7v8w` |
| 23-24 | 0.23-0.24 | RTL8139, smoltcp, HTTP | ~1.000 | — |
| 27-30 | 0.27-0.30 | Cortex LLM, xHCI, USB | ~1.500 | — |
| 40-45 | 0.40-0.45 | Agent system, Display, VirtIO-GPU | ~2.000 | — |
| 50-56 | 0.50-0.56 | Security, Safety, Medusa, MemoryTree | ~2.500 | — |
| 59-61 | 0.59-0.61 | Bootloader, compositor, temas, mouse | ~2.000 | — |
| 74-76 | 0.74-0.76 | TPM, FAT32, NVMe, adaptive heap | ~1.500 | — |
| 77-80 | 0.77-0.80 | Agentic, LLM Infra, AVX2, KV Cache | ~3.100 | — |
| 81-83 | 0.81-0.83 | SMP, Work-Stealing, Polimento | ~1.200 | — |
| 84-85 | 0.84-0.85 | GPU Foundations, Decode | ~2.700 | — |
| 86-90 | 0.86-0.90 | JARVIS, Emotion, Desktop, Deep Cognitive | ~5.000 | — |
| 91-94 | 0.91-0.94 | LAN, WASM, Vision, Display | ~3.000 | — |
| 95-96 | 0.95-0.96 | Cognitive Engine, Self-Healing | ~860 | — |
| 97 | 0.97.x | RustCoder Expert + Trinity MoE | ~300 | `575115b` |
| 98 | 0.98.x | Trinity MoE no LLM | ~50 | `7b3e428` |
| 99 | 0.99.x | SDIO Dataset (2.794 entradas) | ~500 | `001c47f` |
| 100 | 0.100.x | Salto 1: Register Map IA | ~250 | `b034a1a` |
| 101 | 0.101.x | Saltos 2+3: Router + Boot Agent | ~130 | `4933f00` |
| 102 | 0.102.x | Trinity AutoLearn | ~170 | `f8edd70` |
| 103 | 0.103.x | SmileyOS Nativo (55+ cmd, drag, wasm, icons) | ~450 | `f94dc48` |

---

> **Total: ~19.000 LOC, 165+ Rust files, 247+ agentes, 39 ADRs, 500+ commits, 0 erros.**
> De um microkernel bare-metal a um sistema operacional neural com IA, GPU, rede, áudio e aprendizado on-device.
