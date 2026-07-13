use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use crate::ata::AtaDriver;
use crate::usb_msc::UsbMassStorage;
use super::disk_info::*;
use super::nvme::NvmeDriver;

pub trait StorageController: Send {
    fn name(&self) -> &str;
    fn controller_type(&self) -> ControllerType;
    fn pci_bdf(&self) -> Option<(u8, u8, u8)>;
    fn probe_disks(&mut self) -> Vec<RawDisk>;
    fn read_blocks(&self, disk: u8, lba: u64, buf: &mut [u8], blocks: usize) -> bool;
    fn write_blocks(&self, disk: u8, lba: u64, data: &[u8], blocks: usize) -> bool;
    fn read_smart(&self, _disk: u8) -> Option<SmartData> { None }
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
        let (is_opal, security_frozen) = unsafe { self.ata_detect_security() };

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
            smart: None,
            is_opal,
            security_frozen,
        };
        self.disks.push(raw);
        self.disks.clone()
    }

    fn read_blocks(&self, _disk: u8, lba: u64, buf: &mut [u8], blocks: usize) -> bool {
        if blocks > 255 { return false; }
        unsafe { self.ata.read_sectors(lba as u32, buf, blocks as u8) }
    }

    fn write_blocks(&self, _disk: u8, lba: u64, data: &[u8], blocks: usize) -> bool {
        if blocks > 255 { return false; }
        unsafe { self.ata.write_sectors(lba as u32, data, blocks as u8) }
    }

    fn read_smart(&self, _disk: u8) -> Option<SmartData> {
        unsafe { self.ata_read_smart() }
    }
}

