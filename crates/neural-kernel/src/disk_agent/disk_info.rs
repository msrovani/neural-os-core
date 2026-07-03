use alloc::string::String;
use alloc::vec::Vec;
use crate::mhi::AllocTier;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControllerType { Ata, Ahci, Nvme, Usb, Scsi, Sas, Raid, Atapi, SdMmc, Virtio, Nvdimm, Unknown }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterfaceType {
    Pata, Sata, Usb2, Usb3, UsbC, Nvme, Pcie5, Pcie4, Pcie3,
    Scsi, Sas3, Sas4, Virtio, Nvdimm, Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VolumeTech { None, Lvm2, LUKS1, LUKS2, BitLocker, CoreStorage, MdRaid, NtfsLdm, Zpool, BtrfsVol, ApfsContainer }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilesystemType {
    Fat12, Fat16, Fat32, ExFat, Ntfs, ReFs,
    Ext2, Ext3, Ext4, Xfs, Jfs, Reiser, Reiser4, Btrfs,
    F2fs, Nils2, Ocfs2, Squashfs, Cramfs, Romfs, Minix,
    Hfs, HfsPlus, Apfs, Zfs, Ufs, Vxfs, Efs,
    Iso9660, Udf,
    Ubifs, Jffs2, Yaffs2, Logfs,
    BeFs, Qnx4, Qnx6, Sysv, Xenix,
    Vmfs, Plan9,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct FsInfo {
    pub fs_type: FilesystemType,
    pub label: String,
    pub uuid: String,
    pub total_bytes: u64,
    pub free_bytes: Option<u64>,
    pub block_size: u32,
    pub is_writeable: bool,
}

#[derive(Debug, Clone)]
pub struct LogicalVolume {
    pub name: String,
    pub uuid: String,
    pub lba_start: u64,
    pub lba_end: u64,
    pub fs_info: Option<FsInfo>,
}

#[derive(Debug, Clone)]
pub struct VolumeGroup {
    pub name: String,
    pub uuid: String,
    pub technology: VolumeTech,
    pub sub_volumes: Vec<LogicalVolume>,
}

#[derive(Debug, Clone)]
pub struct PartitionInfo {
    pub index: u8,
    pub lba_start: u64,
    pub lba_end: u64,
    pub sector_count: u64,
    pub mbr_type: u8,
    pub fs_info: Option<FsInfo>,
    pub is_bootable: bool,
    pub mhi_tier: AllocTier,
    pub mount_point: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RawDisk {
    pub name: String,
    pub controller: String,
    pub pci_bdf: Option<(u8, u8, u8)>,
    pub capacity_bytes: u64,
    pub sector_size: u16,
    pub interface: InterfaceType,
    pub is_removable: bool,
    pub is_volatile: bool,
    pub model: String,
    pub serial: String,
    pub firmware_rev: String,
    pub max_read_bw_mbs: u32,
    pub rotational: bool,
    pub partitions: Vec<PartitionInfo>,
    pub volume_groups: Vec<VolumeGroup>,
}
