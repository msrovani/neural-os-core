

use crate::pci::scan_pci;

#[derive(Clone)]
pub struct AtaDriver {
    pub io_base: u16,
    pub pci_bus: u8,
    pub pci_device: u8,
    pub pci_func: u8,
}

impl AtaDriver {
    pub fn pci_bdf(&self) -> Option<(u8, u8, u8)> {
        Some((self.pci_bus, self.pci_device, self.pci_func))
    }

    pub unsafe fn probe() -> Option<Self> {
        // 1. Tenta PCI class 0x01 com BARs validos
        let devs = scan_pci();
        for d in &devs {
            if d.class == 0x01 && (d.subclass == 0x01 || d.subclass == 0x06) {
                let io = (d.bar0 as u16) & 0xFFF0;
                if io == 0 || io == 0xFFFF { continue; }
                if Self::detect(io, 0xA0) || Self::detect(io, 0xB0) { return Some(AtaDriver { io_base: io, pci_bus: d.bus, pci_device: d.device, pci_func: d.function }); }
            }
        }
        // 2. Fallback: portas legadas ISA (PIIX3 legacy mode - BARs zerados)
        for &base in &[0x1F0u16, 0x170u16] {
            if Self::detect(base, 0xA0) || Self::detect(base, 0xB0) {
                return Some(AtaDriver { io_base: base, pci_bus: 0, pci_device: 0, pci_func: 0 });
            }
        }
        None
    }

    unsafe fn detect(io: u16, sel: u8) -> bool {
        write_io(io + 6, sel);
        core::arch::asm!("out 0x80, al", in("al") 0u8, options(nostack));
        let st = read_io(io + 7);
        st != 0 && st != 0xFF
    }

    unsafe fn wait_bsy(&self) {
        for _ in 0..10000000 {
            if read_io(self.io_base + 7) & 0x80 == 0 { return; }
            core::hint::spin_loop();
        }
    }

    unsafe fn wait_drq(&self) -> bool {
        for _ in 0..10000000 {
            let st = read_io(self.io_base + 7);
            if st & 0x08 != 0 { return true; }
            if st & 0x01 != 0 { return false; }
            core::hint::spin_loop();
        }
        false
    }

    unsafe fn cmd(&self, lba: u32, count: u8, cmd: u8) {
        self.wait_bsy();
        write_io(self.io_base + 6, 0xE0 | ((lba >> 24) as u8));
        write_io(self.io_base + 1, 0);
        write_io(self.io_base + 2, count);
        write_io(self.io_base + 3, (lba & 0xFF) as u8);
        write_io(self.io_base + 4, ((lba >> 8) & 0xFF) as u8);
        write_io(self.io_base + 5, ((lba >> 16) & 0xFF) as u8);
        write_io(self.io_base + 7, cmd);
    }

    /// ATA IDENTIFY — obtem informacoes do disco
    unsafe fn identify(&self) -> Option<[u16; 256]> {
        self.wait_bsy();
        
        // Comando IDENTIFY
        write_io(self.io_base + 6, 0xA0);
        write_io(self.io_base + 2, 0);
        write_io(self.io_base + 3, 0);
        write_io(self.io_base + 4, 0);
        write_io(self.io_base + 5, 0);
        write_io(self.io_base + 7, 0xEC);
        let st = read_io(self.io_base + 7);
        if st == 0 { return None; }
        self.wait_bsy();
        if !self.wait_drq() { return None; }
        let mut data = [0u16; 256];
        for i in 0..256 {
            let lo: u8; let hi: u8;
            core::arch::asm!("in al, dx", out("al") lo, in("dx") self.io_base, options(nostack, preserves_flags));
            core::arch::asm!("in al, dx", out("al") hi, in("dx") (self.io_base + 1), options(nostack, preserves_flags));
            data[i] = (lo as u16) | ((hi as u16) << 8);
        }
        Some(data)
    }

    /// Total de setores do disco (via IDENTIFY word 60-61)
    pub unsafe fn total_sectors(&self) -> Option<u64> {
        let id = self.identify()?;
        let lo = id[60] as u64;
        let hi = id[61] as u64;
        let total = lo | (hi << 16);
        if total > 0 { Some(total) } else { None }
    }

    pub unsafe fn read_sectors(&self, lba: u32, buf: &mut [u8], count: u8) -> bool {
        self.cmd(lba, count, 0x20);
        for s in 0..count as usize {
            self.wait_bsy();
            if !self.wait_drq() { return false; }
            for i in 0..256 {
                let lo: u8; let hi: u8;
                core::arch::asm!("in al, dx", out("al") lo, in("dx") self.io_base, options(nostack, preserves_flags));
                core::arch::asm!("in al, dx", out("al") hi, in("dx") (self.io_base + 1), options(nostack, preserves_flags));
                let off = s * 512 + i * 2;
                if off < buf.len() { buf[off] = lo; }
                if off + 1 < buf.len() { buf[off + 1] = hi; }
            }
        }
        true
    }

    /// Envia comando DATA SET MANAGEMENT (TRIM) para range de LBAs.
    /// Usa o DSM TRIM bit (word 0, bit 0) via buffer de 512 bytes com ranges.
    pub unsafe fn trim(&self, lba_start: u32, count: u32) -> bool {
        if count == 0 || lba_start == 0 { return false; }
        let mut buf = [0u8; 512];
        // Range descriptor: 8 bytes LBA + 2 bytes count + 6 bytes reserved
        buf[0..4].copy_from_slice(&lba_start.to_le_bytes());
        buf[4..8].copy_from_slice(&0u32.to_le_bytes()); // LBA high (LBA48)
        buf[8..10].copy_from_slice(&(count as u16).to_le_bytes());
        self.cmd(lba_start, 1, 0x06); // DATA SET MANAGEMENT
        self.wait_bsy();
        if !self.wait_drq() { return false; }
        for i in 0..256 {
            let lo = buf[i * 2];
            let hi = buf[i * 2 + 1];
            core::arch::asm!("out dx, al", in("dx") self.io_base, in("al") lo, options(nostack, preserves_flags));
            core::arch::asm!("out dx, al", in("dx") (self.io_base + 1), in("al") hi, options(nostack, preserves_flags));
        }
        self.wait_bsy();
        write_io(self.io_base + 7, 0xE7);
        self.wait_bsy();
        true
    }

    pub unsafe fn write_sectors(&self, lba: u32, data: &[u8], count: u8) -> bool {
        if count == 0 { return false; }
        self.cmd(lba, count, 0x30);
        for s in 0..count as usize {
            self.wait_bsy();
            if !self.wait_drq() { return false; }
            let off = s * 512;
            for i in 0..256 {
                let lo = if off + i * 2 < data.len() { data[off + i * 2] } else { 0 };
                let hi = if off + i * 2 + 1 < data.len() { data[off + i * 2 + 1] } else { 0 };
                core::arch::asm!("out dx, al", in("dx") self.io_base, in("al") lo, options(nostack, preserves_flags));
                core::arch::asm!("out dx, al", in("dx") (self.io_base + 1), in("al") hi, options(nostack, preserves_flags));
            }
        }
        self.wait_bsy();
        write_io(self.io_base + 7, 0xE7);
        self.wait_bsy();
        true
    }
}

pub unsafe fn read_io(port: u16) -> u8 {
    let v: u8;
    core::arch::asm!("in al, dx", out("al") v, in("dx") port, options(nostack, preserves_flags));
    v
}

pub unsafe fn write_io(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nostack, preserves_flags));
}
