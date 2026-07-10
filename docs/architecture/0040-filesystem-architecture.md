# ADR-0040: Filesystem Architecture — SFS Híbrido Multi-FS

**Data:** 2026-07-10  
**Status:** Draft  
**Substitui:** ADR-0010 (Phase 4 — SFS), ADR-0030 (DiskIntelligenceAgent) parcialmente  
**Depende de:** ADR-0004 (Page Tables), ADR-0037 (NVMe DMA), ADR-0039 (Boot Flow)  
**Sprint Target:** N (exFAT + BlockDevice) até N+3 (SFS + Network)

---

## 1. Problema

O neural-os-core hoje tem apenas **FAT32** como FS gravável. Em HW real, isso significa:

- Pendrives >4GB (99% exFAT) → **não lê**
- HDs NTFS/EXT4 → **não lê** (só detecta assinatura)
- SDHC/SDXC → **não lê** (sem driver SDHCI)
- Modelos .bitnet >4GB → **não cabem** no FAT32 (limite 4GB)
- NVMe → **read-only** (sem escrita, sem SFS)
- MHI → **tracking only** (nunca move dados entre tiers)
- Múltiplas cópias: disco → kernel buffer → heap → tensor (3-4 cópias por operação)

Citando ADR-0010 (2026): *"Legacy filesystems introduce multiple copy operations: disk → kernel buffer → user buffer → application. This latency is unacceptable for real-time neural inference in Ring 0."*

---

## 2. Referências

### Crates.io (selecionadas por compatibilidade no_std)

| Crate | Versão | Função | Downloads | no_std? |
|-------|--------|--------|:---------:|:-------:|
| `embedded-sdmmc` | 0.9.0 | Driver SD/MMC, suporta FAT32 + exFAT | 328K | ✅ |
| `hadris-fat` | 1.2.1 | FAT12/16/32 + exFAT leitura/escrita | 5.4K | ✅ |
| `hadris-part` | 1.2.1 | Tabelas MBR + GPT + Hybrid MBR | 6.9K | ✅ |
| `exfat-slim` | 0.5.0 | exFAT embedded, safe Rust | 175 | ✅ |
| `embedded-exfat` | 0.4.0 | exFAT async + embedded | 9.2K | ✅ |
| `starry-fatfs` | 0.4.1 | FAT fork no_std melhorado | 23.8K | ✅ |
| `am-fs-core` | 0.2.2 | BlockRead/BlockDevice traits + LRU | 5.2K | ⚠️ (std?) |

### arXiv (relevantes para design)

| Paper | Ano | Ideia |
|-------|-----|-------|
| **MAIF** (2511.15097) | 2025 | Formato de arquivo AI-native com proveniência criptográfica, 2.720 MB/s streaming. Inspiração para `.bitnet` v5 com assinatura + metadados. |
| **ByteRover** (2604.01599) | 2026 | Memória hierárquica LLM como árvore de arquivos markdown. Zero vector DB. Valida nossa abordagem VFS + HermesFsAgent. |
| **SHIELD** (2501.16619) | 2025 | Detecção de ransomware em camada FS via deep filesystem features. 97.29% acurácia. Valida logs de filesystem como dado de segurança. |

---

## 3. Arquitetura Proposta — SFS Híbrido 3 Camadas

