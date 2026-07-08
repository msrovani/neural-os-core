# 🧠 Idea Bank — neural-os-core

**Última atualização:** 2026-07-08 — ~410 ideias catalogadas (#1-#408 + letras). Consolidado após verificação cruzada com TECNOLOGIAS.md e TODO.md.
**Documento vivo:** Toda ideia discutida neste projeto tem destino conhecido.

---

## Premissa Básica

> **Toda ideia, conceito, decisão ou sugestão já discutida neste projeto — entre qualquer dev e a IDA IA — DEVE ter um destino conhecido e documentado neste arquivo.**

Nada é descartado sem registro. Ideias podem ser:
- ✅ **Implementada** — já está no código
- 🟡 **Agendada** — sprint/bloco definido
- ⏳ **Pós-MVP** — adiada com dependências documentadas (ver Seção 3)
- 💰 **Sponsor** — requer hardware/parceria/financiamento
- ❌ **Descartada** — com justificativa explícita
- 🔄 **Fundida** — absorvida por item maior

**Por que esta premissa existe:** Estamos inovando em caminhos pouco ou não trilhados (bare-metal neural OS, Memory Hierarchy Index, intent routing em Ring 0). Muitas ideias não são implementáveis hoje — seja por limitação tecnológica, falta de hardware, ou prioridade. Mas amanhã um dev pode saber como fazer, a tecnologia pode melhorar, ou podem surgir patrocinadores. Se a ideia não estiver registrada, ela morre.

**Como usar este documento:**
- **Consultar:** antes de tomar decisão arquitetural, verifique se a ideia já existe e qual seu status
- **Atualizar:** quando uma ideia muda de status, edite este arquivo (não a ADR-0015)
- **Adicionar:** toda nova ideia discutida deve ganhar uma linha aqui na seção apropriada
- **Origem:** o seed inicial veio da ADR-0014 (ideias de hardware) e da ADR-0015 (curso correção MVP). Novas ideias entram diretamente aqui.

---

## Legenda dos Status

| Marca | Significado | Exemplo |
|---|---|---|
| ✅ Block N | Implementado no bloco N da chain MVP | ✅ Block 2 |
| 🟡 Sprint N | Agendado para sprint específica | 🟡 Sprint 19 |
| 🔴 Bloqueado | Bloqueado por dependência externa (ex: B-01) | 🔴 B-01 |
| ⏳ Pós-MVP | Adiado, ver Seção 3 para dependências | ⏳ Pós-MVP |
| 💰 Sponsor | Requer hardware/parceria | 💰 Sponsor |
| ❌ Descartado | Não será feito, com motivo | ❌ Descartado |
| 🔄 Fundido | Absorvido por item maior | 🔄 Fundido |

---

## Seção 1 — Master Registry (Inventário Completo)

### 1.1. The Agency — HW Agents + User Agents (IDEA #277)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 277a | HwRegistry: cada PCI/USB vira HwAgent com capabilities | 🟡 Sprint 92+ | v0.92+ | LLM pergunta "o que tem de HW" → ativa agentes |
| 277b | Agency: 12 divisões, 30+ agentes especializados | 🟡 Sprint 92+ | v0.92+ | Port do The Agency (50K★) para nosso ecossistema |
| 277c | LLM-aware hardware activation por intent | 🟡 Sprint 92+ | v0.92+ | "quero video chamada" → mic+camera+display+net |

### 1.2. GGUF Format Support (IDEA #278)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 278a | Loader GGUF mínimo para kernels no_std (~500 LOC) | 🟡 Sprint 92+ | v0.92+ | Modelos maiores (9B+ via GGUF Q4) |
| 278b | .bitnet v3: header extensível com metadata | 🟡 Sprint 92+ | v0.92+ | Alternativa mais leve que GGUF |

### 1.3. SmileyOS Patterns (IDEA #279)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 279a | Shell com 40+ comandos (ls, cat, ps, uptime, theme) | 🟡 Sprint 91 | v0.91+ | Port da UX do SmileyOS (~90K LOC Rust) |
| 279b | Sistema de temas (5+ cores, hot-swap) | 🟡 Sprint 91 | v0.91+ | theme list + theme <name> |
| 279c | Filesystem proprio com permissoes | 🟡 Sprint 91 | v0.91+ | Substituir FAT12 mínimo |
| 279d | Compositor multi-window (dock, menus, drag) | 🟡 Sprint 92+ | v0.92+ | DisplayAgent atual é single-tela |
| 279e | v86 browser demo (WebAssembly x86 emulator) | 🟡 Sprint 92+ | v0.92+ | Bootar no navegador |
| 279f | App SDK via trait + registry (JA TEMOS!) | ✅ Confirmado | — | Nosso Agent trait + AgentRegistry validado |

### 1.4. Ecosystem Batch 3 — 12 Repos Portados (IDEA #280)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 280a | redox-os/redox: SchemeHandler trait (scheme.rs) | ✅ v0.59.2 | v0.59.2 | Namespace I/O: gpu://, usb:// |
| 280b | theseus-os/Theseus: TypedAgent<Boot\|Running\|Faulted> (state.rs) | ✅ v0.59.2 | v0.59.2 | Type-safe lifecycle transitions |
| 280c | embassy-rs/embassy: TimerWheel 64-slot (timer_wheel.rs) | ✅ v0.59.2 | v0.59.2 | Agendamento eficiente de wake-ups |
| 280d | openai/swarm: Handoff enum (SwitchTo/Escalate/Delegate) | ✅ v0.59.2 | v0.59.2 | Agent handoff protocol |
| 280e | tock/tock: Register<T> + RegisterField (mmio.rs) | ✅ v0.59.2 | v0.59.2 | Typed MMIO sem unsafe manual |
| 280f | raga-ai-hub/Catalyst: Span tracer 256-entry (tracer.rs) | ✅ v0.59.2 | v0.59.2 | Tracing de spans de agentes |
| 280g | kyegomez/swarms: Orchestrator decompose+assign | ✅ v0.59.2 | v0.59.2 | Task decomposition por keywords |
| 280h | TransformerOptimus/SuperAGI: SkillScore scoring | ✅ v0.59.2 | v0.59.2 | Skill performance ranking |
| 280i | VRSEN/agency-swarm: SpecialistAgent (ja tinhamos!) | ✅ Confirmado | v0.59.1 | 147 agentes em 12 divisões |
| 280j | browser-use: HwRegistry device tree (ja tinhamos!) | ✅ Confirmado | v0.59.1 | HW context para LLM |
| 280k | micro/go-micro: endpoints discovery (ja tinhamos!) | ✅ Confirmado | v0.55.0 | AgentManifest extensivel |
| 280l | pydantic-ai: SkillManifest derive macro (conceitual) | 🟡 Sprint 91 | v0.91+ | Proc-macro para manifests |

### 1.6. VFS + MHI Bridge (IDEA #281)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 281a | VfsRegistry: mount table + resolve + lookup | ✅ v0.62.0 | v0.62.0 | Foundation para todo FS |
| 281b | VfsNode: arvore de diretorios com FileMode | ✅ v0.62.0 | v0.62.0 | Mount points + virtual files |
| 281c | Path utils: canonicalize, split, join, parent | ✅ v0.62.0 | v0.62.0 | Processamento de paths |
| 281d | MHI ARC-style suggest_tier (ZFS-inspired) | ✅ v0.62.0 | v0.62.0 | MFU→Dram, MRU→Nvme, cold→Hdd |

### 1.7. Storage Agents (IDEA #282)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 282a | FilesystemAgent trait + VFS bridge | ✅ v0.62.1 | v0.62.1 | Interface padrao FS agents |
| 282b | AtaAgent: /mnt/hdd/ + block R/W | ✅ v0.62.1 | v0.62.1 | ATA via DriverAgent |
| 282c | DevFsAgent: /dev/pci/ + NIC + USB + mem | ✅ v0.62.1 | v0.62.1 | Hardware como arquivos |
| 282d | ProcFsAgent: /proc/agent/mem/uptime/cpu | ✅ v0.62.1 | v0.62.1 | Sistema como arquivos |
| 282e | InferenceFsAgent: /inference/ com LLM | 🟡 Sprint 92+ | v0.92+ | LLM gera arquivos |
| 282f | HermesFsAgent: /chat/send + /chat/history | 🟡 Sprint 92+ | v0.92+ | Chat como FS |
| 282g | RamFsAgent: /mnt/ram/ cache DRAM | 🟡 Sprint 92+ | v0.92+ | Cache tiers inferiores |
| 282h | Auto tier migration via MhiScheduler | 🟡 Sprint 92+ | v0.92+ | Promove/demove por acesso |

### 1.8. Desktop Cube (IDEA #283)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 283a | Workspace Cube 3D com rotação via GPU (VirtIO-GPU) | 🟡 Sprint 91 | v0.91+ | 3 workspaces (main/dev/chat) como faces de cubo giratório |
| 283b | Transição crossfade entre workspaces (fallback sem GPU) | 🟡 Sprint 91 | v0.91+ | Efeito moderno sem 3D, ~100 LOC |

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 1 | xHCI controller mínimo (<500 LOC, BAR0, port status) | ⏳ Pós-MVP | Sprint 23+ | MVP usa PS/2 legacy. xHCI requer PCI (Block 1) + driver USB (~500 LOC). |
| 2 | `identify_device()` → VID/PID/class | ⏳ Pós-MVP | Sprint 23+ | Bloqueado pelo xHCI driver. |
| 3 | Neural Cortex classify (MLP 7→5: allow/deny/learn/no_intent/suspect) | ✅ Sprint 25 | Sprint 25 | Implementado como `cortex::Cortex::think()` com 12 intenções. Substitui INTENT_MLP antigo (16→8→3). |
| 4 | Trust Cache (TrustEntry, TrustTable, trust-once-use-always) | 🔄 Fundido no Block 5 | Sprint 22 | TrustCache do MVP (Block 5) é versão simplificada. |
| 5 | Trust Cache: regra de 5 situações (auto-ON, conhecido, novo, rejeitado, desconhecido) | 🔄 Fundido no Block 5 | Sprint 22 | Incorporado à TrustTable do MVP. |
| 6 | Trust Cache: persistência no SFS (`/system/trust/usb.tbl`) | ⏳ Pós-MVP | Sprint 24+ | Requer SFS (Sprint 24). MVP sem persistência. |
| 7 | Trust Cache: revogação ("não confio mais") | 🔄 Fundido no Block 5 | Sprint 22 | `trust deny <skill>` no MVP. |
| 8 | WASM skill dispatch para protocolos USB | ⏳ Pós-MVP | Sprint 25+ | Requer WASM embedder (Sprint 25). |
| 9 | Nível 1 — HW Detection (xHCI mínimo, sem IA) | ⏳ Pós-MVP | Sprint 23+ | Bloqueado pelo xHCI driver. |
| 10 | Nível 2 — Device Classification (MLP 7→5) | ⏳ Pós-MVP | Sprint 23+ | MLP arquitetura (Block 4) é primeiro passo. |
| 11 | Nível 3 — Dynamic Interface Creation (WASM) | ⏳ Pós-MVP | Sprint 25+ | Requer WASM embedder. |
| 12 | USB flow: dispositivo desconhecido → porta desabilitada | ⏳ Pós-MVP | Sprint 23+ | Mesma dependência do xHCI. |
| 13 | USB flow: trust-once → segunda conexão auto-ON | ⏳ Pós-MVP | Sprint 23+ | TrustCache existe (Block 5), falta xHCI. |
| 14 | USB flow: usuário precisa inferir intenção (nada automático) | ⏳ Pós-MVP | Sprint 23+ | Princípio arquitetural. Depende de xHCI. |
| 15 | "Zero autorun, zero superfície de ataque USB" | ⏳ Pós-MVP | Sprint 23+ | Princípio adotado como diretriz. |

### 1.2. SMP / APIC / Multicore

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 16 | APIC Local (LAPIC) init no BSP | ✅ Block 1 | Sprint 18 | Implementado: SVR, TPR, timer masked. |
| 17 | IOAPIC init (roteamento IRQ externo) | ✅ Block 1 | Sprint 18 | Implementado: timer→vec32, keyboard→vec33. |
| 18 | x2APIC mode (MSR-based, sem MMIO) | ✅ Bloco 21a | Sprint 81 | x2APIC enable implementado no SMP Foundation. |
| 19 | MADT parsing (ACPI → LAPIC list) | ✅ Block 1 | Sprint 18 | Implementado: type 0 (LAPIC), type 1 (IOAPIC), type 2 (x2APIC). |
| 20 | CPUID leaf 0x1A (P-core / E-core detection) | ✅ Block 2 | Sprint 19 | Essencial para CorePools inteligente. |
| 21 | CPUID leaf 0x0B (Extended Topology) | ✅ Block 2 | Sprint 19 | Necessário para distinguir HT de cores físicos. |
| 22 | CorePools / ComputePools (P→Ring0/1, E→Ring2) | ✅ Block 2 | Sprint 19 | Atribuição por tipo de core + fallback homogêneo. |
| 23 | Algoritmo `assign_cores()` — P/E-aware + N+1 + fallback | ✅ Block 2 | Sprint 19 | Adicionado ao Block 2 após cross-ref. |
| 24 | PerCpu struct (core_id, lapic_id, core_type, ring, stack, queue) | ✅ Bloco 21a | Sprint 81 | PerCpu dinâmico implementado no SMP Foundation. |
| 25 | GS.base segment register per-core | ✅ Bloco 21a | Sprint 81 | GS.base individual por AP implementado. |
| 26 | INIT-SIPI-SIPI via LAPIC ICR | ✅ Bloco 21a | Sprint 81 | IPI funcional para acordar APs sob demanda. |
| 27 | Trampoline assembly (16→32→PAE→64→Rust) | ✅ Block 2 | Sprint 19 | Ponte entre modo real e long mode. |
| 28 | AP startup IPI (BSP → INIT → SIPI → SIPI) | ✅ Bloco 21a | Sprint 81 | AP startup implementado no SMP Foundation. |
| 29 | Stack separada por core (64 KB cada) | ✅ Bloco 21a | Sprint 81 | Stack separada por AP implementada. |
| 30 | Regras de escalonamento por pool | ✅ Block 2 | Sprint 19 | Tabela: qual trabalho → qual pool. |
| 31 | "Se só E-cores, tudo roda em E-cores mais lentos" | ✅ Block 2 | Sprint 19 | Caso de borda documentado. |
| 32 | "Se 1 core apenas (QEMU -smp 1), tudo no mesmo core" | ✅ Block 2 | Sprint 19 | Caso de borda documentado. |
| 33 | "HT: 1 thread por core físico no Ring 0/1, restante no Ring 2" | ✅ Block 2 | Sprint 19 | Regra de atribuição incluída. |
| 34 | `acpi` crate para parser MADT/PPTT | 🟡 Sprint 18+ | Sprint 18+ | Parser ACPI mínimo implementado (sem crate externo). |
| 35 | `raw-cpuid` crate para detecção de features | 🟡 Block 2 | Sprint 19 | No MVP, CPUID inline assembly. |
| 36 | SPSC ring lockless (bbqueue) | ✅ Bloco 21a | Sprint 81 | Fila lock-free Single-Producer Single-Consumer implementada. |
| 37 | `#[repr(align(64))]` cross-core | ✅ Bloco 21a | Sprint 81 | Prevenção de false sharing em atomics compartilhados. |
| 38 | IPI handler registrável | ✅ Bloco 21a | Sprint 81 | Callback por vetor IPI implementado. |
| 39 | Work-stealing Chase-Lev | ✅ Bloco 21b | Sprint 82 | Deques por core, steal quando vazio implementado. |
| 40 | Parallel-for AVX2 matmul | ✅ Bloco 21b | Sprint 82 | Chunk hidden dim por core, sem lock implementado. |
| 41 | AgentScheduler multicore | ✅ Bloco 21b | Sprint 82 | 4 run queues, steal entre cores implementado. |
| 42 | Per-CPU slab allocator | ✅ Bloco 21b | Sprint 82 | Alocar sem lock no hot path implementado. |

### 1.3. NPU (AMD XDNA)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 43 | `Npu` struct + `try_init()` via PCI scan | 💰 Sponsor | Sprint 25+ | Requer AMD APU real (XDNA) ou QEMU com NPU virtual. |
| 44 | `Accelerator::XDNA(Npu)` / `Accelerator::Software` enum | 💰 Sponsor | Sprint 25+ | Depende de #43. |
| 45 | Command queue circular + doorbell write | 💰 Sponsor | Sprint 25+ | Requer documentação do XDNA. |
| 46 | Overlay loading via MMIO | 💰 Sponsor | Sprint 25+ | Vendor-specific. AMD Vitis AI compiler. |
| 47 | MSI-X interrupt registration | 💰 Sponsor | Sprint 25+ | Depende de #43 + IOAPIC/MSI. |
| 48 | Fallback automático: init_npu() → se falha → Software | ✅ Block 4 | Sprint 21 | Se NPU ausente, cai para software. |
| 49 | 3 cenários: QEMU / APU sem driver / APU com driver | 🟡 Block 4 | Sprint 21 | Lógica de fallback documentada. |
| 50 | Cadeia de programação: Modelo → Overlay → DRAM | 💰 Sponsor | Sprint 25+ | Requer toolchain AMD Vitis. |
| 51 | Ring 0 MLP NÃO precisa do NPU — 20 pesos rodam em 1 core | ✅ Block 4 | Sprint 21 | Premissa arquitetural adotada. |
| 52 | Caminho de migração: QEMU → APU f1 → f2 → f3 | 💰 Sponsor | Sprint 25+ | Depende de patrocínio/hardware. |

### 1.4. AI-Driven Hardware Detection

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 53 | `HardwareInventory::collect()` | ✅ Block 4 | Sprint 21 | Coração do Block 4. |
| 54 | `cortex::infer_architecture(&inventory)` | ✅ Block 4 | Sprint 21 | MLP 512→256→64→9 ternário. |
| 55 | MLP 512→256→64→9 ternário (~37 KB, pesos embutidos) | ✅ Block 4 | Sprint 21 | ~150k pesos ternários em .rodata. |
| 56 | `SystemArchitecture` struct (12 saídas categóricas) | ✅ Block 4 | Sprint 21 | ring0, ring1, ring2, heap, sfs, trust, power, tiers. |
| 57 | Boot flow adaptativo: collect → infer → init | ✅ Block 4 | Sprint 21 | Substitui boot sequence fixo atual. |
| 58 | Treinamento offline do MLP (10k hardware profiles) | ⏳ Pós-MVP | Sprint 21+ | Pesos iniciais heurísticos. Treinamento real depois. |
| 59 | Atualização do MLP via skill WASM | ⏳ Pós-MVP | Sprint 25+ | Requer WASM embedder. |
| 60 | Fallback seguro: MLP absurdo → valores default clamped | ✅ Block 4 | Sprint 21 | Heap mínimo 64 KB, ring0 sempre fallback software. |
| 61 | "MLP cabe no kernel — 37 KB no .rodata" | ✅ Block 4 | Sprint 21 | Premissa verificada. |
| 62 | "Inferência é rápida — µs" | ✅ Block 4 | Sprint 21 | MLP ternário em 1 core = microssegundos. |

### 1.5. Memory Hierarchy Index (MHI)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 63 | `struct MemoryTier { device, kind, capacity, bandwidth, latency }` | ✅ Block 4 | Sprint 21 | Adicionado ao MVP (cross-ref). |
| 64 | `struct MemoryHierarchy { tiers: Vec<MemoryTier> }` ordenado | ✅ Block 4 | Sprint 21 | Adicionado ao MVP. |
| 65 | `enum AllocTier { Dram, Vram, Nvme, Hdd }` | ✅ Block 4 | Sprint 21 | Adicionado ao MVP. |
| 66 | `fn alloc_by_tier(tier, size) -> Option<PhysAddr>` | ✅ Block 4 | Sprint 21 | Dram implementado. Vram/Nvme → None com diagnóstico. |
| 67 | `AllocTier::Vram` → alocar no BAR da GPU | 🟡 Bloco 21c | Sprint 84 | Requer driver GPU + BAR mapeado. |
| 68 | `AllocTier::Nvme` → alocar no NVMe via SFS | ⏳ Pós-MVP | Sprint 24+ | Requer NVMe driver + SFS. |
| 69 | `AllocTier::Hdd` → cold storage | ⏳ Pós-MVP | Sprint 24+ | Requer SFS + driver ATA/NVMe. |
| 70 | MLP saídas: heap_tier, tensor_tier, kv_cache_tier, sfs_active_tier | ✅ Block 4 | Sprint 21 | 4 tiers de saída no MLP do MVP. |
| 71 | MLP saídas opcionais: sfs_cold_tier, tensor_swap_tier, skill_heap_tier | 🟡 Block 4 | Sprint 21 | Campos opcionais no SystemArchitecture. |
| 72 | Exemplo real: notebook i5 + GTX 1050 + NVMe + HDD | ✅ Doc | README | Caso de uso documentado. |
| 73 | Exemplo real: Xeon 6900 (1 TB RAM, NVMe RAID) | ✅ Doc | ADR-0015 | Caso de uso documentado. |
| 74 | Exemplo real: AMD APU Strix Point (unified memory) | ✅ Doc | ADR-0015 | Caso de uso documentado. |

### 1.6. Periféricos (PCI, NVMe, VirtIO)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 68 | PCI config space access (CF8/CFC) | ✅ Block 1 | Sprint 18 | Implementado: read_config_dword/word, BARs. |
| 69 | PCI scan: vendor, device, class, subclass, BARs | ✅ Block 1 | Sprint 18 | Implementado: 256 busses, 32 devices, BAR0-5. |
| 70 | PCI bridges (hierarquia de barramento) | 🟡 Block 1 | Sprint 18 | Suporte básico: multi-função em bridges PCI-PCI. |
| 71 | NVMe driver (PCI Class 01.08) | ⏳ Pós-MVP | Sprint 24+ | MVP é stateless. Sem SFS, NVMe é desnecessário. |
| 72 | VirtIO-blk (PCI 1AF4:1001) | ⏳ Pós-MVP | Sprint 24+ | Alternativa QEMU ao NVMe. |
| 73 | VirtIO-net (PCI 1AF4:1041) | 🟡 Sprint 23 (⚠️ não 100%) | Sprint 23 | Driver manual implementado sem `virtio-drivers` crate. Pendente: IRQ, TX recycling. |
| 73b | **VirtIO-GPU (PCI 1AF4:1050)** | 🟡 Sprint 45 (⚠️ 95%) | Sprint 45 | Driver manual PCI caps + MMIO + queue setup. GET_DISPLAY_INFO ⏳ (QEMU TCG lento). |
| 166 | **Multi-mode Trust** | ✅ v0.49.0 | Sprint 49 | PermissionMode enum. `trust_allow_with_mode()`. |
| 176 | **Ed25519 Cryptographic Identity** | ✅ v0.50.0 | Sprint 50 | `identity.rs`, `CapabilityToken` enum, `verify_signature()` bare-metal. |
| 198 | **Boot-time security policy** | ✅ v0.49.0 | Sprint 49 | `TrustCache::load_boot_policy()` seta `PolicyState::Contain` no boot. |
| 256 | **Path Confinement** | ✅ v0.49.0 | Sprint 49 | `PathRule` + `check_path()` no TrustCache. |
| 257 | **Mask Secrets** | ✅ v0.49.0 | Sprint 49 | `mask_secrets()` — 12 padrões, substitui por "*" |
| 258 | **Graduated Enforcement** | ✅ v0.49.0 | Sprint 49 | `PolicyState` máquina: Observe→Warn→Contain→Enforce. |
| 259 | **Posture-Aware Alerting** | ✅ v0.49.0 | Sprint 49 | `posture_check()` verifica hardware antes de executar skill. |
| 260 | **Event→Detector→Response Pipeline** | ✅ v0.50.0 | Sprint 50 | 5 detectores (PortScan, ArpSpoof, PingFlood, DhcpStarvation, TimerAnomaly) + correlação. |
| 74 | VirtIO-gpu (PCI 1AF4:1050) | ⏳ Pós-MVP | Sprint 24+ | MVP usa VGA text. |
| 75 | Intel HDA audio | ⏳ Pós-MVP | Fase 5+ | Nenhuma skill de áudio no MVP. |
| 76 | Sem kernel thread de hotplug | ✅ Princípio | — | Diretriz adotada. |
| 77 | Sem sysfs genérico | ✅ Princípio | — | Diretriz adotada. |
| 78 | Cada driver é módulo autocontido, sem trait Device universal | ✅ Princípio | — | Diretriz adotada. |

### 1.7. Áudio/Vídeo

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 79 | UEFI framebuffer (BGRA32 writer) | ⏳ Pós-MVP | Sprint 23+ | VGA text serve. |
| 80 | Font rendering para alta resolução | ⏳ Pós-MVP | Sprint 23+ | Depende de #79. |
| 81 | VirtIO-GPU 2D/3D acelerado | ⏳ Pós-MVP | Sprint 24+ | Requer VirtIO. |
| 82 | Tensor visualization no framebuffer | ⏳ Pós-MVP | Fase 5+ | Depende de #79 + #81. |
| 83 | Intel HDA audio driver — Áudio via PCI HDA controller. Essencial para TTS/STT do JARVIS sem depender de USB. | 🟡 Sprint Sound | Sprint Sound | JARVIS precisa ouvir e falar. Port do driver Linux HDA. |
| 84 | Áudio via USB (UAC) — USB Audio Class para fones/microfone USB. Alternativa ao HDA quando não disponível. | 🟡 Sprint Sound | Sprint Sound | JARVIS voice I/O via USB. |

### 1.8. Princípios Arquiteturais

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 85 | Mínimo viável: só implementar driver se requisito para skill WASM ou boot | ✅ Princípio | — | Guia todas as decisões do MVP. |
| 86 | VirtIO first: QEMU antes de hardware real | ✅ Princípio | — | Diretriz adotada. |
| 87 | Polling > Interrupção para dispositivos de baixa taxa | ✅ Princípio | — | Adotado. |
| 88 | Sem HAL genérica — cada driver é módulo autocontido | ✅ Princípio | — | Adotado. |
| 89 | "O usuário precisa inferir" — nenhum dispositivo tem autoridade implícita | ✅ Princípio | — | Fundamento do zero-trust. |
| 90 | Trust-once-use-always usabilidade | ✅ Block 5 | Sprint 22 | TrustCache implementa. |

### 1.9. Roadmap Original — Memória

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 91 | Bitmap Frame Allocator | ✅ Block 0 | Sprint 11 | Já implementado. |
| 92 | Huge Pages (2 MiB) | ⏳ Pós-MVP | Sprint 23+ | Otimização para modelos pesados pós-MVP. |
| 93 | Huge Pages (1 GiB) | ⏳ Pós-MVP | Sprint 24+ | Depende de #92. |
| 94 | Slab Allocator | ✅ Block 2 | Sprint 19 | Essencial para heap dinâmico. |

### 1.10. Roadmap Original — Kernel Abstraction

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 95 | Async Neural Executor | ✅ Block 0 | Sprint 12 | Já implementado. |
| 96 | Agent Scheduler (round-robin) | ⏳ Pós-MVP | Sprint 24+ | Executor cooperativo segura 1-4 cores. |
| 97 | Budget de execução (tokens_consumed) | ⏳ Pós-MVP | Sprint 24+ | Depende de #96. |
| 98 | MLP decide prioridade no scheduler | ⏳ Pós-MVP | Sprint 24+ | Depende de #96 + MLP. |

### 1.11. Roadmap Original — EventBus

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 99 | EventBus + CapabilityToken | ✅ Block 0 | Sprint 13 | Já implementado. |
| 100 | Topic enum completo | ⏳ Pós-MVP | Sprint 23+ | Strings funcionam. Enum é segurança de tipo. |
| 101 | ML-based routing (EventBus consulta Intent Router) | ⏳ Pós-MVP | Sprint 23+ | Inovação futura. BTreeMap resolve. |

### 1.12. Roadmap Original — Skill Registry

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 102 | Skill trait + MCP + Registry | ✅ Block 0 | Sprint 14 | Já implementado. |
| 103 | WASM embedder (wasmi) | ⏳ Pós-MVP | Sprint 25+ | Skills Rust traits bastam para MVP. |
| 104 | Linear memory pool (256 KB por skill) | ⏳ Pós-MVP | Sprint 25+ | Depende de #103. |

### 1.13. Roadmap Original — Cognitive Runtime

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 105 | Intent Planner (sequência de SkillCommands) | ⏳ Pós-MVP | Fase 6 | MVP classifica intent única. |
| 106 | Success Engine (feedback loop, ajuste online de pesos) | ⏳ Pós-MVP | Fase 6 | Depende de #105. Pesquisa acadêmica. |
| 107 | Neural Cache (lookup table 50 ns em Huge Pages) | ⏳ Pós-MVP | Fase 6 | Depende de #92 + #105. |
| 108 | MatMul-free LM (RWKV/Mamba/ternary pooling) | ⏳ Pós-MVP | Fase 7 | Meta futura distante. |

### 1.14. Roadmap Original — Timeline

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 109 | Sprint 16: Slab Allocator | 🔄 Remapeado | Block 2 S19 | Movido após PCI+APIC. |
| 110 | Sprint 17: Agent Scheduler | 🔄 Remapeado | Sprint 24+ | Executor cooperativo é suficiente. |
| 111 | Sprint 18+: Cognitive Runtime | 🔄 Remapeado | Fase 6 | MVP primeiro. |

### 1.15. Outras Ideias

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 112 | Bootável em hardware x86 real (UEFI) | ✅ MVP | Sprint 22 | Critério de aceite do MVP. |
| 113 | Nome "Hermes" como identidade do MVP | ✅ Adotado | — | README + ADR-0015 usam. |
| 114 | Chat loop estilo Hermes Agent (Nous Research) | ✅ Block 3 | Sprint 20 | Inspiração direta. |
| 115 | Sponsor: NPU AMD XDNA requer parceria | 💰 Sponsor | Sprint 25+ | Sem hardware, sem implementação. |
| 116 | Sponsor: port para ARM/RISC-V | 💰 Sponsor | Futuro | Fora do escopo x86-64. |

### 1.16. Rede/Network Stack

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 117 | VirtIO-net driver (PCI) sobre `virtio-drivers` crate | 🟡 Sprint 23 | Sprint 23 | PCI scan já detecta 1AF4:1041. VirtIO é spec simples. |
| 118 | smoltcp TCP/IP stack integration | 🟡 Sprint 23 | Sprint 23 | ARP/IP/TCP/UDP/DNS no_std, usado pelo Redox OS. |
| 119 | DNS resolver (smoltcp `dns` feature) | 🟡 Sprint 23 | Sprint 23 | Resolução de hostnames para HTTP. |
| 120 | HTTP GET/POST client minimal (~200 LOC) | 🟡 Sprint 23 | Sprint 23 | Saída de smoltcp TCP para skills e weight updates. |
| 121 | Hermes `/fetch` command | 🟡 Sprint 23 | Sprint 23 | Comando de shell para baixar arquivos via HTTP. |
| 122 | Skill manifest field `requires_network: bool` | 🟡 Sprint 23 | Sprint 23 | Skills podem declarar necessidade de rede. |
| 123 | TLS 1.3 client (`embedded-tls` crate) | ⏳ Pós-MVP | Sprint 25+ | Obrigatório para HTTPS. Postergado para WASM. |
| 124 | Wi-Fi / Ethernet (e1000/RTL8139 para HW real) | ⏳ Pós-MVP | Sprint 26+ | VirtIO só funciona em QEMU. HW real precisa de driver nativo. |
| 250 | **Comando `/ping <ip>`** — ICMP Echo Request via e1000 | ✅ Block 6 | Sprint 23 | `net::ping()` usa `icmp_echo_request` + `parse_icmp_reply`. |
| 251 | **DHCP timer-based wait** — refatorar spin loops para `hlt()` com timeout por timer ticks | 🟡 Sprint 24 | Sprint 24 | Spin loops não funcionam no QEMU TCG (slirp não processa I/O). Necessário para DHCP dinâmico. |
| 252 | **ARP não-bloqueante** — timeout com retry usando timer ticks em vez de spin loop | 🟡 Sprint 24 | Sprint 24 | ARP sem resposta no QEMU TCG. Gateway MAC hardcoded temporariamente. |
| 253 | **e1000 TDT protocol fix** — `send()` escrevia REG_TDT = idx (== TDH), hardware via ring vazio. Corrigido: TDT = (idx+1) % NUM_DESC | ✅ Block 6 | Sprint 23 | Causa raiz TPT=0. Descritor lido mas pacote não enviado. |
| 254 | **e1000 NUM_DESC 32→48** — 82540EM requer mínimo 48 descritores RX | ✅ Block 6 | Sprint 23 | Linux e1000 driver docs: "48-256 for 82542 and 82543-based adapters". |
| 255 | **Arquitetura Neural de Rede** — init_driver_network() → HW_NET_E1000 EventBus → network_bootstrap() → network_health_daemon() → skill routing | ✅ Block 6 | Sprint 23 | Hardware detection first, IA decide routing. |

### 1.20. Tier 3 Security Patterns — InnerWarden, ai-jail, vexfs, Chisel

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 256 | **Path Confinement para Skills** — SkillRegistry verifica allowlist de paths por token antes de operar (Chisel + ai-jail) | 🟡 Sprint 24 | Sprint 24 | ~60 LOC. TrustCache já faz validação similar. |
| 257 | **Mask Secrets** — TrustCache/SkillRegistry mascara paths/env vars sensíveis antes de expor para skills (ai-jail `--mask`) | 🟡 Sprint 24 | Sprint 24 | ~50 LOC. Substitui padrões por "[REDACTED]". |
| 258 | **Graduated Enforcement** — PolicyState machine: Observe→Warn→Contain→Enforce (InnerWarden) | 🟡 Sprint 24 | Sprint 24 | ~80 LOC. Adiciona estado ao SkillRegistry. |
| 259 | **Posture-Aware Alerting** — Skills verificam estado do hardware antes de agir (InnerWarden) | 🟡 Sprint 24 | Sprint 24 | ~40 LOC. Se link down → não configura rede. |
| 260 | **Event→Detector→Response Pipeline** — EventBus → Detector stateful → Correlation → Response Skill (InnerWarden core) | 🟡 Sprint 25 | Sprint 25 | ~200 LOC. 5 detectores iniciais (PortScan, ArpSpoof, PingFlood, DhcpStarvation, TimerAnomaly). Novo crate `security-pipeline`. |
| 261 | **Decision Review + Human Escalation** — Detector com baixa confiança publica NEEDS_REVIEW com timeout (InnerWarden) | 🟡 Sprint 25 | Sprint 25 | ~120 LOC. Timeout auto-resolve. High severity nunca auto-resolve. |
| 262 | **Hash Chain Audit Trail** — EventLog com SHA-256 chain: cada evento contém hash do anterior (InnerWarden) | 🟡 Sprint 25 | Sprint 25 | ~60 LOC. Extensão do #231. verify_chain() → bool. |
| 263 | **Knowledge Graph para Eventos de Segurança** — Grafo em memória: 6 node types, ~20 relations (InnerWarden knowledge graph) | 🟡 Sprint 26 | Sprint 26 | ~400 LOC. Node types: Process, NetworkEndpoint, File, Skill, Hardware, User. |
| 264 | **Cross-Layer Correlation Rules** — Regras multi-estágio: ARP Spoof→Port Scan→Data Exfil (InnerWarden 69 regras) | 🟡 Sprint 27 | Sprint 27 | ~300 LOC. 5 regras iniciais. Risco de falso positivo. |
| 265 | **Filesystem como Vector Search** — Operações de arquivo expõem vector search via xattr (vexfs) | ⏳ Pós-MVP | Sprint 28+ | Depende de SFS implementado. |
| 266 | **Multi-dialect Vector API** — API server compatível com ChromaDB/Qdrant (vexfs) | ⏳ Pós-MVP | Sprint 28+ | Depende de MemPalace ou SFS com embeddings. |
| 267 | **OverlayFS Copy-on-Write** — Writes de agentes vão para overlay separado (ai-jail) | ⏳ Pós-MVP | Sprint 28+ | Depende de VFS implementada. |

### 1.17. Documentação e ADRs

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 125 | ADR-0016: Network Strategy | ✅ Documentado | Sprint 20 | Decisão arquitetural sobre quando/como implementar rede. |

### 1.18. Neural Cortex — BitNet LLM (Novo Plano Diretor)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 126 | **Transformer Engine** — Attention (`QK^T/√d`), causal mask, softmax, FFN (SiLU), residual | ✅ Sprint 26 | Sprint 26 | Implementado em cortex.rs: 4 layers, Attention Q/K/V/O, causal mask, RMSNorm, SiLU FFN, residual. |
| 127 | **Tokenizer character-level** — ASCII 32-126 + `<BOS>/<EOS>/<PAD>` | 🟡 Sprint 25 | Sprint 25 | Entrada/saída de texto bare-metal. Reutiliza `scancode_to_ascii()`. |
| 128 | **Autoregressive generation** — loop `tokenize → forward → sample → next` | 🟡 Sprint 25 | Sprint 25 | ~30 LOC. Gera resposta token por token até `<EOS>`. |
| 129 | **Model format `.bitnet`** — binary spec com magic, header, packed ternary weights | 🟡 Sprint 25 | Sprint 25 | Formato padronizado para modelos exportados do Python. |
| 130 | **Model loader** — `include_bytes!` + `allocate_contiguous()` → `PackedTernaryTensor` | 🟡 Sprint 25 | Sprint 25 | Carrega micro-modelo (~1M params, ~250 KB). |
| 131 | **Micro-model TinyStories** (1M params, 4 layers, hidden=128) treinado em Python | 🟡 Sprint 25 | Sprint 25 | Modelo de prova para testar pipeline completo. |
| 132 | **Cortex Daemon** — async task que recebe `LLM_REQUEST` → gera → publica resposta | ✅ Sprint 27 | Sprint 27 | Implementado como `cortex_llm_daemon` (8ª task). TransformerModel carregado no boot. |
| 133 | **Modelo 1.5B params** (distilado do Llama 3.2 1B → ternário 2-bit, ~375 MB) | 🟡 Sprint 26 | Sprint 26 | Cérebro completo do AIOS. ~5-15 tok/s em x86-64. |
| 134 | **Model update via HTTP** — download `.bitnet` → validar hash → hot-swap | 🟡 Sprint 26 | Sprint 26 | Permite evolução do modelo sem recompilar kernel. |
| 135 | **LLM decide hardware arch** — substitui `SystemArchitecture::infer()` heurístico | 🟡 Sprint 26 | Sprint 26 | MLP (item #51) vira LLM query. |
| 136 | **LLM decide memory tier** — roteia alocações Dram/Vram/Nvme/Hdd | 🟡 Sprint 26 | Sprint 26 | Substitui `AllocTier` heurístico. |
| 137 | **LLM classifica USB devices** — Neural Cortex 7→5 allow/deny/learn/no_intent/suspect | 🟡 Sprint 27 | Sprint 27 | Substitui item #3 (MLP 7→5). |
| 138 | **LLM dispatch skills** — qual skill executar para cada intenção | 🟡 Sprint 27 | Sprint 27 | Evolução do roteamento atual. |
| 139 | **Reflex MLP threshold tuning** — se confiança > 0.9, bypassa LLM | 🟡 Sprint 27 | Sprint 27 | Performance: decisões simples em microssegundos. |
| 140 | **Speculative decoding** — Reflex MLP prediz próximo token, LLM verifica | ⏳ Pós-MVP | Sprint 27+ | Acelera geração 2-3×. |
| 141 | **1.5B model benchmark** — 5-15 tok/s on single x86-64 core (AVX2) | 🟡 Sprint 26 | Sprint 26 | Critério de aceite do Cortex. |

### 1.19. Transformer Engine (Detalhamento Técnico)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 142 | `Attention` struct — q_proj, k_proj, v_proj, o_proj (todos `Linear`) | 🟡 Sprint 25 | Sprint 25 | Bloco fundamental do transformer. |
| 143 | `causal_mask` — triângulo superior -inf, diag/abaixo 0 | 🟡 Sprint 25 | Sprint 25 | Impede token de "ver" o futuro. |
| 144 | `softmax` row-wise em cima de `Tensor` | 🟡 Sprint 25 | Sprint 25 | Normalização das probabilidades de atenção. |
| 145 | `TransformerBlock` — RMSNorm → Attn → residual → RMSNorm → FFN(SiLU) → residual | 🟡 Sprint 25 | Sprint 25 | Camada completa do transformer. |
| 146 | `Transformer` — embed → N×TransformerBlock → RMSNorm → unembed | 🟡 Sprint 25 | Sprint 25 | Modelo completo. |
| 147 | `generate()` — loop: forward → sample → next | 🟡 Sprint 25 | Sprint 25 | Geração autoregressiva. |
| 148 | Sampling: argmax, top-k(3/5/10), temperature | 🟡 Sprint 27 | Sprint 27 | Controla criatividade da resposta. |

### 1.20. Success Engine (Ajuste Online)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 149 | Feedback loop — usuário avalia resposta (👍/👎) | ⏳ Pós-MVP | Sprint 29+ | Input para ajuste de pesos. |
| 150 | Ternary weight update — {-1,0,+1} → {-1,0,+1} com probabilidade | ⏳ Pós-MVP | Sprint 29+ | Algoritmo de aprendizado online. Pesquisa. |
| 151 | Experience replay buffer (últimas N interações) | ⏳ Pós-MVP | Sprint 29+ | Evita esquecimento catastrófico. |
| 152 | Weight consolidation — export modelo atualizado | ⏳ Pós-MVP | Sprint 29+ | Persistência do aprendizado. |

### 1.21. Treinamento (Host-side, Python)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 153 | Train micro BitNet (1M params, TinyStories) → export `.bitnet` | 🟡 Sprint 25 | Sprint 25 | Modelo de teste para integrar. |
| 154 | Distil Llama 3.2 1B → ternário → `.bitnet` 1.5B | 🟡 Sprint 26 | Sprint 26 | Modelo completo do AIOS. |
| 155 | Pipeline `bitnet.cpp` quantization script | 🟡 Sprint 25 | Sprint 25 | Ferramenta para quantizar qualquer modelo. |
| 156 | Ferramenta de validação — forward match kernel vs Python | 🟡 Sprint 25 | Sprint 25 | Garante que kernel e Python produzem mesmos outputs. |

### 1.22. Self-Optimization / Workflow Learning

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 157 | **Usage Pattern Analyzer** — LLM observa últimas N intenções, detecta workflow do usuário (hora, frequência, recursos) | 🟡 Sprint 27 | Sprint 27 | Base de todo o ciclo de auto-otimização. Analisa EventBus history + MLP decisions. |
| 158 | **Workflow Predictor** — pré-carrega recursos (MHI tiers, scheduler priority) baseado em hora/dia/padrão detectado | 🟡 Sprint 27 | Sprint 27 | Ex: "14h toda segunda → pré-alocar 6 GB RAM + GPU para CAD". Depende de #157. |
| 159 | **Auto-Skill Generator** — cria skill WASM para tarefa repetitiva detectada (≥3 ocorrências no mesmo workflow) | 🟡 Sprint 28 | Sprint 28 | Ex: "render_batch" skill gerada automaticamente. Depende de #103 (WASM). |
| 160 | **Dynamic Resource Scaling** — MHI ajusta tiers (Dram/Vram/Nvme) dinamicamente pelo uso real, não só por boot | 🟡 Sprint 27 | Sprint 27 | MHI hoje é estático (boot). Evolui para auto-ajuste. Depende de #56-67. |
| 161 | **Self-Optimizing Scheduler** — prioriza agentes conforme workflow detectado (render → GPU agent high prio) | 🟡 Sprint 27 | Sprint 27 | Depende de #96 (Agent Scheduler) + #157. |
| 162 | **Workflow Profile** — perfil salvo exportável ("arquiteto", "escritório", "dev") com recursos, skills, prioridades | 🟡 Sprint 28 | Sprint 28 | Permite trocar perfil sem rebuild. Depende de #157 + SFS (Layer 2). |
| 163 | **Hardware Config Learning** — `SystemArchitecture` evolui com feedback do usuário (não só heurística de boot) | 🟡 Sprint 27 | Sprint 27 | LLM ajusta `SystemArchitecture` baseado em uso real. Depende de #135 + #157. |

---

### 1.23. Crom Ecosystem — Ideas Ported from MrJc01 (75 repos)

| # | Item | Classificação | Sprint | Motivação |
|---|---|---|---|---|
| 164 | **XOR Delta reconstruction** — modo Archive lossless no PackedTernaryTensor; armazena resíduo XOR para round-trip bit-exact | ✅ Imediata | Sprint 24 | ~50 LOC sobre operações bitwise existentes. Permite verificação SHA-256 do output. |
| 165 | **CDC Rabin Fingerprint** — Content-Defined Chunking via rolling hash p/ dividir `.bitnet` models em chunks carregáveis | ✅ Imediata | Sprint 24 | ~80 LOC, rolling hash polinomial. Útil para carregamento sob demanda de modelos grandes. |
| 166 | **Multi-mode Trust** — PermissionMode enum (TotalAccess/AskEveryTime/Scoped) no TrustCache | 🟡 Baixa | Sprint 27 | ~100 LOC sobre TrustCache existente. Alinha com HITL do Crom-Agente. |
| 167 | **TV-DSL Co-processor** — AST determinístico para expressões matemáticas; Hermes chama co-processador para cálculos exatos sem alucinação | 🟡 Baixa | Sprint 27 | ~200 LOC, parser de expr matemática em `no_std` (reusa `libm`). Zero alucinação aritmética — crítico p/ arquiteto (volumetria) e escritório (impostos). |
| 168 | **PonderNet dynamic stop** — Reflex MLP decide quantos ciclos de inferência executar (não fixo) baseado em confiança | 🟡 Baixa | Sprint 27 | ~150 LOC sobre executor existente. Adaptive compute = eficiência energética. |
| 169 | **Codebook Compression (VQ)** — Vector Quantization p/ PackedTernaryTensor; substitui `quantize_to_packed()` por `train_codebook()` + `lookup()` O(1) | 🟠 Média | Sprint 28 | ~300 LOC kernel + script Python treinamento. Crompressor-Neurônio: 97.56% acc com 40.8× compressão. |
| 170 | **KV Cache Codebook** — aplica VQ ao cache de atenção do Transformer Engine; 94.2% redução real (Crompressor-Neurônio Lab06) | 🟠 Média | Sprint 28 | Depende de #126-131 (Transformer Engine pronto). Reduz cache de 2 MB p/ ~120 KB por camada. |
| 171 | **ReAct loop com auto-correção** — NeuralExecutor evolui com fase de verificação: hash de ações recentes, detecção de loop infinito, re-tentativa em erro | 🟠 Média | Sprint 28 | ~300 LOC. Crom-Agente: 40 capacidades, loop ReAct com auto-verificação via lint/test. |
| 172 | **MCP Server support** — EventBus + SkillRegistry evoluem para suportar servidores MCP externos via JSON-RPC 2.0 | 🟠 Média | Sprint 28 | ~400 LOC. Requer parser JSON em `no_std` ou protocolo binário custom. Compatibilidade com ecossistema MCP. |
| 173 | **Codebook LLM finetune** — treinar APENAS o codebook (5.770 params) em vez dos pesos (235K), superando baseline 98.08% vs 97.53% | ⏳ Pós-MVP | Sprint 29+ | Pesquisa: Crompressor-Neurônio Tensor-Vivo Exp2. Success Engine pode usar codebook learning p/ ajuste online. |
| 174 | **Delta branches (speculative decoding)** — branches de inferência paralela com 99.9% economia de memória via XOR delta entre branches | ⏳ Pós-MVP | Sprint 29+ | Crompressor-Neurônio Lab07. Requer scheduler maduro + múltiplos cores. Viabilidade depende de benchmark real. |
| 175 | **Workspace isolation** — per-project config (skills/recursos/trust) isolados por workspace, estilo `.crom/config.json` | ⏳ Pós-MVP | Sprint 29+ | Crom-Agente workspace isolation. Requer SFS (Layer 2) para persistência. |
| 176 | **Ed25519 Cryptographic Identity for TrustCache** — substitui `CapabilityToken(u64)` estático por assinatura Ed25519; Token vira chave pública + assinatura da requisição; Zero-Trust real em nível de kernel | 🟡 Baixa | Sprint 27 | Crom-meueu: identidade criptográfica Ed25519 portada para bare-metal `no_std`. ~300 LOC usando `ed25519-dalek` (sem std) ou implementação custom. Depende de #166 (Multi-mode Trust) como camada de permissão sobre a identidade. |

**ADR-0020:** `docs/architecture/0020-crom-ecosystem-analysis.md` — Análise de viabilidade Rust com código modelo para 9 features (#164-175). Item #176 (Ed25519) adicionado posteriormente.

### 1.24. DiskIntelligenceAgent (IDEA #303 expandido)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 303a | DiskIntelligenceAgent com StorageController trait | ✅ v0.75.1 | v0.75.1 | "Mestre dos discos" — 6 controladoras, 10+ FS probes, MHI, VFS |
| 303b | S.M.A.R.T. monitoring com self-heal alert | ✅ v0.75.2 | v0.75.2 | Detecção precoce de falha de disco |
| 303c | GPT probe + SED/OPAL + EROFS/ReFS | ✅ v0.75.3 | v0.75.3 | Tabela GPT, Self-Encrypting Drives, mais FS |
| 303d | USB-MSC bulk fix + BOT protocol | ✅ v0.75.4 | v0.75.4 | xHCI IOC+ring+ERDP, SCSI INQUIRY/READ/WRITE |
| 303e | NVMe driver (Admin queue + Identify + I/O) | ✅ v0.75.5 | v0.75.5 | PCI 0x01/0x08, PRP1, SQ/CQ doorbell |
| 303f | ARC cache + tier migration | ✅ v0.75.6 | v0.75.6 | 1MB DRAM cache, write-back, MHI update |

### 1.25. AIOS Cross-OS Compatibility + WASM + J.A.R.V.I.S. (IDEA #306-#310)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 306a | **Windows binary compat** — PE32+ loader + syscall translation (NT→AIOS). DLL loader stub, SEH handler, PEB emulação | ⏳ Pós-MVP | v0.80+ | Rodar .exe nativos sem emulação |
| 306b | **Linux ELF compat** — ELF x86-64 loader + syscall translation (open/read/write/mmap/clone → agent skills) | ⏳ Pós-MVP | v0.80+ | Aproveitar ecossistema Linux |
| 306c | **macOS/iOS Mach-O compat** — Mach-O loader + XNU syscall translation. Desafio: APIs Cocoa/AppKit fechadas | ⏳ Pós-MVP | v0.85+ | Apps nativos Apple |
| 306d | **Android APK compat** — ART runtime como skill, Binder IPC → agentes. Desafio: framework Java → tradução | ⏳ Pós-MVP | v0.85+ | 3M+ apps Android no AIOS |
| 307 | **Syscall-to-Skill Translation Layer** — Camada única: syscall (NT/Linux/XNU) → skill request → agent.response. "abrir /etc/passwd" → DiskAgent.read() | ⏳ Pós-MVP | v0.80+ | Base p/ compatibilidade cross-OS |
| 308a | **Update/Upgrade Agent** — Dual Kernel Slot A/B no FAT32. Baixa novo kernel via smoltcp HTTP GET, verifica Ed25519 + SHA-256, escreve KERNEL~2, switch BOOTCFG.JSON, reboot | 🟡 Pós-MVP | v0.80+ | ~500 LOC. Viability 9/10. Bloqueado por B-01 (rede). ADR-0031 |
| 308b | **Update channels** — stable (Sprint), nightly (HEAD), security (hotfix). Channel manifesto via HTTP GET de update-server. Poll 3600s/600s/60s | 🟡 Pós-MVP | v0.80+ | ~200 LOC. ADR-0031 |
| 308c | **Rollback automático** — BootSelfHealAgent detecta crash pós-update → restaura BOOTCFG.JSON → last_good slot. Três falhas seguidas → rollback forçado | 🟡 Pós-MVP | v0.80+ | ~100 LOC. ADR-0031 |
| 309a | **WASM Skill Runtime (wasmi)** — wasmi v0.42+ intérprete no_std. Cada .wasm vira agente com sandbox, 256KB linear memory, fuel metering (100k/tick), capability tokens | 🟡 Pós-MVP | v0.75+ | ~800 LOC. Viability 8/10. WASI→agent skill mapping (20 syscalls). ADR-0031 |
| 309b | **IDE Agent (BitNet IDE)** — IDE no-navegador no AIOS, assistida por Cortex LLM BitNet. Escreve, debuga, compila para WASM | ⏳ Pós-MVP | v0.85+ | ~2000 LOC. Requer WASM + Cortex 1.5B + J.A.R.V.I.S. ADR-0031 |
| 309c | **Agentes: kernel vs WASM (Hybrid)** — Tier 0-2 kernel (boot, HW, runtime crítico). Tier 3 WASM (user-extensible). Tier 4 external MCP. 20 agentes kernel, ∞ WASM | 🟡 Pós-MVP | v0.80+ | ~100 LOC (policy config). Viability 9/10. ADR-0031 |
| 310a | **J.A.R.V.I.S. Layer** — Camada de persona acima do Hermes: SOUL.md, contexto persistente (MemoryTree+KG), notificações proativas (NotificationGate), conversation engine (greetings, mood, task decomposition) | 🟡 Sprint 77 | Sprint 77 | ~950 LOC (Sprint 77). Substituído pelo ADR-0036 detalhado (#315) |
| 310b | **Stack final:** Boot → Kernel → Cortex/LLM → Hermes → J.A.R.V.I.S. — Ver diagrama ADR-0036. Boot minimalista. Kernel acorda Cortex (BitNet). Cortex alimenta Hermes (intent). Hermes delega para J.A.R.V.I.S. (persona). Tudo agentes, tudo skills | 🟡 Sprint 77 | Sprint 77 | ADR-0036 |

### 1.26. Trinity Model Hub — Mixture of Experts (IDEA #311)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 311a | **Router BitNet (68KB)** — classifica intenção e roteia para expert correto | 🟡 Sprint 77 | Sprint 77 | 5-10 classes de intenção |
| 311b | **hw_identify (68KB, ✅ existente)** — 66K pares PCI/USB, 99% precisão | ✅ v0.30 | Sprint 30 | Kernel mode |
| 311c | **rust_coder (444KB)** — gera código Rust no_std. "crie funcao rust para" → expert gera resposta direta sem LLM principal | ✅ Sprint 97 | Sprint 97 | Hidden=128, 6L, 1.6M params, loss 0.34 |
| 311d | **disk_diag (50KB)** — padrões SMART + erros de disco. Diagnóstico + ação sugerida | 🟡 Sprint 78 | Sprint 78 | Self-heal logs |
| 311e | **security (50KB)** — assinaturas de ataque + CVE patterns | 🟡 Sprint 78 | Sprint 78 | CVE + InnerWarden |
| 311f | **On-demand training** — "quero pilotar helicóptero" → manual → .bitnet → skill | ⏳ Pós-MVP | v0.80+ | Pipeline treino |
| 311g | **Generator 1.5B (375MB)** — tool-use: expert classifica, generator explica | 🟡 Sprint 77 | Sprint 77 | GGUF loader |

### 1.27. TrainingAgent — On-Device + GPU Learning (IDEA #312)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 312a | **GPU detection for training** — MemoryAgent already detects GPU + VRAM. If VRAM > 4GB → full training viable. If < 4GB → fine-tuning only | ✅ v0.75.1 | v0.75.1 | GPU detect exists |
| 312b | **Fine-tuning (CPU, ADD/SUB kernel)** — 100 exemplos, 50 iterações, ~2 segundos. BitNet ternário usa só ADD/SUB, sem FPU. On-device sempre disponível | 🟡 Sprint 77 | Sprint 77 | ~300 LOC kernel |
| 312c | **Transfer learning (CPU/GPU)** — 1000 exemplos, adapta modelo existente para novo domínio. Ex: disk_diag → net_diag (adapta padrões SMART para padrões de rede) | 🟡 Sprint 78 | Sprint 78 | ~200 LOC |
| 312d | **Full training (GPU, internet)** — 100K+ exemplos. Equivalente ao gen_micro_model.py rodando como agente. Pipeline: fetch data via HTTP → train PyTorch → export .bitnet → load no kernel. Só viável com B-01 + GPU ≥ 4GB VRAM | ⏳ Pós-MVP | v0.81+ | Depende de B-01 (rede) |
| 312e | **TrainingAgent** — agente que detecta GPU, escolhe modo (fine-tune/transfer/full), coleta dados do FS ou internet, executa treino, registra modelo no Trinity Hub | 🟡 Sprint 78 | Sprint 78 | ~500 LOC |
| 312f | **Federated learning** — múltiplos AIOS compartilham gradientes (não dados). Agregador central (Hermes Master) combina updates → modelo global melhor | ⏳ Pós-MVP | v0.85+ | Depende de B-01 + MCP |

### 1.28. Self-Learning OS — Dados Gerados Pelo Próprio Sistema (IDEA #313)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 313a | **Dados gerados pelo AIOS** — EventBus (10K+ eventos/hora), boot logs, SMART data, self-heal logs, Hermes conversas, consciousness metrics, disk I/O patterns, network packets (c/ B-01), GPU errors. Tudo vira dataset de treino. | 🟡 Sprint 78 | Sprint 78 | ~300 LOC DataCollector |
| 313b | **Pipeline: LogAgent → DataCollector → TrainingAgent → .bitnet → Trinity Hub** — LogAgent coleta eventos, DataCollector estrutura como pares (input, output), TrainingAgent treina modelo, .bitnet vai pro Trinity Hub, Router classifica quando usar. | 🟡 Sprint 78 | Sprint 78 | ~200 LOC pipeline |
| 313c | **Melhoria contínua** — Cada boot gera dados. Cada treino melhora o sistema. O AIOS de hoje é melhor que o de ontem. Próximo boot: modelo treinado com dados do boot anterior. Sem internet. Sem humano. | 🟡 Sprint 78 | Sprint 78 | Integração boot sequence |

### 1.29. SleepCycle — Aprendizado Inspirado no Sono Humano (IDEA #314)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 314a | **Experience Replay** — Ring buffer dos últimos 1000 eventos do EventBus. Amostra 64 por ciclo de sono. Treina BitNet com replay. | 🟡 Sprint 77 | Sprint 77 | ~50 LOC |
| 314b | **Generative Replay (Dream)** — BitNet gera variações sintéticas dos exemplos reais (temperature sampling). "E se o erro fosse +20%? E se o usuário perguntasse X?" | 🟡 Sprint 77 | Sprint 77 | ~200 LOC BitNet VAE |
| 314c | **Elastic Weight Consolidation** — Protege skills existentes durante novo aprendizado. Pesos importantes para skill A não mudam quando treina skill B. | 🟡 Sprint 77 | Sprint 77 | ~150 LOC |
| 314d | **Synaptic Homeostasis (Pruning)** — Pesos com frequência de ativação < threshold viram 0. Elimina ruído, preserva sinal. ~18% de redução por ciclo (medido em cérebros reais). | 🟡 Sprint 77 | Sprint 77 | ~80 LOC |
| 314e | **Metacognitive Reflection** — Confidence tracking. Amostra interações com baixa confiança, gera micro-lessons corretivas, treina. | 🟡 Sprint 77 | Sprint 77 | ~250 LOC |
| 314f | **SleepCycle Agent** — CronAgent dispara a cada período de idle. 5 fases: REPLAY → DREAM → CONSOLIDATE → PRUNE → REFLECT. Ao final: modelo .bitnet atualizado no Trinity Hub. | 🟡 Sprint 77 | Sprint 77 | ~50 LOC scheduler |
| 314g | **Pioneirismo** — NENHUM sistema bare-metal implementa ciclo de sono/aprendizado. Neural AIOS seria o primeiro no mundo. | ✅ Conceito | — | Fato |

### 1.31. J.A.R.V.I.S. Unified Interaction Layer — Persona + Memória Cognitiva (IDEA #315)
**28 features, 5-layer architecture, Sprints 77-80+N+1+N+2. ADR-0036 substitui ADR-0034 + ADR-0035.**

| # | Item | Destino | Sprint | LOC | Fonte |
|---|---|---|---|---|---|
| 315.1 | **SOUL.md Personality Engine** — Parser markdown minimalista \| nome, tom, humor_level, formality, empathy, greetings, farewell, notification rules. Adapta tom por contexto (Fluid Personality paper) | 🟡 Sprint 77 | 77 | ~300 | JARVIS C# + BeFree JARBAS |
| 315.2 | **IPW Monitor (RAPL MSR 0x610)** — Intelligence Per Watt. Mede energia via PKG_ENERGY_STATUS, calcula tokens/watt, cache hit/miss ratio. Acoplado ao MemoryAgent | 🟡 Sprint 77 | 77 | ~150 | OpenJarvis Stanford |
| 315.3 | **Session Compression (4 strategies)** — Summarize (BitNet), DropLowest (importance), MergeSimilar (embedding), SegmentMeans (SKYNET). Mantém últimas N mensagens literais | 🟡 Sprint 77 | 77 | ~200 | OpenJarvis + SKYNET |
| 315.4 | **Notification Gate (4 urgency levels)** — Critical (imediato), High (30s), Medium (idle), Low (log). Rate limiting, dedup, startup grace period. Regras do SOUL.md | 🟡 Sprint 77 | 77 | ~200 | JARVIS C# + BeFree |
| 315.5 | **Sessionless Thread** — Modo de conversa contínua sem reset de contexto. JARVIS mantém thread ativa entre comandos, sem perder estado | 🟡 Sprint 77 | 77 | ~100 | Residuum |
| 315.6 | **Emotion Analysis (BitNet classifier)** — 7 emoções (Joy/Sadness/Anger/Fear/Surprise/Disgust/Neutral), intensidade 0-1, sarcasmo, urgência. Expert Trinity (~50KB). Ajusta tom da resposta | 🟡 Sprint 78 | 78 | ~250 | JARVIS C# |
| 315.7 | **Capability Contract + Consent Gates** — 3 níveis (Safe/Moderate/Dangerous). SkillRegistry + SafetyAgent validam antes de executar. Baseado em terminal-jarvis + Moltis | 🟡 Sprint 78 | 78 | ~200 | terminal-jarvis + Moltis |
| 315.8 | **Skill Discovery (DSPy/ACE)** — SkillObserver monitora padrões de uso, sugere novas skills. Pipeline: observe → analyze → propose → generate | 🟡 Sprint 78 | 78 | ~300 | OpenJarvis + SynkraAI |
| 315.9 | **ADE Pipeline (Spec→Execute→Review→Recover)** — 4 fases: Specification (SDD-style), Execution (AgentScheduler), Review (verification contracts), Recover (self-heal fallback) | 🟡 Sprint 78 | 78 | ~200 | SynkraAI |
| 315.10 | **Semantic Cache (5-tier routing)** — Tier 1: SHA-256 exact \| Tier 2: embedding similarity >0.95 \| Tier 3: pattern (intent+entities) \| Tier 4: fallback round-robin \| Tier 5: cold start. 97.5% reduction (NabaOS) | 🟡 Sprint 78 | 78 | ~150 | NabaOS |
| 315.11 | **Persona Pipeline (16 stages)** — SafetyCheck→StopHandler→Converse→SkillHigh→Persona→SkillMedium→CommonQA→FallbackLow→Reflexive→Dreaming→EgoUpdate→SessionCompress→NotificationGate→Heartbeat→BabelIndex→AuditLog. OVOS-inspired | 🟡 Sprint 78 | 78 | ~100 | OVOS |
| 315.12 | **Dreaming/Consolidation** — CronAgent noturno: agrupa memórias similares, gera insights sintéticos (BitNet), remove contradições, promove frequentes para LTM, Ebbinghaus decay | 🟡 Sprint 79 | 79 | ~200 | mem0-supabase Layer 6 |
| 315.13 | **Ego Layer (self-model)** — Meta-cognitive identity synthesis. JARVIS sabe o que sabe/não sabe. Confidence tracking por domínio. Atualiza auto-modelo por interação. Twin Agents paper | 🟡 Sprint 79 | 79 | ~250 | mem0-supabase Layer 12 |
| 315.14 | **Proactive Heartbeats** — JARVIS inicia conversa proativamente: "Disk 90% full, sir." Baseado em eventos do EventBus + regras do NotificationGate | 🟡 Sprint 79 | 79 | ~100 | mem0-supabase Layer 12 |
| 315.15 | **Tool-State Save Game** — Snapshot do estado das ferramentas/agentes antes de executar skill. Rollback automático se skill falhar | 🟡 Sprint 79 | 79 | ~100 | mem0-supabase Layer 9 |
| 315.16 | **Auto-Skill Generation** — Cratos-inspired: observa interações, gera SKILL.md com padrões detectados. Pipeline: watch → pattern → propose → generate → register | 🟡 Sprint 79 | 79 | ~150 | Cratos |
| 315.17 | **Babel-Index (entropy monitoring)** — Monitora entropia, contradiction rate, staleness index da memória. Prevê colapso de coerência → dispara consolidação automática | 🟡 Sprint 79 | 79 | ~100 | NEOTH |
| 315.18 | **Fail-Closed Safety Invariant** — SafetyAgent sempre nega por padrão. Toda skill precisa autorização explícita. SMT-proof (Z3-style): 4 invariants: process separation, pre-action, fail-closed, signed evidence | 🟡 Sprint 80 | 80 | ~200 | Unfireable Safety Kernel paper |
| 315.19 | **Merkle Audit Trail (Ed25519 signed)** — Chain de audit entries: tick, agent, action, payload_hash, prev_hash, Ed25519 signature. Verificação de integridade a cada entry. Ring buffer 4096 | 🟡 Sprint 80 | 80 | ~200 | PunkGo + NEOTH |
| 315.20 | **Fluid Persona (context-adaptive)** — Persona muda por contexto: urgente→preciso, triste→empático, irritado→formal. 3 eixos: persona metafórica (coach/tutor/tool) + intensidade (low/med/high) + traits do usuário | 🟡 Sprint 80 | 80 | ~100 | Fluid Personality paper |
| 315.21 | **Pocket TTS Integration** (TTS) — Via sherpa-onnx (Rust bindings). PocketTTS engine 100M params, CPU-native, ~200ms latência, voice cloning, 6 idiomas. Alternativa: Kokoro via sherpa-onnx. Pós B-01 | 🔴 Pós B-01 | Sprint Sound | ~400 | k2-fsa/sherpa-onnx |
| 315.22 | **STT (sherpa-onnx Whisper)** — Speech-to-text via sherpa-onnx Rust bindings. Whisper engine, CPU offline. Alternativa: Vosk. Pós B-01 | 🔴 Pós B-01 | Sprint Sound | ~400 | k2-fsa/sherpa-onnx |
| 315.23 | **Wake Word (Rustpotter)** — Detecção de "Jarvis" via Rustpotter crate. Publica WAKEWORD_DETECTED no EventBus. Pós B-01 | 🔴 Pós B-01 | N+1 | ~100 | Priler/jarvis |
| 315.24 | **Audio Ring Buffer** — Circular buffer PCM lockless para DMA audio entre HDA/USB e voice pipeline. Produtor/consumidor SPSC via EventBus. | 🔴 Pós B-01 | Sprint Sound | ~100 | — |
| 315.25 | **Voice Pipeline** — Pipeline de áudio nativo Rust sobre EventBus: Mic→WakeWord→STT→Cortex→TTS→Speaker. Frame types (AudioFrame, TranscriptionFrame, TTSCommandFrame) + PipelineAgent que orquestra. Ref arquitetural: pipecat pipeline composition pattern. | 🔴 Pós B-01 | Sprint Sound | ~800 | EventBus + sherpa-onnx + rustpotter |
| 315.26 | **Multi-device sync (CRDT)** — Sincronização de memória/contexto entre dispositivos via CRDT (Automerge-style). Pós B-01 | 🔴 Pós B-01 | N+1 | ~300 | SKYNET + BeFree |
| 315.27 | **SKYNET Mesh Node** — Participa da malha SKYNET como nó L1 (PC) ou L2 (workstation). Speculative decoding distribuído. Pós B-01 | 🔴 Pós B-01 | N+2 | ~300 | SKYNET |
| 315.28 | **Gamification** — Recompensas, streaks, achievements para interação com JARVIS. OptimizerAgent + CronAgent | 🟢 Futuro | N+1 | ~200 | Jotape |

**Total:** ~5650 LOC (Sprints 77-80: ~3550, N+1: ~1600, N+2: ~500). ADR-0036.

### 1.30. Bugfix Estrutural (Sprint 45) — H3 a H12

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 268 | **H3 — APIC SVR vetor espúrio** — SVR escrito com vetor 0, causa #DE falso em interrupções espúrias | ✅ v0.45.0 | Sprint 45 | Fix: SVR = `(svr & 0xFFFFFF00) \| 0xFF \| 0x100` (vetor 255 + APIC enable) |
| 269 | **H4 — IDT sem cobertura total 0-31** — Exceções #DE, #UD, #NM, #MC, #XM, #VE, #CP sem handlers, causam Triple Fault silencioso | ✅ v0.45.0 | Sprint 45 | Fix: 32 handlers nomeados com dump textual de InterruptStackFrame |
| 270 | **H5 — PIC EOI sem duplo para escravo** — Interrupções do PIC escravo (vetores 40-47) não recebem EQI no 0xA0 | ✅ v0.45.0 | Sprint 45 | Fix: `send_eoi(vector)` envia para 0x20 e 0xA0 se vector >= 40 |
| 271 | **H11 — PCI multi-function sem verificação** — Scanner força funções 1-7 em dispositivos single-function, desperdiçando ciclos | ✅ v0.45.0 | Sprint 45 | Fix: `header_type` (offset 0x0E) bit 7 verificado antes de escanear |
| 272 | **H12 — IOAPIC RTEs desmascaradas** — Ruído elétrico em linhas não usadas gera interrupções fantasmas | ✅ v0.45.0 | Sprint 45 | Fix: Todas RTEs inicializadas com bit 16 = MASK |
| 273 | **VirtIO-GPU driver manual** — PCI capabilities + MMIO mapping + control queue. GET_DISPLAY_INFO pendente (resposta 0x0) | 🟡 Sprint 45 (⚠️ 95%) | Sprint 45 | Sem zerocopy-derive. Feature negotiation, queue enable, ring layout corrigidos. Falta response. |



### 1.24. Life OS / Personal OS Ecosystem (20 repos Tier 1)

| # | Item | Classificação | Sprint | Motivação |
|---|---|---|---|---|
| 177 | **7D Spectrum Graph Leve** — Grafo de conhecimento 7-dimensional para EventBus; Edge Prophecy (Jaccard similarity) p/ predizer conexões. Substitui BTreeMap por `Vec<(u64,u64,u8,u64)>`. Dá memória associativa ao Hermes | ✅ Imediata | Sprint 24 | ~200 LOC sobre EventBus existente. Spectrum Graph do PrismOS-AI portado para no_std. |
| 178 | **Runtime SDD (Structured Decision Document)** — Antes de executar skill, Hermes mostra goal/context/plan/expected outcome/rollback. Reasoning visível no VGA | ✅ Imediata | Sprint 24 | ~80 LOC no intent_router_daemon. Alinha com chain-of-thought determinístico. |
| 179 | **File System as Context** — Em vez de RAG embedding, usa filesystem como índice de conhecimento. CDC Rabin chunking + grep-like scan sobre arquivos `.bitmem` | 🟡 Baixa | Sprint 27 | ~300 LOC sobre CDC Rabin (#165). Requer VirtIO-blk (Sprint 24+). Bridge p/ SFS (Layer 2). |
| 180 | **DA Identity Layer** — Persona persistente do Hermes (SOUL.md/IDENTITY.md/TELOS.md). Hermes atual é stateless; identidade dá voz/personalidade consistentes | 🟡 Baixa | Sprint 27 | ~100 LOC. Parser markdown mínimo. Identidade hardcoded como fallback sem disco. |
| 181 | **Temporal Knowledge Graph** — Grafo temporal com validity windows e contradiction detection 97% LongMemEval-S. Extensão do Spectrum Graph (#177) com `(t_start, t_end)` por aresta | 🟠 Média | Sprint 28 | ~500 LOC sobre #177. taOSmd port: archive append-only, detecção de sobreposição temporal. |
| 182 | **Proactive Push / Heartbeat Scheduler** — Tentáculos autônomos que monitoram fontes e fazem push proativo. NeuralExecutor ganha agendamento push baseado em LAPIC timer ticks | 🟠 Média | Sprint 28 | ~400 LOC. Dedup hash + priority queue. Push externo requer Network Sprint. |
| 183 | **WASM Sandbox para Skills** — Sandbox via paging (não wasmtime): skill executa em página separada com PTE NX + shared memory controlado. Fuel metering + auto-rollback | 🟠 Média | Sprint 28 | ~800 LOC. Alternativa no_std ao wasmtime. Depende de #172 MCP Server. |
| 184 | **Intent Transparency** — Após cada resposta Hermes, mostrar query type, reasoning band, confidence, alternatives. MLP argmax era silencioso — agora é visível | 🟡 Baixa | Sprint 27 | ~200 LOC. Log estruturado no intent_router_daemon. Sem dependências. |
| 185 | **Energy / Circadian Tracking** — Usuário reporta energia (1-10) via `/energy`. Scheduler casa tarefas com capacidade real. 15-25 min task chunks, dopamine hooks | 🟡 Baixa | Sprint 27 | ~150 LOC como skill. Requer #157 Usage Pattern Analyzer p/ correlação. |
| 186 | **AppForge / App Store** — Plataforma de apps com catalog, instalação one-click, hardware-aware filtering. Store backend sobre MCP + SFS | 🔴 Alta | Sprint 29+ | ~1500 LOC total. Frontend inviável sem framebuffer (💰 Sponsor). |
| 187 | **Multi-User / Multi-Persona** — Vários usuários com memória isolada, trust tiers diferentes. Certificate Authority + Dual-LLM split (quarentena/planejamento) | 🔴 Alta | Sprint 29+ | ~600 LOC. Redesign do scheduler. PerCpu → PerUser. TrustCache multicamada. |
| 188 | **Visual Workflow Builder** — Drag-and-drop pipeline DAG (Trigger/Tool/Agent/Condition/Loop/Gate). AI workflow designer via chat | ⏳ Futuro | — | Requer framebuffer VESA + mouse. CLI ASCII DAG perde valor visual. |
| 189 | **Federated Cluster / P2P Workers** — Mesh de AI compute (gaming PC, Mac, RPi, Android). Auto-descoberta, pareamento PIN, checkpoint distribuído | ⏳ Futuro | — | Depende de toda stack de rede + scheduler distribuído + WASM remoto. |
| 190 | **Algorithm loop de 7 fases no Hermes** — THINK antes de agir (carrega contexto adicional), VERIFY depois (confirma ISC). Não só MLP→argmax→skill. THINK consulta KNOWLEDGE graph, VERIFY checa resultado contra critério | 🟡 Baixa | Sprint 27 | ~300 LOC. PAI Algorithm v6.3.0: OBSERVE→THINK→PLAN→BUILD→EXECUTE→VERIFY→LEARN. Ref: `docs/architecture/0021-life-os-ecosystem-analysis.md#2-o-algorithm-v630` |
| 191 | **Council skill** — Antes de decisão ambígua, 3 vozes (OtImista, Cético, Pragmático) votam. Argmax vence. Melhora qualidade de intent classification em bordas | 🟡 Baixa | Sprint 27 | ~150 LOC como skill. PAI Council skill: multiple perspective simulation. Ref: `0021-life-os-ecosystem-analysis.md#skills-como-deterministic-units` |
| 192 | **Loop Detection no NeuralExecutor** — Monitora repetição de `AgentTask.id`, força break/log warning após N≥3 repetições sem progresso | ✅ Imediata | Sprint 24 | ~80 LOC. PAI Loop skill detecta repeat patterns em tool calls. Ref: `0021-life-os-ecosystem-analysis.md#skills-como-deterministic-units` |
| 193 | **Bitter Pill Engineering** — Força etapas obrigatórias (cargo check antes de deploy, test antes de merge) mesmo que usuário peça atalho. Hermes recusa pular passos críticos | 🟡 Baixa | Sprint 27 | ~100 LOC no intent_router. PAI BitterPillEngineering skill. Ref: `0021-life-os-ecosystem-analysis.md#skills-como-deterministic-units` |
| 194 | **ISA como formato de sprint** — Cada sprint tem ISA (Ideal State Artifact) com ISCs verificáveis. Substitui verificação ad-hoc "cargo check + QEMU boot" por critérios binários formais | 🟡 Baixa | Sprint 27 | ~200 LOC + docs. PAI ISA: 12 seções (Problem→Vision→Goal→Criteria→Test Strategy→Verification). Ref: `0021-life-os-ecosystem-analysis.md#3-o-isa-ideal-state-artifact` |
| 195 | **Hermes Rating (Satisfaction Capture)** — Após cada resposta, usuário dá 👍/👎 via `/rate`. Alimenta TrustCache weight adjustment + Success Engine feedback loop | 🟡 Baixa | Sprint 27 | ~100 LOC no hermes_console_daemon. PAI SatisfactionCapture.hook.ts (18 KB). Ref: `0021-life-os-ecosystem-analysis.md#4-o-sistema-de-hooks-37-hooks` |
| 196 | **Evals skill** — Avalia respostas do Hermes contra critérios predefinidos antes de mostrar ao usuário. Se confidence < 0.7, re-executa com mais contexto | 🟠 Média | Sprint 28 | ~300 LOC. PAI Evals skill. Ref: `0021-life-os-ecosystem-analysis.md#skills-como-deterministic-units` |
| 197 | **Container Zones via TrustCache** — Trust token define quais regiões de memória/skills a skill pode acessar. Implementa containment zones do PAI em bare-metal | 🟠 Média | Sprint 28 | ~400 LOC sobre #176 Ed25519. PAI ContainmentGuard.hook.ts + containment-zones.ts. Ref: `0021-life-os-ecosystem-analysis.md#10-security--containment` |
| 198 | **Boot-time security policy (.pai-protected.json equivalente)** — Banco de regexes de segurança compilado no boot. Skills são validadas contra patterns antes de executar | 🟡 Baixa | Sprint 27 | ~100 LOC. PAI .pai-protected.json: 17 categorias, 100+ regexes. Ref: `0021-life-os-ecosystem-analysis.md#10-security--containment` |
| 199 | **IterationBudget com Grace Cycle** — Max poll cycles per AgentTask, um grace cycle extra após exhaustion para finalização limpa | ✅ Imediata | Sprint 24 | ~50 LOC. Hermes Agent `agent/iteration_budget.py`. Ref: ADR-0022. |
| 200 | **Skill Metadata Frontmatter** — `version`, `author`, `description` (≤60 chars), `tags` na Skill trait. Routing constraint de 60 chars para caber em linha VGA 80-col | ✅ Imediata | Sprint 24 | ~80 LOC. Hermes Agent `/learn` + OpenClaw marketplace. Ref: ADR-0022. |
| 201 | **Audit Ring Buffer** — Ring buffer fixo de eventos de auditoria no executor (task_id, tool_name, outcome, LAPIC tick). Expor via syscall | ✅ Imediata | Sprint 24 | ~80 LOC. GitAgent `.gitagent/audit.jsonl`. Ref: ADR-0022. |
| 202 | **Agent Identity Awakening Mode** — Duas personalidades Hermes: "Awakening" (primeiro boot) e "Established" (memória presente). MLP weights diferentes selecionados via HAS_MEMORY flag | ✅ Imediata | Sprint 24 | ~50 LOC. GitAgent `src/context.ts` + PAI SOUL.md. Ref: ADR-0022. |
| 203 | **Context Fencing + Streaming Scrubber** — Byte-level type markers no EventBus (`[UserInput]`, `[HardwareTelemetry]`). State machine scrubber remove na recepção | 🟡 Baixa | Sprint 27 | ~150 LOC. Hermes Agent `StreamingContextScrubber`. Ref: ADR-0022. |
| 204 | **Heartbeat Idle Gate com Open Work Digest** — Watchdog detecta idle vs active. Tick é idle só quando sem reminders E sem subagentes ativos | 🟡 Baixa | Sprint 27 | ~200 LOC. Lethe `scheduler/brainstem.rs`. Ref: ADR-0022. |
| 205 | **ProactiveRateLimiter com Deferred Outbox** — Rolling window (24h ticks) + cooldown. Outbox segura 1 msg deferida, msg nova superseded, stale >6h descartada | 🟡 Baixa | Sprint 27 | ~150 LOC. Lethe `scheduler/proactive.rs`. Ref: ADR-0022. |
| 206 | **Lifecycle Hooks via Pre/Post Poll Callbacks** — HookRegistry com slots fixos de function pointers. Hooks retornam Allow/Block/Modify | 🟡 Baixa | Sprint 27 | ~200 LOC. GitAgent `hooks/hooks.yaml`. Ref: ADR-0022. |
| 207 | **MemoryProvider + MemoryManager Trait** — Trait pluggável sobre MHI tiers. MemoryManager orquestra prefetch/sync em background via executor cooperativo | 🟠 Média | Sprint 28 | ~400 LOC. Hermes Agent `agent/memory_manager.py` + Lethe `memory/store.rs`. Ref: ADR-0022. |
| 208 | **Capability-Based Tool Permission Model** — TrustCache verifica (token, skill, tier) antes da execução. Skills declaram tiers de memória + tokens autorizados | 🟠 Média | Sprint 28 | ~400 LOC. Hermes Agent `acp_adapter/permissions.py` + Ironclaw WASM sandbox. Ref: ADR-0022. |
| 209 | **Actor Registry com Permission Model** — Registry de subagentes: spawn/terminate/kill, can_message() hierarchical, task state machine (Planned→Running→Blocked→Done), open_work tracking | 🟠 Média | Sprint 28 | ~500 LOC. Lethe `actor/registry.rs` (~46KB). Kameo-inspired. Ref: ADR-0022. |
| 210 | **Subagent Crash-Recovery Persistence** — Estado de subagentes persistido em região de memória reservada. Boot walk + rehidrata. Serialização postcard/bincode | 🔴 Alta | Sprint 29+ | ~600 LOC. Lethe `actor/persistence.rs` + Ironclaw state. Ref: ADR-0022. |
| 211 | **ComputeBackend Trait** — Abstrai 3 rings (NPU/GPU/CPU) atrás de trait. Intent router chama `COMPUTE_BACKEND.execute()` sem saber qual ring | 🔴 Alta | Sprint 29+ | ~800 LOC. ZeroClaw Peripheral trait + Ironclaw WASM/Docker. Ref: ADR-0022. |
| 212 | **Plugin System via Loadable Page Ranges** — Plugin = região page-aligned em RAM com PluginDescriptor + tools + hooks. Walk linked list de regiões | ⏳ Futuro | — | Requer SFS Layer 2. GitAgent `plugins/<id>/plugin.yaml`. Ref: ADR-0022. |
| 213 | **WASM + Docker Sandbox para Skills** — Ferramentas em WASM containers isolados com capability-based permissions + rate limiting + resource limits | ⏳ Futuro | — | Requer #183 WASM Sandbox + Network Sprint. ZeroClaw/Ironclaw. Ref: ADR-0022. |

**ADR-0021:** `docs/architecture/0021-life-os-ecosystem-analysis.md` — Análise de 20 repos Tier 1 (Life OS / Personal OS). 13 ideias extraídas + 9 do PAI deep-dive (#190-198). Total: 22 ideias.
**ADR-0022:** `docs/architecture/0022-personal-ai-assistant-ecosystem-analysis.md` — Análise de 21 repos Tier 2 (Personal AI Assistants). 15 ideias extraídas (#199-213). Deep-dives: Lethe (Rust), Hermes Agent (202k ★), GitAgent, Rust ecosystem (ZeroClaw, Ironclaw).

### 1.26. Tier 3 — Memory Systems & Second Brain (2026-06-25)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 214 | **SHA-256 Memory Dedup** — Port SHA-256 dedup (5min sliding window) to no_std; prevent duplicate EventBus messages and TrustCache entries | 🟡 Sprint 23 | Sprint 23 | agentmemory dedup.ts. ~100 LOC. Sem novas deps (SHA-256 via `sha2` ou manual). |
| 215 | **Privacy Filter for Memory** — Strip API keys, secrets, `<private>` tagged content before memory storage; regex + pattern matching | 🟡 Sprint 23 | Sprint 23 | agentmemory privacy.ts. ~80 LOC. Proteção zero-trust para TrustCache. |
| 216 | **Memory TTL/Eviction** — Auto-evict stale memory entries based on configurable TTL; EvictionPolicy enum (TimeToLive, ImportanceRank, AccessFrequency) | 🟡 Sprint 23 | Sprint 23 | agentmemory evict.ts, auto-forget.ts. ~150 LOC. |
| 217 | **Hybrid Search (BM25 + MLP)** — RRF fusion for Hermes intent routing; combine MLP intent classifier with BM25 keyword fallback; Reciprocal Rank Fusion with k=60 | 🟡 Sprint 24 | Sprint 24 | agentmemory search.ts. ~200 LOC. BM25 já implementável em no_std. |
| 218 | **4-Tier Memory Consolidation** — Working→Episodic→Semantic→Procedural pipeline in Hermes daemon; EventBus topics for each tier transition | 🟡 Sprint 24 | Sprint 24 | agentmemory consolidation-pipeline.ts. ~400 LOC. Aproveita EventBus. |
| 219 | **Ebbinghaus Decay for TrustCache** — strength = importance × e^(-λ_eff × days) × (1 + recall_count × 0.2); λ_eff = 0.16 × (1 - importance × 0.8) | 🟡 Sprint 24 | Sprint 24 | nexo + YourMemory. ~120 LOC. libm expf. |
| 220 | **Session Replay** — Record Hermes daemon turns as atomic events; replay with speed control for debugging | 🟡 Sprint 24 | Sprint 24 | agentmemory replay.ts. ~200 LOC. |
| 221 | **Knowledge Graph on MHI** — Entity extraction + BFS traversal over semantic file system; GraphNode/GraphEdge with bi-temporal timestamps | 🟡 Sprint 25 | Sprint 25 | agentmemory graph.ts, temporal-graph.ts. ~500 LOC. |
| 222 | **Metacognitive Guard** — Before each Hermes skill execution, check TrustCache for past mistakes; inject known error patterns, schemas, blocking rules | 🟡 Sprint 25 | Sprint 25 | nexo guard system. ~300 LOC. |
| 223 | **Draft→Review→Merge Memory** — Memory changes staged as drafts on EventBus; Hermes daemon reviews before merge; approval/rejection workflow | 🟡 Sprint 25 | Sprint 25 | novyx-vault workflow. ~350 LOC. |
| 224 | **Atkinson-Shiffrin Cognitive Memory** — Full 3-tier memory: Sensory Register (48h) → STM (7d) → LTM (permanent, semantic-indexed); promotion on access frequency | 🟡 Sprint 26 | Sprint 26+ | nexo brain architecture. ~800 LOC. Maior item individual do Tier 3. |
| 225 | **Bi-temporal Knowledge Graph for MHI** — Track MHI tiers across time; append-only, old facts superseded never deleted | 🟡 Sprint 26 | Sprint 26+ | MemoryOS + agentmemory temporal-graph.ts. ~600 LOC. |
| 226 | **Team/Shared Memory** — Namespaced memory across neural-os-core instances; shared + private isolation per agent ring | ⏳ Pós-MVP | — | agentmemory team.ts. ~400 LOC. |
| 227 | **Memory Git Snapshots** — Version, rollback, diff memory state; SHA-256 commit chain for TrustCache and Hermes memory | ⏳ Pós-MVP | — | agentmemory snapshot.ts. ~500 LOC. |

**ADR-0023:** `docs/architecture/0023-memory-systems-second-brain-analysis.md` — Análise de 14 repos Tier 3 (Memory Systems & Second Brain). 14 ideias extraídas (#214-227). Deep-dives: agentmemory (24k ★, 60+ source files), nexo (Atkinson-Shiffrin + Ebbinghaus), novyx-vault (Draft→Review→Merge + Ghost Connections), MemoryOS (bi-temporal KG).

### Tier 4 — Agent Frameworks (#228-#249)

Ideias portadas de 6 repos Tier 4: Cline (63.9k ★), Agent Zero (18.2k ★), Microsoft Agent Framework (11.7k ★), OpenHands (77k ★), opencode/Crush (13.1k ★), open-agent (new).

| # | Item | Destino | Target | Fonte + Detalhes |
|---|---|---|---|---|
| 228 | **Tool Policy Registry** — Extend SkillRegistry with `{ enabled: bool, autoApprove: bool }` per tool, wildcard fallback, `validate_tool_call()` denies blocked tools | ✅ Sprint 23 | Sprint 23 | Implementado em skill-registry (ToolPolicy + set_policy + is_enabled + is_auto_approve). Cline agent-runtime.ts. |
| 229 | **Usage Tracker** — Lightweight token/metrics accumulator for hardware_context_tensor(): track input_chars, output_chars, cache_hits, iterations per daemon | ✅ Sprint 23 | Sprint 23 | Implementado em usage.rs (UsageTracker + record_call + snapshot + to_metrics_tensor). Cline AgentUsage. |
| 230 | **Auto-Compact Hermes Buffer** — After 3+ conversation cycles without user input, trigger summarize_context skill to compact buffer into single [System Note] | ✅ Sprint 23 | Sprint 23 | Implementado em hermes.rs (ConversationTracker + needs_compact + compact, AUTO_COMPACT_THRESHOLD=3). opencode pattern. |
| 231 | **Event-Sourced Conversation State** — Replace mutable String buffer in Hermes with VecDeque<ConversationEvent { type, payload, timestamp }> immutable event log | ✅ Sprint 23 | Sprint 23 | Implementado em conversation.rs (EventLog + events_since + last_n + summarize + ContextCompacted). OpenHands typed events. |
| 232 | **Cron Scheduler** — ScheduleService with CronSpec { prompt, schedule, enabled, model_id, tool_policies }, periodic poll via LAPIC timer, markdown report writer | 🟡 Sprint 24 | Sprint 24 | Cline CronRunner + SqliteCronStore. ~350 LOC. |
| 233 | **Session Checkpoint/MHI Snapshot** — checkpoint() saves kernel state + MHI tier stats to reserved frames; restore() rollback on Double Fault | 🟡 Sprint 37+ | Sprint 37+ | Cline ClineCore restore(). ~200 LOC. Passo natural apos SelfHealing. |
| 234 | **Plan/Execute Modes** — Hermes dual-mode: plan mode (analysis only, no tool execution), act mode (full execution with auto-approve) | 🟡 Sprint 24 | Sprint 24 | Cline + MS Agent. ~150 LOC. |
| 235 | **Graph-Based Multi-Daemon Orchestration** — Extend EventBus with sequential / concurrent / handoff patterns for daemon chains | 🟡 Sprint 24 | Sprint 24 | MS Agent graph-based workflows. ~250 LOC. |
| 236 | **Plugin Hub / MCP Index** — McpRegistry discovers and installs skill packages from remote index with AI-driven security scanning | 🟡 Sprint 25 | Sprint 25 | Agent Zero Plugin Hub (100+ plugins). ~400 LOC. |
| 237 | **Completion Terminal Skills** — lifecycle.completes_run: bool on McpManifest, SkillRegistry auto-routes terminal response to HERMES_RESPONSE | 🟡 Sprint 25 | Sprint 25 | Cline completesRun + completionPolicy. ~120 LOC. |
| 238 | **Claim-Based Daemon Lease** — LeaseDaemon skill: acquire lease with TTL, heartbeat via LAPIC timer, release on completion; prevents double-execution | 🟡 Sprint 25 | Sprint 25 | Cline claimDueRuns + claimLeaseHeartbeat. ~200 LOC. |
| 239 | **Time Travel / Workspace Snapshot** — Capture BitmapFrameAllocator state + MHI tiers at checkpoint, restore on Double Fault or /restore command | 🔲 — | — | Postergado apos SelfHealing (Sprint 37+). |
| 240 | **Context Compaction with Ebbinghaus Decay** — Conversation events decay via Ebbinghaus formula; summarize_context when budget < 20% remaining | 🟡 Sprint 25 | Sprint 25 | Cline + Tier 3 Ebbinghaus. ~150 LOC. |
| 241 | **OpenTelemetry-Like Observability** — Trace event logging via serial output, structured log format, per-daemon latency/usage metrics | 🟡 Sprint 26+ | Sprint 26+ | MS Agent OpenTelemetry. ~500 LOC. |
| 242 | **AI-Driven Security Scan for Skills** — Use Intent MLP to classify skill behavior as safe/suspicious/malicious before execution | 🟡 Sprint 26+ | Sprint 26+ | Agent Zero AI-driven plugin security scan. ~350 LOC. |
| 243 | **Hub Discovery / Multi-Instance Board** — EventBus instances discover each other via shared MHI memory region, coordinate task assignment | 🟡 Sprint 26+ | Sprint 26+ | Cline cline-hub Kanban multi-agent board. ~400 LOC. |
| 244 | **Human-in-the-Loop Approval** — request_tool_approval() blocks tool execution until keyboard confirmation via /approve or /deny | 🟡 Sprint 26+ | Sprint 26+ | MS Agent + Cline tool approval. ~250 LOC. |
| 245 | **Remote Agent Execution** — Hub daemon on separate machine, EventBus over VirtIO-net TCP | ⏳ Pós-MVP | — | Cline hub discovery. ~800 LOC. |
| 246 | **Skill Marketplace** — Signed, versioned MCP packages published to remote registry; Hermes /install <pkg> | ⏳ Pós-MVP | — | Agent Zero Plugin Index. ~600 LOC. |
| 247 | **Automatic Context Compaction Agent** — Dedicated daemon that monitors conversation budget and proactively compacts | ⏳ Pós-MVP | — | Cline + opencode. ~300 LOC. |
| 248 | **Docker Sandbox** — Containerized execution environment for skills | ❌ Descartado | — | Incompatível com bare-metal no_std (sem container runtime em Ring 0-2). |
| 249 | **Python/.NET Runtime** — Multi-language agent runtime support | ❌ Descartado | — | Barreira de linguagem; Python requer OS que neural-os-core substitui. |

**ADR-0024:** `docs/architecture/0024-agent-frameworks-analysis.md` — Análise de 6 repos Tier 4 (Agent Frameworks). 22 ideias extraídas (#228-249). Deep-dive: Cline (63.9k ★, 293 releases, 6.338 commits, AgentRuntime + ClineCore + CronRunner). Ideias imediatas: Tool Policy Registry, Usage Tracker, Auto-Compact Buffer, Event-Sourced Conversation (Sprint 23, ~230 LOC total).

---

### 1.28. Agent/Skill-First Architecture — A Grande Virada

**Bloco 11 — Sprints 39-42 (consolidado)**
**Data:** 2026-06-26
**Status:** ⚡ Paradigma fundamental. Sprints 39-40 implementados (SkillLoader, Agent trait, AgentRegistry, SystemAgent). Sprints 41-42 continuam migração.

#### O Princípio

> **Tudo no Neural OS Hermes é um Agente ou uma Skill.** Não existem "tasks", "services", "drivers" ou "daemons" como conceitos separados. Cada entidade no sistema é um Agente com identidade, manifesto, ciclo de vida e capacidades declaradas. Habilidades (Skills) são a interface de requisição-resposta dos Agentes.

#### Por que esta virada?

O projeto começou com 8 `async fn` hardcoded no executor (`system_daemon`, `hw_bridge_daemon`, `input_daemon`, etc.). Cada sprint adicionava mais uma task. Skills (EchoSkill, SystemStatusSkill) eram uma preocupação separada. Drivers (rtl8139, xhci) estavam fora do sistema de skills.

Isso criou **3 regimes ontológicos diferentes** — tasks, skills, drivers — cada um com suas próprias regras, apesar de todos serem, na prática, agentes.

A virada **Agent/Skill-First** unifica tudo:

| Antes | Depois |
|---|---|
| 8 `async fn` tasks | 8+ Agent instances no AgentRegistry |
| SkillRegistry separado | SkillRegistry = catálogo de skills dos agentes |
| Drivers mod.rs avulsos | Driver Agents com `HardwareDriver` capability |
| Boot linear de funções | Boot = chain de agent activations |
| Executor coopera loop | AgentScheduler coordena agentes |
| Trust por token | Trust por agente + token + capability |
| `/add_skill` via LLM | LLM cria Agent + Skills atomically |

#### O Agente

```rust
pub enum AgentKind {
    System,      // init, monitor, lifecycle
    Driver,      // hardware driver
    Inference,   // LLM, MLP
    Router,      // intent routing
    Console,     // I/O (keyboard, VGA, serial)
    Network,     // network stack poll
    Skill,       // pure skill agent (no persistence)
}

pub struct AgentManifest {
    pub name: &'static str,
    pub kind: AgentKind,
    pub capabilities: &'static [Capability],
    pub auto_start: bool,         // ativa no boot
    pub persist: bool,            // respawn on crash
    pub schedule: ScheduleKind,   // PollEveryNTicks, EventDriven, Continuous
    pub trust_tokens: &'static [u64],
}
```

#### As Skills

Cada Agente expõe zero ou mais Skills:
```rust
pub struct SkillManifest {
    pub name: &'static str,
    pub description: &'static str,
    pub agent: &'static str,         // agente dono
    pub required_tokens: &'static [u64],
    pub completes_run: bool,         // skill terminal?
    pub instructions: &'static str,  // para LLM routing
}
```

#### Boot = Agent Activation Chain

```
cargo run → bootloader → kernel_main
  ├─ [Agent] vga_buffer::init()           → ConsoleAgent (VGA+Serial)
  ├─ [Agent] interrupts::init_idt()       → SystemAgent (GDT+IDT+TSS)
  ├─ [Agent] memory::init_memory()        → MemoryAgent (PageTable)
  ├─ [Agent] allocator::init_heap()       → MemoryAgent (Heap)
  ├─ [Agent] simd::enable_simd()          → SystemAgent (CR0/CR4)
  ├─ [Agent] pci::init_pci()              → PCIAgent (PCI scan)
  ├─ [Agent] acpi::init_acpi()            → ACPIAgent (RSDP+MADT)
  ├─ [Agent] smp::init_smp()              → SMPAgent (AP boot)
  ├─ [Agent] inventory::hardware_scan()   → HwDiscoverAgent
  │   ├── detecta RTL8139 → NetDriverAgent
  │   └── detecta xHCI   → UsbDriverAgent
  ├─ [Agent] cortex::TransformerModel     → CortexAgent (LLM)
  ├─ [Agent] HermesAgent (intent + console + input)
  └─ AgentScheduler::run()
       └─ Cada tick: poll agents, route events, respawn mortos
```

#### Reclassification Grid (Implementado → Migrar)

| ID Antigo | Nome | Vira Agente | Tipo | Novo ID |
|---|---|---|---|---|
| task #1 | system_daemon | SystemAgent | System | A-001 |
| task #2 | hardware_monitor | MonitorAgent | System | A-002 |
| task #3 | hw_bridge | HwBridgeAgent | Router | A-003 |
| task #4 | network_agent | NetAgent | Network | A-004 |
| task #5 | input_daemon | InputAgent | Console | A-005 |
| task #6 | cortex_llm | CortexAgent | Inference | A-006 |
| task #7 | intent_router | HermesAgent | Router | A-007 |
| task #8 | hermes_console | ConsoleAgent | Console | A-008 |
| — | rtl8139 driver | NetDriverAgent | Driver | A-009 |
| — | xhci driver | UsbDriverAgent | Driver | A-010 |
| — | self_heal | SelfHealAgent | System | A-011 |
| — | memory/allocator | MemoryAgent | System | A-012 |
| — | pci, acpi, apic | PlatformAgent | System | A-013 |
| — | smp | SMPAgent | System | A-014 |
| — | trust cache | TrustAgent | System | A-015 |
| — | skill_loader | SkillManagerAgent | System | A-016 |

#### AgentRegistry + Scheduler

O `AgentRegistry` substitui o `SkillRegistry` atual como catálogo central:

```rust
pub struct AgentRegistry {
    agents: Vec<AgentInstance>,        // todos agentes registrados
    skills: BTreeMap<String, SkillRef>, // skills catalogadas por nome
    scheduler: AgentScheduler,
}

pub struct AgentInstance {
    manifest: AgentManifest,
    state: AgentState,  // Inactive, Active, Blocked, Crashed
    tick_budget: u64,
    last_poll: u64,
}
```

O `AgentScheduler` substitui o `NeuralExecutor`:
- Cada agente declara seu ScheduleKind
- A cada tick, scheduler pergunta: "este agente deve rodar agora?"
- Se sim, chama `agent.tick()` (que pode fazer polling de eventos, processar I/O, etc.)
- Se crash, verifica `persist` → respawn automático

#### | # | Item | Destino | Target | Motivação |

| A-001 | **Agent trait + AgentManifest** — nova trait unificada que substitui as 8 async fn avulsas. `Agent { manifest, tick(), skills() }` | 🟡 Sprint 40+ | Sprint 40 | Coração da virada. Cada task vira um Agent. |
| A-002 | **AgentRegistry** — catálogo central de todos agentes. Substitui parcialmente SkillRegistry (skills viram sub-recursos dos agentes) | 🟡 Sprint 40+ | Sprint 40 | Registry de agentes vivos + skills indexadas. |
| A-003 | **AgentScheduler** — substitui NeuralExecutor. Poll agents pelo schedule declarado, não por ordem fixa. Suporta: tick-based, event-driven, continuous | 🟡 Sprint 40+ | Sprint 40 | Executor v2. |
| A-004 | **Reclassification: 8 tasks → 8 agents** — system_daemon→SystemAgent, hw_bridge→HwBridgeAgent, input→InputAgent, cortex_llm→CortexAgent, intent_router→HermesAgent, hermes_console→ConsoleAgent, monitor→MonitorAgent, network→NetAgent | 🟡 Sprint 40+ | Sprint 40 | Refactor do main.rs. |
| A-005 | **Driver Agents** — rtl8139, xhci, pci, acpi, apic, smp viram agents com `AgentKind::Driver` ou `AgentKind::System`. Cada um expõe skills de hardware | 🟡 Sprint 41+ | Sprint 41 | Drivers entram no ecossistema de agentes. |
| A-006 | **AgentState Machine** — Inactive→Active→Blocked→Crashed→Respawn. Scheduling consciente do estado | 🟡 Sprint 40+ | Sprint 40 | Scheduler só polla agentes Active. |
| A-007 | **Capability-Based Routing** — EventBus roteia eventos para agents que declaram `Capability` relevante. Substitui match fixo do intent_router | 🟡 Sprint 41+ | Sprint 41 | Routing dinâmico. |
| A-008 | **Agent Identity + SOUL.md** — Cada agente tem identidade persistente (SOUL.md no SFS). CortexAgent tem persona, ConsoleAgent tem voice | ⏳ Pós-MVP | Sprint 42+ | Requer SFS. |
| A-009 | **SkillManagerAgent** — Agente especializado em criar, editar, remover skills. `/add_skill` vira delegado a este agente. LLM gera skill, SkillManagerAgent registra | 🟡 Sprint 40+ | Sprint 40 | Já parcialmente implementado via PENDING_SKILL. |
| A-010 | **TrustAgent** — Centraliza toda autorização. Substitui TrustCache avulso. Agents consultam TrustAgent antes de executar skills | 🟡 Sprint 40+ | Sprint 40 | Trust como agente, não cache solto. |
| A-011 | **SelfHealAgent** — Já implementado como SelfHeal struct. Migrar para AgentKind::System com skill `recover` | 🟡 Sprint 40+ | Sprint 40 | Self-healing como agente. |
| A-012 | **MemoryAgent** — Gerencia BitmapFrameAllocator, MHI tiers, Slab. Skills: `alloc`, `dealloc`, `status` | 🟡 Sprint 41+ | Sprint 41 | Memória como agente. |
| A-013 | **Agent Schedules** — Tick-based (poll a cada N ticks), Event-driven (só acorda com evento), Continuous (roda todo tick), Idle (só responde, nunca inicia) | 🟡 Sprint 40+ | Sprint 40 | Eficiência energética. |
| A-014 | **Agent Budget + Watchdog** — Cada agente tem tick_budget por ciclo. Se excede, watchdog pausa. Implementa IterationBudget (#199) | 🟡 Sprint 40+ | Sprint 40 | Previne runaway agents. |
| A-015 | **Agent Hooks** — Pre/Post tick hooks. HookRegistry com slots fixos de function pointers. Hooks retornam Allow/Block/Modify (#206) | 🟡 Sprint 41+ | Sprint 41 | Plugin system via hooks. |
| A-016 | **Multi-Agent Orchestration** — Graph-based: sequential, concurrent, handoff entre agents. EventBus padrão MS Agent (#235) | 🟡 Sprint 41+ | Sprint 41 | Composição de agentes. |
| A-017 | **Agent as Pure Function** — Event-sourced: `f(history) -> next action`. Cada tick do agent é um ConversationEvent (#231) | 🟡 Sprint 41+ | Sprint 41 | Replay, debug, rollback. |
| A-018 | **Agent Identity Awakening** — Duas personalidades por agent: "Awakening" (first boot) e "Established" (memória presente). MLP weights diferentes (#202) | ⏳ Pós-MVP | Sprint 42+ | Requer SFS para memória. |
| A-019 | **Council Agent** — Antes de decisão ambígua, 3 sub-agentes (Otimista, Cético, Pragmático) votam. Argmax vence (#191) | ⏳ Pós-MVP | Sprint 42+ | Qualidade de decisão. |
| A-020 | **HermesAgent como Supervisor** — HermesAgent (intent_router atual) coordena os demais agentes. Decide qual agente ativar baseado na intenção do usuário | 🟡 Sprint 40+ | Sprint 40 | Já é assim na prática. Formalizar. |

**Nota:** A refatoração agent-first é **aditiva**, não disruptiva. Cada agent pode ser introduzido um por vez, mantendo compatibilidade com o executor atual. A migração começa encapsulando as 8 async fn em `Agent::tick()`, depois substitui o NeuralExecutor pelo AgentScheduler.

---

## Seção 2 — Mapa de Calor

| Fonte | Total | ✅ No MVP | 🟡 Sprint | ⏳ Pós-MVP | 💰 Sponsor | ❌ Descarte |
|---|---|---|---|---|---|---|
| USB | 15 | 0 | 0 | 15 | 0 | 0 |
| SMP/APIC | 20 | 17 | 3 | 0 | 0 | 0 |
| NPU XDNA | 10 | 1 | 1 | 0 | 8 | 0 |
| AI Detection | 10 | 9 | 0 | 1 | 0 | 0 |
| MHI | 12 | 8 | 1 | 3 | 0 | 0 |
| Periféricos | 11 | 3 | 0 | 6 | 0 | 0 |
| Rede/Network | 8 | 0 | 6 | 2 | 0 | 0 |
| Áudio/Vídeo | 6 | 0 | 0 | 4 | 0 | 2 |
| Princípios | 6 | 6 | 0 | 0 | 0 | 0 |
| Roadmap Memória | 4 | 2 | 0 | 2 | 0 | 0 |
| Roadmap Kernel | 4 | 1 | 0 | 3 | 0 | 0 |
| Roadmap EventBus | 3 | 1 | 0 | 2 | 0 | 0 |
| Roadmap Skills | 3 | 1 | 0 | 2 | 0 | 0 |
| Roadmap Cognitive | 4 | 0 | 0 | 4 | 0 | 0 |
| Roadmap Timeline | 3 | 0 | 0 | 3 | 0 | 0 |
| Outras | 5 | 4 | 0 | 0 | 1 | 0 |
| Docs/ADRs | 3 | 3 | 0 | 0 | 0 | 0 |
| Neural Cortex LLM (1.18) | 16 | 0 | 14 | 2 | 0 | 0 |
| Transformer Engine (1.19) | 7 | 0 | 7 | 0 | 0 | 0 |
| Success Engine (1.20) | 4 | 0 | 0 | 4 | 0 | 0 |
| Treinamento (1.21) | 4 | 0 | 4 | 0 | 0 | 0 |
| Self-Optimization (1.22) | 7 | 0 | 5 | 2 | 0 | 0 |
| Crom Ecosystem (1.23) | 13 | 2 | 5 | 6 | 0 | 0 |
| Life OS Ecosystem (1.24) | 22 | 0 | 18 | 4 | 0 | 0 |
| Tier 2 PAI Ecosystem (1.25) | 15 | 4 | 6 | 3 | 0 | 2 |
| Tier 3 Memory Systems (1.26) | 14 | 0 | 9 | 5 | 0 | 0 |
| Tier 4 Agent Frameworks (1.27) | 22 | 0 | 17 | 3 | 0 | 2 |
| Self-Healing Kernel (Sprints 32-37) | 6 | 6 | 0 | 0 | 0 | 0 |
| Agent/Skill-First Architecture | 20 | 0 | 20 | 0 | 0 | 0 |
| Bugfix Estrutural (Sprint 45) | 6 | 5 | 1 | 0 | 0 | 0 |
| Sprint Planning (Seção 6) | 47 | 0 | 47 | 0 | 0 | 0 |
| **Total** | **336** | **73 (22%)** | **164 (49%)** | **84 (25%)** | **9 (3%)** | **6 (2%)** |

---

## Seção 3 — Hierarquia Técnica de Dependências (Pós-MVP)

Cada item ⏳ e 💰 abaixo tem seus pré-requisitos e bloqueios mapeados. A regra: **um item na camada N só começa quando todos os pré-requisitos das camadas < N estão estáveis.**

### Notação

```
Item [ID] — nome
  Pré: IDs dos pré-requisitos
  → Bloqueia: IDs que dependem deste
  Razão: por que está aqui
```

### Camada 0 — Já Existe (MVP Genesis)

```
[46-55] HardwareInventory + MLP 512→256→64→9
[56-67] MemoryHierarchy + AllocTier + alloc_by_tier(Dram)
[68-69] PCI scan CF8/CFC
[16-19] LAPIC/IOAPIC + MADT
[24-33] PerCpu + trampoline + SMP
[94] Slab Allocator
[91] Bitmap Frame Allocator
[95] Async Neural Executor
[99] EventBus + CapabilityToken
[102] Skill trait + MCP + Registry
```

Nada nesta camada depende de itens pós-MVP.

### Camada 1 — Drivers de Dispositivo (Sprint 23+)

```
[1] xHCI controller mínimo
  Pré: [68] PCI scan, [17] IOAPIC
  → Bloqueia: [2, 3, 6, 8, 9, 10, 11, 12, 13, 14, 84]
  Razão: PS/2 legacy funciona. USB = centenas de LOC, sem skill no MVP.

[2] identify_device() → VID/PID/class
  Pré: [1]
  Razão: sem xHCI, sem dispositivo USB.

[9] Nível 1 — HW Detection (xHCI sem IA)
  Pré: [1], [2]
  Razão: depende de xHCI funcionando.

[10] Nível 2 — Device Classification (MLP 7→5)
  Pré: [9], [47]
  Razão: primeiro hardware real para classificar.

[11] Nível 3 — Dynamic Interface Creation (WASM)
  Pré: [9], [103]
  Razão: requer WASM + xHCI.

[12] USB flow: desconhecido → porta desabilitada
  Pré: [1], [89] zero-autorun
  Razão: política, mas precisa de xHCI.

[13] USB flow: trust-once → auto-ON
  Pré: [1], [4] TrustCache
  Razão: TrustCache existe, falta xHCI.

[14] USB flow: usuário precisa inferir intenção
  Pré: [12]
  Razão: princípio + xHCI.

[15] "Zero autorun, zero superfície de ataque USB"
  Pré: [12, 13, 14] fluxos completos
  Razão: princípio final.

[79] UEFI framebuffer (BGRA32)
  Pré: BootInfo::framebuffer (do bootloader crate)
  → Bloqueia: [80, 81, 82]
  Razão: VGA text serve. Framebuffer é upgrade visual.

[80] Font rendering
  Pré: [79]
  Razão: sem framebuffer, sem render.

[60] AllocTier::Vram (BAR da GPU)
  Pré: [68] PCI + BAR mapeado, [79] ou driver GPU
  Razão: BAR existe, mas driver GPU não. MVP aloca em DRAM.
```

### Camada 2 — Armazenamento e Persistência (Sprint 24+)

```
[71] NVMe driver (PCI Class 01.08)
  Pré: [68] PCI, [17] IOAPIC/MSI-X, [25] PerCpu
  → Bloqueia: [61, 62, 72]
  Razão: MVP é stateless. Sem SFS, NVMe é peso morto.

[72] VirtIO-blk (PCI 1AF4:1001)
  Pré: [68] PCI, [17] IOAPIC
  → Bloqueia: [61, 62]
  Razão: alternativa NVMe. Mesma dependência SFS.

[73] VirtIO-net (PCI 1AF4:1041)
  Pré: [68] PCI, [17] IOAPIC/MSI
  Razão: MVP sem rede. Nenhuma skill precisa de rede.

[61] AllocTier::Nvme (alocar via SFS)
  Pré: [71] NVMe ou [72] VirtIO-blk + SFS
  → Bloqueia: [62]
  Razão: requer NVMe + SFS.

[62] AllocTier::Hdd (cold storage)
  Pré: [61] ou driver ATA + SFS
  Razão: cold storage = SFS sobre HDD.

[70] PCI bridges (hierarquia)
  Pré: [68] (scan cego funciona sem)
  Razão: scan bus 0..255 funciona. Bridges são refinamento.

[6] Trust Cache persistente no SFS
  Pré: [4] TrustCache (Block 5), SFS
  Razão: TrustCache existe, mas sem SFS é volátil.

[52] Atualizar MLP via WASM
  Pré: [103] WASM, [73] VirtIO-net (rede)
  Razão: requer WASM + rede.
```

### Camada 3 — VirtIO e Aceleração Gráfica (Sprint 24+)

```
[81] VirtIO-GPU 2D/3D
  Pré: [74] VirtIO-gpu básico
  Razão: VGA text é suficiente.

[82] Tensor visualization no framebuffer
  Pré: [79] framebuffer, [81] VirtIO-GPU
  Razão: depende de framebuffer + GPU.
```

### Camada 4 — Scheduler e Runtime (Sprint 24+)

```
[96] Agent Scheduler (round-robin)
  Pré: [95] Executor (existe), [24-33] SMP (>1 core)
  → Bloqueia: [97, 98, 105]
  Razão: Executor cooperativo funciona para 1-4 cores.

[97] Budget de execução (tokens_consumed)
  Pré: [96]
  Razão: sem scheduler, budget não tem onde atuar.

[98] MLP decide prioridade no scheduler
  Pré: [96], [47] MLP
  Razão: scheduler precisa existir antes.

[100] Topic enum completo
  Pré: [99] EventBus (existe)
  Razão: strings funcionam. Enum é segurança de tipo.

[101] ML-based routing no EventBus
  Pré: [99] EventBus, [47] MLP, [100] Topic enum
  Razão: inovação futura. BTreeMap resolve.
```

### Camada 5 — WASM Embedder (Sprint 25+)

```
[103] WASM embedder (wasmi no_std)
  Pré: [94] Slab, [96] Scheduler
  → Bloqueia: [8, 11, 52, 104]
  Razão: Skills Rust traits bastam. WASM é upgrade de portabilidade.

[104] Linear memory pool (256 KB/skill)
  Pré: [103]
  Razão: sem WASM, sem pool.

[8] WASM skill dispatch para USB
  Pré: [1] xHCI, [103] WASM
  Razão: USB + WASM = duplo pós-MVP.

[159] Auto-Skill Generator — cria skill WASM para workflow detectado
  Pré: [103] WASM, [157] Usage Pattern Analyzer
  Razão: requer WASM + detector de padrões de uso.
```

### Camada 6 — Memória Avançada (Sprint 23-24+)

```
[92] Huge Pages 2 MiB
  Pré: [91] BitmapAllocator (existe), page table 2 MiB mapper
  → Bloqueia: [93, 107]
  Razão: MVP não tem inferência pesada. MLP de arquitetura (37 KB) cabe em 1 página 4 KiB.

[93] Huge Pages 1 GiB
  Pré: [92], CPUID check
  → Bloqueia: [107]
  Razão: 1 GiB depende de 2 MiB + hardware real.

[107] Neural Cache (lookup table 50 ns)
  Pré: [92] Huge Pages, [105] Intent Planner
  Razão: cache de decisões só faz sentido com planner.
```

### Camada 7 — Cognitive Runtime (Fase 6)

```
[105] Intent Planner (sequência de SkillCommands)
  Pré: [96] Scheduler, [47] MLP, [103] WASM
  → Bloqueia: [106, 107]
  Razão: MVP classifica intent única. Planner multi-etapa requer scheduler + WASM.

[106] Success Engine (feedback loop online)
  Pré: [105] Planner, [47] MLP (pesos ajustáveis)
  Razão: pesquisa acadêmica. Ajuste online de pesos em no_std.

[51] Treinamento offline do MLP (10k profiles)
  Pré: [47] MLP Block 4, dataset sintético
  Razão: pesos heurísticos funcionam. Treinamento real depois.
```

### Camada 8 — Meta / MatMul-Free (Fase 7)

```
[108] MatMul-free LM (RWKV/Mamba/ternary pooling)
  Pré: [107] Neural Cache, [92] Huge Pages, [103] WASM
  Razão: futuro distante. Roadmap original já marcava Fase 7.
```

### Camada 9 — Self-Optimization & Workflow Learning (Sprint 27+)

```
[157] Usage Pattern Analyzer — LLM detecta workflow do usuário
  Pré: [126-131] Transformer Engine + Cortex Daemon (Sprint 25), [99] EventBus
  → Bloqueia: [158, 159, 161, 162, 163]
  Razão: precisa do LLM rodando para analisar padrões de intenção.

[158] Workflow Predictor — pré-carrega recursos por hora/padrão
  Pré: [157]
  → Bloqueia: [162]
  Razão: predição sem análise de padrão é chute.

[160] Dynamic Resource Scaling — MHI auto-ajuste por uso real
  Pré: [56-67] MHI tiers (existe), [157] Usage Pattern Analyzer
  Razão: MHI hoje é estático. Scaling dinâmico requer análise de uso.

[161] Self-Optimizing Scheduler — prioriza por workflow detectado
  Pré: [96] Agent Scheduler (Layer 4), [157] Usage Pattern Analyzer
  Razão: scheduler precisa existir antes de ser auto-otimizado.

[162] Workflow Profile — perfil exportável
  Pré: [157], [158] Workflow Predictor, SFS (Layer 2)
  Razão: requer análise + predição + persistência.

[163] Hardware Config Learning — SystemArchitecture evolve
  Pré: [135] LLM decide hardware arch (Sprint 26), [157]
  Razão: heurística de boot vira LLM query contínua.
```

### Camada S — Sponsor / Hardware Real

```
[36-40, 43, 45] NPU XDNA driver completo
  Pré: [68] PCI, AMD APU real, documentação XDNA
  Razão: sem hardware, sem QEMU com NPU, sem testabilidade.

[116] Port ARM/RISC-V
  Pré: nova arch target
  Razão: x86-64 é o target do MVP. ARM/RISC-V seria novo projeto.
```

### Grafo Resumido

```
MVPs ─── B1(PCI) ─── B2(SMP) ─── B3(Chat) ─── B4(MLP) ─── B5(Skills) ─── MVP
  │           │                                          │
  │           ▼                                          ▼
  │     ┌───────────┐                            ┌──────────────┐
  │     │ Layer 1   │                            │ Layer 4      │
  │     │ S23+      │                            │ S24+         │
  │     │ xHCI/FB   │                            │ Scheduler    │
  │     └─────┬─────┘                            └──────┬───────┘
  │           ▼                                        ▼
  │     ┌───────────┐                            ┌──────────────┐
  │     │ Layer 2   │                            │ Layer 5      │
  │     │ S24+      │                            │ S25+         │
  │     │ NVMe/SFS  │                            │ WASM         │
  │     └─────┬─────┘                            └──────┬───────┘
  │           ▼                                        ▼
  │     ┌───────────┐                            ┌──────────────┐
  │     │ Layer 3   │◄── [107] NCache ◄── [105]  │ Layer 7      │
  │     │ VirtIO-GPU│                            │ Planner      │
  │     └───────────┘                            └──────┬───────┘
  │           ▼                                        ▼
  │     ┌───────────┐                            ┌──────────────┐
  │     │ Layer 6   │◄────────────────────── [108]│ MatMul-Free  │
  │     │ HugePages │                            │ (Fase 7)     │
  │     └───────────┘                            └──────┬───────┘
  │           ▼                                        ▼
  │     ┌───────────┐                            ┌──────────────┐
  │     │ Layer 9   │◄── [157-163]                │ Self-Optim   │
  │     │ Workflow  │                            │ Sprint 27+   │
  │     │ Learning  │                            └──────────────┘
  │     └───────────┘
  │
  └── Layer S (Sponsor): NPU XDNA, ARM/RISC-V — sem data
```

---

## Seção 4 — Regras de Engenharia (derivadas da hierarquia)

1. **Camadas estritas:** Item na camada N só começa quando todos os pré-requisitos das camadas < N estão estáveis. Ex: NVMe (Layer 2) não começa antes de PCI (Layer 0) estar compilando e testado.

2. **Teto de camada por sprint:** Cada sprint tem um teto de camada. Sprint 23 → Layer 1. Sprint 24 → Layer 2. Sem dispersão.

3. **Sponsor = sem data:** A stack de software (PCI, APIC, SMP) estará pronta antes. NPU pode ser integrada assim que hardware chegar.

4. **Nada bloqueia o MVP:** Todo item pós-MVP tem caminho de volta para a chain principal (Block 1→5). Se MVP termina em S22, Layer 1 começa limpo em S23.

5. **Revisão contínua:** Se um pré-requisito muda de camada (ex: Huge Pages se torna essencial para MLP), o item sobe. A hierarquia é revisada a cada sprint review.

---

## Seção 6 — Sprint Planning (itens 🟡 remanescentes, consolidados por bloco)

Blocos reconsolidados após v0.47.0. Itens já implementados foram removidos. Blocos com afinidade de assunto foram fundidos.

### Bloco 12 — Network + Platform Drivers (fundido com Bloco 18)
**Foco:** MCP Server, Cron Scheduler, PCI bridges, Huge Pages, x2APIC

| Item | O que | LOC |
|---|---|---|
| #172 | MCP Server support via EventBus + JSON-RPC | ~400 |
| #236 | Plugin Hub / MCP Index com AI security scan | ~400 |
| #232 | Cron Scheduler baseado em LAPIC timer | ~350 |
| #18 | x2APIC mode (MSR-based, sem MMIO) | ~100 |
| #34 | acpi crate para parser MADT/PPTT | ~200 |
| #35 | raw-cpuid crate (features de CPU) | ~100 |
| #70 | PCI bridges (hierarquia multi-barramento) | ~100 |
| #92 | Huge Pages 2 MiB | ~200 |
| #93 | Huge Pages 1 GiB | ~100 |
| | **Total bloco** | **~1950 LOC** |

**Itens do Bloco 12 original já implementados:** DHCP (#251), ARP (#252), VirtIO-net (#73), requires_network (#122).

### Bloco 13 — Trust & Security
**Foco:** Upgrade do TrustCache, identidade criptográfica, pipeline de segurança

| Item | O que | LOC |
|---|---|---|
| #166 | Multi-mode Trust (TotalAccess/AskEveryTime/Scoped) | ~100 |
| #176 | Ed25519 Cryptographic Identity (substitui CapabilityToken) | ~300 |
| #256 | Path Confinement para Skills | ~60 |
| #257 | Mask Secrets no TrustCache | ~50 |
| #258 | Graduated Enforcement (Observe→Warn→Contain→Enforce) | ~80 |
| #259 | Posture-Aware Alerting (skills checam hardware antes) | ~40 |
| #198 | Boot-time security policy (regex patterns compilados) | ~100 |
| #260 | Event→Detector→Response Pipeline (5 detectores) | ~200 |
| | **Total bloco** | **~930 LOC** |

### Bloco 14 — Hermes Cognitive + Self-Optimization (fundido com Bloco 16)
**Foco:** HermesAgent upgrade, ReAct 7 fases, Usage Pattern Analyzer, scheduler adaptativo

| Item | O que | LOC |
|---|---|---|
| #178 | Runtime SDD (goal/context/plan/rollback antes de skill) | ~80 |
| #190 | Algorithm loop 7 fases (THINK→PLAN→BUILD→EXECUTE→VERIFY→LEARN) | ~300 |
| #191 | Council skill (3 vozes Otimista/Cético/Pragmático votam) | ~150 |
| #193 | Bitter Pill Engineering (força cargo check antes de deploy) | ~100 |
| #184 | Intent Transparency (mostrar confidence, alternatives) | ~200 |
| #203 | Context Fencing + Streaming Scrubber | ~150 |
| #180 | DA Identity Layer (SOUL.md para Hermes) | ~100 |
| #157 | Usage Pattern Analyzer (LLM detecta workflow) | ~250 |
| #158 | Workflow Predictor (pré-carrega MHI por hora/padrão) | ~200 |
| #160 | Dynamic Resource Scaling (MHI auto-ajuste) | ~200 |
| #161 | Self-Optimizing Scheduler (prioriza por workflow) | ~300 |
| #163 | Hardware Config Learning (SystemArchitecture evolve) | ~150 |
| #135 | LLM decide hardware arch (substitui heurística) | ~100 |
| #136 | LLM decide memory tier (roteia alocações) | ~100 |
| #139 | Reflex MLP threshold tuning (bypassa LLM se >0.9) | ~80 |
| | **Total bloco** | **~2460 LOC** |

### Bloco 15 — Memory Systems + Semantic Snapshot
**Foco:** Memória persistente, Ebbinghaus decay, grafo de conhecimento, CDC+XDI já base

| Item | O que | LOC |
|---|---|---|
| #214 | SHA-256 Memory Dedup (5min sliding window) | ~100 |
| #215 | Privacy Filter (stripa secrets antes de armazenar) | ~80 |
| #216 | Memory TTL/Eviction (TimeToLive, ImportanceRank) | ~150 |
| #219 | Ebbinghaus Decay para TrustCache | ~120 |
| #217 | Hybrid Search (BM25 + MLP) para intent routing | ~200 |
| #218 | 4-Tier Memory Consolidation (Working→Episodic→Semantic→Procedural) | ~400 |
| #222 | Metacognitive Guard (verifica erros passados antes de skill) | ~300 |
| #223 | Draft→Review→Merge Memory (workflow de aprovação) | ~350 |
| #224 | Atkinson-Shiffrin 3-tier (Sensory→STM→LTM) | ~800 |
| | **Total bloco** | **~2500 LOC** |

**Base já implementada:** CDC Rabin chunking (`chunker.rs`), XOR Delta (`delta.rs`), Semantic Snapshot via `SelfHeal::semantic_snapshot()`.

**✅ Implementado em v0.56.0:** SHA-256 Dedup (#214), Privacy Filter (#215), Memory TTL/Eviction (#216), Hybrid Search (#217), 4-Tier Consolidation (#218), Ebbinghaus Decay (#219), Metacognitive Guard (#222), Draft→Review→Merge (#223), Atkinson-Shiffrin (#224).

### Bloco 17 — Cortex LLM v2
**Foco:** Modelo maior, sampling, codebook compression, update HTTP

**✅ Implementado em v0.56.0:** Sampling (#148), Codebook VQ (#169), MODEL_UPDATE topic (#134).

| Item | O que | LOC |
|---|---|---|
| #148 | Sampling: argmax, top-k(3/5/10), temperature | ~80 |
| #133 | Modelo 1.5B params (distilado Llama 3.2 → ternário) | Python |
| #134 | Model update via HTTP (download .bitnet → hot-swap) | ~200 |
| #141 | 1.5B model benchmark (5-15 tok/s x86-64) | ~50 |
| #169 | Codebook Compression (VQ) para PackedTernaryTensor | ~300 |
| #170 | KV Cache Codebook (VQ no cache de atenção) | ~200 |
| | **Total bloco** | **~830 LOC + Python** |

### Bloco 21 — Foundation Quick Wins (Sprint 77)
**Itens independentes dos sprints 60/67/72 — sem dependências entre si**

| Item | Origem | O que | LOC |
|---|---|---|---|
| 60.1b | Sprint 60 | Prompt `>` interativo (Hermes aguarda input) | ~30 |
| 67.0.3 | Sprint 67 | Pre-Flight Principle (Skill::verify pré-execução) | ~80 |
| 67.2.3 | Sprint 67 | Background Fan-out (delegação automática) | ~80 |
| 72.2 | Sprint 72 | TaskSchema + JobPreconditions (schema de tarefas) | ~200 |
| 72.6 | Sprint 72 | SkillIndex + MCP Catalog (índice + catálogo) | ~150 |
| 67.2.2 | Sprint 67 | Completion Contracts (verificação pós-skill) | ~100 |
| 67.2.1 | Sprint 67 | `/learn` command (SKILL.md generator) | ~120 |
| | | **Total bloco** | **~760 LOC** |

### Bloco 22 — Agentic Evolution + Memory Systems (Sprint 78)
**Completa Agentic Evolution (72) + Memory bridge + Loaders**

| Item | Origem | O que | LOC |
|---|---|---|---|
| 72.1 | Sprint 72 | Crew + FlowTrigger (orquestração multi-agente) | ~300 |
| 72.3 | Sprint 72 | IntentCache + OutputCache (cache de intenção/saída) | ~200 |
| 72.4 | Sprint 72 | WorkflowEngine + SelfCritique (engine de workflow) | ~250 |
| 72.5 | Sprint 72 | StateGraph Scheduler (agendamento por grafo) | ~200 |
| 60.8c | Sprint 60 | migrate_to_tier() (page table manipulation MHI) | ~170 |
| 62.2 | Sprint 62 | MHI+FS Bridge (suggest_tier_for_path) | ~300 |
| #278 | GGUF Loader (modelos 1B+ params) | ~500 |
| #306 | WASM Runtime (wasmi + WASI→Skill bridge) | ~800 |
| | | **Total bloco** | **~2720 LOC** |

### Bloco 23 — LLM Infrastructure + MoE (Sprint 79)
**Model loading, router MoE, training infrastructure**

| Item | O que | LOC |
|---|---|---|
| #311 | Trinity Router (MoE — classifica intenção, roteia expert) | ~500 |
| #312 | TrainingAgent (fine-tune/transfer/full on-device) | ~500 |
| #313 | Self-Learning OS (DataCollector dos próprios logs) | ~300 |
| — | AVX2 BitNet Kernel (intrinsics SIMD) | ~150 |
| | **Total bloco** | **~1450 LOC** |

### Bloco 24 — JARVIS Core Persona (Sprint 80)
**Foco:** SOUL.md personality engine, IPW monitoring, session compression, notification gate, sessionless thread

| Item | O que | LOC |
|---|---|---|
| #315.1 | SOUL.md Personality Engine — parser + persona + adaptive tone | ~300 |
| #315.2 | IPW Monitor — RAPL MSR 0x610, tokens/watt, cache ratio | ~150 |
| #315.3 | Session Compression — Summarize/DropLowest/MergeSimilar/SegmentMeans | ~200 |
| #315.4 | Notification Gate — 4 urgency levels, rate limiting, dedup | ~200 |
| #315.5 | Sessionless Thread — conversa contínua sem reset | ~100 |
| | **Total bloco** | **~950 LOC** |

### Bloco 25 — JARVIS Emotion + Cache + Pipeline (Sprint 81)
**Foco:** Emotion analysis, capability contracts, skill discovery, semantic cache, persona pipeline

| Item | O que | LOC |
|---|---|---|
| #315.6 | Emotion Analysis — BitNet classifier 7 emoções + adjust_tone | ~250 |
| #315.7 | Capability Contract + Consent Gates — 3 níveis de risco | ~200 |
| #315.8 | Skill Discovery — DSPy/ACE pipeline observe→analyze→propose | ~300 |
| #315.9 | ADE Pipeline — Spec→Execute→Review→Recover | ~200 |
| #315.10 | Semantic Cache — 5-tier routing (NabaOS-inspired) 97.5% reduction | ~150 |
| #315.11 | Persona Pipeline — 16 stages (OVOS-inspired) | ~100 |
| | **Total bloco** | **~1200 LOC** |

### Bloco 26 — JARVIS Cognitive Memory (Sprint 82)
**Foco:** Dreaming/consolidation, ego layer, proactive heartbeats, auto-skill generation, Babel-Index, SleepCycle

| Item | O que | LOC |
|---|---|---|
| #315.12 | Dreaming/Consolidation — CronAgent noturno, memória sintética | ~200 |
| #315.13 | Ego Layer — self-model, confidence tracking, can_answer() | ~250 |
| #315.14 | Proactive Heartbeats — JARVIS inicia conversa | ~100 |
| #315.15 | Tool-State Save Game — snapshot + rollback de skills | ~100 |
| #315.16 | Auto-Skill Generation — watch→pattern→propose→generate | ~150 |
| #315.17 | Babel-Index — entropy + contradiction + staleness monitoring | ~100 |
| #314 | SleepCycle Agent — 5 fases: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT | ~780 |
| | **Total bloco** | **~1680 LOC** |

### Bloco 27 — JARVIS Security + AHCI (Sprint 83)
**Foco:** Fail-closed safety, Merkle audit trail, fluid persona, SATA driver

| Item | O que | LOC |
|---|---|---|
| #315.18 | Fail-Closed Safety Invariant — SMT-proof, 4 invariants | ~200 |
| #315.19 | Merkle Audit Trail — Ed25519 signed chain, ring buffer 4096 | ~200 |
| #315.20 | Fluid Persona — context-adaptive, coach/tutor/tool modes | ~100 |
| — | AHCI driver (SATA 6G NCQ) | ~700 |
| | **Total bloco** | **~1200 LOC** |

### Bloco 21c — GPU Foundations (Sprint 84)
**Foco:** GPU BAR mapping, secure boot (ACR/PSP/GuC), doorbell, SPSC job ring, VRAM allocator. NVIDIA/AMD/Intel.

| Item | O que | LOC |
|---|---|---|
| #67 | AllocTier::Vram — alocar no BAR da GPU | ~50 |
| #326 | GPU BAR0/BAR1 mapping UC (NVIDIA/AMD/Intel) | ~300 |
| #327 | GPU doorbell + SPSC job ring | ~400 |
| #328 | VRAM buddy allocator | ~400 |
| #352 | Secure Boot GPU — ACR/PSP/GuC pipeline | ~600 |
| #353 | GPU Compute Pipeline — submissão genérica | ~300 |
| | **Total bloco** | **~1700 LOC** |

### Bloco 21d — GPU Decode (Sprint 85)
**Foco:** BitNet decode na GPU. Prefill CPU, decode GPU.

| Item | O que | LOC |
|---|---|---|
| #329 | Agent.xpu prefill/decode split | ~400 |
| #330 | GPU matmul kernel ternário (PTX/AQL/GEN) | ~300 |
| #331 | CPU→GPU KV cache DMA | ~200 |
| #332 | XQueue preemptível (XSched, 3 níveis) | ~600 |
| | **Total bloco** | **~1500 LOC** |

### Bloco 29+ — AIOS Evolution (Sprint 85+, pós B-01)
**Tudo que depende de rede (LAN) — B-01 é o gatekeeper**

| Item | O que | LOC | Bloqueador |
|---|---|---|---|
| B-01 | RX fix (RTL8139 DHCP/RX) | ~500 | 🔴 QEMU SLiRP |
| #307 | WWW Agents (Browser, Email, RSS, Search, Download, WS) | ~2600 | 🔴 B-01 |
| #306b | Self-Update Agent (A/B slots + rollback) | ~800 | 🔴 B-01 |
| #236 | Plugin Hub + Marketplace | ~400 | 🔴 B-01 |
| #315.N+1 | Voice Pipeline (Piper TTS + Vosk STT + Wake Word + Wyoming) | ~1600 | 🔴 B-01 |
| #315.N+1b | Multi-device sync (CRDT) | ~300 | 🔴 B-01 |
| #315.N+2 | SKYNET Mesh Node | ~300 | 🔴 B-01 |
| B-29 | WiFi (Intel/Atheros/Realtek 802.11) | ~1000 | 🔴 B-01 |
| | | **~7500 LOC** | |

### Resumo dos 10 blocos futuros (Sprints 77-86)

| Bloco | Sprint | Foco | LOC estimado | Itens 🟡 |
|---|---|---|---|---|
| **21** | **77** | **Foundation Quick Wins** | **~760** | **7** |
| **22** | **78** | **Agentic Evolution** | **~2720** | **8** |
| **23** | **79** | **LLM Infrastructure + MoE** | **~1450** | **4** |
| **24** | **80** | **JARVIS Persona + IPW** | **~950** | **5** |
| **25** | **81** | **JARVIS Emotion + Cache** | **~1200** | **6** |
| **26** | **82** | **JARVIS Cognitive Memory** | **~1680** | **7** |
| **27** | **83** | **JARVIS Security + AHCI** | **~1200** | **4** |
| **21c** | **84** | **GPU Foundations** | **~1700** | **6** |
| **21d** | **85** | **GPU Decode** | **~1500** | **4** |
| **30** | **86** | **JARVIS Persona** | **~950** | **5** |
| **31** | **87** | **JARVIS Security+AHCI** | **~1200** | **4** |
| **32** | **88** | **JARVIS Emotion+Cache** | **~1200** | **6** |
| **33** | **89** | **SleepCycle+Memory** | **~2500** | **11** |
| **34** | **90** | **JARVIS Deep Cognitive** | **~1200** | **6** |
| **35** | **91** | **Polimento+Ecosystem** | **~2500** | **10** |
| **36+** | **92+** | **AIOS Evolution** | **~15000** | **25+** |
| | | **Total** | **~20.260 LOC** | **50** |

### Notas

1. **Readequação 2026-07-04:** Roadmap reorganizado por dependência evolutiva. Foundation → Agentic → LLM → JARVIS → GPU → AIOS.
2. **Blocos 21-23 novos:** Itens independentes dos sprints 60/67/72 que não dependem de B-01.
3. **Blocos 24-27:** JARVIS features reordenadas de Sprints 77-80 para Sprints 80-83.
4. **Bloco 21c (GPU Foundations):** Sprint 84, NVIDIA/AMD/Intel. HW real com GPU.
5. **Bloco 21d (GPU Decode):** Sprint 85, BitNet offload. Depende de Bloco 21c.
6. **Blocos 30-35 (JARVIS + Memory + Polimento):** Sprints 86-91, independentes de HW.
7. **Bloco 36+ (AIOS Evolution):** Sprint 92+, 🔴 bloqueado por B-01.
6. **Blocos legacy (12-17)** mantidos como estão — implementados ou com itens 🟡 independentes.

---


## Seção 1.9 — Itens Catalogados (#355-#408)

Itens adicionados via changelog (2026-07-06 a 2026-07-07).

| # | Item | Destino | Status |
|---|------|---------|--------|
| 355 | buddy-slab-allocator | Sprint 86 | ✅ |
| 356 | edge-dhcp | Sprint 88 | ✅ |
| 357 | khal-std | -- | ❌ |
| 358 | ruvix-net | -- | 🔵 |
| 359 | BGE Embedding | Sprint 89 | ✅ |
| 360 | Kokoro-82M TTS | Sprint 94 | 🟡 |
| 361 | Zero-Copy SFS | Sprint 96 | ✅ |
| 362 | Episodic memory NVMe | Sprint 95 | ✅ |
| 363 | Skills-as-Modules | Sprint 96 | ✅ |
| 364 | Zero-Trust Syscall | Sprint 92 | 🟡 |
| 365 | Neural Cache decisions | Sprint 92 | 🟡 |
| 366-374 | Self-Healing (9 itens) | Sprint 96 | ✅ |
| 375-377 | GGUF phased (3 itens) | Sprint 99+ | 🟡 |
| 378-382 | GPU Architecture (5 itens) | Sprint 98 | ✅ |
| 383-390 | AIOS Evolution (8 itens) | Sprint 93-97 | ✅ |
| 391-396 | WASM Apps (6 itens) | Sprint 93 | ✅ |
| 397-401 | Micro-Learning (5 itens) | Sprint 95 | ✅ |
| 402-403 | WASM pool + ABI | Sprint 93 | ✅ |
| 404 | NVMe SQ/CQ | Sprint 99+ | ✅ |
| 405 | Capability token crypto | Sprint 93 | 🟡 |
| 406 | VirtIO-GPU fix | Sprint 97 | 🟡 |
| 407-408 | WiFi SoftMAC + Async | Pos B-01 | 🔴 |

## Seção 5 — Changelog do Idea Bank

| Data | Mudança | Responsável |
|---|---|---|
| 2026-06-23 | Criação do IDEA_BANK.md — seed dos 116 itens da ADR-0014 + ADR-0015 + dependências | IDA IA |
| 2026-06-23 | Sprint 18: Itens 16-19 (LAPIC/IOAPIC/MADT), 68-69 (PCI scan) → ✅ Block 1 | Dev + IDA IA |
| 2026-06-23 | Itens 34 (acpi crate) → 🟡 Sprint 18 (parser ACPI mínimo implementado, crate não usado) | Dev + IDA IA |
| 2026-06-23 | Sprint 19: Itens 20-33 (SMP multi-core boot) → ✅ Block 2; AP boots with -smp 2 and -smp 4; race fix with spin::Mutex on CPU_COUNT | Dev + IDA IA |
| 2026-06-23 | Sprint 20: Itens 114 (Hermes Chat) → ✅ Block 3; IntentMlp MLP + command parser + console daemon | Dev + IDA IA |
| 2026-06-23 | ADR-0016: Itens 117-125 (Network Strategy) → adicionados; VirtIO-net + smoltcp + HTTP movidos para Sprint 23; MVP+1 = Network Sprint | IDA IA |
| 2026-06-24 | ADR-0017: Itens CRÍTICOS corrigidos (e1000, DHCP, slab, nos, bridge, xsdt, mhi, nn) | IDA IA |
| 2026-06-24 | ADR-0018: Sprint 24 plan (12 HIGH + 16 MEDIUM + 12 LOW bugs) | IDA IA |
| 2026-06-24 | ADR-0019: Itens 126-156 (Neural Cortex BitNet LLM) → adicionados; Transformer Engine + Cortex Daemon + Success Engine + Training Pipeline | IDA IA |
| 2026-06-24 | Itens 157-163 (Self-Optimization / Workflow Learning) → adicionados; Usage Pattern Analyzer, Workflow Predictor, Auto-Skill Generator, Dynamic Resource Scaling, Self-Optimizing Scheduler, Workflow Profile, Hardware Config Learning | Dev + IDA IA |
| 2026-06-24 | Itens 164-175 (Crom Ecosystem Analysis) → adicionados; 12 ideias portadas de MrJc01/75 repos: XOR Delta, CDC, TV-DSL, Codebook VQ, ReAct loop, MCP Server, Workspace isolation | IDA IA |
| 2026-06-24 | ADR-0020 (Crom Ecosystem Rust Viability Analysis) → criado; código modelo no_std para 9 items (#164-175), ~1.780 LOC kernel + ~300 LOC Python | IDA IA |
| 2026-06-24 | Item 176 (Ed25519 Cryptographic Identity for TrustCache) → adicionado; upgrade do CapabilityToken(u64) para assinatura Ed25519 real; derivado de Crom-meueu | IDA IA + Auditoria Externa |
| 2026-06-25 | ADR-0021: Itens 177-189 (Life OS Ecosystem Analysis) → adicionados; 13 ideias extraídas de 20 repos: Spectrum Graph, Runtime SDD, FS as Context, DA Identity, Temporal KG, Proactive Push, WASM Sandbox, Intent Transparency, Energy Tracking, AppForge, Multi-User, Workflow Builder, Federated Cluster | IDA IA |
| 2026-06-25 | PAI Deep-Dive: Itens 190-198 → adicionados; 9 ideias extraídas de danielmiessler/LifeOS (PAI v5.0.0): Algorithm 7-phase loop, Council skill, Loop Detection, Bitter Pill, ISA format, Hermes Rating, Evals, Container Zones, Boot security policy | IDA IA |
| 2026-06-25 | ADR-0022: Itens 199-213 (Tier 2 PAI Ecosystem) → adicionados; 15 ideias extraídas de 21 repos: IterationBudget, Skill Metadata, Audit Ring, Awakening Mode, Context Fencing, Heartbeat Idle Gate, RateLimiter, Lifecycle Hooks, MemoryProvider, Tool Permissions, Actor Registry, Crash-Recovery, ComputeBackend, Plugin System, WASM Sandbox | IDA IA |
| 2026-06-25 | ADR-0023: Itens 214-227 (Tier 3 Memory Systems & Second Brain) → adicionados; 14 ideias extraídas de 14 repos: SHA-256 Dedup, Privacy Filter, TTL Eviction, Hybrid Search, 4-Tier Consolidation, Ebbinghaus Decay, Session Replay, Knowledge Graph, Metacognitive Guard, Draft→Review→Merge, Atkinson-Shiffrin Memory, Bi-temporal KG, Team Memory, Git Snapshots. Deep-dive: agentmemory (24k ★, 60+ source files). | IDA IA |
| 2026-06-25 | ADR-0024: Itens 228-249 (Tier 4 Agent Frameworks) → adicionados; 22 ideias extraídas de 6 repos: Tool Policy Registry, Usage Tracker, Auto-Compact Buffer, Event-Sourced Conversation, Cron Scheduler, Session Checkpoint, Plan/Execute Modes, Graph Orchestration, Plugin Hub, Completion Terminal Skills, Claim-Based Lease, Time Travel, Context Compaction, Observability, AI Security Scan, Hub Discovery, Human-in-the-Loop, Remote Execution, Skill Marketplace, Context Compaction Agent. Deep-dive: Cline (63.9k ★, 293 releases, 6.338 commits, AgentRuntime + ClineCore + CronRunner). | IDA IA |
| 2026-06-25 | Sprint 23 Bugfix: Itens 250-252 (e1000 DMA fix, /ping, DHCP/ARP refactor pendente) → adicionados; allocate_contiguous fix (start de next_free_bit), DHCP skip, /ping command. Boot QEMU validado: e1000 init OK, executor 11000+ ticks. | Dev + IDA IA |
| 2026-06-25 | Network Sprint: Itens 253-255 (e1000 TDT protocol fix, NUM_DESC 48, PTHRESH 8, Neural Network Architecture) → adicionados; TPT=0 ainda não resolvido (qemu_send_packet não chamado). Novo modelo: init_driver_network() → HW_NET_E1000 → network_bootstrap() → skill-based routing. | Dev + IDA IA |
| 2026-06-25 | ADR-0025: Itens 256-267 (Tier 3 Security Patterns) → adicionados; 12 padrões extraídos de 5 repos (InnerWarden 159★, ai-jail 595★, vexfs 24★, Chisel 12★, cori-kernel 17★). 7 itens viaveis Sprints 24-27 (256-264), 3 ideias futuras Sprint 28+ (265-267), 6 padrões descartados. Deep-dive: InnerWarden (2057 commits, 7900+ testes, 45 eBPF programas, 82 detectores, 69 regras correlação). | IDA IA |
| 2026-06-26 | **Section 1.28 Agent/Skill-First Architecture** — 20 itens (A-001 a A-020) adicionados. Reclassificação: tudo vira agente/skill, nada de tasks/serviços. Paradigma fundamental. | Dev + IDA IA |
| 2026-06-26 | **IDEA_BANK total: 275 itens.** Heat map atualizado: 68 ✅, 116 🟡, 76 ⏳, 9 💰, 6 ❌ | Dev + IDA IA |
| 2026-06-26 | **Seção 6 — Sprint Planning** adicionada: 7 blocos futuros (12-18) com 55 itens 🟡 consolidados. Total: 330 itens. | Dev + IDA IA |
| 2026-06-27 | **Bloco 12 implementado:** x2APIC, Huge Pages, PCI bridges recursivo, Cron Scheduler, MCP Server (#18, #70, #92, #93, #172, #232). | Dev + IDA IA |
| 2026-06-27 | **Bloco 13 implementado:** Multi-mode Trust, Path Confinement, Mask Secrets, Graduated Enforcement, Posture-Aware, Boot Policy, Security Pipeline, Ed25519 identity (#166, #176, #198, #256-260). | Dev + IDA IA |
| 2026-06-27 | **Bloco 14 implementado:** Hermes Cognitive completo — Identidade (#180), SDD (#178), ReAct (#190), Transparency (#184), Council (#191), Bitter Pill (#193), Context Fencing (#203), Usage Analyzer (#157), Workflow Predictor (#158), Dynamic Scaling (#160), Reflex Threshold (#139), Self-Scheduler (#161), Config Learner (#163), LLM arch/tier (#135/#136). | Dev + IDA IA |
| 2026-06-29 | **Ecosystem Batch 3 (IDEA #280):** 12 repos portados (redox, Theseus, Embassy, Tock, Swarm, RagaAI, Swarms, SuperAGI). 8 arquivos, 601 LOC. `cargo check --release`: 0 errors. | Dev + IDA IA |
| 2026-06-29 | **The Agency port (IDEA #277):** HwRegistry + HwAgent por dispositivo PCI. Agency struct com 12 divisoes, 30+ agentes especializados. LLM-aware: "quero video chamada" → ativa mic+camera+display+net. | Dev + IDA IA |
| 2026-06-29 | **GGUF loader research (IDEA #278):** Formato GGUF documentado. Portavel (~500 LOC) mas modelos 9B+ exigem heap >5GB. Alternativa: expandir .bitnet v3. | Dev + IDA IA |
| 2026-06-29 | **SmileyOS patterns (IDEA #279):** 11 padrões identificados — compositor multi-window, filesystem proprio, app SDK via trait, temas, v86 browser demo, shell 40+ comandos. Prioritario: temas + shell expandida. | Dev + IDA IA |
| 2026-06-30 | **VFS Layer + MHI ARC (IDEA #281):** VfsRegistry com mount table, resolve, lookup. MHI arc_suggest_tier() ZFS-inspired (MFU→Dram, MRU→Nvme, cold→Hdd). 8 mounts padrao. Path utils. | Dev + IDA IA |
| 2026-06-30 | **Storage Agents (IDEA #282):** FilesystemAgent trait. AtaAgent (/mnt/hdd/), DevFsAgent (/dev/), ProcFsAgent (/proc/). VFS bridge: read_vfs/write_vfs/list_vfs. | Dev + IDA IA |
| 2026-07-01 | **GPU Detection + VRAM Tier (IDEA #283):** PCI scan class 0x03, 30+ GPUs detectadas por device ID, BAR0/BAR2 mapping, VRAM bump allocator com next_offset, DEADBEEF test. GpuInfo com vendor/arch/vram/display/compute. | Dev + IDA IA |
| 2026-07-01 | **Intel Ring Buffer Compute (IDEA #284):** Intel Gen9+ ring buffer init, write/submit/wait_idle, exec_batch, gpu_blit (XY_SRC_COPY_BLT). MI_BATCH_BUFFER_START/END. unsafe impl Send para Mutex. | Dev + IDA IA |
| 2026-07-01 | **GPU Backend Selector (IDEA #285):** GpuAccel enum (Intel/CpuOnly), Mutex-safe, auto-select Intel→AMD→NVIDIA→CPU. gpu_matmul() com fallback CPU. | Dev + IDA IA |
| 2026-07-01 | **Desktop Cube Crossfade (IDEA #286):** Transicao entre workspaces sem float (FPU desabilitado). Inteiros step 0..50, AtomicBool em vez de static mut, split animado com fill_rect. | Dev + IDA IA |
| 2026-07-01 | **Bughunt GPU Sprint 66:** 24 bugs corrigidos (3 crit, 8 high, 6 med, 7 low). Destaques: vec! sem alloc, gpu_blit morto, vram_alloc sem bump, static mut UB, float sem FPU, BAR sem validacao, mod gpu ausente. commit 1d66a17. | Dev + IDA IA |
| 2026-07-01 | **TODO.md mestre:** docs/TODO.md com 28 pendências catalogadas, sub-itens, dificuldades, travas, fontes, esforço. Para qualquer AI DEV localizar e contribuir. | Dev + IDA IA |
| 2026-07-01 | **WiFi (B-29):** Intel Wireless / Atheros / Realtek 802.11. Scan, association, WPA2/WPA3. Thread: firmware loading, crypto, frame format. Bloqueado por B-01 (rede). | Dev + IDA IA |
| 2026-07-02 | **Boot Bughunt Sprint 71 (IDEA #300-#304):** Boot refatorado para agent-first com 8 fases (BOOT_PHASE events). DiagnosticSkill substitui 90+ linhas de teste inline. CortexAgent acorda antes do HW discovery. Xuvisco corrigido (framebuffer antes de VGA CRTC). FAT12 log finalmente funcional (boot_logger + BootLogAgent). `cargo check --release`: 0 errors. | Dev + IDA IA |
| 2026-07-03 | **305** | **TPM 2.0 Measured Boot (v0.74.1)** — TIS MMIO driver (279 LOC), SHA256 embedded, PCI config 0xFED40000 probe, locality 0 request, FIFO send/recv, PCR[8] extend com kernel hash. Fallback silencioso se TPM ausente. Depende de Ed25519 kernel signing. | ✅ v0.74.1 | v0.74.1 | 279 LOC for TIS driver + SHA256 |
| 2026-07-03 | **Sprint 74 (v0.74.0-0.74.2):** Seguranca — particao FAT mascarada como 0x1C (Hidden FAT32 LBA, bootloader aceita via mbr_nostd), assinatura Ed25519 do kernel com auto-verificacao, TPM 2.0 TIS driver (probe+fallback+PCA extend). Shutdown tracking (4 causas, persistencia FAT12+VFS, BootSelfHealAgent analisa). | Dev + IDA IA |
| 2026-07-03 | **Particao FAT 0x1C (v0.74.2):** Mascara tipo 0x0C→0x1C (Hidden FAT32 LBA) via build scripts (offset 0x1D2). Bootloader aceita (mbr_nostd 0.1.0 mapeia 0x1C como PartitionType::Fat32). Kernel aceita 0x1C em todos os checks. Fallback 0x73 mantido. QEMU boot OK, VB OK. | Dev + IDA IA |
| 2026-07-03 | **FAT32-only (v0.75.0):** Fat12Writer removido. write_boot_log, boot_log_agent, boot_logger, shutdown usam apenas FAT32. 102 LOC removidos. | Dev + IDA IA |
| 2026-07-03 | **DiskIntelligenceAgent (v0.75.1-0.75.6):** 6 controladoras (ATA, USB, NVMe), 10+ FS probes (FAT32 a ReFS), GPT, SED/OPAL, S.M.A.R.T., I/O Scheduler, ARC cache 1MB, tier migration MHI. ~2.400 LOC. 0 erros. | Dev + IDA IA |
| 2026-07-03 | **IDEA #306-#310:** AIOS Evolution — compatibilidade cross-OS (PE/ELF/Mach-O/APK + syscall-to-skill), Update/Upgrade Agent com rollback, WASM Skill Runtime + BitNet IDE, J.A.R.V.I.S. Layer, stack final Boot→Kernel→Cortex→Hermes→J.A.R.V.I.S. | Dev + IDA IA |
| 2026-07-03 | **311** | **Trinity Model Hub (MoE — Mixture of Experts)** — Múltiplos modelos microscópicos especializados (<150KB cada): `hw_identify` (68KB), `rust_coder` (444KB), `disk_diag` (50KB), `security` (50KB). Router BitNet (68KB) classifica a intenção e roteia para o expert correto. Modelo generativo 1.5B recebe output do expert como contexto. Self-hosting: "crie app" → rust_coder gera código. Novos experts treinados on-demand. | ✅ Sprint 97 | Sprint 97 | RUSTCODER_MODEL global + fast-path HermesAgent + loading FAT32 |
| 2026-07-03 | **DiskIntelligenceAgent (v0.75.1-0.75.6):** 6 controladoras (ATA, USB, NVMe), 10+ FS probes (FAT32 a ReFS), GPT, SED/OPAL, S.M.A.R.T., I/O Scheduler, ARC cache 1MB, tier migration MHI. ~2.400 LOC. 0 erros. | Dev + IDA IA |
| 2026-07-03 | **IDEA #306-#310:** AIOS Evolution — compatibilidade cross-OS (PE/ELF/Mach-O/APK + syscall-to-skill), Update/Upgrade Agent com rollback, WASM Skill Runtime + BitNet IDE, J.A.R.V.I.S. Layer, stack final Boot→Kernel→Cortex→Hermes→J.A.R.V.I.S. | Dev + IDA IA |
| 2026-07-03 | **ADR-0031: AIOS Evolution Research** — análise completa (self-update A/B dual-slot, WASM wasmi runtime + WASI mapping, J.A.R.V.I.S. conversational layer, hybrid kernel/WASM agent architecture). Viability scores, LOC estimates, dependency chain, recommended sprint order. `docs/architecture/0031-aios-self-update-wasm-jarvis.md` | Dev + IDA IA |
| 2026-07-03 | **312** | **TrainingAgent — On-Device + GPU Learning** — 3 modos: Fine-tuning (CPU ADD/SUB, 100ex, ~2s), Transfer (1000ex, adapta dominio), Full (GPU+internet, 100K+ex). Fontes: FS local, HTTP (B-01), Federated. Pipeline: GPU detect → collect → train → .bitnet → Trinity Hub register. | 🟡 Sprint 79 | Sprint 79 | ~500 LOC + B-01 |
| 2026-07-03 | **313** | **Self-Learning OS — Aprende dos próprios dados** — O AIOS coleta seus próprios EventBus events, boot logs, self-heal logs, SMART data, conversas Hermes, padrões de erro, e usa como dataset de treino. Sem internet. Sem humano. Pipeline: LogAgent → DataCollector → TrainingAgent → .bitnet → Trinity Hub. | 🟡 Sprint 78 | Sprint 78 | ~300 LOC DataCollector + integração |
| 2026-07-03 | **314** | **SleepCycle Agent — Aprendizado inspirado no sono humano** — 5 fases: REPLAY (reproduz eventos recentes) → DREAM (BitNet gera variações sintéticas) → CONSOLIDATE (EWC protege skills existentes) → PRUNE (zera pesos fracos, ~18% redução) → REFLECT (confidence tracking, preenche gaps). Guard rails por fase: REPLAY filtra eventos maliciosos, DREAM rejeita sonhos perigosos, CONSOLIDATE protege skills de segurança c/ EWC max, PRUNE exempt pesos críticos, REFLECT roteia gaps proibidos pra humano. Agendado pelo CronAgent em períodos idle. NENHUM sistema bare-metal implementa isso. Neural AIOS seria pionero. | 🟡 Sprint 82 | Sprint 82 | ~1280 LOC + guard rails |
| 2026-07-04 | **ADR-0036** | **J.A.R.V.I.S. Unified Interaction Layer** — ADR-0036 criado, unifica ADR-0034 + ADR-0035. 28 features, 5-layer architecture (Boot→Kernel→Cortex→Hermes→JARVIS), Sprints 77-80+N+1+N+2, ~5650 LOC total. JARVIS = persona do Hermes, não camada separada. Rust no_std com código de referência para todos os componentes. | ✅ Aceito | — | 1126 lines ADR |
| 2026-07-04 | **315** | **J.A.R.V.I.S. Unified Interaction Layer (IDEA #315)** — 28 features catalogadas de ADR-0036. Sprint 80: SOUL.md, IPW, Session Compression, Notification Gate, Sessionless Thread (~950 LOC). Sprint 81: Emotion, Contracts, Discovery, Cache, Pipeline (~1200 LOC). Sprint 82: Dreaming, Ego, Heartbeats, Auto-Skills, Babel-Index, SleepCycle (~1680 LOC). Sprint 83: Fail-Closed, Merkle, Fluid Persona, AHCI (~1200 LOC). N+1 (Sprint 85+): Voice, CRDT, Gamification (~1600 LOC). N+2 (Sprint 85+): SKYNET Mesh (~500 LOC). Substitui 310a/b (JARVIS Layer original). | 🟡 Sprint 80 | Sprint 80 | ~5650 LOC total |
| 2026-07-04 | **Roadmap readequado** | IDEA_BANK.md reorganizado: Blocos 21-24 → 21-30. Foundation Quick Wins (Sprint 77), Agentic Evolution (Sprint 78), LLM Infrastructure (Sprint 79), JARVIS reordenado para Sprints 80-83, GPU (Sprint 84), AIOS (Sprint 85+). #311/#314 sprint numbers corrigidos. Activation on Demand premise adicionada ao AGENTS.md. roadma p.md + README.md + STATE.md sincronizados. | IDA IA |
| 2026-07-05 | **316** | **VGA Buffer Clear fix (v0.79.1)** — Xuvisco na transição bootloader→kernel causado por VGA text buffer 0xB8000 nunca limpo + framebuffer nunca limpo + `[BOOT] FB ativo — VGA text mode desligado` mentiroso. Fix: `vga_buffer::clear_physical_buffer()` escreve zeros em 0xB8000 sem tocar CRTC; `fb::probe_uefi_framebuffer()` limpa FB para preto; ambos chamados antes de qualquer `println!`. Zero I/O a 0x3D4/0x3D5. | ✅ v0.79.1 | v0.79.1 | 3 files / ~10 LOC |
| 2026-07-05 | **317** | **WHPX AVX2 Detection (v0.80.0)** — WHPX emula instruções VEX/AVX2 como VM exits (~10k+ ciclos cada), tornando AVX2 **2x mais lento** que scalar puro. CPUID leaf 0x40000000 retorna vendor "Microsoft Hv". `has_avx2()` e `avx2_available()` agora detectam WHPX e retornam false, forçando scalar path que roda nativo. Descoberta crítica: `unpack_all` não era o gargalo — emulação VEX sim. Per-layer timing: 2218 ticks/layer (scalar, WHPX) vs 4443 (AVX2, WHPX). | ✅ v0.80.0 | v0.80.0 | 2 files / ~30 LOC |
| 2026-07-05 | **318** | **KV Cache (v0.80.1)** — `KvCache` struct per-layer Vec<f32> para K e V. `forward_with_kv()` processa só tokens novos, atenção GQA usa K/V concatenados (cache + novo). `generate_speculative` refatorado: prompt preenche cache, cada step gera 1 token via cache. Ganho: ~6h → ~84s (200x+ speedup). | ✅ v0.80.1 | v0.80.1 | +210/-36 LOC cortex.rs |
| 2026-07-05 | **319** | **SPSC ring lockless (bbqueue)** — Fila Single-Producer Single-Consumer lock-free baseada em BipBuffer. Ideal para comunicação IRQ→task, core→core, GPU→CPU. no_std, sem CAS em targets sem atomics. Base para toda comunicação cross-core. | 🟡 Bloco 25 | Sprint N | ~100 LOC |
| 2026-07-05 | **320** | **IPI vetorizado** — `send_ipi(lapic_id, vector)` para cross-core interrupts. Atualmente só INIT/SIPI existem. Necessário para TLB shootdown, wake AP, interrupção entre cores. | 🟡 Bloco 25 | Sprint N | ~150 LOC |
| 2026-07-05 | **321** | **PerCpu dinâmico** — Alocar struct PerCpu por AP + GS.base individual via wrmsr. Hoje só BSP_PCPU estático existe, todos APs recebem o mesmo ponteiro. Base para scheduler multicore, slab local, métricas por core. | 🟡 Bloco 25 | Sprint N | ~300 LOC |
| 2026-07-05 | **322** | **Work-stealing Chase-Lev scheduler** — Deques lock-free por core (Chase-Lev algoritmo). Quando queue local vazia, steal de core vizinho. Distribui agents entre 4 cores automaticamente. Referência: crossbeam-deque + fast-steal (crates.io no_std). | 🟡 Bloco 26 | Sprint N+1 | ~400 LOC |
| 2026-07-05 | **323** | **Parallel-for AVX2 matmul** — Chunk hidden dimension (2560) em 4 partes, cada core processa um chunk, barreira atômica (AtomicU32 spin). Sem lock — só barreira. Speedup estimado: 2-3.5× sobre single-core. | 🟡 Bloco 26 | Sprint N+1 | ~300 LOC |
| 2026-07-05 | **324** | **AgentScheduler multicore** — 4 run queues (uma por core), steal entre cores quando queue local vazia. AgentTier define affinity: Permanent→core fixo, UserDemand→work-stealing. RebalanceAgent monitora carga e move agents. | 🟡 Bloco 26 | Sprint N+1 | ~200 LOC |
| 2026-07-05 | **325** | **Per-CPU slab allocator** — Alocar sem lock no hot path. Slab local por core, lote de 64 frames quando vazio (lock curto no allocator global). Reduz contenção no LockedHeap compartilhado. | 🟡 Bloco 26 | Sprint N+1 | ~300 LOC |
| 2026-07-05 | **326** | **GPU BAR0/BAR1 mapping UC** — Mapear PCI BARs da GPU como uncacheable (PWT|PCD) para MMIO direto. BAR0 = register file, BAR1 = VRAM aperture. Genérico: NVIDIA (nova-core), AMD (amdgpu), Intel (i915). | 🟡 Bloco 27 | Sprint N+2 | ~300 LOC |
| 2026-07-05 | **327** | **GPU doorbell + SPSC job ring** — CPU escreve job descriptor no ring buffer, escreve doorbell register (BAR0 offset), GPU lê doorbell, executa job, atualiza tail. Ring com `alignas(64)` head/tail para false sharing prevention. | 🟡 Bloco 27 | Sprint N+2 | ~400 LOC |
| 2026-07-05 | **328** | **VRAM buddy allocator** — Gerenciar VRAM da GPU (GDDR NVIDIA/AMD, DRAM carveout Intel). Alocação contígua para kernels GPU. Free list com coalescing. Base para MSched evicção ótima (Belady) futura. | 🟡 Bloco 27 | Sprint N+2 | ~400 LOC |
| 2026-07-05 | **329** | **Agent.xpu prefill/decode split** — Prefill (processamento do prompt completo) fica na CPU (4 cores paralelos). Decode (geração token a token) vai para GPU via job ring. CPU faz tokenization + embedding, GPU faz matmul. Referência: arXiv 2506.24045. | 🟡 Bloco 28 | Sprint N+3 | ~400 LOC |
| 2026-07-05 | **330** | **GPU matmul kernel ternário** — BitNet ADD/SUB matmul implementado como compute shader para GPU. NVIDIA PTX, AMD AQL, Intel GEN assembly. Speedup estimado: 10-25× sobre CPU (VRAM bandwidth vs DDR). | 🟡 Bloco 28 | Sprint N+3 | ~300 LOC |
| 2026-07-05 | **331** | **CPU→GPU KV cache DMA** — Transferir KV cache de 307 MB entre RAM e VRAM via DMA engine. Estimado 200-400ms para swap completo (PCIe 3.0 x16 = ~16GB/s). Referência: dmaplane (arXiv 2603.10030). | 🟡 Bloco 28 | Sprint N+3 | ~200 LOC |
| 2026-07-05 | **332** | **XQueue preemptível (XSched)** — Fila de comandos GPU com 3 níveis de preempção: pending (não submetido), in-flight (submetido mas não executando), running (em execução). Política agnóstica de hardware. Referência: XSched (OSDI 2025). | 🟡 Bloco 28 | Sprint N+3 | ~600 LOC |
| 2026-07-05 | **333** | **burn-flex backend port** — Portar o backend CPU do burn-flex (tracel-ai/burn) para nosso kernel. SIMD gemm + quantization + fused ops. Elimina bitnet_avx2 manual (~800 LOC). 2-95× speedup documentado. Referência: github.com/antimora/burn-flex. | 🟡 Bloco 29 | Sprint N+4 | ~800 LOC |
| 2026-07-05 | **334** | **MSched evicção VRAM** — Belady (OPT) eviction policy para VRAM. Prediz working set do próximo kernel GPU, pré-carrega de DRAM, evicção ótima. Referência: arXiv 2512.24637. | 🟡 Bloco 29 | Sprint N+4 | ~500 LOC |
| 2026-07-05 | **335** | **CFS scheduler (Completely Fair)** — Substituir round-robin do AgentScheduler por CFS baseado em vruntime. Fairness entre agents. Referência: echOS-x64 + moss-kernel. | 🟡 Bloco 29 | Sprint N+4 | ~500 LOC |
| 2026-07-05 | **336** | **GPU + Display co-existência** — iGPU (Intel) faz display (framebuffer), dGPU (NVIDIA) faz compute. Quando só dGPU existe, time-sharing via XQueue. Referência: coconutOS. | 🟡 Bloco 29 | Sprint N+4 | ~300 LOC |
| 2026-07-05 | **337** | **SMP+GPU Research (ADR-0037 v1)** — 30 fontes analisadas (arXiv, GitHub, crates.io, listas kernel). | ✅ ADR-0037 | N | ~900 LOC ADR |
| 2026-07-05 | **338** | **Pesquisa Expandida AMD+Intel+NPU+Apple+NVIDIA bare-metal** — ADR-0037 v2 expandido. Cobertura completa: AMD ROCm/KFD/TrustOS, Intel Level Zero/Xe, NPU XDNA+Intel NPU, Apple Silicon honeycrisp/aruminium/metaltile, processadores modernos AMX/AVX-512/APX, abordagens multiplataforma Rust GPU. Matriz de decisão por HW real. | ✅ ADR-0037 v2 | N | +300 LOC ADR |
| 2026-07-05 | **339** | **TrustOS (nathan237) — Blueprint bare-metal AMD GPU em Rust** — 264K LOC, zero blobs. AMD GPU bring-up do zero: SDMA engine, ring buffer, firmware loading no RX 580X (Polaris 10). Root cause de 14 iterações: Graphics Memory Controller desinicializado. Prova que bare-metal GPU em Rust no_std é VIÁVEL. | 🔵 Referência | — | ADR-0037 |
| 2026-07-05 | **340** | **pascal-egpu (TheTom) — Blueprint NVIDIA Pascal GPU do zero** — GTX 1060 via BAR MMIO. Plano 8 fases: PCIe → BAR → ACR → FIFO → GR → Compute. **Referência NVIDIA.** Para AMD: amdgpu Linux driver + GPUOpen docs. Para Intel: i915 Linux driver. | 🔵 Referência | — | ADR-0037 |
| 2026-07-05 | **341** | **folkering-os — AI-native OS similar ao nosso** — SMP 4 cores, AVX2+FMA, VirtIO-GPU, WASM JIT, smoltcp, lock-free telemetry ring, self-healing, capability tokens. 4 meses de desenvolvimento solo. Prova que nossa arquitetura (Rust no_std, SMP, AVX2, WASM, AI-native) é viável em timeline curta. | 🔵 Referência | — | ADR-0037 |
| 2026-07-05 | **342** | **honeycrisp (cyberia-to) — Apple Silicon GPU/NEON/AMX/ANE bare-metal Rust** — Quatro crates: unimem (memória compartilhada), acpu (NEON+AMX), aruminium (Metal GPU puro Rust, 1.79× faster), rane (ANE). Prova que acesso bare-metal a todos compute units Apple é possível. | 🔵 Referência futuro (ARM64) | — | ADR-0037 |
| 2026-07-05 | **343** | **AMD ROCm + KFD — Compute via Linux kernel** — RDNA2/3 suportado via /dev/kfd. T0-GPU (50K LOC Rust) prova AMD compute em Rust via KFD. TheRock (AMD) tem pure-Python KFD driver. **Não bare-metal** — depende de Linux. | ❌ Descartado (nosso HW NVIDIA) | — | ADR-0037 |
| 2026-07-05 | **344** | **Intel Level Zero + Xe GPU Compute** — Intel Compute Runtime suporta Tiger Lake+ (Gen12+). **Skylake (i5-6400) NÃO é suportado** (Gen9 mínimo). Intel HD Graphics 530 não tem Level Zero, XMX, nem compute capability. | ❌ Descartado (incompatível HW) | — | ADR-0037 |
| 2026-07-05 | **345** | **NPU AMD XDNA — Spatial dataflow accelerator** — Firmware fechado, toolchain offline (MLIR-AIE/IRON/Triton-XDNA), sem programabilidade geral. XDNA1 abandonado pela AMD no Linux. XDNA2 suportado via Ryzen AI Software. Performance máxima: ~68 GFLOPS (512³ int16) no XDNA1. | ❌ Descartado (sem HW, firmware fechado) | — | ADR-0037 |
| 2026-07-05 | **346** | **NPU Intel — LEON RT + NCE tiles** — Firmware fechado, OpenVINO+Level Zero, offline compile. NPU 3720 (Meteor Lake) 9.5 TOPS, NPU 4000 (Lunar Lake) 48 TOPS. **Nosso i5-6400 não tem NPU.** | ❌ Descartado (sem HW) | — | ADR-0037 |
| 2026-07-05 | **347** | **AMX/AVX-512/APX — Feature set moderno** — AMX em Sapphire Rapids+ (Xeon 2023). AVX-512 em Skylake-SP (Xeon) e Rocket Lake (2021). APX em Nova Lake+ (2026+). P-cores/E-cores em Alder Lake+ (2021). **i5-6400 (Skylake client) não tem nenhum.** AVX2+FMA é o máximo disponível. | ❌ Descartado (HW incompatível) | — | ADR-0037 |
| 2026-07-05 | **348** | **any-gpu (cochranblock) — Tensor engine wgpu multi-GPU** — CausalLM, SDPA, GQA, LayerPager. Roda em qualquer GPU via wgpu (Vulkan/Metal/DX12). 309 testes. **Não bare-metal** — depende de driver gráfico do SO. Inspiração para design de API. | 🔵 Referência arquitetural | — | ADR-0037 |
| 2026-07-05 | **349** | **Firmware GPU disponível em linux-firmware** — NVIDIA Pascal (FECS+GPCCS signed), AMD RDNA (PSP MIT license), Intel Gen6+ (GuC/HuC open). Todos disponíveis para download e redistribuição. Documentação: nouveau, amdgpu, i915. | ✅ FIRMWARE DISPONÍVEL | N+2 | ADR-0037 |
| 2026-07-05 | **350** | **nova-core (NVIDIA/KHaddock) — Driver NVIDIA oficial em Rust** — Em desenvolvimento no LKML (2025-2026). Suporta Ampere+ (GSP). Pascal NÃO é target (Pascal usa Falcon, não GSP). **Referência arquitetural:** estrutura de driver GPU profissional em Rust, BAR mapping, doorbell, channel management. | 🔵 Referência arquitetural | — | ADR-0037 |
| 2026-07-05 | **351** | **Matriz de Decisão Multi-Vendor** — O neural-os-core deve suportar qualquer HW detectado. Prioridade de implementação: NVIDIA (primeiro, firmware + docs mais acessíveis) → AMD (segundo, GPUOpen) → Intel (terceiro, i915 público) → NPU (futuro). SMP é pré-requisito para todos. | ✅ ADR-0037 v2 | N | ADR-0037 |
| 2026-07-05 | **352** | **Secure Boot GPU — Pipeline multi-vendor** — NVIDIA ACR: FECS blobs → WPR → LS ucode → signature. AMD PSP: firmware MIT → PM4 init ring. Intel GuC: firmware open → HuC auth → submission. Pipe: linux-firmware → kernel → BAR0 → GPU engine. | 🟡 Bloco 27 (GPU Foundations) | Sprint N+2 | ~600 LOC |
| 2026-07-05 | **353** | **GPU Compute Pipeline completo** — Pipeline de submissão genérico: BAR0 MMIO → GPU boot (ACR/PSP/GuC) → command ring init → compute dispatch. Implementação por vendor: NVIDIA (PFIFO + PUSH_BUFFER), AMD (PM4 ring), Intel (GEN ring buffer). Pipeline de execução: CPU prepara comando → ring entry → doorbell → GPU executa → completion. | 🟡 Bloco 27-28 | Sprint N+2 a N+3 | ~2500 LOC total |
| 2026-07-05 | **354** | **TrustOS lessons para GPU bare-metal** — Lições chave de TrustOS: (1) GMC (Graphics Memory Controller) e VM são a causa raiz de falhas — não registros, não PCIe link; (2) Ring buffer precisa estar em GART (Graphics Aperture Remap Table); (3) RPTR/WPTR avançando = firmware responsivo; (4) Firmware loading é sequencial e frágil — um passo errado e tudo aborta. | 🔵 Referência arquitetural | — | ADR-0037 |
| 2026-07-06 | **355** | **buddy-slab-allocator (arceos-hypervisor)** — Substituir slab.rs + gpu/vram.rs pelo crate no_std maduro (30K downloads). Per-CPU slab caches, remote-free lock-free, buddy allocator. Usado no ArceOS (bare-metal Rust OS similar). Apache-2.0. | 🟡 Sprint 86 | Sprint 86 | ADR-0038 |
| 2026-07-06 | **356** | **edge-net (edge-dhcp)** — Cliente DHCP no_std + no-alloc para resolver B-01. 225★ GitHub, 42 forks. Pode operar sem heap, antes do smoltcp. | 🟡 Sprint 88 | Sprint 88 | ADR-0038 |
| 2026-07-06 | **357** | **khal-std (dimforge/khal)** — GPU compute shaders Rust→SPIR-V/PTX/CPU. **Não viável diretamente** (requer wgpu/std), mas arquitetura inspira nossa futura GPU compute. | ❌ Inviável (std) | — | ADR-0038 |
| 2026-07-06 | **358** | **ruvix-net (ruvnet/ruvector)** — Kernel cognitivo Rust bare-metal similar ao nosso. Stack de rede mínima para "RuVix Cognition Kernel". Referência arquitetural. | 🔵 Referência | — | ADR-0038 |
| 2026-07-06 | **359** | **BGE-Small-EN-v1.5 Embedding (BAAI)** — Modelo de embedding semântico 33.4M params, 384-dim, ONNX, MIT license. Converter para .bitnet e integrar como skill semantic_search no HermesAgent. MTEB 62.17, 62M downloads/mês. | 🟡 Sprint 89 | Sprint 89 | ADR-0038 v2 |
| 2026-07-06 | **360** | **Kokoro-82M TTS (ONNX Community)** — Modelo TTS 82M params, 24kHz, 28 vozes, Apache-2.0. Formato ONNX com quantizações (86 MB Q8). Converter para .bitnet e integrar como skill TTS. Único modelo TTS viável para bare-metal. | 🟡 Sprint 94 | Sprint 94 | ADR-0038 v2 |
| 2026-07-06 | **361** | Zero-Copy SFS via zerocopy crate — Transmuting &[u8] ↔ &Tensor sobre páginas NVMe mapeadas. Serialização sem serde, sem alocação. | 🟡 Sprint 96 | Sprint 96 | ADR-0010 |
| 2026-07-06 | **362** | Episodic memory via battery-backed NVMe — KV-cache persistida entre reboots. Páginas físicas mantidas via NVMe battery-backed ou S3 sleep. | 🟡 Sprint 95 | Sprint 95 | ADR-0010 |
| 2026-07-06 | **363** | Skills-as-Modules capability import — Allowlist-based validação (nn:silu, tensor:matmul como imports declarados por skill). | 🟡 Sprint 96 | Sprint 96 | ADR-0010 |
| 2026-07-06 | **364** | Zero-Trust Syscall Categories — 4 classes: Read-only (always allow), Ephemeral allocate (budget), Persistent write (Cortex eval), Hardware access (always deny). | 🟡 Sprint 92 | Sprint 92 | ADR-0010 |
| 2026-07-06 | **365** | Neural Cache decisions per token — Cache de avaliações LLM por capability token para evitar latência repetida. | 🟡 Sprint 92 | Sprint 92 | ADR-0010 |
| 2026-07-06 | **366** | Failure Taxonomy Enum — FailureClass::MemoryFault, ExecutionFault, ResourceFault, LogicFault, ExternalFault. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **367** | Exception Handlers + SelfHeal — Page fault e GPF handlers: coletam contexto, publicam KERNEL_ERROR, chamam SelfHeal::analyze(), tentam recovery. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **368** | Corrective Prompting — Erro → consulta LLM com contexto + histórico + lições. "Erro X no daemon Y. Qual melhor estratégia?" | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **369** | Verifier Pós-Recovery — Verifica recovery: task completou 1 tick sem panic? Skill registrada no SKILL_REGISTRY? Se falhou, próxima estratégia. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **370** | Erros no EventLog (EventKind::KernelError) — Nova variante no EventLog para publicar erros de kernel. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **371** | Budgeted Recovery — Tentativas limitadas por budget durante self-healing. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **372** | Silent Failure Detection — LLM verifica se output está correto mesmo sem erro explícito. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **373** | Multi-level Failure Architecture — Supervisionado (classificação) + não-supervisionado (anomalias) para detecção de falhas. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **374** | Failure Prediction — Predizer falhas antes de ocorrerem baseado em tendências. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **375** | GGUF phased implementation (Phase 1-3) — P1: parser header+metadata (150 LOC), P2: Q4_0 dequantization (200 LOC), P3: streaming ATA/USB (150 LOC). | 🟡 Sprint 99+ | Sprint 99+ | ADR-0028 |
| 2026-07-06 | **376** | f16_to_f32() manual conversion — u16 + função manual para dequantização GGUF Q4_0. | 🟡 Sprint 99+ | Sprint 99+ | ADR-0028 |
| 2026-07-06 | **377** | GGUF streaming from ATA/USB — Page table mapping para modelos >4GB, streaming blocks on demand. | 🟡 Sprint 99+ | Sprint 99+ | ADR-0028 |
| 2026-07-06 | **378** | Per-vendor GPU driver LOC breakdown — intel.rs (~700), nvidia.rs (~1500), amd.rs (~2000), virtio.rs (~400). | 🟡 Sprint 98 | Sprint 98 | ADR-0029 |
| 2026-07-06 | **379** | NVIDIA Pascal Push Buffer channel layout — Register map: PUSH_BUFFER 0x002000, size 0x002004, tail 0x002008. | 🟡 Sprint 98 | Sprint 98 | ADR-0029 |
| 2026-07-06 | **380** | AMD RDNA PM4 packet types — PKT3_WRITE_DATA, PKT3_ACQUIRE_MEM, PKT3_DMA_DATA, PKT3_RELEASE_MEM, PKT3_SET_BASE. | 🟡 Sprint 98 | Sprint 98 | ADR-0029 |
| 2026-07-06 | **381** | Model swap flow (/model \<path.gguf\>) — Detecta GPU, checa VRAM, carrega modelo se couber, fallback DRAM. | 🟡 Sprint 98 | Sprint 98 | ADR-0029 |
| 2026-07-06 | **382** | iGPU display + dGPU compute architecture — iGPU display, dGPU compute. GPU única faz ambos via VBIOS/UEFI GOP. | 🟡 Sprint 98 | Sprint 98 | ADR-0029 |
| 2026-07-06 | **383** | Detalhado WASI→Skill mapping (20 syscalls) — fd_read→FileAgent, clock_time_get→TimeAgent, poll_oneoff→EventBusAgent. | 🟡 Sprint 97 | Sprint 97 | ADR-0031 |
| 2026-07-06 | **384** | Tier 0-4 Agent Classification — Core, Hardware, Runtime, WASM, External MCP. | 🟡 Sprint 97 | Sprint 97 | ADR-0031 |
| 2026-07-06 | **385** | WASM Host Function Interface signatures — vfs_read, skill_invoke, clock_time, http_get, event_publish, agent_yield. | 🟡 Sprint 93 | Sprint 93 | ADR-0031 |
| 2026-07-06 | **386** | Performance budget table (kernel vs WASM) — vfs_read 50µs kernel vs 500µs WASM = 10x overhead. | 🟡 Sprint 93 | Sprint 93 | ADR-0031 |
| 2026-07-06 | **387** | ChromeOS A/B update reference architecture — GPT partition attributes, dm-verity Merkle tree, Omaha protocol. | 🟡 Sprint 97 | Sprint 97 | ADR-0031 |
| 2026-07-06 | **388** | J.A.R.V.I.S. Context Window Manager — Cortex (LLM) + Hermes (orchestrator) + Kernel (execution). JARVIS persona com proactive notification bus. | 🟡 Sprint 97 | Sprint 97 | ADR-0031 |
| 2026-07-06 | **389** | J.A.R.V.I.S. Notification Gate rules — Startup grace 60s, dedup 30s, priority queue Critical>High>Info>Debug, interrupt gate. | 🟡 Sprint 96 | Sprint 96 | ADR-0031 |
| 2026-07-06 | **390** | Update channel strategy — Stable (3600s), nightly (600s), security (60s). | 🟡 Sprint 97 | Sprint 97 | ADR-0031 |
| 2026-07-06 | **391** | AgentManifest JSON format specification — name, kind, schedule, auto_start, persist, description, required_tokens, version, author, icon. | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **392** | Developer contract for WASM agents — manifest(), tick(tick, tick_count) → u32, teardown(). | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **393** | 15 specific WASI→skill mappings for WASM agents — disk_read/write, net_http_get/post, time_now/sleep, publish/subscribe events. | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **394** | BitNet IDE with HOWTO feature — IDE rodando no AIOS com Cortex-assisted code generation. Editor + Cortex Panel. | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **395** | Marketplace/App Store agent — HTTP agent: search, install, Ed25519 verification, versioning, ratings. | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **396** | DiskMonitor example agent — 48 KB WASM agent: manifest(), tick() checando SMART a cada 1000 ticks, publish_event(). | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **397** | SleepCycle guard rails per phase — REPLAY rejeita security_bypass, DREAM rejeita weapon/exploit, CONSOLIDATE protege safety skills. | 🟡 Sprint 95 | Sprint 95 | ADR-0033 |
| 2026-07-06 | **398** | BitNetTrainer implementation — PackedTernaryWeights, train_step() com STE, signum() gradient update, clamp(-1,1). | 🟡 Sprint 95 | Sprint 95 | ADR-0033 |
| 2026-07-06 | **399** | Candle Trainer sidecar — ELF binary com candle crate. Kernel carrega ELF, aloca stack+heap, mapeia segments, executa treino GPU. | 🟡 Sprint 95 | Sprint 95 | ADR-0033 |
| 2026-07-06 | **400** | Task Spawner (ELF loader) — spawn_elf() com goblin::elf::Elf::parse(), allocate_contiguous_pages(), map_elf_segment(), TSS stack. | 🟡 Sprint 95 | Sprint 95 | ADR-0033 |
| 2026-07-06 | **401** | Three data sources for on-device training — Local FS (SMART logs, self-heal), Pre-loaded (/data/rust_training/), Internet (crawl). | 🟡 Sprint 95 | Sprint 95 | ADR-0033 |
| 2026-07-06 | **402** | WASM linear memory pool (256 KB per skill) — 64-page slabs pre-alocadas por skill, alocadas do heap. | 🟡 Sprint 93 | Sprint 93 | ADR-0010 |
| 2026-07-06 | **403** | Skill ABI design — Como skills comunicam resultado ao Cortex: tensor return path, logging, completion signaling. | 🟡 Sprint 93 | Sprint 93 | ADR-0010 |
| 2026-07-06 | **404** | NVMe submission/completion queue architecture — Submission/completion queues, MSI-X, PRP lists, SGL. | 🟡 Sprint 99+ | Sprint 99+ | ADR-0010 |
| 2026-07-06 | **405** | Capability token cryptographically signed — Skill recebe contexto com token criptografado scoped a operações permitidas, verificado pelo kernel. | 🟡 Sprint 93 | Sprint 93 | ADR-0010 |
| 2026-07-06 | **406** | VirtIO-GPU GET_DISPLAY_INFO pending fix — Bug do QEMU TCG onde GET_DISPLAY_INFO retorna 0x0. | 🟡 Sprint 97 | Sprint 97 | ADR-0029 |
| 2026-07-07 | **407** | WiFi SoftMAC firmware loading — Carregar blob iwlwifi.ucode via PCIe DMA para chips Intel AX200/AX210. Requer DMA ring + firmware loader da flash FAT. | 🔴 Pós B-01 | N+2 | ~800 | generic_wifi.rs |
| 2026-07-07 | **408** | Async executor bare-metal x86_64 — Port do Embassy para x86_64 (APIC timer + MSI-X como drivers de temporizador/interrupção). Necessário para SoftMAC (ACK 802.11 em ~10µs). | 🔴 Pós B-01 | N+2 | ~1500 | embassy-rs + generic_wifi.rs |
| 2026-07-07 | **409** | USB CDC ECM driver — Driver USB Ethernet Control Model para dongles WiFi HardMAC (Realtek RTL8188/RTL8192). Comunicação via xHCI isochronous + bulk. | 🔴 Pós B-01 | N+2 | ~500 | xHCI + generic_wifi.rs |
| 2026-07-07 | **410** | Bridge smoltcp::phy::Device para WifiChipset — Implementar trait Device do smoltcp sobre a trait WifiChipset do generic_wifi para unificar o path de pacotes. | 🟡 Sprint atual | Atual | ~50 | generic_wifi.rs + netstack.rs |