impl AtaCtrl {
    unsafe fn ata_enable_smart(&self) -> bool {
        let io = self.ata.io_base;
        let _status = |io: u16| -> u8 { core::arch::asm!("in al, dx", out("al") _, in("dx") io + 7, options(nostack, preserves_flags, readonly)); 0 };
        let wait = |io: u16| -> bool {
            for _ in 0..10000 {
                let s: u8; core::arch::asm!("in al, dx", out("al") s, in("dx") io + 7, options(nostack, preserves_flags, readonly));
                if s & 0x80 == 0 { return true; }
                core::hint::spin_loop();
            }
            false
        };
        if !wait(io) { return false; }
        core::arch::asm!("out dx, al", in("dx") io + 6, in("al") 0xE0u8, options(nostack, preserves_flags)); // master
        core::arch::asm!("out dx, al", in("dx") io + 1, in("al") 0xD8u8, options(nostack, preserves_flags)); // SMART ENABLE
        core::arch::asm!("out dx, al", in("dx") io + 2, in("al") 0x01u8, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") io + 3, in("al") 0x4Fu8, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") io + 4, in("al") 0xC2u8, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") io + 5, in("al") 0x00u8, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") io + 7, in("al") 0xB0u8, options(nostack, preserves_flags)); // SMART CMD
        for _ in 0..10000 {
            let s: u8; core::arch::asm!("in al, dx", out("al") s, in("dx") io + 7, options(nostack, preserves_flags, readonly));
            if s & 0x80 == 0 { let _ = s; break; }
            if s & 0x01 != 0 { return false; } // error
            core::hint::spin_loop();
        }
        true
    }

    unsafe fn ata_read_smart(&self) -> Option<SmartData> {
        use core::arch::asm;
        let io = self.ata.io_base;
        let wait_not_busy = || -> bool {
            for _ in 0..100000 {
                let s: u8; asm!("in al, dx", out("al") s, in("dx") io + 7, options(nostack, preserves_flags, readonly));
                if s & 0x80 == 0 { return true; }
                core::hint::spin_loop();
            }
            false
        };
        if !wait_not_busy() { return None; }

        // Select master
        asm!("out dx, al", in("dx") io + 6, in("al") 0xE0u8, options(nostack, preserves_flags));

        // Try ENABLE SMART first (some drives need this once per power cycle)
        asm!("out dx, al", in("dx") io + 1, in("al") 0xD8u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 2, in("al") 0x01u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 3, in("al") 0x4Fu8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 4, in("al") 0xC2u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 5, in("al") 0x00u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 7, in("al") 0xB0u8, options(nostack, preserves_flags));
        if !wait_not_busy() { return None; }

        // Check for error (error bit might be set for unsupported SMART on some QEMU configs)
        let st: u8; asm!("in al, dx", out("al") st, in("dx") io + 7, options(nostack, preserves_flags, readonly));
        if st & 0x01 != 0 {
            // SMART not supported or disabled — not an error, just no data
            return None;
        }

        // SMART READ DATA (0xD0)
        if !wait_not_busy() { return None; }
        asm!("out dx, al", in("dx") io + 1, in("al") 0xD0u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 2, in("al") 0x01u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 3, in("al") 0x4Fu8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 4, in("al") 0xC2u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 5, in("al") 0x00u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 7, in("al") 0xB0u8, options(nostack, preserves_flags));

        // Wait DRQ
        for _ in 0..100000 {
            let s: u8; asm!("in al, dx", out("al") s, in("dx") io + 7, options(nostack, preserves_flags, readonly));
            if s & 0x08 != 0 { break; } // DRQ
            if s & 0x01 != 0 { return None; } // error
            if s & 0x80 == 0 {} // not busy
            core::hint::spin_loop();
        }

        let mut data = [0u16; 256];
        for word in &mut data {
            asm!("in ax, dx", out("ax") *word, in("dx") io, options(nostack, preserves_flags));
        }

        // Parse SMART data
        let bytes: &[u8; 512] = core::mem::transmute(&data);

        let revision = u16::from_be_bytes([bytes[0], bytes[1]]);
        if revision == 0 || revision > 16 { return None; } // invalid revision

        let mut temp_c = 0u16;
        let mut power_on_hours = 0u32;
        let mut realloc_sectors = 0u32;
        let mut pending_sectors = 0u32;
        let mut crc_errors = 0u32;
        let mut wear_level = None;

        // Parse 30 vendor-specific attributes (each 12 bytes starting at offset 2)
        for i in 0..30 {
            let off = 2 + i * 12;
            let attr_id = bytes[off];
            if attr_id == 0 { continue; }
            let raw = u64::from_le_bytes([
                bytes[off + 5], bytes[off + 6], bytes[off + 7],
                bytes[off + 8], bytes[off + 9], bytes[off + 10], 0, 0,
            ]);
            let current_val = bytes[off + 3];

            match attr_id {
                0x05 => realloc_sectors = raw as u32,
                0x09 => power_on_hours = raw as u32,
                0xC2 => temp_c = raw as u16,
                0xC5 => pending_sectors = raw as u32,
                0xC7 => crc_errors = raw as u32,
                0xE8 => wear_level = Some(current_val), // SSD endurance
                0xE7 => wear_level = Some(current_val), // SSD remaining life
                _ => {}
            }
        }

        let healthy = realloc_sectors < 100 && pending_sectors < 10 && crc_errors < 1000;

        Some(SmartData { healthy, temp_c, power_on_hours, realloc_sectors, pending_sectors, crc_errors, wear_level })
    }

    /// Detecta SED (Self-Encrypting Drive / OPAL) via ATA IDENTIFY word 82
    unsafe fn ata_detect_security(&self) -> (bool, bool) {
        use core::arch::asm;
        let io = self.ata.io_base;
        let wait = || -> bool {
            for _ in 0..100000 {
                let s: u8; asm!("in al, dx", out("al") s, in("dx") io + 7, options(nostack, preserves_flags, readonly));
                if s & 0x80 == 0 { return true; }
                core::hint::spin_loop();
            }
            false
        };
        if !wait() { return (false, false); }
        asm!("out dx, al", in("dx") io + 6, in("al") 0xE0u8, options(nostack, preserves_flags));
        // IDENTIFY DEVICE (0xEC)
        asm!("out dx, al", in("dx") io + 2, in("al") 0x00u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 3, in("al") 0x00u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 4, in("al") 0x00u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 5, in("al") 0x00u8, options(nostack, preserves_flags));
        asm!("out dx, al", in("dx") io + 7, in("al") 0xECu8, options(nostack, preserves_flags));
        for _ in 0..100000 {
            let s: u8; asm!("in al, dx", out("al") s, in("dx") io + 7, options(nostack, preserves_flags, readonly));
            if s & 0x08 != 0 { break; } // DRQ
            if s & 0x01 != 0 { return (false, false); }
            core::hint::spin_loop();
        }
        let mut data = [0u16; 256];
        for word in &mut data {
            asm!("in ax, dx", out("ax") *word, in("dx") io, options(nostack, preserves_flags));
        }
        // Word 82 bits: bit 5 = Security Feature Set supported
        let word82 = data[82];
        let is_opal = (word82 & (1 << 5)) != 0;
        // Word 91 bits: bit 0 = Security enabled (locked), bit 1 = Security frozen
        let word91 = data[91];
        let _security_enabled = (word91 & (1 << 0)) != 0;
        let security_frozen = (word91 & (1 << 1)) != 0;
        (is_opal, security_frozen)
    }
}

// ── UsbMscCtrl ─────────────────────────────────────────────
pub struct UsbMscCtrl {
    msc: UnsafeCell<UsbMassStorage>,
    probed: bool,
    disks: Vec<RawDisk>,
}

