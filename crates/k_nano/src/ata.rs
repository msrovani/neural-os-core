/// ATA PIO driver — suporte a 512B e 4Kn (4096 bytes/setor lógico).
///
/// ATA PIO sempre transfere 256 words (512 bytes) por comando.
/// Para discos 4Kn, o LBA lógico aponta para setores de 4096 bytes.
/// O driver traduz: LBA 4Kn → 8 reads de 512B para compor o setor completo.

#[derive(Clone)]
pub struct AtaDriver {
    pub io_base: u16,
    pub pci_bus: u8,
    pub pci_device: u8,
    pub pci_func: u8,
    pub slave: bool,
    /// Bytes por setor lógico (512 padrão; 4096 para 4Kn).
    /// Detectado via IDENTIFY words 117-118.
    pub lba_size: u32,
}

/// Setor PIO fixo — ATA sempre transfere 256 words = 512 bytes.
const PIO_SECTOR_BYTES: u32 = 512;

impl AtaDriver {
    pub fn pci_bdf(&self) -> Option<(u8, u8, u8)> {
        Some((self.pci_bus, self.pci_device, self.pci_func))
    }

    /// Tamanho do setor lógico detectado (512 padrão; 4096 para 4Kn).
    pub fn sector_size(&self) -> u32 {
        self.lba_size
    }

