# k_nano — Auditoria Completa (2026-08-24)

## 1. Visão Geral
- **151 arquivos .rs, 47,080 LOC** — maior crate do workspace
- **1,205 funções pub** — kernel Ring 0, drivers, FS, memória, SMP
- 0 `todo!`/`unimplemented!` — honestidade de código

## 2. Subsistema por Subsistema

### 2.1 ATA (272 LOC) — ⚠️ GAP: 4Kn hardcoded

| Item | Estado | Notas |
|------|--------|-------|
| PIO read/write (`in ax, dx`) | ✅ Corrigido (SESSION_112) | Fix do bug v0.1→v1.1.5 |
| IDENTIFY | ✅ Funcional | Detecta容量 e geometria |
| TRIM (DATA SET MANAGEMENT) | ✅ Implementado | Comando 0x06, 8-byte range descriptors |
| read_any (master→slave fallback) | ✅ Funcional | Tenta master primeiro, depois slave |
| **4Kn (lba_size=4096)** | ❌ **Ausente** | `AtaDriver` não tem campo `lba_size`; `read_sectors` hardcodes 256 words (512B). DISCOS 4Kn REAIS ( WD Red, Seagate Exos) vão corromper dados |
| write_sectors | ✅ Funcional | Word-level write via `out dx, ax` |
| **PROBLEMA: spin loop 10000** | ⚠️ WARN | `read_sectors` tem `for _ in 0..10000 { spin_loop() }` — 10ms fixo pós-comando. QEMU tolera; HW real pode ser mais lento |

### 2.2 NVMe (508 + 637 LOC) — ✅ Sólido

| Item | Estado | Notas |
|------|--------|-------|
| Admin queue (sub/compl) | ✅ Funcional | 4K pages, 64B entries |
| I/O queue | ✅ Funcional | PRP list para scatter-gather |
| IDENTIFY controller | ✅ Funcional | Namespace discovery |
| **4Kn (LBADS field)** | ✅ Funcional | `lba_size = 1 << lbads` — detecta 4096 corretamente |
| Write com PRP layout | ✅ Funcional | nvme_prp_layout para DMA |
| DMA alloc (PhysicalBuffer) | ✅ Funcional | clflushopt + sfence |
| **PROBLEMA: page_size hardcoded 4096** | ⚠️ WARN | `const PS: usize = 4096` — certo para x86_64, mas gap para Huge Pages |

### 2.3 AHCI (335 LOC) — ⚠️ Limitado

| Item | Estado | Notas |
|------|--------|-------|
| Port discovery | ✅ Funcional | MMIO ABAR, port reg scan |
| Command list + FIS | ✅ Funcional | Setup correto |
| **PROBLEMA: SATA NCQ** | ❌ Ausente | Sem Native Command Queuing — 1 comando por vez |
| **PROBLEMA: hot-plug** | ❌ Ausente | Sem suporte a drive hot-plug |
| Bind to BlockDevice | ✅ Funcional | `ahci_bind_device` wired |

### 2.4 Disk Agent (2,285 LOC) — ✅ Bem estruturado

| Item | Estado | Notas |
|------|--------|-------|
| Controller (ATA/AHCI/NVMe/USB) | ✅ Unificado | `disk_controller.rs` com auto-detect |
| Cache LRU | ✅ Funcional | 320 LOC, LRU eviction |
| fs_probe (FAT32/ext2/NeuralFS) | ✅ Funcional | Multi-FS detection |
| Vol_mgr | ⚠️ Mínimo | 71 LOC — só mount/unmount básico |
| **PROBLEMA: cache callers** | ⚠️ WARN | `disk_agent::cache` tem 0 external callers — cache existe mas não é usada pelo ATA/NVMe path principal |

### 2.5 MHI (879 LOC) — ✅ Bem conectado

