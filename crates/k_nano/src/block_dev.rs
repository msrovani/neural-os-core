use crate::ata::AtaDriver;
use crate::ahci::AhciDriver;
use crate::disk_agent::nvme::NvmeDriver;

pub trait BlockDevice {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool;
    fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool;
    /// Total de setores 512B (0 = desconhecido).
    fn total_sectors(&self) -> u64 {
        0
    }
    /// Nome curto para StorageBus / logs (`nvme0`, `ahci0`, …).
    fn name(&self) -> &str {
        "blk"
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
    fn name(&self) -> &str {
        "ata0"
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
    fn name(&self) -> &str {
        "ahci0"
    }
}

impl BlockDevice for NvmeDriver {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        unsafe { self.read_sectors_bounce(lba, buf) }
    }
    fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> bool {
        unsafe { self.write_sectors_bounce(lba, buf) }
    }
    fn total_sectors(&self) -> u64 {
        if self.lba_size == 0 {
            return 0;
        }
        // BlockDevice usa unidades 512B
        self.lba_count.saturating_mul(self.lba_size as u64 / 512)
    }
    fn name(&self) -> &str {
        "nvme0"
    }
}