    pub unsafe fn probe() -> Option<Self> {
        // T-011: TCG + PIO identify trava o boot (SESSION_243). Skip honesto;
        // QEMU-loader / NoDisk seguem. Metal/WHPX medem de verdade.
        if crate::storage_bw::skip_measure() {
            crate::slog_nano!(
                "Disk",
                "ata",
                "CRITICO ATA probe skip (TCG) — boot sem disco PIO"
            );
            return None;
        }
        let mut best: Option<AtaDriver> = None;
        let mut best_type: u8 = 0;
        // Candidatos com FS de dados; preferir exFAT (QEMU disk_qemu) e maior tamanho.
        let mut data_best: Option<(AtaDriver, u64, bool)> = None; // drv, sectors, is_exfat
        for &base in &[0x1F0u16, 0x170u16] {
            for &slave in &[false, true] {
                if !Self::detect(base, if slave { 0xB0 } else { 0xA0 }) { continue; }
                let drv = AtaDriver {
                    io_base: base, pci_bus: 0, pci_device: 0, pci_func: 0,
                    slave, lba_size: 512, // default; detectado no identify
                };
                if let Some(id) = drv.identify() {
                    // Detectar 4Kn via IDENTIFY words 117-118 (logical/physical sector size)
                    let lba_size = Self::detect_sector_size(&id);
                    let mut drv = drv;
                    drv.lba_size = lba_size;

                    let total = (id[60] as u64) | ((id[61] as u64) << 16);
                    if total > 0 && total < 0xFFFFFFFF {
                        crate::slog_nano!("Disk", "ata",
                            "ISA {}: {} {} setores lba_size={}",
                            base, if slave { "slave" } else { "master" }, total, lba_size);
                        write_io(base + 6, if slave { 0xB0 } else { 0xA0 });
                        for _ in 0..1000 { core::hint::spin_loop(); }
                        if drv.has_mbr() {
                            let has_ex = crate::fat32::disk_has_exfat(&drv);
                            let has_fat = crate::fat32::disk_has_fat32(&drv);
                            if has_ex || has_fat {
                                crate::slog_nano!(
                                    "Disk", "ata",
                                    "ISA {}: {} data FS exfat={} fat32={} sectors={} bps={}",
                                    base,
                                    if slave { "slave" } else { "master" },
                                    has_ex, has_fat, total, lba_size
                                );
                                let better = match &data_best {
                                    None => true,
                                    Some((_, sec, was_ex)) => {
                                        (has_ex && !*was_ex) || (has_ex == *was_ex && total > *sec)
                                    }
                                };
                                if better {
                                    data_best = Some((drv.clone(), total, has_ex));
                                }
                                continue;
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
        if let Some((drv, sec, ex)) = data_best {
            crate::slog_nano!(
                "Disk", "ata",
                "escolhido data disk slave={} sectors={} exfat={} bps={}",
                drv.slave, sec, ex, drv.lba_size
            );
            return Some(drv);
        }
        if best.is_some() { crate::slog_nano!("Disk", "ata", "Usando fallback type={:#x}", best_type); }
        best
    }

    /// Detecta bytes per sector via IDENTIFY words 117-118.
    /// Word 117 = logical sector size (low 16 bits)
    /// Word 118 = logical sector size (high 16 bits)
    /// Valor 0 = não reportado → assume 512.
    fn detect_sector_size(id: &[u16; 256]) -> u32 {
        let lo = id[117] as u32;
        let hi = id[118] as u32;
        let bps = lo | (hi << 16);
        // FAT spec: 512, 1024, 2048, 4096. Outros valores → 512.
        match bps {
            512 | 1024 | 2048 | 4096 => bps,
            _ => 512,
        }
    }

    unsafe fn has_mbr(&self) -> bool {
        let mut buf = [0u8; 512];
        if !self.read_sectors(0, &mut buf, 1) {
            crate::slog_nano!("Disk", "ata", "has_mbr: read_sectors falhou para slave={}", self.slave);
            return false;
        }
        let ok = buf[0x1FE] == 0x55 && buf[0x1FF] == 0xAA;
        crate::slog_nano!("Disk", "ata", "has_mbr: slave={} 55AA={}", self.slave, ok);
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

    /// Total de setores LBA do disco em unidades 512B (para BlockDevice).
    /// Se lba_size > 512, converte: total_512 = total_lba × (lba_size / 512).
    pub unsafe fn total_sectors_512(&self) -> u64 {
        let id = self.identify().unwrap_or([0u16; 256]);
        let lo = id[60] as u64;
        let hi = id[61] as u64;
        let total_lba = lo | (hi << 16);
        total_lba * (self.lba_size as u64 / PIO_SECTOR_BYTES as u64)
    }

    /// Lê setores lógicos do disco.
    ///
    /// Para 512B: cada LBA = 1 comando PIO (256 words = 512 bytes).
    /// Para 4Kn: cada LBA = 8 comandos PIO (8 × 512 = 4096 bytes).
    /// O caller passa LBA lógico e count de setores lógicos.
    pub unsafe fn read_sectors(&self, lba: u32, buf: &mut [u8], count: u8) -> bool {
        if self.lba_size <= PIO_SECTOR_BYTES {
            // 512B: caminho direto (1 comando PIO por setor)
            return self.read_sectors_512(lba, buf, count);
        }
        // 4Kn: para cada setor lógico, faz `ratio` reads de 512B
        let ratio = (self.lba_size / PIO_SECTOR_BYTES) as u8;
        for s in 0..count as usize {
            let log_lba = lba + s as u32;
            let buf_off = s * self.lba_size as usize;
            for sub in 0..ratio {
                // LBA 4Kn: setor lógico N = LBAs físicos [N*ratio .. N*ratio+ratio-1]
                let phys_lba = log_lba * ratio as u32 + sub as u32;
                let sub_off = buf_off + sub as usize * PIO_SECTOR_BYTES as usize;
                if !self.read_sectors_512(phys_lba, &mut buf[sub_off..], 1) {
                    crate::slog_nano!("Disk", "ata",
                        "read4kn: falhou LBA={} sub={}/{} slave={}", log_lba, sub, ratio, self.slave);
                    return false;
                }
            }
        }
        true
    }

    /// Lê setores de 512B (caminho PIO direto, sem adaptação 4Kn).
    unsafe fn read_sectors_512(&self, lba: u32, buf: &mut [u8], count: u8) -> bool {
        self.cmd(lba, count, 0x20);
        // 400ns delay pós-comando (ATA spec t0). Em TSC calibrado ~100-400 cycles.
        crate::tsc::sleep_us(1);
        for s in 0..count as usize {
            self.wait_bsy();
            if !self.wait_drq() {
                crate::slog_nano!("Disk", "ata", "read: DRQ nao pronto LBA={} s={}/{} slave={}", lba, s, count, self.slave);
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
        let m = AtaDriver {
            io_base: self.io_base, pci_bus: 0, pci_device: 0, pci_func: 0,
            slave: false, lba_size: self.lba_size,
        };
        if m.read_sectors(lba, buf, count) { return true; }
        let s = AtaDriver {
            io_base: self.io_base, pci_bus: 0, pci_device: 0, pci_func: 0,
            slave: true, lba_size: self.lba_size,
        };
        s.read_sectors(lba, buf, count)
    }

    /// Escreve setores lógicos no disco.
    /// Para 4Kn: cada LBA = `ratio` writes de 512B.
    pub unsafe fn write_sectors(&self, lba: u32, data: &[u8], count: u8) -> bool {
        if count == 0 { return false; }
        if self.lba_size <= PIO_SECTOR_BYTES {
            return self.write_sectors_512(lba, data, count);
        }
        // 4Kn: para cada setor lógico, faz `ratio` writes de 512B
        let ratio = (self.lba_size / PIO_SECTOR_BYTES) as u8;
        for s in 0..count as usize {
            let log_lba = lba + s as u32;
            let data_off = s * self.lba_size as usize;
            for sub in 0..ratio {
                let phys_lba = log_lba * ratio as u32 + sub as u32;
                let sub_off = data_off + sub as usize * PIO_SECTOR_BYTES as usize;
                let sub_end = core::cmp::min(sub_off + PIO_SECTOR_BYTES as usize, data.len());
                if !self.write_sectors_512(phys_lba, &data[sub_off..sub_end], 1) {
                    crate::slog_nano!("Disk", "ata",
                        "write4kn: falhou LBA={} sub={}/{} slave={}", log_lba, sub, ratio, self.slave);
                    return false;
                }
            }
        }
        true
    }

    /// Escreve setores de 512B (caminho PIO direto).
    unsafe fn write_sectors_512(&self, lba: u32, data: &[u8], count: u8) -> bool {
        if count == 0 { return false; }
        self.cmd(lba, count, 0x30);
        for s in 0..count as usize {
            self.wait_bsy();
            if !self.wait_drq() { return false; }
            let off = s * 512;
            for i in 0..256 {
                let lo = if off + i * 2 < data.len() { data[off + i * 2] } else { 0 };
                let hi = if off + i * 2 + 1 < data.len() { data[off + i * 2 + 1] } else { 0 };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_sector_size_512() {
        let mut id = [0u16; 256];
        id[117] = 512;
        id[118] = 0;
        assert_eq!(AtaDriver::detect_sector_size(&id), 512);
    }

    #[test]
    fn detect_sector_size_4096() {
        let mut id = [0u16; 256];
        id[117] = 4096;
        id[118] = 0;
        assert_eq!(AtaDriver::detect_sector_size(&id), 4096);
    }

    #[test]
    fn detect_sector_size_zero_defaults_512() {
        let id = [0u16; 256];
        assert_eq!(AtaDriver::detect_sector_size(&id), 512);
    }

    #[test]
    fn detect_sector_size_invalid_defaults_512() {
        let mut id = [0u16; 256];
        id[117] = 750; // não é potência de 2 válida
        assert_eq!(AtaDriver::detect_sector_size(&id), 512);
    }

    #[test]
    fn sector_size_accessor() {
        let drv = AtaDriver {
            io_base: 0x1F0, pci_bus: 0, pci_device: 0, pci_func: 0,
            slave: false, lba_size: 4096,
        };
        assert_eq!(drv.sector_size(), 4096);
    }
}
