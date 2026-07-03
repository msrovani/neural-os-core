use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use crate::ata::AtaDriver;
use crate::mhi::AllocTier;
use super::disk_info::*;

pub trait StorageController: Send {
    fn name(&self) -> &str;
    fn controller_type(&self) -> ControllerType;
    fn pci_bdf(&self) -> Option<(u8, u8, u8)>;
    fn probe_disks(&mut self) -> Vec<RawDisk>;
    fn read_blocks(&self, disk: u8, lba: u64, buf: &mut [u8], blocks: usize) -> bool;
    fn write_blocks(&self, disk: u8, lba: u64, data: &[u8], blocks: usize) -> bool;
    fn measure_bandwidth(&self, disk: u8) -> u32 {
        let mut buf = alloc::vec![0u8; 512 * 256];
        let start = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        if !self.read_blocks(disk, 0, &mut buf, 256) { return 0; }
        let elapsed = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) - start;
        if elapsed == 0 { return 0; }
        (256 * 512 / (elapsed as u32 * 55 / 1000).max(1) / 1024 / 1024).max(1)
    }
}

pub struct AtaCtrl {
    ata: AtaDriver,
    probed: bool,
    disks: Vec<RawDisk>,
}

impl AtaCtrl {
    pub fn new(ata: AtaDriver) -> Self {
        AtaCtrl { ata, probed: false, disks: Vec::new() }
    }
}

impl StorageController for AtaCtrl {
    fn name(&self) -> &str { "ata0" }
    fn controller_type(&self) -> ControllerType { ControllerType::Ata }
    fn pci_bdf(&self) -> Option<(u8, u8, u8)> { self.ata.pci_bdf() }

    fn probe_disks(&mut self) -> Vec<RawDisk> {
        if self.probed { return self.disks.clone(); }
        self.probed = true;

        let total = unsafe { self.ata.total_sectors().unwrap_or(0) };
        if total == 0 { return Vec::new(); }

        let bw = self.measure_bandwidth(0);
        let interface = if bw > 200 { InterfaceType::Sata } else { InterfaceType::Pata };

        let raw = RawDisk {
            name: alloc::format!("sda"),
            controller: self.name().into(),
            pci_bdf: self.pci_bdf(),
            capacity_bytes: total as u64 * 512,
            sector_size: 512,
            interface,
            is_removable: false,
            is_volatile: false,
            model: String::new(),
            serial: String::new(),
            firmware_rev: String::new(),
            max_read_bw_mbs: bw,
            rotational: true,
            partitions: Vec::new(),
            volume_groups: Vec::new(),
        };
        self.disks.push(raw);
        self.disks.clone()
    }

    fn read_blocks(&self, _disk: u8, lba: u64, buf: &mut [u8], blocks: usize) -> bool {
        unsafe { self.ata.read_sectors(lba as u32, buf, blocks as u8) }
    }

    fn write_blocks(&self, _disk: u8, lba: u64, data: &[u8], blocks: usize) -> bool {
        unsafe { self.ata.write_sectors(lba as u32, data, blocks as u8) }
    }
}
