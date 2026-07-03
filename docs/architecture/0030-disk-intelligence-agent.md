# ADR-0030: DiskIntelligenceAgent — "O Mestre dos Discos"

**Data:** 2026-07-03  
**Status:** Draft  
**Sprint:** 75

---

## 1. Problema

Hoje o kernel descobre discos e FS de forma fragmentada:
- `ata.rs` probe manual → `fat.rs` mount → `mhi.rs` registro
- `usb_msc.rs` stub (bulk I/O não funciona)
- NVMe, AHCI, RAID, SCSI não têm driver
- Cada FS novo exige código novo e manual
- Sem cache, sem notificação de hotplug, sem unified topology
- MHI sugere tiers mas **nunca move dados**
- Nenhuma inteligência sobre qual interface é mais rápida
- Volume managers (LVM, LUKS, BitLocker) ignorados — partição é tratada como FS direto
- Nenhuma abstração para armazenamento em nuvem/rede (iSCSI, NVMe-oF, NBD)

## 2. Solução: DiskIntelligenceAgent

Um **agente System (Oneshot → Continuous)** que centraliza TODO o armazenamento:

```
┌──────────────────────────────────────────────────────────────────────┐
│                         DiskIntelligenceAgent                          │
│                                                                        │
│  ┌──────────────────┐  ┌─────────────────────┐  ┌──────────────────┐  │
│  │ StorageRegistry  │  │ VolumeManagerProbe  │  │ FsProbeRegistry  │  │
│  │ (6+ control.)    │  │ (LVM, LUKS, BS...)  │  │ (35+ signatures)  │  │
│  └────────┬─────────┘  └──────────┬──────────┘  └────────┬─────────┘  │
│           │                       │                       │           │
│  ┌────────▼───────────────────────▼───────────────────────▼──────────┐ │
│  │                     TopologyMap                                    │ │
│  │   Disk → VolumeGroup → LogicalVolume → Partitions → FS → Tier     │ │
│  └────────────────────────────────┬───────────────────────────────────┘ │
│                                   │                                    │
│  ┌────────────────────────────────▼──────────────────────────────────┐ │
│  │          AccessTracker + ArcCache + BandwidthBench                 │ │
│  │  (mede MB/s real, MFU/MRU, prefetch, write-back coalesce)         │ │
│  └────────────────────────────────┬──────────────────────────────────┘ │
│                                   │                                    │
│  ┌────────────────────────────────▼──────────────────────────────────┐ │
│  │              EventBus Publisher + Insight Engine                   │ │
│  │  DISK_ATTACHED/DETACHED/TIER_PROMOTED/TIER_DEMOTED/FS_DETECTED    │ │
│  └───────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────┘
         │                    │                       │
         ▼                    ▼                       ▼
       MHI                  VFS                  Cortex/Hermes
   (tier registry)       (mount points)        (topology insights)
```

## 3. Três Tiers de Operação

| Tier | Onde | O quê | Responsável |
|------|------|-------|-------------|
| **Tier 0** | Boot (hardcoded) | ATA PIO + FAT32 read (KERNEL~1) + FAT32 write (boot log) + Ed25519 + TPM | `main.rs` inline |
| **Tier 1** | Agent primeiro tick | Probe TODOS os controladores, discos, volumes, partições, FS. MHI register. VFS mount. | `DiskIntelligenceAgent.probe_all()` |
| **Tier 2** | Agent ticking | ARC cache, tier migration real, hotplug, benchmark re-mede, insights | `DiskIntelligenceAgent.tick()` |

**Tier 0 é o minimo absoluto** — só o que o kernel precisa antes de qualquer agente existir. O agente **re-probeia** inclusive o disco de boot para descobrir partições que o Tier 0 ignorou.

## 4. Traits

### StorageController
```rust
pub trait StorageController: Send {
    fn name(&self) -> &str;
    fn controller_type(&self) -> ControllerType;
    fn pci_bdf(&self) -> Option<(u8, u8, u8)>; // bus, dev, func
    fn probe_disks(&mut self) -> Vec<RawDisk>;
    fn read_blocks(&self, disk: u8, lba: u64, buf: &mut [u8], blocks: usize) -> bool;
    fn write_blocks(&self, disk: u8, lba: u64, data: &[u8], blocks: usize) -> bool;
}
```

