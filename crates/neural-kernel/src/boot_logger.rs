use alloc::vec::Vec;
use alloc::string::String;
use alloc::string::ToString;
use spin::Mutex;

pub static SESSION_FILENAME: Mutex<Option<String>> = Mutex::new(None);

/// Gera nome 8.3: SES<TICK_HEX>.LOG (ex: SES0001A5.LOG)
fn session_name(tick: u64) -> String {
    let hex = alloc::format!("{:05X}", tick);
    let n = if hex.len() > 5 { &hex[hex.len()-5..] } else { &hex };
    alloc::format!("SES{}.LOG", n)
}

/// Inicializa logger. Cria arquivo de sessao no primeiro FAT32 encontrado.
pub fn init(ata: Option<&crate::ata::AtaDriver>, parts: &[crate::fat::Partition]) {
    if let Some(a) = ata {
        let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        let name = session_name(tick);
        // Cria arquivo vazio na particao
        for part in parts {
            if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
                unsafe {
                    let writer = crate::fat::Fat32Writer::new(a, part);
                    if let Some(w) = writer {
                        let header = alloc::format!("[S] neural-os-core {} boot\n[T+0] [BOOT] Session started\n", env!("CARGO_PKG_VERSION"));
                        w.write_file(&name, header.as_bytes());
                        *SESSION_FILENAME.lock() = Some(name);
                    }
                }
                break;
            }
        }
    }
    // Se ATA nao disponivel, nao ha o que fazer — BootLog em RAM serve
}

/// Registra mensagem no log serial + disco de sessao
pub fn log(msg: &str) {
    crate::serial_println!("[LOG] {}", msg);

    let sfn_guard = SESSION_FILENAME.lock();
    if let Some(ref name) = *sfn_guard {
        let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        let log_line = alloc::format!("[T+{}] [LOG] {}\n", tick, msg);
        unsafe {
            let ata_guard = crate::ATA_DRIVER.lock();
            if let Some(ref ata) = *ata_guard {
                let parts = crate::fat::read_mbr(ata);
                for part in &parts {
                    if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
                        let writer = crate::fat::Fat32Writer::new(ata, part);
                        if let Some(w) = writer {
                            if let Some(existing) = w.reader.read_file(name) {
                                let mut new_data = existing;
                                new_data.extend_from_slice(log_line.as_bytes());
                                w.write_file(name, &new_data);
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
}
