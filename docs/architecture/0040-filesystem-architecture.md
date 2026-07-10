# ADR-0040: Neural Memory & Storage Architecture — MHI Ativo + Multi-FS

**Data:** 2026-07-10 (v2)  
**Status:** Draft  
**Substitui:** ADR-0010 (SFS Phase 4), ADR-0030 (DiskIntelligenceAgent), ADR-0040 v1  
**Depende de:** ADR-0004 (Page Tables), ADR-0037 (NVMe DMA GPU), ADR-0039 (Boot Flow)

---

## 1. Realidade do Mundo Real

| Mídia | Capacidade típica | Interface | FS comum | Nosso suporte |
|-------|:-----------------:|:---------:|:--------:|:--------------:|
| DRAM | 16-128 GB | Memória | — | ✅ Buddy allocator |
| **VRAM** (GPU) | 4-24 GB | PCIe BAR | — | ✅ Buddy + MSched |
| NVMe | 1-8 TB | PCIe 3.0/4.0 | NTFS / EXT4 | ✅ Leitura, ⚠️ sem escrita |
| SSD SATA | 512 GB - 4 TB | AHCI | NTFS / EXT4 | ✅ AHCI, ⚠️ sem escrita |
| HDD | 4-24 TB | SATA / SAS | NTFS / EXT4 / Btrfs | ✅ ATA, ⚠️ sem escrita |
| Pendrive | 256 GB - 2 TB | USB 3.0 | **exFAT** / NTFS | ❌ Só FAT32 (4GB limite) |
| SDHC/SDXC | 512 GB - 1 TB | SDHCI / USB | **exFAT** | ❌ Não lê |
| Cloud | ∞ | Rede | S3 / WebDAV | ❌ Não existe |

**Conclusão:** FAT32 morreu. exFAT é o **mínimo viável** para qualquer armazenamento removível moderno.

---

## 2. Arquitetura MHI 2.0 — O Cérebro da Memória

### 2.1 Hierarquia Real

```
Tier 0: VRAM (GPU)     ─ 4-24 GB     →  2 GB/s (GTX 1050)
Tier 1: DRAM            ─ 16-128 GB   →  20 GB/s
Tier 2: NVMe            ─ 1-8 TB      →  3.5 GB/s
Tier 3: SSD/HDD         ─ 512GB-24TB  →  500 MB/s / 200 MB/s
Tier 4: USB/SDHC        ─ 256GB-2TB   →  400 MB/s (USB 3.0)
Tier 5: Cloud/Network   ─ ∞           →  10-100 MB/s
```

**A regra de ouro:** cada tier é 5-10× mais lento que o anterior e 10-100× maior.

### 2.2 O que já temos (e funciona)

| Componente | Status | O que faz |
|------------|--------|-----------|
| **VRAM Buddy** (`gpu/vram.rs`) | ✅ | Allocator power-of-2, split/merge, BAR2 mapping UC, 2MB huge pages |
| **MSched** (`gpu/msched.rs`) | ✅ | Belady OPT eviction predictor, janela 1024 acessos |
| **DRAM alloc** (`mhi.rs`) | ✅ | `alloc_by_tier(Dram)` via frame allocator |
| **AllocProfile** (`mhi.rs`) | ✅ | Tracking de acesso, latência, dono por alocação |
| **arc_suggest_tier()** (`mhi.rs`) | ✅ | MFU→VRAM, MRU→NVMe, cold→HDD |
| **Megatrain queue** (`mhi.rs`) | ✅ | Prefetch requests entre tiers (vacuum ainda) |
| **MHI_REGISTRY** (`mhi.rs`) | ✅ | Banco central de todas as alocações |

### 2.3 O que falta — Mover dados de verdade

O MHI hoje **sugere** migração mas **nunca executa**. O pulo do gato é:

```rust
// MHI Ativo — executa migrations em background
pub fn mhi_tick() {
    // 1. Sugere migrations baseado em perfil de acesso
    let migrations = MHI_REGISTRY.lock().suggest_migration(tick);
    
    // 2. Para cada migração, executa DMA copy entre tiers
    for (addr, from, to) in migrations {
        match (from, to) {
            (Nvme, Dram) => { /* NVMe → DRAM: copy via DMA */ }
            (Dram, Vram) => { /* DRAM → VRAM: pcie_dma() */ }
            (Dram, Nvme) => { /* DRAM → NVMe: swap out */ }
            (Hdd, Nvme) => { /* HDD → NVMe: prefetch */ }
        }
    }
    
    // 3. ARC cache write-back para dirty pages
    ARC_CACHE.lock().writeback_if_needed();
}
```

**DMA ring entre tiers:** usando o `dma_alloc_coalesced()` + SPSC ring do SMP:

```
NVMe ──[DMA]──→ DRAM    (prefetch de modelo)
DRAM ──[PCIe]─→ VRAM    (matmul offload)
DRAM ──[DMA]──→ NVMe    (swap out de KV-cache frio)
NVMe ──[DMA]──→ HDD     (cold storage)
```

---

## 3. FS Multi-Tier (substitui ADR-0040 v1)

### 3.1 Plano de implementação realista

| Sprint | FS | Por que | LOC | Crate |
|--------|:---:|---------|:---:|-------|
| **N** | **exFAT r/w** | Pendrives, SDHC, câmeras, >4GB | ~400 | `hadris-fat` ou `exfat-slim` |
| **N** | **BlockDevice+ write** | NVMe, AHCI, USB-MSC escreverem | ~150 | `block_dev.rs` |
| **N+1** | **EXT2 r** | HDs Linux, raiz de boot | ~400 | `ext2.rs` próprio |
| **N+1** | **NTFS r** | HDs Windows, NVMe do usuário | ~800 | `ntfs.rs` próprio |
| **N+2** | **MHI ativo** | Migração real entre tiers | ~500 | `mhi.rs` + DMA |
| **N+2** | **ARC cache dinâmico** | Cache LRU/MFU configurável (MB-GB) | ~300 | `disk_agent/cache.rs` |
| **N+3** | **NTFS w** | Escrever em HD do Windows | ~800 | — |
| **N+3** | **EXT3/4 w** | Escrever em HD Linux | ~600 | — |
| **N+3** | **Cloud mounts** | S3/WebDAV via serial tunnel | ~400 | `stt_bridge.py` pattern |

### 3.2 FS Deferidos (justificativa)

| FS | Motivo |
|----|--------|
| **Btrfs** | COW, subvolumes, checksum, RAID. ~5000+ LOC. Inviável no_std. |
| **ZFS** | ~1M LOC C. Impossível portar. Arcabouço de storage completo. |
| **HFS+/APFS** | Apple legado. Sem documentação pública de APFS. |
| **ReFS** | Microsoft reservado. Sem especificação pública. |
| **EROFS** | Read-only Linux embedded. Se precisarmos, ~300 LOC. |

### 3.3 FilesystemDriver Trait

```rust
pub trait FilesystemDriver: Send {
    fn name(&self) -> &str;
    fn detect(block: &mut dyn BlockDevice) -> Option<Box<Self>> where Self: Sized;
    fn mount(&mut self, block: &mut dyn BlockDevice) -> Result<(), &'static str>;
    fn read(&self, path: &str, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str>;
    fn write(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<(), &'static str>;
    fn list(&self, path: &str) -> Result<alloc::vec::Vec<alloc::string::String>, &'static str>;
    fn free_space(&self) -> u64;
    fn total_space(&self) -> u64;
}
```

---

## 4. VRAM + MSched — O Que Já Temos

### `gpu/vram.rs` ✅ Buddy Allocator

```
niveis: 2^12 (4KB) a 2^32 (4GB)
alloc(size) → split até order mínimo
free(addr, size) → merge com buddy se livre
init_vram_tier(gpu) → mapeia BAR2 como UC 2MB pages
```

### `gpu/msched.rs` ✅ Belady Predictor