### VolumeManagerProbe
```rust
pub trait VolumeManagerProbe: Send {
    fn name(&self) -> &str;
    fn probe(&self, disk: &RawDisk, read_fn: &dyn Fn(u64, &mut [u8]) -> bool)
        -> Option<VolumeGroup>;
}

pub struct VolumeGroup {
    pub name: String,
    pub uuid: String,
    pub technology: VolumeTech, // Lvm2, LUKS, BitLocker, CoreStorage, MdRaid, NtfsLdm, Zpool, BtrfsVol
    pub sub_volumes: Vec<LogicalVolume>,
}

pub struct LogicalVolume {
    pub name: String,
    pub uuid: String,
    pub lba_start: u64,
    pub lba_end: u64,
    pub fs_info: Option<FsInfo>,
}
```

### FilesystemProbe
```rust
pub trait FilesystemProbe: Send {
    fn fs_type(&self) -> FilesystemType;
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, max_lba: u64)
        -> Option<FsInfo>;
    fn priority(&self) -> u8; // ordem de tentativa (mais específico = maior)
}

pub struct FsInfo {
    pub fs_type: FilesystemType,
    pub label: String,
    pub uuid: String,
    pub total_bytes: u64,
    pub free_bytes: Option<u64>, // None se o probe não sabe calcular
    pub block_size: u32,
    pub is_writeable: bool,
    pub is_case_sensitive: bool,
    pub max_name_len: u8,
}
```

### StorageProvider (Rede/Nuvem)
```rust
pub trait StorageProvider: Send {
    fn name(&self) -> &str;
    fn requires_network(&self) -> bool;
    fn probe(&self, net_ready: bool) -> Vec<RawDisk>;
    fn needs_poll(&self) -> bool; // se requer tick() para manter conexão
    fn tick(&mut self);
    fn read_blocks(&self, disk: u8, lba: u64, buf: &mut [u8], blocks: usize) -> bool;
    fn write_blocks(&self, disk: u8, lba: u64, data: &[u8], blocks: usize) -> bool;
}
```

## 5. Estruturas de Dados

### RawDisk
```rust
pub struct RawDisk {
    pub name: String,              // "sda", "nvme0n1", "usb1"
    pub controller: String,        // "ata0", "xhci1", "nvme0"
    pub pci_bdf: Option<(u8, u8, u8)>,
    pub capacity_bytes: u64,
    pub sector_size: u16,          // 512 (ATA), 4096 (Advanced Format), 2048 (ATAPI)
    pub interface: InterfaceType,
    pub is_removable: bool,
    pub is_volatile: bool,         // USB, iSCSI (pode sumir)
    pub model: String,
    pub serial: String,
    pub firmware_rev: String,
    pub max_read_bw_mbs: u32,      // benchmark real
    pub max_write_bw_mbs: u32,
    pub rotational: bool,          // true = HDD, false = SSD
    pub partitions: Vec<PartitionInfo>,
    pub volume_groups: Vec<VolumeGroup>, // LVM, LUKS, etc.
}
```

### PartitionInfo
```rust
pub struct PartitionInfo {
    pub index: u8,
    pub lba_start: u64,
    pub lba_end: u64,
    pub sector_count: u64,
    pub mbr_type: u8,             // 0x0C, 0x1C, 0x07, 0x83, etc.
    pub gpt_guid: Option<[u8; 16]>,
    pub fs_info: Option<FsInfo>,
    pub is_bootable: bool,
    pub mhi_tier: AllocTier,
    pub mount_point: Option<String>,
}
```

