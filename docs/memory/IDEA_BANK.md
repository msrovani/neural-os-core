# 🧠 Idea Bank — neural-os-core

**Última atualização:** 2026-06-23 (Sprint 18 — Block 1 concluído)  
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
| ⏳ Pós-MVP | Adiado, ver Seção 3 para dependências | ⏳ Pós-MVP |
| 💰 Sponsor | Requer hardware/parceria | 💰 Sponsor |
| ❌ Descartado | Não será feito, com motivo | ❌ Descartado |
| 🔄 Fundido | Absorvido por item maior | 🔄 Fundido |

---

## Seção 1 — Master Registry (Inventário Completo)

### 1.1. USB

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 1 | xHCI controller mínimo (<500 LOC, BAR0, port status) | ⏳ Pós-MVP | Sprint 23+ | MVP usa PS/2 legacy. xHCI requer PCI (Block 1) + driver USB (~500 LOC). |
| 2 | `identify_device()` → VID/PID/class | ⏳ Pós-MVP | Sprint 23+ | Bloqueado pelo xHCI driver. |
| 3 | Neural Cortex classify (MLP 7→5: allow/deny/learn/no_intent/suspect) | ⏳ Pós-MVP | Sprint 23+ | MLP arquitetura (Block 4) pode ser estendido. |
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
| 18 | x2APIC mode (MSR-based, sem MMIO) | 🟡 Sprint 18+ | Sprint 18+ | MSR APIC_BASE lido. x2APIC enable postergado para SMP. |
| 19 | MADT parsing (ACPI → LAPIC list) | ✅ Block 1 | Sprint 18 | Implementado: type 0 (LAPIC), type 1 (IOAPIC), type 2 (x2APIC). |
| 20 | CPUID leaf 0x1A (P-core / E-core detection) | ✅ Block 2 | Sprint 19 | Essencial para CorePools inteligente. |
| 21 | CPUID leaf 0x0B (Extended Topology) | ✅ Block 2 | Sprint 19 | Necessário para distinguir HT de cores físicos. |
| 22 | CorePools / ComputePools (P→Ring0/1, E→Ring2) | ✅ Block 2 | Sprint 19 | Atribuição por tipo de core + fallback homogêneo. |
| 23 | Algoritmo `assign_cores()` — P/E-aware + N+1 + fallback | ✅ Block 2 | Sprint 19 | Adicionado ao Block 2 após cross-ref. |
| 24 | PerCpu struct (core_id, lapic_id, core_type, ring, stack, queue) | ✅ Block 2 | Sprint 19 | Essencial para APs saberem quem são. |
| 25 | GS.base segment register per-core | ✅ Block 2 | Sprint 19 | Mecanismo de acesso ao PerCpu. |
| 26 | INIT-SIPI-SIPI via LAPIC ICR | ✅ Block 2 | Sprint 19 | Protocolo Intel de wake. |
| 27 | Trampoline assembly (16→32→PAE→64→Rust) | ✅ Block 2 | Sprint 19 | Ponte entre modo real e long mode. |
| 28 | AP startup IPI (BSP → INIT → SIPI → SIPI) | ✅ Block 2 | Sprint 19 | Depende do trampoline + alloc_below_1mb. |
| 29 | Stack separada por core (64 KB cada) | ✅ Block 2 | Sprint 19 | Essencial para APs não compartilharem stack. |
| 30 | Regras de escalonamento por pool | ✅ Block 2 | Sprint 19 | Tabela: qual trabalho → qual pool. |
| 31 | "Se só E-cores, tudo roda em E-cores mais lentos" | ✅ Block 2 | Sprint 19 | Caso de borda documentado. |
| 32 | "Se 1 core apenas (QEMU -smp 1), tudo no mesmo core" | ✅ Block 2 | Sprint 19 | Caso de borda documentado. |
| 33 | "HT: 1 thread por core físico no Ring 0/1, restante no Ring 2" | ✅ Block 2 | Sprint 19 | Regra de atribuição incluída. |
| 34 | `acpi` crate para parser MADT/PPTT | 🟡 Sprint 18+ | Sprint 18+ | Parser ACPI mínimo implementado (sem crate externo). |
| 35 | `raw-cpuid` crate para detecção de features | 🟡 Block 2 | Sprint 19 | No MVP, CPUID inline assembly. |

