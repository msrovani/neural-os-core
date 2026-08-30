//! StorageBus — registry de block devices + auto-detect FS (ADR-0062 P2).
//! Não toma ownership dos drivers (AHCI/ATA/NVMe/USB ficam nos estáticos do boot).

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::block_dev::BlockDevice;
use crate::exfat::ExfatFs;
use crate::fs_driver::FilesystemDriver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusKind {
    Nvme,
    Ahci,
    Ata,
    Usb,
    VirtioBlk,
}

pub struct StorageMount {
    pub mount_point: &'static str,
    pub fs_type: &'static str,
    pub start_lba: u64,
    pub label: String,
}

pub struct StorageEntry {
    pub kind: BusKind,
    pub name: &'static str,
    pub total_sectors_512: u64,
    pub mbr_ok: bool,
    pub mounts: Vec<StorageMount>,
}

pub struct StorageBus {
    entries: Vec<StorageEntry>,
}

impl StorageBus {
    pub const fn new() -> Self {
        StorageBus {
            entries: Vec::new(),
        }
    }

    pub fn device_count(&self) -> usize {
        self.entries.len()
    }

    pub fn entries(&self) -> &[StorageEntry] {
        &self.entries
    }

    /// Registra device, smoke MBR, detecta exFAT e publica VFS mount canônico.
    pub fn register_probe(&mut self, kind: BusKind, name: &'static str, dev: &mut dyn BlockDevice) {
        let total = dev.total_sectors();
        let mut mbr = [0u8; 512];
        let mbr_ok = dev.read_sectors(0, &mut mbr)
            && mbr[0x1FE] == 0x55
            && mbr[0x1FF] == 0xAA;
        crate::slog_nano!(
            "StorageBus",
            "reg",
            "{} sectors={} mbr_ok={}",
            name,
            total,
            mbr_ok
        );
        if mbr_ok {
            crate::slog_nano!(
                "StorageBus",
                "smoke",
                "{} BlockDevice read MBR OK",
                name
            );
        }

        let mut mounts = detect_exfat(dev, kind);
        mounts.extend(detect_ext(dev, kind));
        mounts.extend(detect_ntfs(dev, kind));
        for m in &mounts {
            crate::slog_nano!(
                "StorageBus",
                "mount",
                "{} -> {} ({}) lba={}",
                m.mount_point,
                m.fs_type,
                m.label.as_str(),
                m.start_lba
            );
            if let Some(ref mut vfs) = *crate::vfs::VFS.lock() {
                vfs.mount(m.mount_point, name);
            }
        }

        self.entries.push(StorageEntry {
            kind,
            name,
            total_sectors_512: total,
            mbr_ok,
            mounts,
        });
    }
}

fn detect_ext(dev: &mut dyn BlockDevice, kind: BusKind) -> Vec<StorageMount> {
    let mut out = Vec::new();
    let mut mbr = [0u8; 512];
    if !dev.read_sectors(0, &mut mbr) || mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
        return out;
    }
    let parts = crate::fat32::parse_mbr_sector(&mbr);
    let mut lbas: Vec<u64> = parts.iter().map(|p| p.lba_start as u64).collect();
    if !lbas.contains(&0) {
        lbas.insert(0, 0);
    }
    let mp: &'static str = match kind {
        BusKind::Nvme => "/mnt/ext",
        BusKind::Ahci => "/mnt/ext",
        BusKind::Ata => "/mnt/ext",
        BusKind::Usb => "/mnt/ext",
        BusKind::VirtioBlk => "/mnt/ext",
    };
    for start_lba in lbas {
        if let Some(mut fs) = crate::ext2_reader::Ext2Reader::detect(dev, start_lba) {
            match fs.mount(dev, start_lba) {
                Ok(info) => {
                    let n = fs.list("/").map(|v| v.len()).unwrap_or(0);
                    crate::slog_nano!(
                        "EXT4",
                        "info",
                        "step=list status=OK entries={} fs={} label={} lba={}",
                        n,
                        info.fs_type,
                        info.label.as_str(),
                        start_lba
                    );
                    out.push(StorageMount {
                        mount_point: mp,
                        fs_type: info.fs_type,
                        start_lba,
                        label: info.label,
                    });
                    break; // first EXT only
                }
                Err(e) => {
                    crate::slog_nano!(
                        "EXT4",
                        "info",
                        "step=mount status=FAIL reason={} lba={}",
                        e,
                        start_lba
                    );
                }
            }
        }
    }
    out
}