### Enums
```rust
pub enum ControllerType {
    Ata, Ahci, Nvme, Usb, Scsi, Sas, Raid, Atapi,
    SdMmc, Virtio, Nvdimm, Unknown,
}

pub enum InterfaceType {
    Pata, Sata, SataExpr, Usb2, Usb3, UsbC, Usb31,
    Nvme, Pcie5, Pcie4, Pcie3,
    Scsi, Sas3, Sas4, Fc, FcNvme,
    Virtio, Nvdimm, Loopback, Unknown,
}

pub enum VolumeTech {
    None, Lvm2, LUKS1, LUKS2, BitLocker, CoreStorage,
    MdRaid0, MdRaid1, MdRaid5, MdRaid6, MdRaid10,
    NtfsLdm, Zpool, BtrfsVol, ApfsContainer,
}

pub enum FilesystemType {
    // DOS/Windows
    Fat12, Fat16, Fat32, ExFat, Ntfs, ReFs,
    // Linux
    Ext2, Ext3, Ext4, Xfs, Jfs, Reiser, Reiser4, Btrfs,
    F2fs, Nils2, Ocfs2, Squashfs, Cramfs, Romfs, Minix,
    // Apple
    Hfs, HfsPlus, Apfs,
    // Solaris/*
    Zfs, Ufs, Vxfs, Efs,
    // Optical
    Iso9660, Udf,
    // Embedded
    Ubifs, Jffs2, Yaffs2, Logfs, Smartfs,
    // SGI/HP
    BeFs, Qnx4, Qnx6, Sysv, Xenix, Hpfs,
    // Virtual/Network
    Vmfs, Fuse, Plan9,
    // Other
    Adfs, AmigaFfs, CbmFfs, Nextstep, AcornAdfs,
    Unknown,
}
```

## 6. Controladoras Físicas

Cada controladora implementa `StorageController`. Protocolo de acesso e complexidade:

| Controladora | PCI class/subclass | Protocolo | Speed típica | LOC estimado | Status |
|---|---|---|---|---|---|
| **ATA PIO** | 0x01/0x01 (IDE) | I/O ports + PIO handshake | 16 MB/s | ✅ ~150 LOC | Existente |
| **AHCI (SATA)** | 0x01/0x06 | MMIO BAR5 + PRD DMA | 550 MB/s | ~700 LOC | Novo |
| **NVMe** | 0x01/0x08 | MMIO BAR0/1 + SQ/CQ doorbell | 7 GB/s | ~800 LOC | Novo |
| **USB-MSC** | xHCI 0x0C/0x03 + BOT | TRB bulk rings + CBW/CSW | 400 MB/s | ~500 LOC | Stub atual |
| **ATAPI** | 0x01/0x01 (IDE + packet) | ATA PACKET cmd + SCSI CDB | 8 MB/s | ~200 LOC | Novo |
| **SCSI/SAS** | 0x01/0x00 ou 0x01/0x07 | HBA mailbox + DMA + CDB | 2.4 GB/s | ~600 LOC | Novo |
| **RAID HW** | 0x01/0x04 | MPT2/3 mailbox + MFA frame | 2.4 GB/s | ~800 LOC | Novo |
| **SD/MMC** | 0x08/0x05 | MMIO + CMD set + DMA | 100 MB/s | ~600 LOC | Novo |
| **VirtIO-blk** | 0x1AF4 (vid) | Virtqueue split ring | near-native | ~400 LOC | Novo |
| **NVDIMM** | ACPI NFIT | Load/store (memória) | >10 GB/s | ~300 LOC | Novo |

## 7. Filesystem Probing — 35+ Assinaturas

Cada probe é uma função que lê setores via callback e busca magic bytes. **Nunca escreve.**

