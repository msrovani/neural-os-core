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

        let mounts = detect_exfat(dev, kind);
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