fn detect_ntfs(dev: &mut dyn BlockDevice, kind: BusKind) -> Vec<StorageMount> {
    use crate::fs_driver::FilesystemDriver;
    let mut out = Vec::new();
    let mut mbr = [0u8; 512];
    if !dev.read_sectors(0, &mut mbr) || mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
        return out;
    }
    let parts = crate::fat32::parse_mbr_sector(&mbr);
    let mut lbas: Vec<u64> = parts.iter().map(|p| p.lba_start as u64).collect();
    if !lbas.contains(&0) {
        lbas.insert(0, 0);
    }
    let mp: &'static str = "/mnt/ntfs";
    let _ = kind;
    for start_lba in lbas {
        if let Some(fs) = crate::ntfs_reader::NtfsReader::detect(dev, start_lba) {
            crate::slog_nano!(
                "NTFS",
                "info",
                "step=detect status=OK label={} lba={} VERDICT=PARTIAL",
                fs.name(),
                start_lba
            );
            out.push(StorageMount {
                mount_point: mp,
                fs_type: "ntfs",
                start_lba,
                label: String::from("ntfs"),
            });
            // btrfs probe same LBA (orthogonal)
            if crate::btrfs_reader::probe_super(dev, start_lba).is_some() {
                crate::slog_nano!(
                    "BTRFS",
                    "info",
                    "step=detect status=OK lba={} VERDICT=PARTIAL",
                    start_lba
                );
            }
            break;
        }
    }
    out
}

fn detect_exfat(dev: &mut dyn BlockDevice, kind: BusKind) -> Vec<StorageMount> {
    let mut out = Vec::new();
    let mut mbr = [0u8; 512];
    if !dev.read_sectors(0, &mut mbr) || mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
        return out;
    }
    let parts = crate::fat32::parse_mbr_sector(&mbr);
    let mut lbas: Vec<u64> = parts.iter().map(|p| p.lba_start as u64).collect();
    if !lbas.contains(&0) {
        lbas.insert(0, 0);
    }
    let mp: &'static str = match kind {
        BusKind::Nvme => "/mnt/data",
        BusKind::Ahci => "/mnt/sata",
        BusKind::Ata => "/mnt/hdd",
        BusKind::Usb => "/mnt/usb",
        BusKind::VirtioBlk => "/mnt/virtio",
    };
    for start in lbas {
        if let Some(mut fs) = ExfatFs::detect(dev, start) {
            if let Ok(info) = fs.mount(dev, start) {
                out.push(StorageMount {
                    mount_point: mp,
                    fs_type: info.fs_type,
                    start_lba: start,
                    label: info.label,
                });
                break; // um mount canônico por device
            }
        }
    }
    out
}

pub static STORAGE_BUS: Mutex<StorageBus> = Mutex::new(StorageBus::new());

pub fn bus_report() -> String {
    let bus = STORAGE_BUS.lock();
    let mut s = String::from("=== StorageBus ===\n");
    s.push_str(&alloc::format!("devices: {}\n", bus.device_count()));
    for e in bus.entries() {
        s.push_str(&alloc::format!(
            "  {} kind={:?} sectors_512={} mbr_ok={}\n",
            e.name,
            e.kind,
            e.total_sectors_512,
            e.mbr_ok
        ));
        for m in &e.mounts {
            s.push_str(&alloc::format!(
                "    {} [{}] lba={} label={}\n",
                m.mount_point,
                m.fs_type,
                m.start_lba,
                m.label
            ));
        }
    }
    s
}