| FS | Assinatura | Offset | Lê setores | LOC |
|---|---|---|---|---|
| **FAT12/16** | `0xEB??90` ou `0xE9????` + BPB | 0x000 | 1 | ~60 |
| **FAT32** | `0xEB??90` + "FAT32   " @ 0x052 | 0x000 | 2 | ~60 |
| **exFAT** | `"EXFAT   "` | 0x003 | 2 | ~100 |
| **NTFS** | `"NTFS    "` | 0x003 | 2 | ~120 |
| **HPFS** | partition 0x07 + boot block + superblock @ 16 | 0x200 | 2 | ~80 |
| **ext2/3/4** | `0xEF53` | 0x438 (sector 2) | 3 | ~70 |
| **XFS** | `"XFSB"` (0x58465342) | 0x000 | 1 | ~60 |
| **JFS** | `"JFS1"` | 0x8000 (sector 64) | 65 | ~60 |
| **ReiserFS** | `"ReIsErFs"`, `"ReIsEr2Fs"`, `"ReIsEr3Fs"` | 0x10000 | 129 | ~60 |
| **Reiser4** | `"ReIsEr4"` | 0x10000 | 129 | ~60 |
| **Btrfs** | `"_BHRfS_M"` | 0x10040 | 129 | ~80 |
| **ZFS** | `0x00BAB10C` ("BAB1") | 0x10000 | 129 | ~100 |
| **APFS** | `"NXSB"` (container) ou `"APSB"` (volume) | 0x000 | 1 | ~80 |
| **HFS+** | `"H+"` (0x482B) ou `"HX"` (0x4858) | 0x400 (sector 2) | 3 | ~60 |
| **HFS** | `0x4244` ("BD") | 0x400 | 3 | ~60 |
| **ISO9660** | `"CD001"` | 0x8001 (sector 16) | 17 | ~50 |
| **UDF** | `"NSR02"` ou `"NSR03"` | 0x8000 | 65 | ~100 |
| **SquashFS** | `"hsqs"` (LE) ou `"sqsh"` (BE v3) ou `"qshs"` (v4) | 0x000 | 1 | ~50 |
| **F2FS** | `0x10A1F253` ("F2FS") | 0x000 | 2 | ~60 |
| **UBIFS** | UBI header `"UBI#"` + UBIFS node `0x22191010` | LEB 0 | 3+ | ~100 |
| **YAFFS2** | Scan chunk tags `0xFFFF` pattern | full scan | — | ~150 |
| **NILFS2** | `"NIL2"` | 0x20000 (sector 256) | 257 | ~70 |
| **OCFS2** | `"OCFSV2"` | 0x2000 (sector 16) | 17 | ~60 |
| **Minix FS** | `0x137F` (v1), `0x2468` (v2), `0x4D5A` (v3) | 0x400 | 3 | ~50 |
| **SYSV** | `0xFD2E` (SYSV4), `0x0204`, `0x0205` | 0x400 | 3 | ~50 |
| **Xenix** | `0x01EF` ou `0x012FF` | 0x400 | 3 | ~50 |
| **BeFS** | `"BFS1"` (0x42465331) | 0x200 (sector 1) | 2 | ~60 |
| **Amiga FFS** | `0x00000001` | root block via bitmap | — | ~120 |
| **QNX4** | `0x002F` | 0x400 | 3 | ~50 |
| **QNX6** | `"QNX6\0\0\0\0"` | 0x2000 (sector 16) | 17 | ~60 |
| **Acorn ADFS** | directory header + flags | 0x400 | 3 | ~80 |
| **RomFS** | `"-rom1fs-"` | 0x000 | 1 | ~40 |
| **CramFS** | `"CramFS"` | 0x000 | 1 | ~40 |
| **EFS (IRIX)** | `0x102A` ou `0x90A9` | 0x10000 | 129 | ~60 |
| **VxFS** | `0x36595801` (Intel) / `0x01595836` (BE) | 0x2000 | 17 | ~70 |
| **VMFS** | `"VMFS\0C0"` | 0x80000 (sector 1024) | 1025 | ~80 |
| **ReFS** | partition 0x07 + B+tree — sem doc pública | — | — | ? |

**Ordem de probe:** `priority()` define qual testar primeiro. Prioridade alta = mais específico/confiável (ex: XFS magic `"XFSB"` é mais confiável que `0xEB??90` da FAT). O probe registry ordena por prioridade descendente.

## 8. Volume Managers

Camada entre partição e FS. O agente detecta na chain:

```
Partição (MBR/GPT)
  └── VolumeManagerProbe? ─── sim → revela VolumeGroup + LogicalVolumes
       └── cada LV → FS probe
  └── FilesystemProbe (se não tem VolumeManager)
```

### Tabela de Volume Managers

