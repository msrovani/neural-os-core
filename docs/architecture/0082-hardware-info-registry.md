# ADR-0082: HardwareInfo Registry — Fonte Única de Verdade de HW

**Status:** Accepted (MVP)  
**Lifecycle:** `fazendo`  
**Ideias relacionadas:** #520–#525  
**Substitui:** § disperso de ADR-0005, ADR-0055 (FeatureGate), ADR-0063 (hamming_dispatch)  
**Arquivo:** `crates/k_nano/src/platform_probe.rs`  

## Contexto

O AIOS precisa saber em que hardware está rodando para se auto-adaptar. Hoje essa informação está fragmentada:

- `FeatureGate` (política ISA + hypervisor) em `platform_probe.rs`
- `CpuFeatures` (CPUID bruto) — struct pública mas não unificada
- `HypervisorKind` — enum separado
- `CacheTopology` — estrutura independente
- GPU, storage, memória — cada um com seus próprios probes, sem registro central
- Cada crate (k_ai→SGDB, cortex→dispatch, hermes→scheduling) consulta separadamente

Isso força cada subsistema a saber **como** perguntar em vez de **o que** perguntar. Um AIOS que gera WASM para si mesmo precisa de uma API única e estável que qualquer agente consulte.

A informação de HW deve ser **gerada no boot e armazenada no SGDB**, não hardcoded num struct Rust que exigiria recompilação para expandir.

## Decisão

**HardwareInfo é um cache early-boot.** O struct `static mut HW_INFO` em `platform_probe.rs` é populado em `detect()` para consulta imediata durante as fases iniciais do boot. Quando o SGDB fica pronto (Fase 6/AgentFleet), `hw_info()` passa a ler do SGDB. O struct `HardwareInfo` nunca mais recebe campos novos — o SGDB é a fonte de verdade a longo prazo.

### Arquitetura em 2 camadas

```
Fase 2 (MemoryCore)
  └─ platform_probe::detect()
  └─ preenche static mut HW_INFO (CPU, ISA, hypervisor, cache)
  └─ consumido por: SGDB hamming dispatch, cortex SIMD dispatch, art.rs
  └─ struct HardwareInfo: apenas campos early-boot

Fase 6 (AgentFleet) — SGDB ready
  └─ boot_init() escreve namespace /hw/ no SGDB:
       put_kv("hw/cpu/avx2", "true")
       put_kv("hw/cpu/isa", "avx2+fma")
       put_kv("hw/cpu/hv", "whpx")
       put_kv("hw/cache/l1d", "32768")
       put_kv("hw/cache/l2", "262144")
       put_kv("hw/cache/l3", "6291456")
       put_kv("hw/mem/total_mb", "2048")
       put_kv("hw/timer/mhz", "0")

  └─ Cada probe posterior escreve seu namespace:
       GPU probe  → put_kv("hw/gpu/0/vendor", "nvidia") / "hw/gpu/0/vram_mb"
       StorageBus → put_kv("hw/storage/0/type", "nvme") / "hw/storage/0/sectors"
       NetAgent   → put_kv("hw/net/0/mac", "52:54:00:12:34:56")
       WifiAgent  → put_kv("hw/wifi/0/phy", "qca6174")
       HDA driver → put_kv("hw/audio/0/playback", "true")
       NPU probe  → put_kv("hw/npu/0/vendor", "amd")

  └─ HW Expert (modelo ML) enriquece após PCI scan:
       HW Expert v3   → put_kv("hw/pci/0/class", "display")
                        put_kv("hw/pci/0/arch", "turing")
                        put_kv("hw/pci/0/vendor_name", "NVIDIA")
       HW Expert v3   → put_kv("hw/pci/1/class", "network")
                        put_kv("hw/pci/1/vendor_name", "Intel")
                        put_kv("hw/pci/1/device_name", "E1000")
       HW Expert      → put_kv("hw/pci/2/vendor_name", "QEMU")
                        put_kv("hw/pci/2/device_name", "VGA")

       Também escreve inferências sobre USB:
       HW Expert      → put_kv("hw/usb/0/class", "mass_storage")
                        put_kv("hw/usb/0/vendor_name", "SanDisk")

Runtime (qualquer agente)
  └─ Consulta via SGDB:
       sgdb::get_kv("hw/cpu/avx2")          → "true"
       sgdb::get_kv("hw/gpu/0/vendor")      → "nvidia"
       sgdb::scan_prefix("hw/storage/")     → ["0/type", "0/sectors", …]
       sgdb::scan_prefix("hw/gpu/")         → ["0/vendor", "0/vram_mb", …]

  └─ WASM agent recebe snapshot /hw/* serializado no init:
       { "cpu": {"avx2": true, "isa": "avx2+fma"},
         "gpu": [{"vendor":"nvidia", "vram_mb":4096}],
         "mem": {"total_mb": 2048} }
```

