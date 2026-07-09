use crate::ata::AtaDriver;
use crate::ahci::AhciDriver;

pub trait BlockDevice {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool;
}

impl BlockDevice for AtaDriver {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        let count = (buf.len() / 512).min(255) as u8;
        unsafe { crate::ata::AtaDriver::read_sectors(self, lba as u32, buf, count) }
    }
}

impl BlockDevice for AhciDriver {
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> bool {
        let count = buf.len() / 512;
        if count == 0 { return false; }
        unsafe { self.read(0, lba, count, buf) }
    }
}