| Tecnologia | Assinatura | Offset | O que extrai | LOC |
|---|---|---|---|---|
| **LVM2 PV** | `"LABELONE"` | LBA 1 (0x200) | PV UUID, VG UUID, PE size, PE count, PV size | ~80 |
| **LUKS1** | `"LUKS"` header magic | LBA 0 (offset 0) | Cipher, hash, MK digest, UUID | ~60 |
| **LUKS2** | `"LUKS"` header + JSON area | LBA 0 | Keyslots, cipher, checksum, label | ~80 |
| **BitLocker** | `"-FVE-FS-"` | LBA 0 (VBR offset 3) | Drive label, encryption type, version | ~50 |
| **Apple CoreStorage** | `"CS"` | LBA 0 (offset 0) | PV UUID, LVG UUID, LV UUID, LV name | ~60 |
| **md RAID** | superblock @ 0x1000 (4K) | LBA 0xFF8 (4K offset) | RAID level, device list, UUID, chunk size | ~70 |
| **NTFS LDM** | `"PRIVHEAD"` | LBA 0 + LDM database | Disk group, volume set, component | ~100 |
| **Btrfs volume** | `"_BHRfS_M"` + chunk tree | 0x10040 | Multi-device, RAID level, profiles | ~80 |
| **ZFS zpool** | uberblock + label | 0x10000 | Pool GUID, vdev tree, ashift | ~100 |

### Exemplo concreto

```
/dev/sda (NVMe 1TB):
├── sda1 (0xEE = GPT protective) ─── GPT
│   ├── sda1: 0x8E00 (LVM) ───→ LVM2 PV → VG "system" → LV "root" → EXT4
│   │                                                   → LV "var"  → XFS
│   │                                                   → LV "swap" → swap
│   └── sda2: 0x0700 (Microsoft) ───→ BitLocker → NTFS

/dev/sdb (USB 64GB):
├── sdb1 (0x0C = FAT32 LBA) ───→ FAT32 → "PENDRIVE"

/dev/nvme1n1 (NVMe 2TB):
├── nvme1n1: 0xAF00 (Apple) ───→ CoreStorage → LVG → LV "MacHD" → APFS
```

O agente constrói uma árvore:

```rust
TopologyNode::Disk {
    name: "sda", controller: "nvme0", capacity: 1_TB,
    children: vec![
        TopologyNode::VolumeGroup {
            tech: Lvm2, name: "system",
            children: vec![
                TopologyNode::LogicalVolume {
                    name: "root", fs: Ext4,
                    mhi_tier: Nvme, mount: "/mnt/system/root",
                },
                TopologyNode::LogicalVolume {
                    name: "var", fs: Xfs,
                    mhi_tier: Nvme, mount: "/mnt/system/var",
                },
            ],
        },
        TopologyNode::Volume {
            name: "sda2", fs: Ntfs(Encrypted),
            mhi_tier: Nvme, mount: "/mnt/windows",
        },
    ],
}
```

## 9. Armazenamento em Rede/Nuvem (StorageProvider)

Quando a rede estiver funcional (B-01 resolvido), o agente ativa providers:

| Provider | Transporte | Descoberta | LOC |
|---|---|---|---|
| **iSCSI** | TCP/port 3260 | SendTargets → Login → SCSI CDB | ~400 |
| **NVMe-oF** | TCP/RDMA | Discovery Log → Connect → NVMe queues | ~500 |
| **NBD** | TCP/port 10809 | Handshake → Export List → read/write | ~250 |
| **Ceph RBD** | TCP (messenger v2) | Monitor map → OSD → striping | ~800 |
| **NFS v3** | TCP/port 2049 | MOUNT + NFS protocol | ~300 |
| **9P (Plan 9)** | TCP/port 564 | version → attach → walk → read/write | ~200 |

`StorageProvider` estende `StorageController` com:
- `requires_network()` → true
- `needs_poll()` → true (mantém conexão)
- `tick()` → heartbeat, reconexão
- O agente só probeia providers quando `net_ready == true`

## 10. Mapeamento MHI + Otimização de Velocidade

O agente **mede bandwidth real** no boot e re-mede periodicamente:

