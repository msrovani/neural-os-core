use crate::ata::AtaDriver;
use crate::fat::{Partition, write_boot_log};
use crate::serial_println;

static LOG_FILE: spin::Mutex<Option<alloc::string::String>> = spin::Mutex::new(None);

pub fn init(ata: &AtaDriver, parts: &[Partition]) {
    let name = alloc::format!("B{:07X}.LOG", 0u64);
    *LOG_FILE.lock() = Some(name);
}

pub fn log(msg: &str) {
    serial_println!("[LOG] {}", msg);
    unsafe {
        let ata_guard = crate::ATA_DRIVER.lock();
        if let Some(ref ata) = *ata_guard {
            let parts = crate::fat::read_mbr(ata);
            for part in &parts {
                if part.type_code == 0x0B || part.type_code == 0x0C {
                    write_boot_log(ata, part, msg);
                    break;
                }
            }
        }
    }
}
