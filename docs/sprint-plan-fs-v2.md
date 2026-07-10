# Sprint Plan FS-v2 — Storage & Memory Ecosystem
# A Era do Armazenamento — Multi-FS, Multi-TB, Multi-Tier

**Data:** 2026-07-10  
**Versão alvo:** v2.0.0-alpha  
**Lema:** *"FAT32 morreu. O armazenamento é Tier 0."*

---

## Visão Geral

O neural-os-core hoje lê apenas FAT32 (limite 4GB/arquivo) e gerencia 4GB de DRAM.
O mundo real tem NVMe de 8TB, HDs de 24TB, pendrives exFAT de 2TB, SDHC de 1TB, e VRAM de 24GB.
Precisamos que o kernel entenda, gerencie e mova dados entre **todos eles**, como um único pool de memória hierárquica.

---

## Subsprint FS-a: Fundação Multi-FS (~1.550 LOC)

**Foco:** exFAT, escrita em todos os drivers, agente de storage inteligente

| Item | IDEIA | LOC | Descrição | Depende de |
|------|-------|:---:|-----------|------------|
| **exFAT leitura/escrita** | #417 | ~400 | Driver exFAT via `hadris-fat` ou `exfat-slim`. Cluster bitmap, diretórios, arquivos >4GB, FAT sub-allocation | — |
| **BlockDevice+ com write** | #417 | ~150 | `write_sectors()` no trait + DMA coalescing + alinhamento 4K. ATA, AHCI, NVMe, USB-MSC implementam | — |
| **USB-MSC multisector** | #417 | ~200 | BOT com READ10/WRITE10 múltiplos setores. Sem single-sector lento | — |
| **DiskIntelligenceAgent v2** | #418 | ~800 | Reescrita do agente: probe real de exFAT/NTFS/EXT com mount. Benchmark bandwidth/latência. Hotplug USB/SDHC via EventBus. SMART preventivo com SelfHeal. ARC cache dinâmico (MB-GB). I/O scheduler com deadline + CFQ-like | BlockDevice+ |
| **GPT escrita** | — | ~150 | `gpt_write()` para criar/modificar partições. Necessário para instalação e formatação | — |
| **Sector size 4Kn** | — | ~100 | Detecção de setor físico 512 vs 4096. Alinhamento de partição automático. TRIM/discard para SSD/NVMe | BlockDevice+ |
| | | **~1.800 LOC** | | |

## Subsprint FS-b: Acessando HDs do Usuário (~1.200 LOC)

**Foco:** Ler HDs NTFS/EXT do usuário, gerenciar partições, monitorar saúde

| Item | IDEIA | LOC | Descrição | Depende de |
|------|-------|:---:|-----------|------------|
| **EXT2 leitura** | — | ~400 | Inode bitmap, block groups, diretórios, symlinks. Base para EXT3/4 | FS-a |
| **NTFS leitura** | — | ~800 | `$MFT` parsing, atributos residentes/não-residentes, diretórios, streams alternados, ACLs básicas | FS-a |
| **SMART longo + histórico** | — | ~200 | Teste SMART extended (leituras destrutivas). Log de histórica de realocados, temperatura, taxa de erro. Alerta preditivo: "este HD vai morrer em ~30 dias" | FS-a |
| **Bad block management** | — | ~200 | Detecção de bad blocks em tempo real (ATA REALLOCATE, NVMe e média). Mapa de setores realocados. Notificação ao SelfHealAgent | FS-b |
| | | **~1.600 LOC** | | |

## Subsprint FS-c: MHI Ativo + App Gestão (~1.400 LOC)

**Foco:** Mover dados entre tiers de verdade, interface de gestão, instalação

| Item | IDEIA | LOC | Descrição | Depende de |
|------|-------|:---:|-----------|------------|
| **MHI Ativo com DMA ring** | #420 | ~500 | `mhi_tick()` executa `arc_suggest_tier()` movendo dados via DMA: DRAM↔NVMe, DRAM↔VRAM (PCIe), NVMe↔HDD. SPSC ring + write-back. MSched + Buddy allocator integrados | FS-b |
| **ARC cache dinâmico** | — | ~200 | Substitui cache fixo de 1MB. Configurável por tier (ex: 256MB para NVMe, 64MB para HDD). Write-back coalescing com dirty page tracking | FS-b |
| **I/O Scheduler avançado** | — | ~200 | Deadline (prioridade por latência) + CFQ (justiça entre processos). Rotação vs SSD detect automático. Multi-fila por core | FS-a |
| **FilesystemDriver trait + App FS Manager** | #419 | ~500 | Trait unificado. App Settings → Storage: lista partições, monta/desmonta, formata (exFAT/EXT2), benchmark real (iops, latência, throughput), SMART health, gráfico de temperatura, gerenciamento de tiers MHI (promover/rebaixar manual) | FS-b |
| **Instalador Neural com IA** | #421 | ~400 | App SysInstaller. Detecta discos. LLM sugere: "nvme0p1: 32GB EXT2 para sistema, nvme0p2: resto para dados". Usuário confirma. Kernel: GPT, format exFAT/EXT2, copia BITNET.BIN + HWEXPRT.BIN + CONFIG + SKILLS/, configura bootloader | FS-c |
| | | **~1.800 LOC** | | |

