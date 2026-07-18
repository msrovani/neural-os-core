//! Boot Logger — buffer RAM → flush FAT32 (`BOOT.LOG` 8.3) no pendrive/ATA.
//!
//! Notebooks modernos sem COM: este é o canal de diagnóstico.
//! Feature `fat-boot-log` (ativa no crate `boot` para imagem HW).
//!
//! `BOOT.LOG` é pré-alocado no mkfat32 (256 KiB) para sobrescrita via BlockDevice
//! (USB-MSC ou ATA) sem alocar clusters novos no boot.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use spin::Mutex;

use crate::block_dev::BlockDevice;

/// Nome 8.3 fixo — fácil achar no Windows após atribuir letra ao volume.
pub const BOOT_LOG_NAME: &str = "BOOT.LOG";
/// Capacidade do arquivo pré-alocado (mkfat32).
pub const BOOT_LOG_CAP: usize = 256 * 1024;

struct StackBuf {
    buf: [u8; 256],
    pos: usize,
}
impl StackBuf {
    fn new() -> Self {
        Self {
            buf: [0; 256],
            pos: 0,
        }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.pos]).unwrap_or("")
    }
}
impl Write for StackBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        if self.pos + b.len() > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.pos..self.pos + b.len()].copy_from_slice(b);
        self.pos += b.len();
        Ok(())
    }
}

pub static SESSION_FILENAME: Mutex<Option<String>> = Mutex::new(None);
pub static FAT_READY: AtomicBool = AtomicBool::new(false);
/// Heap talc ja inicializado — obrigatorio antes de qualquer alloc no logger.
static HEAP_READY: AtomicBool = AtomicBool::new(false);

pub fn heap_ready() -> bool {
    HEAP_READY.load(Ordering::Relaxed)
}

pub fn mark_heap_ready() {
    HEAP_READY.store(true, Ordering::Relaxed);
}

const PRE_FAT_CAPACITY: usize = 512;
static PRE_FAT_COUNT: AtomicUsize = AtomicUsize::new(0);
static PRE_FAT_BUF: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());
/// Conteúdo acumulado da sessão (cabe no BOOT.LOG pré-alocado).
static SESSION_BODY: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static DISK_WRITES: AtomicUsize = AtomicUsize::new(0);
/// Mensagens desde o último flush bem-sucedido (USB MSC é lento: não reescreve a cada linha).
static SINCE_FLUSH: AtomicUsize = AtomicUsize::new(0);
const FLUSH_EVERY: usize = 8;

fn buffer_log(msg: &str) {
    if !HEAP_READY.load(Ordering::Relaxed) {
        return;
    }
    let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let line = alloc::format!("[T+{}] {}\n", tick, msg);
    let mut buf = PRE_FAT_BUF.lock();
    if buf.len() < PRE_FAT_CAPACITY {
        buf.push(line.into_bytes());
    }
    PRE_FAT_COUNT.fetch_add(1, Ordering::Relaxed);
}

fn encode_83(name: &str) -> [u8; 11] {
    let mut out = [b' '; 11];
    let upper = name.to_ascii_uppercase();
    let (base, ext) = match upper.rsplit_once('.') {
        Some((b, e)) => (b, e),
        None => (upper.as_str(), ""),
    };
    for (i, &c) in base.as_bytes().iter().take(8).enumerate() {
        out[i] = c;
    }
    for (i, &c) in ext.as_bytes().iter().take(3).enumerate() {
        out[8 + i] = c;
    }
    out
}