impl UsbMscCtrl {
    pub fn new(msc: UsbMassStorage) -> Self {
        UsbMscCtrl { msc: UnsafeCell::new(msc), probed: false, disks: Vec::new() }
    }
}

impl StorageController for UsbMscCtrl {
    fn name(&self) -> &str { "usb0" }
    fn controller_type(&self) -> ControllerType { ControllerType::Usb }
    fn pci_bdf(&self) -> Option<(u8, u8, u8)> { None }

    fn probe_disks(&mut self) -> Vec<RawDisk> {
        if self.probed { return self.disks.clone(); }
        self.probed = true;
        let msc = unsafe { &mut *self.msc.get() };
        if msc.max_lba == 0 { return Vec::new(); }
        let bw = self.measure_bandwidth(0);

        let raw = RawDisk {
            name: alloc::format!("sdb"),
            controller: self.name().into(),
            pci_bdf: None,
            capacity_bytes: msc.max_lba * msc.sector_size as u64,
            sector_size: msc.sector_size,
            interface: InterfaceType::Usb3,
            is_removable: true,
            is_volatile: true,
            model: core::str::from_utf8(&msc.model[8..32]).unwrap_or("USB").trim().into(),
            serial: String::new(),
            firmware_rev: String::new(),
            max_read_bw_mbs: bw,
            rotational: false,
            partitions: Vec::new(),
            volume_groups: Vec::new(),
            smart: None,
            is_opal: false,
            security_frozen: false,
        };
        self.disks.push(raw);
        self.disks.clone()
    }

    fn read_blocks(&self, _disk: u8, lba: u64, buf: &mut [u8], blocks: usize) -> bool {
        let msc = unsafe { &mut *self.msc.get() };
        for i in 0..blocks {
            let mut sector = [0u8; 512];
            if !unsafe { msc.read_sector(lba + i as u64, &mut sector) } { return false; }
            let offset = i * 512;
            if offset + 512 <= buf.len() {
                buf[offset..offset + 512].copy_from_slice(&sector);
            }
        }
        true
    }

    fn write_blocks(&self, _disk: u8, lba: u64, data: &[u8], blocks: usize) -> bool {
        let msc = unsafe { &mut *self.msc.get() };
        for i in 0..blocks {
            let offset = i * 512;
            let mut sector = [0u8; 512];
            if offset + 512 <= data.len() {
                sector.copy_from_slice(&data[offset..offset + 512]);
            }
            if !unsafe { msc.write_sector(lba + i as u64, &sector) } { return false; }
        }
        true
    }
}

// ── NvmeCtrl ──────────────────────────────────────────────
pub struct NvmeCtrl {
    nvme: core::cell::UnsafeCell<NvmeDriver>,
    probed: bool,
    disks: Vec<RawDisk>,
}

impl NvmeCtrl {
    pub fn new(nvme: NvmeDriver) -> Self {
        NvmeCtrl { nvme: core::cell::UnsafeCell::new(nvme), probed: false, disks: Vec::new() }
    }
}

impl StorageController for NvmeCtrl {
    fn name(&self) -> &str { "nvme0" }
    fn controller_type(&self) -> ControllerType { ControllerType::Nvme }
    fn pci_bdf(&self) -> Option<(u8, u8, u8)> { None }

    fn probe_disks(&mut self) -> Vec<RawDisk> {
        if self.probed { return self.disks.clone(); }
        self.probed = true;
        let nm = unsafe { &mut *self.nvme.get() };
        if nm.lba_count == 0 { return Vec::new(); }

        let model = core::str::from_utf8(&nm.model).unwrap_or("NVMe").trim().into();
        let raw = RawDisk {
            name: alloc::format!("nvme0n1"),
            controller: self.name().into(),
            pci_bdf: None,
            capacity_bytes: nm.lba_count * nm.lba_size as u64,
            sector_size: nm.lba_size as u16,
            interface: InterfaceType::Nvme,
            is_removable: false, is_volatile: false,
            model, serial: String::new(), firmware_rev: String::new(),
            max_read_bw_mbs: 3000,
            rotational: false,
            partitions: Vec::new(), volume_groups: Vec::new(), smart: None,
            is_opal: false, security_frozen: false,
        };
        self.disks.push(raw);
        self.disks.clone()
    }

    fn read_blocks(&self, _disk: u8, lba: u64, buf: &mut [u8], blocks: usize) -> bool {
        let nm = unsafe { &mut *self.nvme.get() };
        unsafe { nm.read_blocks(lba, buf.as_mut_ptr(), blocks as u32) }
    }

    fn write_blocks(&self, _disk: u8, _lba: u64, _data: &[u8], _blocks: usize) -> bool {
        false
    }
}