```rust
// Benchmark: lê 1024 setores sequenciais, cronometra
let bw = controller.measure_bandwidth(disk_idx);
disk.max_read_bw_mbs = bw;
disk.mhi_tier = match bw {
    bw if bw > 2000 => AllocTier::Nvme,    // NVMe
    bw if bw > 100  => AllocTier::Hdd,     // SATA SSD/HDD
    _                => AllocTier::UsbMsc,  // USB
};
```

### MHI Tiers × Tipos de Armazenamento

| Tier | Tipo | BW min | Latência | O que vai aqui |
|---|---|---|---|---|
| **Dram** | RAM | >10 GB/s | <100 ns | ARC cache, inference results, hot skills |
| **Vram** | GPU | >100 GB/s | <200 ns | Model weights (Cortex LLM) |
| **Nvme** | NVMe | >2 GB/s | <10 µs | System partitions, active models |
| **Hdd** | SATA | >100 MB/s | <1 ms | Data partitions, cold storage |
| **UsbMsc** | USB | <100 MB/s | >5 ms | Removable media, network mounts |

### Cadeia de Velocidade para Inovações

| Inovação | Path de dados | Otimização do agente |
|---|---|---|
| **Inference FS** | Model weights → NVMe → DRAM cache | Prefetch modelo antes do tick do Cortex |
| **Self-heal logs** | Boot log → DRAM buffer → FAT32 | Write-back coalesced (N logs em 1 write) |
| **Agent actuation** | Skill definitions → hot tier | ARC promove skills frequentes para DRAM |
| **Plug-and-play IA** | USB → detect → EventBus → Cortex | Mapeia: porta USB → disk → FS → label IA |
| **MHI migration** | Dado frio → HDD, dado quente → DRAM | Movimentação real (não só log) |

## 11. Hotplug (USB)

O agente em `tick()`:
1. Para cada controladora USB, re-scanneia portas xHCI
2. Se novo dispositivo MSC:
   - Enumera: INQUIRY → READ CAPACITY → MBR/GPT → FS probe
   - Registra no MHI (`AllocTier::UsbMsc`, volatile=true)
   - Monta no VFS (`/mnt/usb{N}`)
   - Publica `DISK_ATTACHED` no EventBus
   - Gera insight para Cortex/Hermes
3. Se dispositivo sumiu (poll falha):
   - Desmonta VFS, flush cache
   - Remove do MHI
   - Publica `DISK_DETACHED` com motivo

## 12. Cache ARC (Fase 2)

O agente gerencia um **cache setorial em DRAM**:

```rust
pub struct ArcCache {
    entries: Vec<CacheEntry>,     // setor → dados + metadados
    capacity: usize,              // max bytes em DRAM
    access_tracker: AccessTracker, // MFU/MRU counters
}

struct CacheEntry {
    lba: u64,
    data: [u8; 4096],             // 4KB blocks (alinhado com page)
    freq: u16,                    // MFU counter
    last_access_tick: u64,        // MRU timestamp
    dirty: bool,                  // write-back pendente
}
```

Políticas:
- **Cache-alocação:** blocks lidos via `read_blocks()` vão para DRAM
- **Promoção:** MFU (access_count > 10 em 1000 ticks) → mantém no cache
- **Despejo:** LRU (idle > 5000 ticks) → libera
- **Write-back:** dirty blocks coalescem em 1 write após N acumulados
- **USB:** write-through (nunca cacheia escrita em volátil)
- **Coerência:** se `DISK_DETACHED`, descarta cache sujo

## 13. Boot Path (Tier 0) vs Agent (Tier 1/2)