```
┌──────────────────────────────────────────────────────────────────┐
│                        VFS (já temos)                           │
│  resolve() → mount point → FilesystemAgent.read()               │
│  Mount: /mnt/hdd, /mnt/ram, /dev, /proc, /chat, /inference    │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  FilesystemDriver trait (NOVO)                           │   │
│  │  Trait unificado para todos os FS:                       │   │
│  │  - read(path, offset, buf) → usize                      │   │
│  │  - write(path, offset, data) → bool                     │   │
│  │  - mount(block_dev) → Result<FsInfo>                    │   │
│  │  - detect(block_dev) → Option<FsType>                   │   │
│  │  - format(block_dev, opts) → Result<()>                 │   │
│  │                                                          │   │
│  │  ┌────────┐ ┌────────┐ ┌────────┐ ┌──────┐ ┌────────┐ │   │
│  │  │ FAT32  │ │ exFAT  │ │ EXT2   │ │ NTFS │ │ SFS    │ │   │
│  │  │ (ok)   │ │ (N)    │ │ (N+1)  │ │(N+2) │ │ (N+2)  │ │   │
│  │  └────────┘ └────────┘ └────────┘ └──────┘ └────────┘ │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  BlockDevice+ (NOVO)                                     │   │
│  │  read_sectors(lba, buf) + write_sectors(lba, data)      │   │
│  │  DMA queue + coalescing + async callback                 │   │
│  │                                                          │   │
│  │  ATA PIO │ AHCI DMA │ NVMe SQ/CQ │ USB-MSC │ SDHCI     │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  MHI Ativo (NOVO)                                        │   │
│  │  alloc_by_tier() move dados entre tiers (DMA ring)      │   │
│  │  ARC cache com write-back + prefetch                     │   │
│  │  SMART monitoring + SelfHeal                             │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  NetworkBackend (N+3)                                    │   │
│  │  iSCSI, NFS, WebDAV, S3 via serial tunnel               │   │
│  └──────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

---

## 4. Planos de Implementação por Sprint

### Sprint N — Fundação Multi-FS

| Item | LOC | Descrição | Crate/Dependência |
|------|:---:|-----------|:-----------------:|
| **BlockDevice+ com write** | ~150 | Adicionar `write_sectors()` ao trait. Implementar em ATA, AHCI, NVMe, USB-MSC | `block_dev.rs` |
| **FilesystemDriver trait** | ~100 | Trait unificado para montar/ler/escrever em qualquer FS | `fs/driver.rs` |
| **exFAT leitura** | ~400 | Driver exFAT baseado em `hadris-fat` ou `exfat-slim`. Cluster bitmap, diretórios, arquivos >4GB | `exfat.rs` |
| **USB-MSC multisector** | ~200 | BOT com múltiplos setores por comando SCSI READ10/WRITE10 | `usb_msc.rs` |
| **DMA coalescing** | ~100 | `dma_alloc_coalesced()` com alinhamento para burst PCIe | `dma.rs` |
| **FilesystemAgent** | ~50 | Adaptador VFS → FilesystemDriver | `fs/agent.rs` |
| | **~1.000 LOC total** | | |

### Sprint N+1 — EXT2 + Instalação

| Item | LOC | Descrição |
|------|:---:|-----------|
| **EXT2 leitura** | ~400 | Inode bitmap, block groups, diretórios. Base para EXT3/4 futuramente |
| **Instalação do pendrive → HD** | ~300 | App `SysInstaller` (Settings → Install). Copia kernel + modelos do pendrive FAT32/exFAT para EXT2 no HD. IA auxilia escolha de partição, tamanho, FS |
| **Formatador de partições** | ~200 | `disk_format(dev, fs_type)` via `FilesystemDriver::format()` |
| **SMART diagnóstico** | ~200 | ATA SMART + NVMe health log. SelfHealAgent analisa e reporta | 
| | **~1.100 LOC total** | |

### Sprint N+2 — SFS + NTFS leitura

| Item | LOC | Descrição |
|------|:---:|-----------|
| **SFS Namespace** | ~500 | `0x5000_0000_0000 - 0x6000_0000_0000` mapeado no VAS. NVMe → page tables → acesso direto |
| **zerocopy transmute** | ~100 | `Tensor::from_raw_pages()` sem cópia |
| **NTFS leitura** | ~800 | `$MFT` parsing, atributos residentes/não-residentes, diretórios |
| **MHI ativo** | ~400 | `alloc_by_tier(Nvme)` move dados via DMA ring. ARC cache write-back |
| **App FS Manager** | ~300 | UI Settings: lista partições, monta/desmonta, formata, mostra SMART, espaço livre |
| | **~2.100 LOC total** | |

### Sprint N+3 — Network + SelfHeal

| Item | LOC | Descrição |
|------|:---:|-----------|
| Network mounts (iSCSI/NFS) | ~400 | via serial tunnel + protocolo host |
| SelfHeal FS | ~200 | Detecção de corrupção, journal replay, fallback para snapshot |
| NTFS escrita | ~800 | (se necessário) |
| EXT3/4 journal | ~600 | (se necessário) |

---

## 5. FS Deferidos (com justificativa)

| FS | Motivo | Prioridade |
|----|--------|:----------:|
| **Btrfs** | COW, subvolumes, checksum, RAID. ~5000+ LOC. Inviável no_std. | 🔴 Nunca |
| **ZFS** | ~1M LOC C. Impossível portar. | 🔴 Nunca |
| **HFS+** | Apple legado, obsoleto. | 🔴 Nunca |
| **ReFS** | Microsoft, sem documentação pública. | 🔴 Nunca |
| **EROFS** | Read-only Linux embedded. Se precisarmos, ~300 LOC. | 🔵 Se necessário |
| **XFS** | Journal + extent trees. ~2000 LOC. NTFS tem prioridade. | 🟡 Baixa |

---

## 6. Integração com o Ecossistema Neural

### MHI + SFS

```
Tensor.aloca(shape, tier=Sfs) → SFS aloca páginas NVMe → mapeia no VAS
    → retorna *mut f32 apontando direto para o NVMe
    → MHI registra acesso → suggest_migration() se padrão mudar
    → zero cópias durante todo o ciclo de vida
