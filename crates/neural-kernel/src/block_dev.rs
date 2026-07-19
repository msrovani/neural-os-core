//! ADR-0042 emagrecer Onda 4 — BlockDevice canônico em k_nano; USB MSC impl local.
pub use k_nano::block_dev::*;

use crate::usb_msc::UsbMassStorage;

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
        self.max_lba.saturating_add(1)
    }
}
