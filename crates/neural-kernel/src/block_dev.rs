use crate::ata::AtaDriver;
use crate::ahci::AhciDriver;
use crate::usb_msc::UsbMassStorage;

pub trait BlockDevice {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool;
    fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool;
    /// Total de setores 512B (0 = desconhecido).
    fn total_sectors(&self) -> u64 {
        0
    }
}

impl BlockDevice for AtaDriver {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        if lba > 0x0FFFFFFF { return false; } // LBA48 not yet implemented
        if buf.len() % 512 != 0 || buf.is_empty() { return false; } // tamanho nao alinhado a setor
        let sectors = buf.len() / 512;
        if sectors > 255 { return false; } // LBA28 PIO: max 255 setores por comando
        unsafe { crate::ata::AtaDriver::read_sectors(self, lba as u32, buf, sectors as u8) }
    }
    fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
        if lba > 0x0FFFFFFF { return false; } // LBA48 not yet implemented
        if buf.len() % 512 != 0 || buf.is_empty() { return false; } // tamanho nao alinhado a setor
        let sectors = buf.len() / 512;
        if sectors > 255 { return false; } // LBA28 PIO: max 255 setores por comando
        unsafe { crate::ata::AtaDriver::write_sectors(self, lba as u32, buf, sectors as u8) }
    }
    fn total_sectors(&self) -> u64 {
        unsafe { AtaDriver::total_sectors(self).unwrap_or(0) }
    }
}

impl BlockDevice for AhciDriver {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        if buf.len() % 512 != 0 || buf.is_empty() { return false; } // tamanho nao alinhado a setor
        let count = buf.len() / 512;
        unsafe { self.read(0, lba, count, buf) }
    }
    fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
        if buf.len() % 512 != 0 || buf.is_empty() { return false; } // tamanho nao alinhado a setor
        let count = buf.len() / 512;
        unsafe { self.write(0, lba, count, buf) }
    }
}

impl BlockDevice for UsbMassStorage {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        if buf.len() % 512 != 0 || buf.is_empty() {
            return false;
        }
        let sectors = buf.len() / 512;
        for i in 0..sectors {
            let mut sector = [0u8; 512];
            if !unsafe { self.read_sector(lba + i as u64, &mut sector) } {
                return false;
            }
            let off = i * 512;
            buf[off..off + 512].copy_from_slice(&sector);
        }
        true
    }
    fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
        if buf.len() % 512 != 0 || buf.is_empty() {
            return false;
        }
        let sectors = buf.len() / 512;
        for i in 0..sectors {
            let off = i * 512;
            let mut sector = [0u8; 512];
            sector.copy_from_slice(&buf[off..off + 512]);
            if !unsafe { self.write_sector(lba + i as u64, &sector) } {
                return false;
            }
        }
        true
    }
    fn total_sectors(&self) -> u64 {
        // READ CAPACITY retorna max_lba (ultimo LBA endereçavel)
        self.max_lba.saturating_add(1)
    }
}