```

### SelfHeal + SMART

```
DiskIntelligenceAgent tick:
  1. Le SMART de cada disco (ATA 0xB0, NVMe 0x02)
  2. Se realocado > threshold → SelfHealAgent copia dados para outro disco
  3. Publica KERNEL_ERROR com diagnóstico
```

### Instalação com IA

```
SysInstaller:
  1. Detecta discos: nvme0, sda1 (pendrive)
  2. LLM sugere: "Instalar em nvme0 com partição EXT2 de 32GB"
  3. Usuário confirma: /sysinstall /dev/nvme0 --from /mnt/pendrive
  4. Kernel: formata nvme0 como EXT2 → copia BITNET.BIN, HWEXPRT.BIN, CONFIG
  5. Configura bootloader para próximo boot via nvme0
```

### App FS Manager (Settings)

```
┌─────────────────────────────────────┐
│  Gerenciador de Discos              │
│                                     │
│  ┌───────────────────────────────┐  │
│  │ nvme0: 256GB │ NVMe │ SAMSUNG│  │
│  │ ├─ nvme0p1: 128GB EXT2  🟢    │  │
│  │ │  Usado: 3.2GB | Livre: 124GB│  │
│  │ └─ nvme0p2: 128GB EXT4  🟡    │  │
│  │    SMART: 98% | Temp: 42°C     │  │
│  └───────────────────────────────┘  │
│  ┌───────────────────────────────┐  │
│  │ sda: 32GB │ USB 3.0 │ SanDisk│  │
│  │ └─ sda1: 32GB exFAT          │  │
│  │    /mnt/pendure               │  │
│  └───────────────────────────────┘  │
│                                     │
│  [ Montar ] [ Desmontar ] [ Smart ] │
│  [ Instalar Neural neste disco ]    │
└─────────────────────────────────────┘
```

---

## 7. Tratamento de Erros e Recuperação

| Erro | Detecção | Ação |
|------|----------|------|
| **Bad sector** | SMART realocado > threshold | SelfHeal copia para setor reserva. Alerta no log |
| **FS corrompido** | Assinatura inválida no mount | Tenta `fsck` básico (EXT2) ou `chkdsk` (FAT). Se falhar, monta read-only |
| **Disco removido** | USB hotplug event | Desmonta VFS, limpa cache ARC, publica evento |
| **NVMe offline** | Health check timeout | Fallback para ATA. MHI migra dados para DRAM |
| **Boot sem disco** | DiskIntelligenceAgent não acha bootável | Mostra tela "Insira um pendrive com neural-os-core" |
| **Cluster loop FAT** | `cluster_iter > max_clusters` (já implementado) | Retorna `None` em vez de loop infinito |

---

## 8. Resumo de Esforço Total

| Sprint | Foco | LOC | Dependências |
|--------|------|:---:|--------------|
| N | exFAT + BlockDevice+ | ~1.000 | ATA, AHCI, NVMe (já temos) |
| N+1 | EXT2 + Instalação | ~1.100 | Sprint N, Bootloader |
| N+2 | SFS + NTFS leitura + App | ~2.100 | Sprint N+1, NVMe DMA |
| N+3 | Network + SelfHeal | ~2.000 | Sprint N+2, Serial tunnel |
| | **Total** | **~6.200** | |

### Release Recommendation

⏳ **Não bloquear v1.0.** FAT32 existente cobre boot e config. exFAT (Sprint N) é o mínimo para HW real com pendrives >4GB. SFS (Sprint N+2) é o salto de performance para modelos grandes.