### MVP existente (já implementado)

O struct `HardwareInfo` serve como **cache de boot** — existe porque o SGDB ainda não está pronto quando o dispatch SIMD é necessário:

```rust
// k_nano::platform_probe — cache early-boot APENAS
pub fn hw_info() -> &'static HardwareInfo

pub struct HardwareInfo {
    pub hv: HypervisorKind,       // baremetal / WHPX / KVM / TCG
    pub cpu: CpuFeatures,         // avx2, avx512, fma, sse42
    pub cache: CacheTopology,     // L1/L2/L3 sizes, line size
    pub isa: IsaPath,             // Scalar | Sse42 | Avx2Fma | Avx512F
    pub storage_bytes: u64,       // reservado
    pub cpu_mhz: u32,             // reservado
}
```

**Este struct NÃO é expandido.** Novas capacidades de HW vão direto para o SGDB.

### Regras

1. **SGDB é a fonte de verdade.** Após Fase 6, qualquer consulta de HW lê do SGDB. O struct `HardwareInfo` é mantido por compatibilidade (early-boot) mas congelado.
2. **Namespace `/hw/`** é hierárquico e extensível sem recompilação. Qualquer probe escreve `put_kv("hw/<categoria>/<id>/<campo>", <valor>)`.
3. **WASM agents recebem snapshot plano** — uma única consulta ao SGDB no init, viram tabela local. Sem chamar FFI.
4. **Nada hardcoded.** Para adicionar um novo atributo de HW, basta o probe escrever no SGDB. Nenhum struct Rust precisa mudar.

### Como usar (exemplos)

**Dispatch SIMD (early-boot, via cache):**
```rust
let hw = k_nano::platform_probe::hw_info();
if hw.avx2_ready() { … }  // usa cache HW_INFO
```

**Agente runtime (via SGDB):**
```rust
if sgdb::get_kv("hw/cpu/avx2") == Some("true".into()) { … }
```

**Probe de GPU escreve no SGDB:**
```rust
sgdb::put_kv("hw/gpu/0/vendor", "nvidia");
sgdb::put_kv("hw/gpu/0/vram_mb", "4096");
sgdb::put_kv("hw/gpu/0/compute", "true");
```

**WASM agent init:**
```rust
// Recebe Map<String,String> com prefixo /hw/* ao instanciar
let avx2 = snapshot.get("cpu/avx2");  // Some("true")
```

## Consequências

### Positivas

- **Zero recompilação** para novo HW — qualquer probe escreve no SGDB
- **WASM agents auto-adaptáveis** sem chamar o kernel
- **Namespace hierárquico** descobrível via `scan_prefix`
- **Cache early-boot** é mínimo e congelado — não infla com cada novo hardware
- **Backward compat:** `hw_info()` continua funcionando (lê cache), depois pode ser alterado para ler do SGDB

### Negativas

- **Two-phase init:** dispatch SIMD usa cache; runtime usa SGDB. Pequena duplicação lógica até a migração completa.
- **Chaves string** em vez de campos tipados — perde type safety da Rust. Mitigação: wrappers `hw_cpu_avx2() -> bool` em `k_nano::platform_probe` que fazem `get_kv` internamente (futuro).

### Riscos

- **SGDB não pronto** no early-boot → se algo consulta HW via SGDB antes da Fase 6, recebe vazio. Solução: `hw_info()` do cache cobre early-boot. Após Fase 6, `hw_info()` pode ser deprecado.
- **Chaves inconsistentes** entre probes → definir naming convention: `hw/<categoria>/<id>/<propriedade>`, tudo lowercase, valores sempre string.

## Implementation CheckList

### Cache early-boot (struct HardwareInfo)
- [x] MVP: `HardwareInfo` struct + `hw_info()` em `platform_probe.rs`
- [x] Conexão SGDB: `hamming_dispatch.rs` usa `hw_info().avx2_ready()`
- [x] Conexão SGDB: `art.rs` usa `hw_info().avx2_ready()`
- [ ] Congelar struct: nenhum campo novo após esta ADR ser aceita