/// Lista partições FAT32 (MBR + GPT hybrid do usb_hw.img).
fn fat32_parts(dev: &mut dyn BlockDevice) -> Vec<crate::fat32::Partition> {
    let mut mbr = [0u8; 512];
    if !dev.read_sectors(0, &mut mbr) || mbr[0x1FE] != 0x55 || mbr[0x1FF] != 0xAA {
        return Vec::new();
    }
    let mut parts = crate::fat32::parse_mbr_sector(&mbr);
    let has_ee = parts.iter().any(|p| p.type_code == 0xEE);
    let has_fat = parts
        .iter()
        .any(|p| matches!(p.type_code, 0x0B | 0x0C | 0x1C | 0x73));
    if has_ee || !has_fat {
        let gpt = crate::fat32::parse_gpt_partitions(|lba, buf| {
            let mut tmp = [0u8; 512];
            if !dev.read_sectors(lba, &mut tmp) {
                return false;
            }
            *buf = tmp;
            true
        });
        for g in gpt {
            if g.type_code == 0xEE {
                continue;
            }
            if parts.iter().any(|p| p.lba_start == g.lba_start) {
                continue;
            }
            parts.push(g);
        }
    }
    parts
}

/// Sobrescreve `BOOT.LOG` pré-alocado no FAT32 via BlockDevice (USB ou ATA).
unsafe fn overwrite_boot_log(dev: &mut dyn BlockDevice, data: &[u8]) -> bool {
    let want = encode_83(BOOT_LOG_NAME);
    let parts = fat32_parts(dev);
    for part in &parts {
        if !matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0x73) {
            continue;
        }
        let lba_start = part.lba_start as u64;
        let mut bpb = [0u8; 512];
        if !dev.read_sectors(lba_start, &mut bpb) {
            continue;
        }
        if &bpb[3..11] == b"EXFAT   " {
            continue;
        }
        let bps = u16::from_le_bytes([bpb[0x0B], bpb[0x0C]]) as u32;
        let spc = bpb[0x0D] as u32;
        let reserved = u16::from_le_bytes([bpb[0x0E], bpb[0x0F]]) as u32;
        let fat_count = bpb[0x10] as u32;
        let root_entries = u16::from_le_bytes([bpb[0x11], bpb[0x12]]);
        if root_entries > 0 || bps == 0 || spc == 0 {
            continue;
        }
        let spf = u32::from_le_bytes([bpb[0x24], bpb[0x25], bpb[0x26], bpb[0x27]]);
        let root_cluster = u32::from_le_bytes([bpb[0x2C], bpb[0x2D], bpb[0x2E], bpb[0x2F]]);
        let fat_lba = lba_start as u32 + reserved;
        let data_lba = fat_lba + fat_count * spf;
        let cluster_bytes = (spc * bps) as usize;

        // Walk root dir
        let mut cluster = root_cluster;
        let mut walked = 0u32;
        while cluster >= 2 && cluster < 0x0FFF_FFF8 && walked < 64 {
            walked += 1;
            let clba = data_lba + (cluster - 2) * spc;
            let mut dir = vec![0u8; cluster_bytes];
            for s in 0..spc {
                let off = (s * bps) as usize;
                if !dev.read_sectors((clba + s) as u64, &mut dir[off..off + bps as usize]) {
                    return false;
                }
            }
            for entry in (0..dir.len()).step_by(32) {
                let first = dir[entry];
                if first == 0 {
                    break;
                }
                if first == 0xE5 {
                    continue;
                }
                if dir[entry + 11] & 0x0F == 0x0F || dir[entry + 11] & 0x08 != 0 {
                    continue;
                }
                if &dir[entry..entry + 11] != &want {
                    continue;
                }
                let alloc_size = u32::from_le_bytes([
                    dir[entry + 28],
                    dir[entry + 29],
                    dir[entry + 30],
                    dir[entry + 31],
                ]) as usize;
                let fc_lo = u16::from_le_bytes([dir[entry + 26], dir[entry + 27]]);
                let fc_hi = u16::from_le_bytes([dir[entry + 20], dir[entry + 21]]);
                let mut fc = ((fc_hi as u32) << 16) | fc_lo as u32;
                let write_len = data.len().min(alloc_size).min(BOOT_LOG_CAP);
                let mut written = 0usize;

                while fc >= 2 && fc < 0x0FFF_FFF8 && written < write_len {
                    let fc_lba = data_lba + (fc - 2) * spc;
                    for s in 0..spc {
                        if written >= write_len {
                            break;
                        }
                        let mut sector = [0u8; 512];
                        let take = (write_len - written).min(512);
                        sector[..take].copy_from_slice(&data[written..written + take]);
                        // resto zero (limpa lixo anterior)
                        if !dev.write_sectors((fc_lba + s) as u64, &sector) {
                            return false;
                        }
                        written += take;
                    }
                    // next FAT entry
                    let fat_off = fc as usize * 4;
                    let fat_sec = fat_lba + (fat_off as u32 / bps);
                    let mut fsec = [0u8; 512];
                    if !dev.read_sectors(fat_sec as u64, &mut fsec) {
                        return false;
                    }
                    let boff = fat_off % bps as usize;
                    fc = u32::from_le_bytes([
                        fsec[boff],
                        fsec[boff + 1],
                        fsec[boff + 2],
                        fsec[boff + 3],
                    ]) & 0x0FFF_FFFF;
                }

                // Atualiza tamanho na dir entry
                let new_size = (write_len as u32).to_le_bytes();
                dir[entry + 28..entry + 32].copy_from_slice(&new_size);
                for s in 0..spc {
                    let off = (s * bps) as usize;
                    if !dev.write_sectors((clba + s) as u64, &dir[off..off + bps as usize]) {
                        return false;
                    }
                }
                return true;
            }
            // next root cluster
            let fat_off = cluster as usize * 4;
            let fat_sec = fat_lba + (fat_off as u32 / bps);
            let mut fsec = [0u8; 512];
            if !dev.read_sectors(fat_sec as u64, &mut fsec) {
                break;
            }
            let boff = fat_off % bps as usize;
            cluster = u32::from_le_bytes([
                fsec[boff],
                fsec[boff + 1],
                fsec[boff + 2],
                fsec[boff + 3],
            ]) & 0x0FFF_FFFF;
        }
    }
    false
}

