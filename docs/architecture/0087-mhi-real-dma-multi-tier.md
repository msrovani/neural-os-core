# ADR-0087: MHI Real — DMA Multi-Tier (VRAM/RAM/NVMe/SSD/HDD)

**Data:** 2026-08-05
**Status:** Accepted
**Lifecycle:** `fazendo`
**Inspirado por:** ADR-0040 §MHI (#420), ADR-0046 (AirLLM DMA prefetch), ADR-0048/49/50 (GPU compute), ADR-0062 (NVMe/AHCI), Linux memory-tiers (LWN 898766/978313/974126), nouveau_dmem, uvm_hal.c, zCore/rCore/RustOS/Redox/spin (unikernels DMA)
**Sprint:** v1.9.9 TEST → v2.0.0
**Documentos fonte:** `docs/architecture/0040-filesystem-architecture.md`, `0048-nvidia-compute-multigeracao.md`, `0049-amd-compute-multigeracao.md`, `0050-intel-compute-multigeracao.md`, `0062-claudioos-vs-neural-aios.md`, `0046-airllm-gguf-streaming.md`
**Nota:** esta ADR **preenche o gap #420/#423** (MHI DMA real) e consolida a pesquisa profunda de DMA em Rust no_std (SESSÃO 2026-08-05). As ADRs 0040/0046/0048-50/0062 continuam como fontes; este é o processo canônico do **MHI com migração real entre tiers**.

---

## 1. Problema

O MHI atual (`crates/k_nano/src/mhi.rs`, 388 LOC) é **telemetria sem callers**:

- `alloc_by_tier` só aloca **DRAM** de verdade; NVMe/VRAM logam "AWAITING"
- `record_access` **não tem nenhum caller** → `access_count=0` para tudo → `arc_suggest_tier` é ruído
- `mhi_tick` faz DRAM→DRAM memcpy (no-op semântico)
- Os dois maiores consumidores bypassam o MHI: **modelo 2B (792MB)** lido via `read_file`/`read_range`, e **índices SGDB** (RAM direto)

Resultado: infraestrutura de 5 tiers construída, tráfego real (modelos + memória IA) nunca passa por ela. E a pesquisa revelou **bugs latentes**: NVMe usa **bounce de 1 página** (sem PRP lists), BCS da Intel tem constante suspeita (`0x220000` vs `0x22000` do PRM), e o SGDB pode colidir com partições GPT no HW real (C1 da análise compat).

## 2. Estado da arte por tier (pesquisa profunda 2026-08-05)

### Tier 0 — VRAM (GPU)

| Vendor | Engine | Classe/Mecanismo | Estado local | Esforço |
|--------|--------|------------------|--------------|---------|
| **NVIDIA (Pascal→Blackwell)** | Copy Engine (CE) | `PASCAL_DMA_COPY_B` 0xc1b5 → `BLACKWELL_DMA_COPY_B` 0xcab5 (+0x200/geração); methods 0x0260/0x0264 (aperture) + 0x0400×8 (src/dst/pitch/npages) + 0x0300 (launch); **SRC_TYPE_PHYSICAL=0x1000/DST=0x2000**; channel privileged (inst 0xe4\|0x20) | pushbuffer GPFIFO existe (`nvidia.rs`); falta channel CE + methods | ~200 LOC |
| **AMD (RDNA)** | SDMA | ring `SDMA0_GFX_RB_*`; `SDMA_OP_COPY`/`SDMA_SUBOP_COPY_LINEAR`; shadow wptr polling | — | AWAITING_HW RDNA |
| **Intel (Gen9/Xe)** | BCS | ring `BLT_RING_BASE` 0x22000; `XY_SRC_COPY_BLT` + `MI_FLUSH_DW` | `blit.rs` ~90% pronto (**suspeita: 0x220000 vs 0x22000**) | fix + flush + pin GTT |

**Barreiras WC vs UC:** CPU→VRAM quer **WC** (PWT=0,PCD=1) + streaming stores `movntdq`; dispositivo→RAM quer **UC** (o `map_page_uc` local resolve este). São duas variantes PAT; o `map_page_uc` cria o mapeamento, falta a variante WC para gravação de VRAM via CPU.

**Fence por geração:** `<Volta` = USERD readback · `>=Volta` = semaphore release non-WFI (padrão 906f) · **Blackwell = sem USERD** (único caminho). Isolar custa ~30 LOC.

**Evolução de classes CE (herança uvm_hal):** Pascal 0xc1b5 → Volta 0xc3b5 → Turing 0xc5b5 → Ampere A 0xc6b5 / B 0xc7b5 → **Ada = Ampere B (0xc7b5, sem classe nova!)** → Hopper 0xc8b5 (encrypt/decrypt inline) → Blackwell A 0xc9b5 / B 0xcab5. **O template físico Pascal (0x0260/0x0400/0x0300) sobrevive mas re-encodou a validação por geração** → dispatch por geração + canário golden obrigatório.

#### 2.0.1 Reconciliar SASOS (0047-GPU §7) com CE DMA — dois mecanismos do Tier 0

A ADR-0047-GPU §7 propõe **SASOS** (mapear VRAM no espaço do heap com páginas UC — zero-copy por ponteiro); esta ADR propõe **Copy Engine** (DMA bulk). **Não são concorrentes — são complementares, para acessos de natureza diferente:**

| Mecanismo | Para que | Vantagem | Quando usar |
|-----------|----------|----------|-------------|
| **SASOS (map UC)** | Acessos pontuais/aleatórios: KV pages, tensores pequenos, debug, leitura direta CPU↔VRAM | Zero-copy por ponteiro, sem fila de comando | KV cache access (H2O working set), tensores < 1MB, interação com `msched.rs` |
| **CE/SDMA/BCS (DMA engine)** | Transfers bulk: pesos de modelo (792MB), prefill, migração de tier | Eficiente p/ grandes volumes, sem ocupar CPU, async | Model load, tier promotion/demotion, prefill GPU |

**Como coexistem no MHI:**

```
Tier 0 (VRAM) = 2 mecanismos:
  ├── SASOS: VRAM mapado no heap (0x4020_0000_0000+) com páginas UC/WC
  │     → acesso direto por ponteiro (KV pages, tensores pequenos)
  │     → base para `Tensor::location = MemTier::Vram` (0047-GPU §7.4)
  │
  └── CE DMA: channel GPFIFO dedicado (runlist CE, privileged)
        → transfers bulk via engine (pesos, migração de tier)
        → fence: USERD (Pascal) / semaphore (Volta+)
        → alimenta o `mhi_tick` para tier1↔tier0 (drivers reais)

Decisão: a alocação SASOS decide ONDE o dado vive (ponteiro);
o CE decide COMO moves bulk acontecem (engine). Ambos registrados
no MHI via `record_access` — o CE para transfers, o SASOS para acesso.
```

**Impacto no roadmap:** a Fase 4 (NVIDIA CE) e o SASOS da 0047-GPU são **paralelizáveis** — o SASOS é pré-requisito para o tensor na VRAM (0047-GPU §7.4, ~100 LOC), o CE para migração bulk. Ordem recomendada: SASOS primeiro (dá o ponteiro), CE depois (dá a velocidade de transfer).

**Nota de WC vs UC (reconciliação):** a 0047-GPU usa páginas UC para SASOS; a análise desta ADR mostra que CPU→VRAM quer **WC** (write-combining) + `movntdq` — o SASOS deve mapear **WC** para gravação de VRAM via CPU, UC para leitura. Duas variantes PAT no mesmo espaço SASOS.

### Tier 2 — NVMe

- **PRP (Physical Region Page)**: `nvme_setup_prps` — offset = dma_addr & (page-1); crossing → PRP1/PRP2 ou lista (entradas 8B page-aligned). **Driver local: ZERO PRP lists** — usa bounce de 1 página (`read_sectors_bounce`).
- **Zero-copy real** = construir PRP list (via `dma_alloc_coalesced` + `map_page_uc`) + path `read_blocks_direct(pa_list)`. **Testável em QEMU `-device nvme`** ✅
- **SGL** (NVMe 1.3+) remove restrição de alinhamento — v2.
- **Interrupções**: polling do CQ (tail + toggle bit) é o padrão bare-metal correto (SPDK é 100% polling). MSI-X quando houver IRQ routing.
- **P2P NVMe↔VRAM (GDS)**: **SKIP** — hairpin/ACS bloqueia em notebook; GDS é NVLink-only. Caminho prático: NVMe→DRAM (PRP) → CE (DRAM→VRAM).

### Tier 3/4 — AHCI/ATA (SSD/HDD)

- **AHCI**: command list (1KB-aligned) → command table (128B) → PRDT (DBA 64-bit, **sem requisito de alinhamento** — vantagem sobre PRP). Upgrade: aceitar `pa` do caller direto (sem bounce) + páginas UC. Driver local já tem PRDT.
- **HDD (tier 4)**: NCQ (SATA FPDMA, tags 0-31) + deadline scheduler (já existe) + readahead + sort por LBA + coalescing = o que `mhi_tick` deve fazer no tier 4 (demote em batch, nunca página a página).

### Padrão Rust no_std (validado por zCore/rCore/RustOS/Redox/spin)

Todos usam o **mesmo padrão do `dma.rs` local**: `alloc_dma(size) -> (va, pa)` (page-aligned, físico contíguo) + `phys_to_virt`. Sem crates novas; só disciplina:
- Toda página DMA = **UC** (PWT|PCD via `map_page_uc`) ou flush `clflushopt` — a lição E1000 se aplica a SQ/CQ NVMe, PRP lists, command tables AHCI, **e pushbuffer GPU**
- Contiguidade física por pool: `dma_alloc_coalesced(n_pages)` cobre PRP (1 página) e PRDT (sem req)
- `#[repr(C, align(4096))]` + `read_volatile`/`write_volatile` (já usado)

## 3. Design — MHI real (5 tiers)

```
record_access (mhi.rs — HOJE SEM CALLERS)
   ├── k_hal/gpu/vram.rs:185 (acessos VRAM — rotear p/ MHI)
   ├── disk_agent read/write paths (NVMe/AHCI — chamar com bytes+tick)
   └── gpu compute dispatch (acessos a pesos em VRAM)
        ↓
Policy de promoção/demotion (espelhar Linux memory-tiers):
   - tier ids: VRAM=300, DRAM=200, NVMe=100, SSD=50, HDD=25
   - demotion order explícita (lista), não hardcoded
   - hot = contador de acessos em janela deslizante + histerese
     (LWN 898766: threshold 1s; rate limit MB/s — evita thrash)
   - cold = aging LRU no tier (LWN 974126: promoção ASYNC, sem stall)
        ↓
mhi_tick → dispatch real (hoje memcpy DRAM→DRAM = no-op):
   tier1→tier0: NVIDIA CE 0xc1b5 (ou BCS/SDMA) — copia via engine, fence semaphore
   tier1→tier2: NVMe write com PRP list direto
   tier2→tier1: NVMe read direto (zero-copy)
   tier3/4: batch demote via AHCI + deadline/readahead (io_scheduler já existe)
```

**Regras de ouro (do kernel):**
1. **Promoção async + rate-limited** — nunca stall o path crítico
2. **Demotion no reclaim** — Linux demota quando a página seria evictada; MHI demota quando o bump/heap falhar, não por timer
3. **Hot-page detection ≠ migração imediata** — histerese

## 4. Veredictos

| Item | Veredicto | Justificativa |
|------|-----------|---------------|
| **NVMe PRP zero-copy** | ✅ Factível AGORA | QEMU `-device nvme` testa; mata bounce de 1 página |
| **MHI wiring `record_access`** | ✅ Factível AGORA | Lógica pura; QEMU/RAM |
| **Intel BCS** | ✅ ~90% pronto | fix 0x22000 + MI_FLUSH_DW + pin GTT; HW i915 |
| **NVIDIA CE Pascal** | ✅ Implementável | template nouveau ~200 LOC; GTX 1050 real (canary 64KB + CRC) |
| **AMD SDMA** | ⏳ AWAITING_HW RDNA | documentado |
| **P2P NVMe↔GPU / GDS** | ❌ Skip | hairpin/ACS em notebook; GDS é NVLink |
| **IOMMU VT-d** | ❌ Overkill agora | flat identity; revisitar com Ring3+WASM maduro |
| **SGL NVMe** | ⏳ v2 | quando PRP provar |

## 5. Roadmap

```
Fase 1 — NVMe PRP lists + read/write_blocks_direct     [QEMU, hoje]
Fase 2 — Wiring MHI: record_access nos paths + policy  [QEMU/RAM, hoje]
Fase 3 — Intel BCS fix + MI_FLUSH_DW + pin GTT          [HW i915]
Fase 4a — SASOS VRAM no heap (0047-GPU §7.4, ~100 LOC) [HW, dá o ponteiro]
Fase 4b — NVIDIA CE Pascal (channel CE + methods)      [GTX 1050, canary 64KB; dá a velocidade]
Fase 5 — Policy estilo Linux (tier ids, histerese)     [QEMU]
Fase 6 — AMD SDMA + SGL + P2P reavaliação              [AWAITING_HW]
```
4a antes de 4b: o SASOS dá o ponteiro (tensor na VRAM via `MemTier::Vram`),
o CE dá a velocidade de transfer bulk (pesos 792MB). Paralelizáveis, mas o
tensor na VRAM é pré-requisito lógico para migração bulk.

**Pré-requisito 4a ✅ (f0e5911):** detecção de VRAM por **medição de BARs** —
`k_nano::pci::read_bar_size` (técnica 0xFFFFFFFF) + `detect.rs` seleciona
MMIO/VRAM por tamanho real (VRAM = maior BAR ≥ 64MB; MMIO = BAR0 exceto
quando BAR0 é a aperture — AMD dGPU → BAR5, APU → BAR5). Corrige o bug de
raiz: AMD mapeia VRAM→BAR0/MMIO→BAR5 (amdgpu Bonaire+), o código assumia
VRAM→BAR2/MMIO→BAR0. APU sem BAR grande → DRAM compartilhada (honesto).
AIOS: mede o silício, não tabela DID.

**Fase 1 ✅ (c222cdc):** NVMe PRP zero-copy — `nvme_prp_layout()` (regras do
Linux `nvme_setup_prps`: PRP1 só; PRP2 = 2ª página; lista ≥3 páginas, 512
entradas), `io_nvm` com prp1+prp2 (cdw8/9 antes ficavam 0 = quebra >1
página), página de lista fixa por driver, `read/write_blocks_direct(lba,
dma_phys, len)` para callers MHI. Bounce path inalterado. 3 testes host.

**Fase 2 ✅ (c222cdc):** MHI wiring — `record_access` tem callers reais
(disk write `io_scheduler_flush`, disk read `readahead_hint` com convenção
lba*512 dos stubs; `vram_alloc` registra / `vram_free` unregister /
`msched_record` acessa). Policy: `hot_hits` + histerese (promoção só com
streak ≥ 2, LWN 898766), `tier_id` (VRAM=300 DRAM=200 NVMe=100 HDD=25
USB=10), VRAM na escada (hot working set → Vram, dispatch AWAITING).
6 testes host.

**Fase 3 ✅ (c4634be):** Intel BCS — 4 bugs de encoding corrigidos (i915
source): `BLT_RING_BASE` 0x220000→**0x22000**, TAIL +0x38→**+0x30** (0x38 é
RING_START), CTL 4096→**0x3001** (RING_CTL_SIZE|VALID), blit header
0x41000000 (XY_COLOR_BLT!)→**0x54F00008** (XY_SRC_COPY_BLT 0x53, depth no
DW1, DW3 x2/y2, src_pitch) + **MI_FLUSH_DW** 0x4C000001 pós-cópia (não o
MI_FLUSH antigo 0x02000000). Ring sem MI_BATCH_BUFFER_END (engine pararia
antes do TAIL → wait_idle timeout). Probe com pin GGTT (RING_START = gtt_off,
não phys). HW-gated: canário blit em i915 real.

**Fase 4a ✅ (9346cd4):** SASOS real — `map_page_uc_at`/`map_region_uc_2mb_at`
(VA arbitrário, não só identidade) + `init_sasos_vram` mapeia a aperture em
0x4020_0000_0000+ UC; `sasos_vram_ptr`/`sasos_phys_to_ptr` dão o ponteiro CPU
unificado (base p/ `Tensor::location = MemTier::Vram`, 0047-GPU §7.4). Wire
no `init_vram_tier` (não-fatal). Substitui o PoC simbólico (dívida morta).

**Fase 4b ✅ (2fd3acc):** NVIDIA CE Pascal — channel dedicado (classe
PASCAL_DMA_COPY_B 0xc1b5, privileged inst|0x20, runlist CE, USERD fence),
DMA_COPY phys→phys (apertures 0x0260/0x0264 SRC=0x1000/DST=0x2000, 0x0400×8,
launch 0x0300), canário 64KB RAM→VRAM→RAM golden; `mhi_tier0_copy()` seam p/
MHI tier1→tier0. Builders puros host-testados (3 testes). Incertezas marcadas
(stride runlist, layout 0x0400 literal, 0x0260/0x0264 separados — canário
GTX 1050 é o árbitro). HW-gated.

**Fase 5 ✅ (f6ddc89):** Policy — `DEMOTION_ORDER` explícita +
`demote_to()` (um degrau por vez, não hardcoded) + `migration_rate_ok()`
(rate limit 64MB/janela de 100 ticks, LWN 898766) respeitado no `mhi_tick`
(promoção async sem thrash). 3 testes novos (9 mhi tests total).

**Fase 4b→5 wiring ✅ (SESSION_274):** o seam `mhi_tier0_copy` agora tem
caller — `k_nano::mhi::register_tier0_copier(copy, free)` é registrado por
`nvidia_pascal_ce::probe_global` **apenas com canário CE golden**; o
`mhi_tick`/`execute_soft_migrate` promove Dram→Vram com DADOS
(`try_tier0_promote`: alloc buddy → CE copy → re-register; falha = rollback
VRAM, lição CoW F2). Sem hook (QEMU) o caminho metadata-only + AWAITING é
inalterado. CE copy também alimenta `record_access` (§2.0.1); `msched_init`
no `init_vram_tier` e `sasos_vram_ptr` registram acesso (Belady vivo).
Evidência HW real (GTX 1050) pendente — IDEA #537.

## 6. Gaps/Notas de compatibilidade (desta sessão)

- **C1 (TickvLite LBA 2048)**: já corrigido — região movida para o fim do disco (f07834f). O MHI tier 2 deve respeitar a mesma região.
- **C5 (MHI sem callers)**: este ADR é o wire — `record_access` nos paths. **Nota pós-C6 (1f71d25):** o `readahead_hint` onde o read-path estava wiredado era código morto (zero callers) e foi deletado; o seam real de leitura agora é `CachedDisk::read_sectors` (1 linha p/ `record_access` — follow-up).
- **C6 (ArcCache morto) ✅ (1f71d25)**: wire como wrapper — `CachedDisk<'a>` (BlockDevice, write-through, read cache por setor 512B, 1MB) inserido no seam `with_dev` do NeuralFS (`/models/` + SGDB). `readahead_hint`/`readahead_cache` deletados (write-only, zero callers). 5 testes host.

## 7. Referências

- `nouveau_dmem.c` (nvc0b5_migrate_copy) — template físico CE
- `NVIDIA/open-gpu-kernel-modules/.../uvm_hal.c` — ce_table[] multi-geração
- `nouveau/include/nvif/class.h` — classes 0xa0b5..0xcab5
- `g_gpu_class_list.c` — Ada = AMPERE_DMA_COPY_B, Hopper = HOPPER_DMA_COPY_A
- `drivers/nvme/host/nvme_setup_prps` — PRP rules
- LWN 898766 (memory tiers), 978313 (DAMOS_MIGRATE), 974126 (CXL promoção async)
- `mikex86/LibreCuda` — bare-metal multigen (BLACKWELL_CHANNEL_GPFIFO_A 0xC96F)
- zCore/rCore/RustOS/Redox/spin — alloc_dma pattern
- `crates/k_nano/src/{mhi,dma,disk_agent/nvme,ahci,ata}.rs` — código local
- `crates/k_hal/src/gpu/{nvidia,amd,intel,vram,blit,detect,pci_bar}.rs` — drivers GPU
