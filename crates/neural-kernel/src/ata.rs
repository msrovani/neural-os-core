use core::sync::atomic::Ordering;
use crate::pci::scan_pci;
use crate::memory::PHYS_MEM_OFFSET;
use crate::serial_println;

/// ATA PIO driver minimal — escreve blocos no primeiro
/// controlador ATA/SATA encontrado. Usado para salvar boot log.
pub struct AtaDriver {
    io_base: u16,
    ctrl_base: u16,
}

impl AtaDriver {
    pub unsafe fn probe() -> Option<Self> {
        let devs = scan_pci();
        for d in &devs {
            if d.class == 0x01 && (d.subclass == 0x01 || d.subclass == 0x06) {
                let io_base = (d.bar0 as u16) & 0xFFF0;
                if io_base == 0 || io_base == 0xFFFF { continue; }
                if Self::detect(io_base) { return Some(AtaDriver { io_base, ctrl_base: (d.bar1 as u16) & 0xFFF0 }); }
            }
        }
        None
    }

    unsafe fn detect(io: u16) -> bool {
        core::arch::asm!("out dx, al", in("dx") (io + 6), in("al") 0xA0u8, options(nostack, preserves_flags));
        let st = Self::read_io(io + 7);
        st != 0 && st != 0xFF
    }

    unsafe fn read_io(port: u16) -> u8 {
        let v: u8;
        core::arch::asm!("in al, dx", out("al") v, in("dx") port, options(nostack, preserves_flags));
        v
    }

    unsafe fn write_io(port: u16, val: u8) {
        core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nostack, preserves_flags));
    }

    unsafe fn wait_bsy(&self) {
        for _ in 0..1000000 {
            if Self::read_io(self.io_base + 7) & 0x80 == 0 { return; }
            core::hint::spin_loop();
        }
    }

    unsafe fn wait_drq(&self) -> bool {
        for _ in 0..1000000 {
            let st = Self::read_io(self.io_base + 7);
            if st & 0x08 != 0 { return true; }
            if st & 0x01 != 0 { return false; }
            core::hint::spin_loop();
        }
        false
    }

    pub unsafe fn write_sectors(&self, lba: u32, data: &[u8], count: u8) -> bool {
        if count == 0 { return false; }
        self.wait_bsy();
        Self::write_io(self.io_base + 6, 0xE0 | ((lba >> 24) as u8)); // LBA high + master
        Self::write_io(self.io_base + 1, 0);                           // features
        Self::write_io(self.io_base + 2, count);                       // sector count
        Self::write_io(self.io_base + 3, (lba & 0xFF) as u8);         // LBA low
        Self::write_io(self.io_base + 4, ((lba >> 8) & 0xFF) as u8);  // LBA mid
        Self::write_io(self.io_base + 5, ((lba >> 16) & 0xFF) as u8); // LBA high
        Self::write_io(self.io_base + 7, 0x30);                        // WRITE SECTORS

        for s in 0..count as usize {
            self.wait_bsy();
            if !self.wait_drq() { return false; }
            let offset = s * 256;
            for i in 0..256 {
                let word = if offset + i < data.len() / 2 {
                    u16::from_le_bytes([data[(offset + i) * 2], data[(offset + i) * 2 + 1]])
                } else { 0u16 };
                core::arch::asm!("out dx, ax", in("dx") self.io_base, in("ax") word, options(nostack, preserves_flags));
            }
        }
        self.wait_bsy();
        // Cache flush
        Self::write_io(self.io_base + 7, 0xE7);
        self.wait_bsy();
        true
    }
}
