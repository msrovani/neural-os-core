//! Boot Logger — buffer em RAM antes do FAT, flush para disco quando disponivel.
//! Gera arquivos /logs/B<TICK>.LOG no primeiro volume FAT32 montado.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;
use core::sync::atomic::{AtomicBool, Ordering};
use core::fmt::Write;

struct StackBuf { buf: [u8; 256], pos: usize }
impl StackBuf {
    fn new() -> Self { Self { buf: [0; 256], pos: 0 } }
    fn as_str(&self) -> &str { core::str::from_utf8(&self.buf[..self.pos]).unwrap_or("") }
}
impl Write for StackBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        if self.pos + b.len() > self.buf.len() { return Err(core::fmt::Error); }
        self.buf[self.pos..self.pos + b.len()].copy_from_slice(b);
        self.pos += b.len();
        Ok(())
    }
}

pub static SESSION_FILENAME: Mutex<Option<String>> = Mutex::new(None);
pub static FAT_READY: AtomicBool = AtomicBool::new(false);

/// Buffer de mensagens anter da inicializacao do FAT
const PRE_FAT_CAPACITY: usize = 256;
static PRE_FAT_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static PRE_FAT_BUF: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// Gera nome: B<XXXXX>.LOG onde XXXXX = tick hex (ex: B0001A.LOG)
fn session_name(tick: u64) -> String {
    let hex = alloc::format!("{:05X}", tick.min(0xFFFFF));
    alloc::format!("B{}.LOG", &hex[hex.len().saturating_sub(5)..])
}

/// Bufferiza mensagem (antes do FAT estar pronto)
fn buffer_log(msg: &str) {
    let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    let line = alloc::format!("[T+{}] [LOG] {}\n", tick, msg);
    let mut buf = PRE_FAT_BUF.lock();
    if buf.len() < PRE_FAT_CAPACITY {
        buf.push(line.into_bytes());
    }
    PRE_FAT_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Cria arquivo de sessao no primeiro FAT32 e faz flush do buffer
pub fn init(ata: Option<&crate::ata::AtaDriver>, parts: &[crate::fat32::Partition]) {
    if let Some(a) = ata {
        let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        let name = session_name(tick);
        for part in parts {
            if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
                unsafe {
                    crate::serial_println!("[LOG] Abrindo FAT32 writer para sessao {}...", name);
                    if let Some(w) = crate::fat32::Fat32Writer::new(a, part) {
                        let ver = env!("CARGO_PKG_VERSION");
                        let mut content = alloc::format!("[S] neural-os-core {} boot tick={}\n", ver, tick);
                        let buf = PRE_FAT_BUF.lock();
                        for line in buf.iter() {
                            content.push_str(core::str::from_utf8(line).unwrap_or(""));
                        }
                        drop(buf);
                        crate::serial_println!("[LOG] Gravando {} ({} bytes)...", name, content.len());
                        // N1.0/N1.1: write FAT no boot sob TCG pode travar minutos (PIO).
                        // Serial continua; disco = best-effort sem bloquear Runtime/STATUS.
                        #[cfg(not(feature = "fat-boot-log"))]
                        {
                            let _ = content;
                            let _ = w;
                            crate::serial_println!(
                                "[LOG] SKIP fat session write (enable feature fat-boot-log to persist)"
                            );
                            FAT_READY.store(false, Ordering::Relaxed);
                        }
                        #[cfg(feature = "fat-boot-log")]
                        if w.write_file(&name, content.as_bytes()) {
                            *SESSION_FILENAME.lock() = Some(name.clone());
                            FAT_READY.store(true, Ordering::Relaxed);
                            let count = PRE_FAT_COUNT.load(Ordering::Relaxed);
                            crate::serial_println!(
                                "[LOG] FAT32 pronto: escrevendo para logs/{} ({} buffered)",
                                name, count
                            );
                        } else {
                            crate::serial_println!(
                                "[LOG] WARN: falha ao gravar {} — seguindo sem log em disco",
                                name
                            );
                        }
                        break;
                    } else {
                        crate::serial_println!("[LOG] WARN: Fat32Writer::new falhou");
                    }
                }
            }
        }
    }
}

/// Contador de appends em disco apos FAT_READY (rate-limit sob TCG).
static DISK_APPENDS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Registra mensagem de log. Antes do FAT: bufferiza. Depois: serial + append ocasional.
pub fn log(msg: &str) {
    crate::serial_println!("[LOG] {}", msg);

    if !FAT_READY.load(Ordering::Relaxed) {
        buffer_log(msg);
        return;
    }

    // Rate-limit: cada append FAT = MBR+Writer+read+write (caro em TCG PIO).
    // Mantém serial sempre; disco só a cada 8ª mensagem (além do flush inicial em init).
    let n = DISK_APPENDS.fetch_add(1, Ordering::Relaxed);
    if n % 8 != 0 {
        return;
    }

    // Anexa ao arquivo de sessao no FAT (sem alloc no path quente)
    let sfn_guard = SESSION_FILENAME.lock();
    if let Some(ref name) = *sfn_guard {
        let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        let mut sb = StackBuf::new();
        let _ = write!(sb, "[T+{}] [LOG] {}\n", tick, msg);
        let log_line = sb.as_str();
        unsafe {
            // try_lock: evita deadlock se o caller ja segura ATA_DRIVER
            let Some(ata_guard) = crate::ATA_DRIVER.try_lock() else {
                return;
            };
            if let Some(ref ata) = *ata_guard {
                let parts = crate::fat32::read_mbr(ata);
                for part in &parts {
                    if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
                        if let Some(w) = crate::fat32::Fat32Writer::new(ata, part) {
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

