# crates/k_nano/src/disk_agent/ — Storage Intelligence Agent

**Responsibility**: `DiskIntelligenceAgent` (Oneshot `agent_core::Agent`) — unified
storage layer over `StorageController` trait objects (AtaCtrl/UsbMscCtrl/NvmeCtrl):
probe disks + S.M.A.R.T., GPT (preferred) / MBR partitions, filesystem detection via
`FsProbeRegistry` (FAT32/NTFS/ext/XFS/ISO9660/exFAT/Btrfs/HFS+/EROFS/ReFS), volume
manager detection (`VolMgrRegistry`: LVM2/LUKS), ARC read/write cache with write-back
coalescing, I/O scheduling (batched flush), MHI tier registration/migration, and VFS
mounting.

**Key symbols**: `DiskIntelligenceAgent`, `register_controller()`, `DISK_AGENT_INIT`,
`StorageController` trait, `RawDisk`/`PartitionInfo`/`FsInfo`/`SmartData` (disk_info),
`FsProbeRegistry`, `VolMgrRegistry`, `cache::ArcCache`, `nvme::NvmeDriver` (production
NVMe, SESSION_171).

**Integration**: bin/agents probe ATA/AHCI/USB-MSC and register controllers before Phase
6; the agent publishes mounts into `vfs::VFS` and registers regions in `mhi::MHI_REGISTRY`;
`fs::read_vfs`/`write_vfs` are the I/O entry points for other crates.