```rust
MschedPredictor {
    history: VecDeque<u64>,  // 1024 acessos
    future: BTreeMap<u64, Vec<usize>>, // indices futuros
}
predict_evict(working_set) → qual página evictar
```

### Integração MHI + VRAM

```
Tensor.aloca(shape, tier=Vram)
  → mhi::alloc_by_tier(Vram, size)
  → vram::vram_alloc(size)  // buddy allocator
  → mhi_register(addr, size, Vram, "tensor")
  → msched_record(addr)
  
Se VRAM cheia:
  → msched_predict(working_set) → addr_vitima
  → vram_free(addr_vitima, size)
  → mhi_migrate(addr_vitima, Vram→Dram)  // copia via PCIe DMA
```

---

## 5. Instalação do Neural do Pendrive para o HD

### Fluxo com IA

```
1. Boot por pendrive (FAT32/exFAT)
2. SysInstallerAgent detecta discos:
   - nvme0: 1TB (vazio)
   - sda: 32GB (pendrive com neural)
3. LLM sugere: "Instalar em nvme0p1 como EXT2, 32GB"
4. Usuário: /sysinstall /dev/nvme0 --from /mnt/pendrive
5. Kernel:
   a. Lê MBR/GPT de nvme0
   b. Cria partição primária EXT2 (32GB)
   c. Formata como EXT2 via FilesystemDriver::format()
   d. Monta /mnt/nvme
   e. Copia: BITNET.BIN, HWEXPRT.BIN, CONFIG, SKILLS/
   f. Configura bootloader: "boot_next = /dev/nvme0p1"
6. Reboot → boot pelo NVMe
```

### App FS Manager (Settings → Storage)

```
┌──────────────────────────────────────────┐
│  Gerenciador de Armazenamento            │
│                                          │
│  nvme0: 1TB NVMe (Samsung 980 Pro)      │
│  ├─ nvme0p1: 256GB EXT2  🟢 montado     │
│  │  Usado: 3.2GB | Livre: 252GB         │
│  │  SMART: 98% | Temp: 42°C             │
│  │  [Desmontar] [SMART] [Benchmark]      │
│  └─ nvme0p2: 744GB EXT4  🟡 detectado   │
│     [Montar] [Formatar]                  │
│                                          │
│  sda: 64GB USB 3.0 (SanDisk)            │
│  └─ sda1: 64GB exFAT 🟢 montado         │
│     /mnt/pendrive                       │
│     [Instalar Neural neste computador]   │
│                                          │
│  [Benchmark TODOS os discos]             │
└──────────────────────────────────────────┘
```

---

## 6. Tratamento de Erros e Recuperação

| Erro | Detecção | Ação |
|------|----------|------|
| **VRAM falha** | Teste write_volatile → read_volatile | Fallback para DRAM. MHI marca VRAM como offline |
| **NVMe timeout** | Health check (identify controller) | Remove tier NVMe do MHI. Dados em DRAM são preservados |
| **Bad sector SMART** | `reallocated_sector_count > threshold` | SelfHeal copia setor para reserva. Alerta no log |
| **FS corrompido** | Mount falha | Tenta fsck EXT2 / chkdsk FAT. Fallback read-only |
| **Pendrive removido** | USB hotplug | Desmonta VFS, limpa cache ARC, publica EVENT |
| **Disco cheio** | `free_space() < 5%` | MHI migra cold data para tier superior. Alerta |
| **Boot sem disco** | DiskAgent não acha bootável | Tela: "Insira pendrive com neural-os-core" |

---

## 7. Resumo de Esforço

| Sprint | Foco | LOC |
|--------|------|:---:|
| **N** | exFAT + BlockDevice+ write | ~550 |
| **N+1** | EXT2 r + NTFS r + MHI ativo (DRAM↔NVMe) | ~1.700 |
| **N+2** | ARC dinâmico + MHI VRAM + NTFS w | ~1.100 |
| **N+3** | Cloud mounts + SelfHeal + App FS | ~1.000 |
| | **Total** | **~4.350** |