### População SGDB no boot
- [x] `boot_init()` do SGDB (store.rs) escreve /hw/cpu/* após detect() (Onda CPU — SESSION 2026-08-05)
- [ ] GPU probe escreve /hw/gpu/* após init
- [ ] StorageBus escreve /hw/storage/* após probe
- [ ] NetAgent escreve /hw/net/* após init
- [ ] WifiAgent escreve /hw/wifi/* após init
- [ ] HDA driver escreve /hw/audio/* após init
- [ ] NPU probe escreve /hw/npu/* após init

### Consumidores
- [ ] Migrar `cortex::simd_dispatch` para `hw_info()` (cache) → futuro SGDB
- [ ] Migrar `hermes::scheduling` para `hw_info()` → futuro SGDB
- [ ] WASM agent init: snapshot /hw/ do SGDB
- [ ] Opcional: wrappers tipados `hw_cpu_avx2()`, `hw_gpu_count()` etc.

## Mapa completo de fornecedores e consumidores

Levantamento feito por varredura automatizada em todo o workspace (`crates/` + `neural-kernel/`).
Inclui fontes estáticas (boot) e dinâmicas (runtime: USB hotplug, WiFi scan, link state, frequência).

### Fornecedores — produzem dados de HW

| Fornecedor | Dados produzidos | Struct / global | Localização | Fase |
|------------|-----------------|----------------|-------------|------|
| `platform_probe::detect()` | CPU features (avx2, avx512, sse42, fma...), hypervisor, cache, ISA path | `HardwareInfo`, `FeatureGate`, `CpuFeatures`, `CacheTopology` | `k_nano::platform_probe` | 2 |
| `detect_cpu_features()` | CPUID leaves 1, 7, 0x1A | `CpuFeatures` | `k_nano::platform_probe:239` | 2 |
| `detect_hypervisor()` | Hypervisor Kind (WHPX/KVM/TCG/VBox...) | `HV_KIND` atomic | `k_nano::platform_probe:192` | 2 |
| `build_gate()` | Política ISA ∩ sandbox (allow_avx2, allow_smp, max_aps) | `GATE_BITS` atomic → `FeatureGate` | `k_nano::platform_probe:416` | 2 |
| `detect_cache_topology()` | L1/L2/L3 line size, ways | `CACHE_TOPO` / `CacheTopology` | `k_nano::platform_probe:374` | 2 |
| `enable_simd()` | ISA efetivamente ativada (CR0/CR4/XCR0) | (efeito colateral em CR4, XCR0) | `k_nano::simd` | 2 |
| **`cpu_features()`** | Struct completa de features CPUID | `CPU_FEATURES` (static mut) | `k_nano::platform_probe:567` | 2+ |
| **`hw_info()`** | Registro consolidado CPU + ISA + cache | `HW_INFO` (static mut) | `k_nano::platform_probe:619` | 2+ |

| `scan_pci()` | PCI devices: bus/dev/fn, vendor_id, device_id, class, BARs | `PciDevice` array | `k_nano::pci:167` | 4 |
| `init_acpi()` | RSDP, FADT, MADT, DSDT, SRAT, S3/S5 suporte | `AcpiInfo`, IOAPIC, LAPIC data | `k_nano::acpi:533` | 4 |
| `init_apic()` | LAPIC/IOAPIC base, timer freq, IRQ routing | LAPIC MMIO, IOAPIC RTE | `k_nano::apic:435` | 4 |
| Timer calibration | CPUID 0x15/0x16 TSC, PIT fallback | `TIMER_FREQ`, `cpu_mhz` | `k_nano::interrupts:613+` | 4 |
| `cpufreq::probe_and_init()` | P-states, governor, energy_perf_bias | MSRs + cpufreq state | `k_nano::cpufreq` | 4+ |
| `hardware/topology.rs` | P/E cores, CCX, chiplet, DRAM freq (LPDDR5/DDR5) | topologia NUMA | `k_nano::hardware::topology` | 4 |
| `hardware/epyc.rs` | AMD EPYC: CCD, CCX, SRAT até 24 nós | topologia EPYC | `k_nano::hardware::epyc` | 4 |
| `hardware/xeon.rs` | Intel Xeon: SRAT, UPI, cache hierarchy | topologia Xeon | `k_nano::hardware::xeon` | 4 |
| `init_global_allocator()` | RAM total (usable ranges), page frames | `TOTAL_RAM_MB`, `BitmapFrameAllocator` | `k_nano::memory:312` | 2 |
| Heap init (+TALC) | Heap size, bump allocator | `HEAP_SIZE=512MB`, `HEAP_LIMIT` | `k_nano::allocator` | 2 |
| `NumaAllocator` | NUMA-aware alloc de SRAT | `NumaTopologyMap` | `k_nano::numa_alloc` | 4+ |
| `hw_profiler::profile_hardware()` | Perfil consolidado: RAM, GPU, WiFi, AVX2 | `HwProfile` | `k_nano::hw_profiler:45` | 6 |
| `k_hal::inventory` / `DeviceTree` | DeviceCap vec (GPU, Net, Block, Snd...) | `device_tree()` | `k_hal::discovery:76` | 5 |
| `k_hal::gpu::detect_all()` | GPU arch (NVIDIA/Intel/AMD), VRAM, BARs | `GpuDeviceInfo`, `VRAM_BUDDY` | `k_hal::gpu::detect:127` | 5+ |
| `k_hal::gpu::backend::init_with_plan()` | Compute gate, backend state | `BACKEND` global | `k_hal::gpu::backend:282` | 5+ |
| `k_hal::npu::init_npu()` | AMD XDNA / Intel NPU PCI detection | NPU verdict | `k_hal::npu` | 5+ |
| `k_hal::audio::hda::probe()` | HDA PCI detect, codec, streams | HDA BAR + SD0/SD1 | `k_hal::audio::hda` | 5+ |
| `k_hal::net::ath10k_*` | ath10k PCI, WMI, scan, assoc | WiFi driver state | `k_hal::net::ath10k_*` | 5+ |
| `k_hal::net::generic_wifi` | PCI scan for Intel WiFi | — | `k_hal::net::generic_wifi:407` | 5+ |
| `k_hal::hw_gate::emit_all()` | HW-GATE tokens: AWAITING_HW status | `GATES: [GateEntry]` | `k_hal::hw_gate:107` | 0+ |

| e1000 probe | MAC, link UP/DOWN, MMIO, DMA descs | e1000 registers | `k_nano::e1000` | 5 |
| i225 probe | MAC, NVM, BAR | i225 registers | `k_nano::i225` | 5 |
| RTL8139 probe | I/O base | RTL8139 ports | `k_nano::rtl8139` | 5 |
| ATA PIO probe | Master/slave, sectors, LBA, MBR/GPT | `ATA_DRIVER` | `k_nano::ata:12` | 5 |
| AHCI probe | HBA present/absent, port count | — | `k_nano::ahci` | 5 |
| NVMe probe | CAP (MQES, DSTRD), BAR, queues | `NVME_DRIVER` | `k_nano::nvme:60` | 5+ |
| xHCI probe | Ports, hub status, devices | `XHCI_STATE` | `k_nano::xhci` | 5 |
| USB MSC | Mass storage class device | — | `k_nano::usb_msc` | 5+ |
| xHCI runtime | Hotplug port status (event TRB) | `hub_ok()`, `hub_child_ok()` | `k_nano::xhci` | Runtime |
| TPM probe | TPM2 table ACPI, MMIO, PCR | `TPM_PRESENT` | `k_nano::tpm:216` | 2+ |
| `BootReport` | Storage, USB, boot_log status | `BootReport` struct | `k_nano::boot_report` | 6 |
| `CapToken` grants | BootSmokeOk, GpuCompute, Wifi | `UnlockDAG` | `k_hal::UnlockDAG` | 0+ |
| Framebuffer probe | Res, bpp, stride, phys addr | `GpuDevice` in `jarbas::fb` | `jarbas::display::fb` | 0 |
| Limine boot handoff | HHDM, memmap, RSDP, framebuffer | `LimineBootInfo` | `k_nano::limine` | 0 |

| **HW Expert v3** | Inferência PCI/USB: vendor_name, device_name, class, arch | `k_ai::hw_expert` | Modelo ML (~1MB, 44K devices) | 5+ |
| **HW Expert v4 (dataset)** | WDM + SDIO + PCI.IDS + USB.IDS unificado | `models/hw_expert/v4/dataset.json` | **60K amostras, zero dedup** | 5+ |
| **SelfHeal agent** | FW blobs ausentes/presentes por device | — | `k_ai::self_heal` | Runtime |
| **Firmware probe** | Blobs FAT: FW_FECS_BL_BIN, FW_I915_*... | — | `k_nano::firmware` | 5 |
| **StorageBus** | Block devices registrados (tipo, setores) | `StorageBus` | `k_nano::storage` | 6 |

### Consumidores — usam dados de HW

| Consumidor | O que consulta | Onde | Decisão |
|-----------|---------------|------|---------|
| **SGDB** `hamming_dispatch` | `hw_info().isa` ou `allow_avx2()` | `k_ai::sgdb::hamming_dispatch:19` | Kernel Hamming: AVX2 / AVX-512 / scalar |
| **SGDB** `art::find_child_byte16` | `hw_info().avx2_ready()` | `k_ai::sgdb::art:60` | SSE2 lookup em Node16 |
| **Cortex** tensor SIMD | `allow_avx2()` | `cortex::tensor:6` | Tensor ops SIMD vs scalar |
| **Cortex** bitnet matmul | `allow_avx2()`, `allow_avx512()` | `cortex::bitnet_sse:28` | Kernel matmul: Sse42 / Avx2 / Avx512 |
| **Cortex** parallel_matmul | `allow_smp()`, cache topology | `cortex::parallel_matmul:57,174` | Tiling + multi-AP dispatch |
| **Cortex** compute | `allow_smp()`, `allow_avx512()` | `cortex::compute:91` | Dispatcher CPU/SMP/GPU |
| **Cortex** model_fit | `total_ram_mb`, `total_vram_mb` | `cortex::model_fit:115` | Orçamento modelo + disco |
| **Cortex** install_adviser | `HwProfile` (RAM, GPU, AVX2) | `cortex::install_adviser:114` | Modelo que cabe no HW |
| **Cortex** LLM decode | RAM/VRAM disponível | `cortex::llm` | Model slot mapping |

| **Hermes** scheduler | SMP count, CorePools | `hermes::scheduler` | Task distribution |
| **Hermes** net init | NIC type (e1000/i210), MAC, link | `hermes::net:120` | Init network stack |
| **Hermes** agents | ACPI, PCI scan | `hermes::agents:1617` | Agent inventory |
| **Hermes** shell | `hw_profiler`, `scan_pci()` | `hermes::shell:104,165` | Shell commands |
| **Hermes** /dev/pci | `scan_pci()` | `hermes::fs::dev_fs_agent:17` | VFS device listing |

| **Jarbas** display | FB res, bpp, stride, phys | `jarbas::display::fb:50` | Config framebuffer + DoubleBuffer |
| **Jarbas** avatar | `fb_stride`, `fb_bpp` | `jarbas::display::avatar:81` | Render |
| **Jarbas** gauges | `vram_usage()` | `jarbas::display::gauges:85` | HUD VRAM gauge |
| **Jarbas** audio jarvis | `hypervisor()` | `jarbas::audio::jarvis:140` | Template vs LLM TTS |
| **Jarbas** persona | GPU vendor/model | `jarbas::persona` | Capability badge |

| **k_nano SMP** | `allow_smp()`, `hypervisor()`, `max_aps()` | `k_nano::smp` | AP bringup, CorePools |
| **k_nano CorePools** | `gate().allow_ep_core_detect` | `k_nano::smp::corepools:120` | P/E core pools |
| **k_nano AP work** | `has_mwait()` | `k_nano::smp::ap_work:126` | Idle governor |
| **k_nano SIMD enable** | `allow_avx2()`, `cpu_features()`, `hypervisor()` | `k_nano::simd:9` | CR0/CR4/XCR0 config |
| **k_nano allocator** | `PHYS_MEM_OFFSET` | `k_nano::allocator:103` | Heap page mapping |
| **k_nano memory** | bootloader usable ranges | `k_nano::memory:141` | TOTAL_RAM_MB |
| **k_nano PCI drivers** | `PHYS_MEM_OFFSET` para MMIO | e1000, ahci, nvme, xhci, dma... | DMA VA mapping |
| **k_nano boot_logger** | `ATA_DRIVER` / `NVME_DRIVER` | `k_nano::boot_logger:261` | BOOT.LOG persist |
| **k_nano flash** | `NVME_DRIVER` presence | `k_nano::flash:165` | TickvLite backend (nvme vs ram) |
| **k_nano sys_installer** | `ATA_DRIVER`, `StorageBus` | `k_nano::sys_installer:62` | Target disk |
| **k_nano TPM** | ACPI TPM2 table, MMIO | `k_nano::tpm` | PCR extend |

| **k_hal GPU backend** | GPU presence, compute cand | `k_hal::gpu::backend:370` | AWAITING / PASS |
| **k_hal NPU** | PCI NPU detection | `k_hal::npu:36` | VPU verdict |
| **k_hal Wi-Fi softmac** | e1000 presence → skip softmac | `k_hal::wifi_softmac:109` | Wi-Fi gate |
| **k_hal HW-GATES** | xHCI hub, CapToken | `k_hal::hw_gate:65` | Gate status |
| **k_hal DeviceCap** | DeviceClass | `k_hal::offer:87` | HalOffer topic |

| **neural-kernel** main.rs:1423 | `platform_probe::detect()` | Boot phase | FeatureGate |
| **neural-kernel** main.rs:1504 | `isa_path()`, `allow_avx2()` | Boot log | SIMD info |
| **neural-kernel** main.rs:1652 | xHCI, USB MSC, HID | Boot phase | Storage + input |
| **neural-kernel** main.rs:1862 | `sgdb::boot_init()` | Boot phase | SGDB + hamming |
| **neural-kernel** main.rs:2242 | GPU `detect_all()`, VRAM, FW | Boot phase | Compute plan |
| **neural-kernel** memory_agent | `TOTAL_RAM_MB`, `VRAM_BUDDY` | Agent | MemoryBudget |
| **neural-kernel** net | `scan_pci()`, `hypervisor()` | Net init | NIC select |
| **neural-kernel** shutdown | ACPI S5, PM1a | Power off | SLP_TYPa |
| **neural-kernel** smp init | `allow_smp()`, `max_aps()` | SMP init | AP count |
| **neural-kernel** jarbas_fb | `PHYS_MEM_OFFSET` | FB map | UC framebuffer |
| **neural-kernel** tls_trust | sgdb KV | TLS | Pins |
| **neural-kernel** exec_arena | `PHYS_MEM_OFFSET` | JIT | W^X mapping |
| **neural-kernel** elf_loader | `PHYS_MEM_OFFSET` | ELF | HHDM |

### Fornecedores dinâmicos (runtime)

| Fornecedor | Dados | Gatilho |
|-----------|-------|---------|
| **USB hotplug** | xHCI port status change, hub child connect/disconnect | Event TRB → `hub_ok()` |
| **WiFi scan** | SSIDs visíveis, RSSI | WMI scan callback |
| **WiFi link** | `WifiState::Connected` | Associação + DHCP |
| **Ethernet link** | e1000 STATUS register link UP/DOWN | IRQ / poll |
| **CPU frequency** | P-state atual, governor | `cpufreq::ondemand_tick()` |
| **TPM PCR** | SHA256 extend | `tpm_extend_pcr()` |
| **Timer ticks** | `TIMER_TICKS` | IRQ 32 (LAPIC timer) |
| **CapToken grants** | `GpuCompute`, `WifiAssociated` | UnlockDAG grant |
| **SelfHeal** | FW download status | Agent tick |
| **SGDB ready** | TICKV mounted | `sgdb::ready()` |
| **Heap resize** | Heap mapping extendido | `resize_bump_heap()` |

### Lacunas identificadas

O scan revelou campos do `HardwareInfo` que **nunca são populados**:

| Campo | Status | O que deveria preencher |
|-------|--------|------------------------|
| `cpu_mhz` | **sempre 0** | `cpufreq` detecta mas não escreve de volta |
| `storage_bytes` | **sempre 0** | `StorageBus` ou `ATA_DRIVER.sectors` |
| `gpu_vram_mb` | **hardcoded 2048** em `hw_profiler.rs` | BAR2 size decode real |
| Bateria | **não detectado** | ACPI battery / EC |
| Térmica | **não detectado** | DTS MSR / ACPI thermal zone |
| Bluetooth | **só enum DeviceClass** | HCI UART/USB scan |
| Câmera/UVC | **só enum DeviceClass** | USB Video Class driver |
| USB class descriptors | **parcial** | xHCI enumera portas mas não lê descriptores por classe |
| NVMe SMART | **não lido** | Admin cmd Identify + Log |
| DRAM frequency | **detectado** em `hardware/topology.rs` mas não propagado | `HardwareInfo` |
| Power states S3/S5 | **parseado** do DSDT mas não exposto | `HardwareInfo` |

### Fluxo de migração para SGDB

```
Fornecedor → (hoje) → struct/variável local → consumidor consulta direto
Fornecedor → (futuro) → put_kv("hw/...") no SGDB → consumidor faz get_kv("hw/...")

static HW_INFO (cache early-boot) cobre apenas os fornecedores da Fase 2-4
que são necessários antes do SGDB existir. Depois da Fase 6, tudo passa pelo SGDB.
```

## Ring Isolation — Por Que o SGDB é a Única Via

Consultar HW Expert ou `k_nano::platform_probe` diretamente de agentes em rings superiores **viola isolamento de anéis** (ADR-0041). A arquitetura em 2 camadas respeita os rings:

```
Ring 2 (cortex/k_ai)
  └── HW Expert v4 inference (boot Fase 5)
       └── put_kv("hw/pci/0/class", "network")
       └── put_kv("hw/pci/0/vendor_name", "Intel")
       └── put_kv("hw/gpu/0/compute", "true")

Ring 2 (k_nano, probes)
  └── platform_probe::detect() → put_kv("hw/cpu/avx2", "true")
  └── PCI scan → put_kv("hw/pci/0/vid", "0x8086")
  └── GPU detect → put_kv("hw/gpu/0/vram_mb", "4096")

Ring 3 (hermes, jarbas, agentes)
  └── get_kv("hw/pci/0/vendor_name")  → ✅ sem cruzar ring
  └── scan_prefix("hw/gpu/")          → ✅ sem cruzar ring
  └── NUNCA chama HW Expert diretamente  → ❌ violaria CapGate

Ring 3 (WASM agents)
  └── recebe snapshot /hw/* no init    → ✅ zero host imports
  └── consulta mapa local              → ✅ sem FFI
```

**Por quê isso é importante:**

1. **CapGate (ADR-0041 §P3):** chamadas entre rings exigem capability tokens. `hw_expert::identify()` em Ring 3 exigiria `Cap::HwIdentify` — que nenhum agente Ring 3 tem.
2. **WASM sandbox (ADR-0059):** skills WASM só têm acesso a host imports explicitamente listados (`aios::*`). Adicionar `aios_hw_identify` seria um vetor de ataque — um skill malicioso enumeraria todo o HW.
3. **Performance:** inferência ML em todo `get` seria inviável. O SGDB faz lookup O(1) em RAM.
4. **Atomicidade:** o snapshot de HW é consistente — todo agente vê a mesma foto do boot. Se houver hotplug, uma nova versão do snapshot é gerada, não dados parciais.

### Como o HW Expert v4 alimenta o SGDB sem violar rings

**Momento único:** Fase 5 (DriverInit), depois do PCI scan, Antes do AgentFleet:

```
Para cada device PCI encontrado:
  1. HW Expert v4 recebe (vid, did) → saída estruturada:
       (family_id, fw_id, agent_id, caps_bits, next_action)
  2. Traduz para chaves /hw/pci/<idx>/ e escreve no SGDB
  3. DeviceTree (k_hal) também lê do mesmo resultado para classificação

NENHUM agente em runtime chama HW Expert. A inferência é pré-computada.
```

Se um device não for coberto pelo HW Expert (device raro/não treinado), cai pra `heuristic_card()` — que também escreve no SGDB. A diferença é que o HW Expert pode classificar devices que a heurística nunca viu (ex: "1234:5678" vira "GPU Nvidia" em vez de "Unknown").

### E se um device for hotplugado depois do boot?

```
USB hotplug (runtime):
  1. xHCI event TRB → hub_ok() detecta novo device
  2. Ring 2 (k_nano): USB class descriptor → (vid, did)
  3. Ring 2 (cortex/k_ai): HW Expert v4 identifica
  4. put_kv("hw/usb/1/vendor", "SanDisk")
  5. EventBus publica HW_CHANGE → agentes reconsultam SGDB

O Ring 3 nunca chamou HW Expert. A cadeia é segura.
```

### Resumo

| Query | Feita por | Ring | Como |
|-------|-----------|------|------|
| `get_kv("hw/pci/0/class")` | Hermes NetAgent | 3 | SGDB KV |
| `get_kv("hw/cpu/avx2")` | SGDB hamming_dispatch | 2 | cache early-boot (depois SGDB) |
| `scan_prefix("hw/gpu/")` | MemoryAgent | 3 | SGDB scan |
| `hw_expert::identify(vid,did)` | **ninguém em runtime** | 2 | só no boot Fase 5 |
| snapshot `/hw/*` | WASM agent | 3 | init do sandbox |

```

## Anexo A — Pesquisa de Mercado: HW Identification em OS

Levantamento feito em Julho/2026 via web search. O objetivo era encontrar projetos, papers ou produtos que usam **ML para identificar hardware no boot de um OS** — e determinar se o HW Expert do Neural-OS é pioneiro.

### Estado da arte (concorrentes)

| Abordagem | Quem usa | Formato | Tamanho | Adaptativo? | ML? |
|-----------|---------|---------|---------|-------------|-----|
| **pci.ids / usb.ids** | Linux, BSD | texto (VID→nome) | 1.6MB | ❌ (arquivo estático) | ❌ |
| **pci_device_id[] C structs** | Linux kernel | C array hardcoded | ~500KB no .ko | ❌ (recompilar) | ❌ |
| **Windows .inf Driver Store** | Windows | catálogo .inf gigante | GBs | ❌ (precisa download) | ❌ |
| **Hardware Inventory (DMF)** | Windows | WMI query | runtime | ❌ (só reporta) | ❌ |
| **HW Expert v3 (nosso)** | **Neural-OS** | **BitNet ternário** | **1MB** | **✅ runtime** | **✅ BitNet** |

**Conclusão: Ninguém usa ML para identificar hardware no boot.** Somos os primeiros.

### Tecnologias que validam nossa direção

| Projeto | Tecnologia | Tamanho | Relevância |
|---------|-----------|---------|------------|
| **BitNetMCU** (cpldcpu) | BitNet ternário em MCU $0.15, 2KB RAM | 12KB modelo | ✅ BitNet ternário funciona até em hardware mínimo |
| **Microsoft BitNet** | Inference framework 1-bit LLM, CPU x86/ARM | 2.4B params | ✅ LUT PSHUFB pode acelerar HW Expert |
| **BNRV** (HKUSTGZ) | BitNet em RISC-V, instruções custom | 3M params | ✅ Aceleração HW para BitNet |
| **bitone** (Huntwter) | BitNet NPU128 bare-metal, DMA double-buffer | ~600MB (3B) | ✅ Kernel bare-metal = mesma abordagem |
| **KWT-Tiny** | Transformer em 64KB RAM, bare-metal C | 1.65KB | ✅ TinyML em bare-metal é viável |
| **TinyML survey** (arXiv 2403.19076) | Co-design algoritmo-sistema para MCU | — | ✅ Tendência consolidada |

### O que podemos absorver

| Técnica | Fonte | Aplicação no HW Expert v4 |
|---------|-------|---------------------------|
| **LUT PSHUFB para unpacking** | Microsoft BitNet, bitone | Acelerar inference 2-4× em CPU x86 |
| **ShiftNorm em vez de RMSNorm** | BitNetMCU (cpldcpu) | Zero multiplicação na inferência — ideal para soft-float |
| **Quantização 2-bit assimétrica** | BitNetMCU, bitone qat.py | Comprimir v4 de 1MB para ~256KB |
| **Pipeline QAT com Triton** | bitone (Huntwter) | Treinar v4 multi-head com Quantization-Aware Training |
| **Ping-pong DMA double-buffer** | bitone NPU128 | Se um dia migrar inference para GPU/NPU |

### Posicionamento competitivo

```
                    ML-based?
                    ^
                    |
        Neural-OS   |   ✦ HW Expert v4
        HW Expert   |   (BitNet ternário, 1MB,
        v3          |    171K HWIDs, structured output)
        (1MB,       |
        64 classes) |
                    |
                    +-----------------------------→ Model size
                        pci.ids    Linux kernel PCI
                        (1.6MB)    (hardcoded C structs)
```

HW Expert ocupa um nicho **inexistente no mercado**: identificação de hardware via ML em bare-metal OS. O concorrente mais próximo é o `pci.ids` do Linux — que é texto estático, não aprende, não infere, não classifica devices desconhecidos.

### Referências da pesquisa

- BitNetMCU: https://github.com/cpldcpu/BitNetMCU — BitNet ternário em MCU $0.15
- Microsoft BitNet: https://github.com/microsoft/BitNet — Inference 1-bit LLM CPU/GPU
- BNRV: https://github.com/HKUSTGZ-MICS-LYU/BNRV — BitNet em RISC-V
- bitone: https://github.com/Huntwter/bitone — BitNet NPU128 bare-metal
- KWT-Tiny: arXiv 2407.16026 — Transformer em 64KB RAM
- TinyML survey: arXiv 2403.19076 — Tiny Machine Learning: Progress and Futures
- pci.ids: https://pci-ids.ucw.cz — PCI ID database
- usb.ids: http://www.linux-usb.org/usb.ids — USB ID database

## Referências

- ADR-0055: FeatureGate + SIMD ISA dispatch (origem do `IsaPath`)
- ADR-0063: SGDB Hamming dispatch (primeiro consumidor do `hw_info()`)
- ADR-0059: App Factory WASM (futuro consumidor do snapshot SGDB)
- ADR-0063 store.rs: `put_kv`/`get_kv` API do SGDB
- `platform_probe.rs`: cache early-boot (~690 LOC)
- **Anexo A** desta ADR: pesquisa de mercado HW identification