fn build_session_bytes() -> Vec<u8> {
    let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let ver = env!("CARGO_PKG_VERSION");
    let mut content = alloc::format!(
        "[S] neural-os-core {} BOOT.LOG tick={} fat-boot-log=1\n",
        ver, tick
    )
    .into_bytes();
    let buf = PRE_FAT_BUF.lock();
    for line in buf.iter() {
        content.extend_from_slice(line);
    }
    drop(buf);
    let body = SESSION_BODY.lock();
    content.extend_from_slice(&body);
    if content.len() > BOOT_LOG_CAP {
        content.truncate(BOOT_LOG_CAP);
    }
    content
}

#[cfg(feature = "fat-boot-log")]
fn persist_now(dev: Option<&mut dyn BlockDevice>) -> bool {
    let content = build_session_bytes();
    let ok = if let Some(d) = dev {
        unsafe { overwrite_boot_log(d, &content) }
    } else {
        // Tenta USB depois ATA
        let mut ok = false;
        if let Some(ref mut msc) = *crate::USB_MSC.lock() {
            ok = unsafe { overwrite_boot_log(msc, &content) };
        }
        if !ok {
            if let Some(ref mut ata) = *crate::ATA_DRIVER.lock() {
                ok = unsafe { overwrite_boot_log(ata, &content) };
            }
        }
        ok
    };
    if ok {
        DISK_WRITES.fetch_add(1, Ordering::Relaxed);
        FAT_READY.store(true, Ordering::Relaxed);
        SINCE_FLUSH.store(0, Ordering::Relaxed);
        *SESSION_FILENAME.lock() = Some(String::from(BOOT_LOG_NAME));
    }
    ok
}

#[cfg(not(feature = "fat-boot-log"))]
fn persist_now(_dev: Option<&mut dyn BlockDevice>) -> bool {
    false
}