### 1.3. NPU (AMD XDNA)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 36 | `Npu` struct + `try_init()` via PCI scan | 💰 Sponsor | Sprint 25+ | Requer AMD APU real (XDNA) ou QEMU com NPU virtual. |
| 37 | `Accelerator::XDNA(Npu)` / `Accelerator::Software` enum | 💰 Sponsor | Sprint 25+ | Depende de #36. |
| 38 | Command queue circular + doorbell write | 💰 Sponsor | Sprint 25+ | Requer documentação do XDNA. |
| 39 | Overlay loading via MMIO | 💰 Sponsor | Sprint 25+ | Vendor-specific. AMD Vitis AI compiler. |
| 40 | MSI-X interrupt registration | 💰 Sponsor | Sprint 25+ | Depende de #36 + IOAPIC/MSI. |
| 41 | Fallback automático: init_npu() → se falha → Software | ✅ Block 4 | Sprint 21 | Se NPU ausente, cai para software. |
| 42 | 3 cenários: QEMU / APU sem driver / APU com driver | 🟡 Block 4 | Sprint 21 | Lógica de fallback documentada. |
| 43 | Cadeia de programação: Modelo → Overlay → DRAM | 💰 Sponsor | Sprint 25+ | Requer toolchain AMD Vitis. |
| 44 | Ring 0 MLP NÃO precisa do NPU — 20 pesos rodam em 1 core | ✅ Block 4 | Sprint 21 | Premissa arquitetural adotada. |
| 45 | Caminho de migração: QEMU → APU f1 → f2 → f3 | 💰 Sponsor | Sprint 25+ | Depende de patrocínio/hardware. |

### 1.4. AI-Driven Hardware Detection

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 46 | `HardwareInventory::collect()` | ✅ Block 4 | Sprint 21 | Coração do Block 4. |
| 47 | `cortex::infer_architecture(&inventory)` | ✅ Block 4 | Sprint 21 | MLP 512→256→64→9 ternário. |
| 48 | MLP 512→256→64→9 ternário (~37 KB, pesos embutidos) | ✅ Block 4 | Sprint 21 | ~150k pesos ternários em .rodata. |
| 49 | `SystemArchitecture` struct (12 saídas categóricas) | ✅ Block 4 | Sprint 21 | ring0, ring1, ring2, heap, sfs, trust, power, tiers. |
| 50 | Boot flow adaptativo: collect → infer → init | ✅ Block 4 | Sprint 21 | Substitui boot sequence fixo atual. |
| 51 | Treinamento offline do MLP (10k hardware profiles) | ⏳ Pós-MVP | Sprint 21+ | Pesos iniciais heurísticos. Treinamento real depois. |
| 52 | Atualização do MLP via skill WASM | ⏳ Pós-MVP | Sprint 25+ | Requer WASM embedder. |
| 53 | Fallback seguro: MLP absurdo → valores default clamped | ✅ Block 4 | Sprint 21 | Heap mínimo 64 KB, ring0 sempre fallback software. |
| 54 | "MLP cabe no kernel — 37 KB no .rodata" | ✅ Block 4 | Sprint 21 | Premissa verificada. |
| 55 | "Inferência é rápida — µs" | ✅ Block 4 | Sprint 21 | MLP ternário em 1 core = microssegundos. |

