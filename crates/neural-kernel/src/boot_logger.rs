use alloc::vec::Vec;
use alloc::string::String;
use spin::Mutex;

static PRE_ATA_BUF: Mutex<Vec<String>> = Mutex::new(Vec::new());
static READY: Mutex<bool> = Mutex::new(false);

pub fn init(ata: &crate::ata::AtaDriver, parts: &[crate::fat::Partition]) {
    // Flush pre-ATA buffer para disco
    let buf = PRE_ATA_BUF.lock();
    let msgs = buf.clone();
    drop(buf);

    for msg in &msgs {
        unsafe { write_disk(ata, parts, msg); }
    }

    PRE_ATA_BUF.lock().clear();
    *READY.lock() = true;
}

pub fn log(msg: &str) {
    crate::serial_println!("[LOG] {}", msg);

    if *READY.lock() {
        // ATA ja disponivel — escreve direto
        unsafe {
            let ata_guard = crate::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                let parts = crate::fat::read_mbr(ata);
                for part in &parts {
                    if part.type_code == 0x0B || part.type_code == 0x0C {
                        crate::fat::write_boot_log(ata, part, msg);
                        break;
                    }
                }
            }
        }
    } else {
        // Pre-ATA: bufferizar em memoria
        PRE_ATA_BUF.lock().push(String::from(msg));
    }
}

unsafe fn write_disk(ata: &crate::ata::AtaDriver, parts: &[crate::fat::Partition], msg: &str) {
    for part in parts {
        if part.type_code == 0x0B || part.type_code == 0x0C {
            crate::fat::write_boot_log(ata, part, msg);
            break;
        }
    }
}