/// Init legado ATA (partições já lidas).
pub fn init(ata: Option<&crate::ata::AtaDriver>, _parts: &[crate::fat32::Partition]) {
    #[cfg(feature = "fat-boot-log")]
    {
        if let Some(a) = ata {
            // AtaDriver: precisamos &mut — try via global
            let _ = a;
            let ok = persist_now(None);
            crate::serial_println!(
                "[LOG] BOOT.LOG persist ATA/USB={} writes={}",
                ok,
                DISK_WRITES.load(Ordering::Relaxed)
            );
            if !ok {
                crate::serial_println!("[LOG] WARN: BOOT.LOG nao gravado — confira FAT32 no stick");
            }
        } else {
            let ok = persist_now(None);
            crate::serial_println!("[LOG] BOOT.LOG persist (no ATA arg) ok={}", ok);
        }
    }
    #[cfg(not(feature = "fat-boot-log"))]
    {
        let _ = ata;
        crate::serial_println!(
            "[LOG] SKIP fat session write (enable feature fat-boot-log to persist)"
        );
        FAT_READY.store(false, Ordering::Relaxed);
    }
}

/// Init imediato após USB-MSC (caminho notebook sem serial).
pub fn init_after_usb() {
    #[cfg(feature = "fat-boot-log")]
    {
        log("BOOT: fat-boot-log init_after_usb");
        let ok = persist_now(None);
        crate::serial_println!(
            "[LOG] init_after_usb BOOT.LOG ok={} (procure BOOT.LOG na raiz FAT32)",
            ok
        );
        if ok {
            // Splash hint se FB vivo
            crate::display::fb::boot_splash("LOG->BOOT.LOG on USB FAT32 (assign drive letter)");
        }
    }
}

/// Registra mensagem. Com fat-boot-log: buffer + flush a cada FLUSH_EVERY msgs.
pub fn log(msg: &str) {
    crate::serial_println!("[LOG] {}", msg);

    if !HEAP_READY.load(Ordering::Relaxed) {
        return;
    }

    if !FAT_READY.load(Ordering::Relaxed) {
        buffer_log(msg);
        #[cfg(feature = "fat-boot-log")]
        {
            let n = SINCE_FLUSH.fetch_add(1, Ordering::Relaxed) + 1;
            // Tentativa oportunista só de N em N (FAT walk + USB write é caro).
            if n >= FLUSH_EVERY {
                let _ = persist_now(None);
            }
        }
        return;
    }

    let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let mut sb = StackBuf::new();
    let _ = write!(sb, "[T+{}] {}\n", tick, msg);
    {
        let mut body = SESSION_BODY.lock();
        if body.len() + sb.pos < BOOT_LOG_CAP - 64 {
            body.extend_from_slice(sb.as_str().as_bytes());
        }
    }

    #[cfg(feature = "fat-boot-log")]
    {
        let n = SINCE_FLUSH.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= FLUSH_EVERY {
            let _ = persist_now(None);
        }
    }
}

/// Flush forçado (checkpoints críticos).
pub fn flush() {
    #[cfg(feature = "fat-boot-log")]
    {
        let ok = persist_now(None);
        crate::serial_println!(
            "[LOG] flush BOOT.LOG ok={} bytes~{}",
            ok,
            build_session_bytes().len()
        );
    }
}

/// Anexa texto sem `serial_println` (evita recursão no path sem-COM do serial.rs).
pub fn append_raw(msg: &str) {
    if !HEAP_READY.load(Ordering::Relaxed) {
        return;
    }
    if !FAT_READY.load(Ordering::Relaxed) {
        buffer_log(msg);
        return;
    }
    let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let line = alloc::format!("[T+{}] {}\n", tick, msg);
    {
        let mut body = SESSION_BODY.lock();
        if body.len() + line.len() < BOOT_LOG_CAP - 64 {
            body.extend_from_slice(line.as_bytes());
        }
    }
    #[cfg(feature = "fat-boot-log")]
    {
        let n = SINCE_FLUSH.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= FLUSH_EVERY {
            let _ = persist_now(None);
        }
    }
}