```
┌──────────────────────────────────────────────────────────────────────┐
│ TIER 0 — Boot (hardcoded, pre-agent)                                 │
│                                                                      │
│ main.rs Phase 4:                                                     │
│   AtaDriver::probe()           → encontra disco de boot              │
│   fat::Fat32Reader + read_file  → lê KERNEL~1                       │
│   identity::verify_signature()  → Ed25519 verify                     │
│   tpm::tpm_extend_pcr()        → TPM PCR[8] extend                  │
│   boot_logger::log()           → FAT32 write (boot log emergencial) │
│                                                                      │
│ Só ATA PIO + FAT32. Sem heap complexo. Sem agentes.                 │
│ Tamanho: ~300 LOC. Blindagem: se agente falhar, boot log existe.    │
├──────────────────────────────────────────────────────────────────────┤
│ TIER 1 — DiskIntelligenceAgent (primeiro tick, Oneshot)             │
│                                                                      │
│ agent.probe_all():                                                   │
│   1. StorageRegistry:                                                │
│      ├── AtaCtrl::new().probe()     → discos ATA + partições        │
│      ├── AhciCtrl::new().probe()    → discos SATA + partições       │
│      ├── NvmeCtrl::new().probe()    → namespaces NVMe + GPT         │
│      ├── UsbMscCtrl.new().probe()  → USBs conectados + partições   │
│      ├── VirtioCtrl::new().probe() → discos VirtIO                  │
│      └── StorageProvider::probe()  → iSCSI/NBD (se net_ready)       │
│   2. Para cada disco: read MBR/GPT → PartitionInfo[]                │
│   3. Para cada partição: VolumeManagerProbe[] → VolumeGroup?        │
│   4. Para cada partição/LV: FilesystemProbe[] → FsInfo              │
│   5. MHI::register(addr, size, tier, owner) para cada partição      │
│   6. VFS::mount(path, agent) para cada partição    │
│   7. BandwidthBench::measure() → atualiza mhi_tier                  │
│   8. generate_topology() → String p/ Cortex/Hermes                  │
│                                                                      │
├──────────────────────────────────────────────────────────────────────┤
│ TIER 2 — DiskIntelligenceAgent (ticking, Continuous)                │
│                                                                      │
│ agent.tick():                                                        │
│   1. Hotplug: re-scan USB ports → DISK_ATTACHED/DETACHED            │
│   2. Network: tick() em StorageProviders → heartbeat, reconexão     │
│   3. AccessTracker: registra access_count, last_access, avg_latency  │
│   4. ArcCache: promove/despeja entries, flush dirty                 │
│   5. MHI: sugere migrações baseado em padrão de acesso              │
│   6. Benchmark: re-mede BW a cada 10000 ticks                       │
│   7. Insight: gera relatório de topologia para Cortex                │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

## 14. Arquivos Novo/Modificados

| Arquivo | Ação | LOC est. | Sprint |
|---|---|---|---|
| `src/disk_agent/mod.rs` | **Novo** — DiskIntelligenceAgent + traits + topology | ~350 | 75.1 |
| `src/disk_agent/controller.rs` | **Novo** — StorageController trait + impls vazios | ~100 | 75.1 |
| `src/disk_agent/disk_info.rs` | **Novo** — structs + enums | ~100 | 75.1 |
| `src/disk_agent/fs_probe.rs` | **Novo** — FilesystemProbe trait + registry | ~80 | 75.1 |
| `src/disk_agent/fs_impls.rs` | **Novo** — 35+ FS probe impls | ~600 | 75.1 |
| `src/disk_agent/vol_mgr.rs` | **Novo** — VolumeManagerProbe trait + impls | ~400 | 75.1 |
| `src/disk_agent/provider.rs` | **Novo** — StorageProvider trait + iSCSI/NBD stubs | ~150 | future |
| `src/disk_agent/cache.rs` | **Novo** — ArcCache + AccessTracker | ~300 | 75.4 |
| `src/ata.rs` | **Modificado** — implementa StorageController trait | +30 | 75.1 |
| `src/usb_msc.rs` | **Modificado** — bulk TRB fix + StorageController | +150 | 75.2 |
| `src/xhci.rs` | **Modificado** — IOC + ring enqueue + ERDP advance | +80 | 75.2 |
| `src/main.rs` | **Modificado** — integra DiskIntelligenceAgent | -10 | 75.1 |
| `src/mhi.rs` | **Modificado** — query + update methods | +40 | 75.1 |
| **Total** | | **~2.400 LOC** | |

## 15. Roteiro de Implementação

### Sprint 75.1 — Estrutura Base + ATA + MBR + FAT32 + MHI

**Objetivo:** Substituir `mount_partitions()` manual pelo agente.

- `disk_agent/mod.rs`: DiskIntelligenceAgent struct, `Agent` impl, `probe_all()`
- `disk_agent/controller.rs`: StorageController trait + AtaCtrl impl (delega ATA)
- `disk_agent/disk_info.rs`: RawDisk, PartitionInfo, FsInfo, enums
- `disk_agent/fs_probe.rs`: FilesystemProbe trait + registry vazio
- `disk_agent/fs_impls.rs`: fat32_probe(), ntfs_probe(), ext_probe(), xfs_probe(), iso9660_probe()
- `disk_agent/vol_mgr.rs`: VolumeManagerProbe trait + lvm_probe(), luks_probe()
- `ata.rs`: implementa StorageController, add measure_bandwidth()
- `main.rs`: integra agente no boot (remove mount_partitions)
- `mhi.rs`: add `update_tier(addr, new_tier)` para migração
- `TODO.md`: marca B-06 como parcial (estrutura base pronta, USB bulk pendente)

**Teste:** QEMU boot com imagem FAT32 → agente descobre partição, registra MHI, monta VFS. `0 erros`.

### Sprint 75.2 — USB-MSC Bulk Fix

**Objetivo:** USB Mass Storage funcional.

- `xhci.rs`: IOC=1 nos TRBs, ring enqueue pointer management, ERDP advance após evento
- `usb_msc.rs`: bulk_write/read funcionais com multi-TRB para >512 bytes
- `disk_agent/controller.rs`: UsbMscCtrl impl (delega usb_msc + xhci)
- `disk_agent/fs_impls.rs`: exfat_probe(), hfsplus_probe(), apfs_probe()

**Teste:** QEMU com `-drive file=pendrive.img,if=none,id=usb1 -device usb-storage,drive=usb1` → agente detecta, monta, lê.

### Sprint 75.3 — Hotplug + Volume Managers + Network Stubs

**Objetivo:** Detecção dinâmica + LVM/LUKS.

- `disk_agent/mod.rs`: hotplug scan em tick() → DISK_ATTACHED/DETACHED events
- `disk_agent/vol_mgr.rs`: LVM2 probe real (lê PV header, descobre VG/LVs), LUKS probe
- `disk_agent/fs_impls.rs`: btrfs_probe(), zfs_probe(), ufs_probe(), vmfs_probe(), 15+ adicionais
- `disk_agent/provider.rs`: iSCSI + NBD stubs (deferidos até B-01)

**Teste:** QEMU com imagem LVM2 + EXT4 → agente descobre VG "vg00" → LV root → EXT4 → monta.

### Sprint 75.4 — NVMe + Cache ARC + Tier Migration

**Objetivo:** Performance máxima.

- `disk_agent/controller.rs`: NvmeCtrl impl (admin + I/O queues, PRP)
- `disk_agent/cache.rs`: ArcCache + AccessTracker + write-back
- `disk_agent/mod.rs`: tier migration real no tick() (copia dados entre tiers)
- `mhi.rs`: updates de tier com data movement

**Teste:** QEMU com NVMe + nvme disk → benchmark mostra 7GB/s → tier=Nvme.

### Futuro (pós-MVP)

- AHCI driver completo (SATA 6G, NCQ)
- StorageProvider実装: iSCSI, NBD, Ceph RBD
- SD/MMC/eMMC (comandos CMD)
- ATAPI para CD/DVD
- NVDIMM (ACPI NFIT)
- RAID HW: MegaRAID MPT3, Intel VMD

## 16. Tecnologias Consideradas e Descartadas

| Tecnologia | Motivo do descarte |
|---|---|
| **FuseFS** | Não é formato de disco — é interface de usuário. O agente não gerencia FUSE. |
| **9P/Plan9** | É protocolo de rede, não FS local. Vira StorageProvider quando B-01 pronto. |
| **CBMFFS (Commodore)** | Obsoleto, hardware 8-bit, sem aplicação prática |
| **SmartFS (SPI Flash)** | Específico para microcontroladores, não para OS |
| **Acorn ADFS** | Arquitetura ARM 32-bit obsoleta |
| **ReFS** | Sem documentação pública do on-disk format |
