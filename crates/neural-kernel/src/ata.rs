

#[derive(Clone)]
pub struct AtaDriver {
    pub io_base: u16,
    pub pci_bus: u8,
    pub pci_device: u8,
    pub pci_func: u8,
    pub slave: bool,
}

impl AtaDriver {
    pub fn pci_bdf(&self) -> Option<(u8, u8, u8)> {
        Some((self.pci_bus, self.pci_device, self.pci_func))
    }

    pub unsafe fn probe() -> Option<Self> {
        let mut best: Option<AtaDriver> = None;
        let mut best_type: u8 = 0;
        for &base in &[0x1F0u16, 0x170u16] {
            for &slave in &[false, true] {
                if !Self::detect(base, if slave { 0xB0 } else { 0xA0 }) { continue; }
                let drv = AtaDriver { io_base: base, pci_bus: 0, pci_device: 0, pci_func: 0, slave };
                if let Some(id) = drv.identify() {
                    let total = (id[60] as u64) | ((id[61] as u64) << 16);
                    if total > 0 && total < 0xFFFFFFFF {
                        k_nano::slog_nano!("Disk", "ata", "ISA {}: {} {} setores", base, if slave { "slave" } else { "master" }, total);
                        write_io(base + 6, if slave { 0xB0 } else { 0xA0 });
                        for _ in 0..1000 { core::hint::spin_loop(); }
                        if drv.has_mbr() {
                            // Prefere disco com FAT32 (MBR 0x0B/0x0C ou GPT Basic Data) —
                            // USB unificado / disk_hw no mesmo stick; evita escolher so ESP GPT.
                            if crate::fat32::disk_has_fat32(&drv) {
                                k_nano::slog_nano!("Disk", "ata", "ISA {}: {} FAT32 (MBR/GPT)!",
                                    base,
                                    if slave { "slave" } else { "master" });
                                return Some(drv);
                            }
                            let mut mbr = [0u8; 512];
                            if drv.read_sectors(0, &mut mbr, 1) {
                                for i in 0..4 {
                                    let off = 0x1BE + i * 16;
                                    let t = mbr[off + 4];
                                    if t == 0xEE && best.is_none() {
                                        best = Some(drv.clone());
                                        best_type = t;
                                    }
                                    if best.is_none() {
                                        best = Some(drv.clone());
                                        best_type = t;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if best.is_some() { k_nano::slog_nano!("Disk", "ata", "Usando fallback type={:#x}", best_type); }
        best
    }

    unsafe fn has_mbr(&self) -> bool {
        let mut buf = [0u8; 512];
        if !self.read_sectors(0, &mut buf, 1) {
            k_nano::slog_nano!("Disk", "ata", "has_mbr: read_sectors falhou para slave={}", self.slave);
            return false;
        }
        let ok = buf[0x1FE] == 0x55 && buf[0x1FF] == 0xAA;
        k_nano::slog_nano!("Disk", "ata", "has_mbr: slave={} 55AA={} bytes510={:02X}511={:02X}", self.slave, ok, buf[0x1FE], buf[0x1FF]);
        ok
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
        let head = if self.slave { 0xF0u8 } else { 0xE0u8 };
        // Delay apos wait_bsy
        for _ in 0..100 { core::hint::spin_loop(); }
        write_io(self.io_base + 6, head | ((lba >> 24) as u8));
        for _ in 0..100 { core::hint::spin_loop(); }
        write_io(self.io_base + 6, head | ((lba >> 24) as u8));
        for _ in 0..100 { core::hint::spin_loop(); }
        write_io(self.io_base + 1, 0);
        write_io(self.io_base + 2, count);
        write_io(self.io_base + 3, (lba & 0xFF) as u8);
        write_io(self.io_base + 4, ((lba >> 8) & 0xFF) as u8);
        write_io(self.io_base + 5, ((lba >> 16) & 0xFF) as u8);
        for _ in 0..100 { core::hint::spin_loop(); }
        write_io(self.io_base + 7, cmd);
    }

    /// ATA IDENTIFY — obtem informacoes do disco
    unsafe fn identify(&self) -> Option<[u16; 256]> {
        self.wait_bsy();
        
        // Comando IDENTIFY — select master (0xA0) or slave (0xB0)
        let sel = if self.slave { 0xB0u8 } else { 0xA0u8 };
        write_io(self.io_base + 6, sel);
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
            let w: u16;
            core::arch::asm!("in ax, dx", out("ax") w, in("dx") self.io_base, options(nostack, preserves_flags));
            data[i] = w;
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
        // Pequena pausa para o comando ser processado
        for _ in 0..10000 { core::hint::spin_loop(); }
        for s in 0..count as usize {
            self.wait_bsy();
            if !self.wait_drq() {
                k_nano::slog_nano!("Disk", "ata", "read: DRQ nao pronto LBA={} s={}/{} slave={}", lba, s, count, self.slave);
                return false;
            }
            for i in 0..256 {
                let w: u16;
                core::arch::asm!("in ax, dx", out("ax") w, in("dx") self.io_base, options(nostack, preserves_flags));
                let off = s * 512 + i * 2;
                if off < buf.len() { buf[off] = w as u8; }
                if off + 1 < buf.len() { buf[off + 1] = (w >> 8) as u8; }
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
            let w = (buf[i * 2] as u16) | ((buf[i * 2 + 1] as u16) << 8);
            core::arch::asm!("out dx, ax", in("dx") self.io_base, in("ax") w, options(nostack, preserves_flags));
        }
        self.wait_bsy();
        write_io(self.io_base + 7, 0xE7);
        self.wait_bsy();
        true
    }

    /// Try master then slave. Returns true on first successful read.
    pub unsafe fn read_any(&self, lba: u32, buf: &mut [u8], count: u8) -> bool {
        let m = AtaDriver { io_base: self.io_base, pci_bus: 0, pci_device: 0, pci_func: 0, slave: false };
        if m.read_sectors(lba, buf, count) { return true; }
        let s = AtaDriver { io_base: self.io_base, pci_bus: 0, pci_device: 0, pci_func: 0, slave: true };
        s.read_sectors(lba, buf, count)
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
                // BUG histórico (v0.1→v1.1.5): escrever "hi" em io_base+1 vai para o
                // registrador FEATURES/ERROR, não para o byte alto do dado. O barramento
                // de dados ATA é de 16 bits — precisa de UMA escrita de word em io_base.
                let w = (lo as u16) | ((hi as u16) << 8);
                core::arch::asm!("out dx, ax", in("dx") self.io_base, in("ax") w, options(nostack, preserves_flags));
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