## Subsprint FS-d: Rede + Proteção + NTFS escrita (~2.000 LOC)

**Foco:** Storage em rede, filesystem resilience, escrita em HDs Windows/Linux

| Item | IDEIA | LOC | Descrição | Depende de |
|------|-------|:---:|-----------|------------|
| **Network mounts** | — | ~400 | iSCSI initiator, NFS v3 client, WebDAV — via serial tunnel para o host. Protocolo host-side bridge (igual serial_bridge.py) | FS-c |
| **S3 / Cloud Storage** | — | ~300 | Montar bucket S3 como diretório local (via tunnel HTTP). Cache local ARC. Upload/Download assíncrono | FS-c |
| **Filesystem SelfHeal** | — | ~300 | Checksum em cada leitura. Se corrompido, tenta recovery (journal EXT3, $LogFile NTFS). Fallback para snapshot automático. Quarentena de bad blocks | FS-c |
| **NTFS escrita** | — | ~800 | Criar/modificar arquivos em NTFS. Suporta $MFT growth, non-resident attributes, compressed files. Crítico para HDs Windows do usuário | FS-b |
| **EXT3/4 journal + escrita** | — | ~600 | Journal replay, extents, criação de arquivos. Para HDs Linux do usuário | FS-b |
| **Disk power management** | — | ~200 | Spin-down de HDDs após N minutos inativos (ATA IDLE). NVMe PS (power state) dinâmico. SSD APC (Aggressive Power Control) | FS-c |
| | | **~2.600 LOC** | | |

---

## Resumo de Esforço

| Subsprint | Foco | LOC | Itens |
|:---------:|------|:---:|:-----:|
| **FS-a** | Fundação Multi-FS | ~1.800 | 6 |
| **FS-b** | HDs do Usuário | ~1.600 | 4 |
| **FS-c** | MHI Ativo + App | ~1.800 | 6 |
| **FS-d** | Rede + Proteção | ~2.600 | 7 |
| | **Total** | **~7.800** | **23** |

---

## Dependências

```
FS-a (exFAT + BlockDevice+ + DiskAgent v2)
  │
  ├── FS-b (NTFS + EXT2 + SMART)
  │     │
  │     ├── FS-c (MHI Ativo + App + Instalador)
  │     │     │
  │     │     └── FS-d (Rede + SelfHeal + NTFS w)
  │     │
  │     └── FS-d depende de FS-b (NTFS/EXT prontos)
  │
  └── FS-d (SelfHeal) depende da SMART de FS-b
```

## Cronograma Estimado

| Subsprint | Início | Término | Marcos |
|:---------:|:------:|:-------:|--------|
| **FS-a** | Imediato | N+3 sem | exFAT monta pendrive de 2TB, todos os drivers escrevem |
| **FS-b** | N+4 sem | N+8 sem | NTFS/EXT lê HDs do usuário, SMART preditivo |
| **FS-c** | N+8 sem | N+12 sem | MHI move dados entre tiers, App Storage funcional, instalador pendrive→HD |
| **FS-d** | N+12 sem | N+18 sem | Network mounts, SelfHeal, NTFS escrita, power management |

## O que NÃO está incluído (arquivado)

| Item | Motivo |
|------|--------|
| **Btrfs r/w** | Complexidade COW + RAID. ~5000+ LOC. Inviável. |
| **ZFS** | ~1M LOC C. Impossível portar. |
| **HFS+/APFS** | Apple legado sem docs. |
| **ReFS** | Microsoft sem especificação pública. |
| **Erasure coding / RAID** | Hardware RAID já é detectado. Software RAID (md) é raro em desktop. |
| **Deduplicação** | Complexidade alta, ganho marginal em single-user. |
| **Compressão on-fly** | Zlib/lz4 em software é caro. Deixar para quando NVMe DMA estiver maduro. |

---

## Integração com outros sistemas

| Sistema | Como se conecta |
|---------|-----------------|
| **Cortex LLM** | Recebe perguntas "quanto espaço livre?" via Hermes → responde com dados do DiskAgent |
| **SelfHeal** | SMART alerts + bad blocks → SelfHealAgent copia dados para setor reserva ou outro disco |
| **Hermes** | `/storage` comando lista discos, `/sysinstall` inicia instalação |
| **Display** | App FS Manager com gráficos de temperatura, espaço, benchmark |
| **Bootloader** | Boot下一跳 configurado pelo Instalador para próximo boot pelo HD |
