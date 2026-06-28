use crate::pci::scan_pci;
use crate::serial_println;

pub struct AtaDriver {
    pub io_base: u16,
}

impl AtaDriver {
    pub unsafe fn probe() -> Option<Self> {
        let devs = scan_pci();
        for d in &devs {
            if d.class == 0x01 && (d.subclass == 0x01 || d.subclass == 0x06) {
                let io = (d.bar0 as u16) & 0xFFF0;
                if io == 0 || io == 0xFFFF { continue; }
                if Self::detect(io) { return Some(AtaDriver { io_base: io }); }
            }
        }
        None
    }

    unsafe fn detect(io: u16) -> bool {
        write_io(io + 6, 0xA0);
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
