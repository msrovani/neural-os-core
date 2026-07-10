with open("crates/neural-kernel/src/self_heal.rs", "a") as f:
    f.write("""

// --- FS SelfHeal --- bad block tracking, CRC verification ---

use spin::Mutex;
use alloc::collections::BTreeSet;

pub static BAD_BLOCKS: Mutex<BTreeSet<u64>> = Mutex::new(BTreeSet::new());

pub fn mark_bad(dev_name: &str, lba: u64) {
    BAD_BLOCKS.lock().insert(lba);
    crate::serial_println!("[SELFHEAL] Bad block {}@{:#x}", dev_name, lba);
}

pub fn is_bad(lba: u64) -> bool {
    BAD_BLOCKS.lock().contains(&lba)
}

pub fn verify_block(data: &[u8; 4096]) -> bool {
    let stored = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let computed = crate::neural_fs::checksum::crc32c(&data[4..4096]);
    stored == computed
}

pub fn read_with_retry(dev: &mut dyn crate::block_dev::BlockDevice, lba: u64, buf: &mut [u8], name: &str) -> bool {
    for attempt in 0..3 {
        if dev.read_sectors(lba, buf) { return true; }
        crate::serial_println!("[SELFHEAL] Retry {} {:#x} (attempt {})", name, lba, attempt + 1);
    }
    mark_bad(name, lba);
    false
}

pub fn write_with_retry(dev: &mut dyn crate::block_dev::BlockDevice, lba: u64, buf: &[u8], name: &str) -> bool {
    for attempt in 0..3 {
        if dev.write_sectors(lba, buf) { return true; }
        crate::serial_println!("[SELFHEAL] Write retry {} {:#x} (attempt {})", name, lba, attempt + 1);
    }
    mark_bad(name, lba);
    false
}
""")
print("Appended OK")