### 1.5. Memory Hierarchy Index (MHI)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 56 | `struct MemoryTier { device, kind, capacity, bandwidth, latency }` | ✅ Block 4 | Sprint 21 | Adicionado ao MVP (cross-ref). |
| 57 | `struct MemoryHierarchy { tiers: Vec<MemoryTier> }` ordenado | ✅ Block 4 | Sprint 21 | Adicionado ao MVP. |
| 58 | `enum AllocTier { Dram, Vram, Nvme, Hdd }` | ✅ Block 4 | Sprint 21 | Adicionado ao MVP. |
| 59 | `fn alloc_by_tier(tier, size) -> Option<PhysAddr>` | ✅ Block 4 | Sprint 21 | Dram implementado. Vram/Nvme → None com diagnóstico. |
| 60 | `AllocTier::Vram` → alocar no BAR da GPU | ⏳ Pós-MVP | Sprint 23+ | Requer driver GPU + BAR mapeado. |
| 61 | `AllocTier::Nvme` → alocar no NVMe via SFS | ⏳ Pós-MVP | Sprint 24+ | Requer NVMe driver + SFS. |
| 62 | `AllocTier::Hdd` → cold storage | ⏳ Pós-MVP | Sprint 24+ | Requer SFS + driver ATA/NVMe. |
| 63 | MLP saídas: heap_tier, tensor_tier, kv_cache_tier, sfs_active_tier | ✅ Block 4 | Sprint 21 | 4 tiers de saída no MLP do MVP. |
| 64 | MLP saídas opcionais: sfs_cold_tier, tensor_swap_tier, skill_heap_tier | 🟡 Block 4 | Sprint 21 | Campos opcionais no SystemArchitecture. |
| 65 | Exemplo real: notebook i5 + GTX 1050 + NVMe + HDD | ✅ Doc | README | Caso de uso documentado. |
| 66 | Exemplo real: Xeon 6900 (1 TB RAM, NVMe RAID) | ✅ Doc | ADR-0015 | Caso de uso documentado. |
| 67 | Exemplo real: AMD APU Strix Point (unified memory) | ✅ Doc | ADR-0015 | Caso de uso documentado. |

### 1.6. Periféricos (PCI, NVMe, VirtIO)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 68 | PCI config space access (CF8/CFC) | ✅ Block 1 | Sprint 18 | Implementado: read_config_dword/word, BARs. |
| 69 | PCI scan: vendor, device, class, subclass, BARs | ✅ Block 1 | Sprint 18 | Implementado: 256 busses, 32 devices, BAR0-5. |
| 70 | PCI bridges (hierarquia de barramento) | 🟡 Block 1 | Sprint 18 | Suporte básico: multi-função em bridges PCI-PCI. |
| 71 | NVMe driver (PCI Class 01.08) | ⏳ Pós-MVP | Sprint 24+ | MVP é stateless. Sem SFS, NVMe é desnecessário. |
| 72 | VirtIO-blk (PCI 1AF4:1001) | ⏳ Pós-MVP | Sprint 24+ | Alternativa QEMU ao NVMe. |
| 73 | VirtIO-net (PCI 1AF4:1041) | ⏳ Pós-MVP | Sprint 24+ | MVP sem rede. |
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
| 83 | Intel HDA audio driver | ❌ Descartado | — | Nenhuma skill de áudio no roadmap. |
| 84 | Áudio via USB (UAC) | ❌ Descartado | — | USB + áudio = duplo pós-MVP. |

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
| Áudio/Vídeo | 6 | 0 | 0 | 4 | 0 | 2 |
| Princípios | 6 | 6 | 0 | 0 | 0 | 0 |
| Roadmap Memória | 4 | 2 | 0 | 2 | 0 | 0 |
| Roadmap Kernel | 4 | 1 | 0 | 3 | 0 | 0 |
| Roadmap EventBus | 3 | 1 | 0 | 2 | 0 | 0 |
| Roadmap Skills | 3 | 1 | 0 | 2 | 0 | 0 |
| Roadmap Cognitive | 4 | 0 | 0 | 4 | 0 | 0 |
| Roadmap Timeline | 3 | 0 | 0 | 3 | 0 | 0 |
| Outras | 5 | 4 | 0 | 0 | 1 | 0 |
| **Total** | **116** | **53 (46%)** | **5 (4%)** | **45 (39%)** | **9 (8%)** | **2 (2%)** |

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
  │     └───────────┘                            └──────────────┘
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

## Seção 5 — Changelog do Idea Bank

| Data | Mudança | Responsável |
|---|---|---|
| 2026-06-23 | Criação do IDEA_BANK.md — seed dos 116 itens da ADR-0014 + ADR-0015 + dependências | IDA IA |
| 2026-06-23 | Sprint 18: Itens 16-19 (LAPIC/IOAPIC/MADT), 68-69 (PCI scan) → ✅ Block 1 | Dev + IDA IA |
| 2026-06-23 | Itens 34 (acpi crate) → 🟡 Sprint 18 (parser ACPI mínimo implementado, crate não usado) | Dev + IDA IA |
| | _Próxima entrada aqui quando uma ideia mudar de status_ | |