| Item | Estado | Notas |
|------|--------|-------|
| Tiered allocation (Dram/Vram/Hdd/Sdhc) | ✅ Funcional | 4 tiers com profiles |
| record_block_access | ✅ Chamado | disk_agent/cache + disk_agent/mod |
| mhi_tick | ✅ Wired | hermes OptimizerAgent chama |
| hhdm_copy_checked | ✅ Funcional | Seguro: valida endereços antes de copiar |
| alloc_by_tier | ✅ Chamado | hermes agents para VRAM |
| register_vram_allocator | ✅ Wired | k_hal vram.rs + bin main.rs |
| Arc suggest_tier | ✅ Funcional | Algoritmo de sugestão baseado em acesso |
| **PROBLEMA: migration倡議 sem executor** | ⚠️ WARN | `suggest_migration()` retorna pares de tiers mas `mhi_tick()` só loga — não migra dados |

### 2.6 DMA (189 LOC) — ✅ Correto

| Item | Estado | Notas |
|------|--------|-------|
| DmaBuf (alloc/free/UC/WB) | ✅ Funcional | MMIO UC + restore WB no drop |
| dma_alloc_coalesced | ✅ Funcional | Contíguo para burst PCIe |
| PhysicalBuffer<N> | ✅ Funcional | const-generic, clflushopt |
| **OWNERSHIP CHECK** | ✅ Implementado | `delivered` bitmap previne double-free (IDEA #526) |
| **PROBLEMA: set_page_uc sem map_page_uc** | ⚠️ WARN | `mark_uc_or_warn` usa `set_page_uc` que SÓ funciona se a página já está mapeada. Se HHDM não mapeou a página → warn log mas sem falha → cache stale silencioso |

### 2.7 Paging (733 LOC) — ✅ Sólido

| Item | Estado | Notas |
|------|--------|-------|
| AddressSpace (CR3/CoW) | ✅ Funcional | `clone_current()`, `map_page()`, `map_user_page()` |
| PT Pool (256 frames) | ✅ Funcional | `init_pt_pool`, `alloc_pt_frame`, `dealloc_pt_frame` |
| W^X exec arena | ✅ Funcional | `jit_write_exec()` para WASM JIT |
| Ring3 iretq | ✅ GATED | `#[cfg(feature = "ring3")]` — ADR-0060, TRY_ENTER_RING3=false |
| sandbox_syscalls | ✅ Funcional | `int 0x90` dispatch |
| **PROBLEMA: alloc_frame usa allocator global** | ⚠️ WARN | `alloc_frame()` chama `alloc_zeroed_frame()` que usa o frame allocator global — se o heap esgota, CoW/Ring3 falham silenciosamente. PT pool mitiga mas não garante |

### 2.8 SMP (2,433 LOC) — ✅ Avançado

| Item | Estado | Notas |
|------|--------|-------|
| INIT-SIPI-SIPI direcionado | ✅ Funcional | Retry 3x, stacks per-AP |
| PerCpu / GS.base | ✅ Funcional | `ap_entry` configura |
| Work-stealing (Chase-Lev) | ✅ Funcional | `work_stealing.rs` |
| Run-queue per-CPU | ✅ Implementado | `runqueue.rs` (645 LOC) |
| CorePools (P/E/System/Compute) | ✅ Funcional | `corepools.rs` |
| **PROBLEMA: AP sem IDT** | ⚠️ GATED | APs sobem sem IDT → `hlt` trava. `ap_pollable` default OFF. Gate: `ADR-0065 FASE 3.1` |
| **PROBLEMA: TSC calibração** | ✅ Resolvido | `tsc.rs` com HPET→PIT→CPUID fallback |

### 2.9 Neural FS (3,031 LOC) — ✅ Produção

| Item | Estado | Notas |
|------|--------|-------|
| B-tree (538 LOC) | ✅ Funcional | Insert/search/split |
| Inode/directory | ✅ Funcional | CRUD completo |
| Journal (117 LOC) | ✅ Funcional | Write-ahead log |
| Checksum (59 LOC) | ✅ Funcional | CRC32C por arquivo |
| Volume (1128 LOC) | ✅ Funcional | Mount/read/write/format |
| **PROBLEMA: FAT não-átomo + crash** | ⚠️ WARN | Reescrever dir cluster a cada flush rasga FAT (SESSION_264) — fix com data-only |

### 2.10 FAT32 (1,260 LOC) — ✅ Maduro

| Item | Estado | Notas |
|------|--------|-------|
| Parser (BPB, clusters, dirs) | ✅ Funcional | BPS variável (512-4096) |
| format_fat32_bps | ✅ Funcional | Suporta 4Kn |
| ExFAT support | ✅ Funcional | Leitura |
| **PROBLEMA: read_sectors hardcoded 512B** | ⚠️ GAP | `fat32.rs` usa `bps` corretamente, mas `AtaDriver.read_sectors()` sempre lê 256 words |

## 3. Dead Code / Módulos Sem Caller

| Módulo | LOC | External Callers | Veredicto |
|--------|-----|------------------|-----------|
| `ntfs_reader` | ~300 | **0** | ❌ DEAD — leitor NTFS sem caller |
| `disk_power` | 25 | **2** (stub only) | ⚠️ STUB — `ata_idle` e `nvme_set_power` são logs |
| `hw_change` | ~200 | **0** | ❌ DEAD — hardware change detection |
| `proof_gate` | ~100 | **0** | ❌ DEAD — proof gate system |
| `storage_bw` | ~80 | **0** | ⚠️ Usado internamente (skip_measure) |
| `slab_buddy` | 59 | **0** | ❌ DEAD — slab buddy allocator |
| `numa_alloc` | 426 | **0** | ⚠️ AWAITING_HW — NUMA allocation para EPYC/Xeon |
| `kernel_hnsw` | ~200 | **0** | ❌ DEAD — HNSW kernel-space |
| `btrfs_reader` | ~150 | **1** (probe_super) | ⚠️ MÍNIMO — probe básico, sem write |
| `ext2_reader` | ~530 | **5** | ✅ Usado mas ext2 write é stub |
| `exfat_write` | ~100 | **11** | ✅ Usado (NeuralFS write path) |

## 4. Prioridades de Correção

| P | Gap | Esforço | Impacto | Bloqueador? |
|---|-----|---------|---------|-------------|
| **P0** | ATA: adicionar `lba_size` + read/setor adaptativo para 4Kn | Médio | Discos 4Kn reais funcionam | **SIM** (gate HW real) |
| **P1** | ATA: remover spin loop 10000 fixo, usar TSC sleep calibrado | Baixo | Timing real vs QEMU | Não |
| **P2** | DMA: `mark_uc_or_warn` → `map_page_uc` antes de `set_page_uc` | Médio | Cache stale silencioso | Não |
| **P3** | MHI: executor de migration (suggest_migration → mhi_tick real) | Alto | Dados migram entre tiers | Não |
| **P4** | Dead code cleanup: ntfs_reader, hw_change, proof_gate, slab_buddy, kernel_hnsw | Baixo | ~850 LOC mortos removidos | Não |
| **P5** | Disk Agent cache: integrar ao ATA/NVMe path principal | Médio | Cache LRU efetivo | Não |
| **P6** | AHCI: NCQ para performance | Alto | I/O paralelo | Não (funcional sem) |

## 5. O que Funciona (confirmado)

1. **ATA PIO read/write** — fix SESSION_112, funciona em QEMU e HW real
2. **NVMe admin/I/O queues** — 4Kn detectado via LBADS, PRP scatter-gather
3. **AHCI port discovery** — funcional para SATA básico
4. **Disk Agent unificado** — auto-detect ATA/AHCI/NVMe/USB
5. **DMA allocation** — UC/WB, contíguo, clflushopt, ownership check
6. **MHI tiered allocation** — 4 tiers, ticked, VRAM hook wired
7. **Paging + PT Pool** — 256 frames dedicados, CoW, sandbox, Ring3 gated
8. **SMP** — 4 cores, work-stealing, run-queue per-CPU, TSC calibrada
9. **Neural FS** — B-tree, journal, checksum, volume CRUD
10. **FAT32** — parser maduro, BPS variável, format_bps 4Kn-ready
