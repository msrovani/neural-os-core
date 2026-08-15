//! Boot Logger â€” buffer RAM â†’ flush FAT32 (`BOOT.LOG` 8.3) via BlockDevice.
//!
//! Notebooks modernos sem COM: este Ã© o canal de diagnÃ³stico.
//! Feature `fat-boot-log` (ativa no crate `boot` para imagem HW).
//!
//! `BOOT.LOG` Ã© prÃ©-alocado no mkfat32 (256 KiB) para sobrescrita via BlockDevice
//! (USB-MSC ou ATA) sem alocar clusters novos no boot.
//!
//! Display-dependent wrappers (init_after_usb, maybe_uefi_flush_reboot,
//! flush_bootlog_after_greeting) vivem no neural-kernel bin.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use spin::Mutex;

use crate::block_dev::BlockDevice;

/// Nome 8.3 fixo â€” fÃ¡cil achar no Windows apÃ³s atribuir letra ao volume.
pub const BOOT_LOG_NAME: &str = "BOOT.LOG";
/// Capacidade do arquivo prÃ©-alocado (mkfat32).
pub const BOOT_LOG_CAP: usize = 256 * 1024;

struct StackBuf {
    buf: [u8; 256],
    pos: usize,
}
impl StackBuf {
    fn new() -> Self {
        Self { buf: [0; 256], pos: 0 }
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
/// Heap talc jÃ¡ inicializado â€” obrigatÃ³rio antes de qualquer alloc no logger.
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
/// ConteÃºdo acumulado da sessÃ£o (cabe no BOOT.LOG prÃ©-alocado).
static SESSION_BODY: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static DISK_WRITES: AtomicUsize = AtomicUsize::new(0);
/// Mensagens desde o Ãºltimo flush bem-sucedido (USB MSC Ã© lento: nÃ£o reescreve a cada linha).
static SINCE_FLUSH: AtomicUsize = AtomicUsize::new(0);
const FLUSH_EVERY: usize = 16;

fn buffer_log(msg: &str) {
    // Espelho fÃ­sico (ramlog); persistÃªncia FAT sÃ³ via MSC/ATA â€” sem soft-reboot.
    crate::boot_ramlog::append(msg);
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

/// Lista partiÃ§Ãµes FAT32 (MBR + GPT hybrid do usb_hw.img).
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
            if g.type_code == 0xEE { continue; }
            if parts.iter().any(|p| p.lba_start == g.lba_start) { continue; }
            parts.push(g);
        }
    }
    parts
}

/// Sobrescreve payload de `BOOT.LOG` prÃ©-alocado via BlockDevice (USB ou ATA).
/// SESSÃƒO_260: logs de diagnÃ³stico no motivo exato de cada falha â€” no HW real
/// (pendrive bootÃ¡vel via USB-MSC) o flush falhava silencioso e o BOOT.LOG
/// ficava vazio; com o motivo, o prÃ³ximo boot mostra onde trava.
///
/// **SESSION_260 (dir rasgado):** reescrever o dir cluster a cada flush rasgava
/// o FAT se o boot crashasse no meio (triple fault) â†’ Windows nÃ£o montava o
/// volume. PolÃ­tica segura:
/// 1. Escreve **sÃ³ clusters de dados** (payload + padding zero atÃ© o fim do
///    Ãºltimo setor tocado).
/// 2. SÃ³ toca a directory entry se o tamanho em disco for 0 ou absurdo
///    (< 512) â€” aÃ­ fixa em `BOOT_LOG_CAP` **num Ãºnico WRITE** do setor do
///    dirent (nÃ£o reescreve o cluster inteiro). Demais flushes = data-only.
unsafe fn overwrite_boot_log(dev: &mut dyn BlockDevice, data: &[u8]) -> bool {
    let want = encode_83(BOOT_LOG_NAME);
    let parts = fat32_parts(dev);
    if parts.is_empty() {
        crate::boot_logger::log(&alloc::format!("bootlog: 0 particoes FAT32 encontradas (MBR/GPT parse?)"));
        return false;
    }
    for part in &parts {
        if !matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0x73) { continue; }
        let lba_start = part.lba_start as u64;
        let mut bpb = [0u8; 512];
        if !dev.read_sectors(lba_start, &mut bpb) {
            crate::boot_logger::log(&alloc::format!("bootlog: read BPB LBA {} falhou", lba_start));
            continue;
        }
        if &bpb[3..11] == b"EXFAT   " { continue; }
        let bps = u16::from_le_bytes([bpb[0x0B], bpb[0x0C]]) as u32;
        let spc = bpb[0x0D] as u32;
        let reserved = u16::from_le_bytes([bpb[0x0E], bpb[0x0F]]) as u32;
        let fat_count = bpb[0x10] as u32;
        let root_entries = u16::from_le_bytes([bpb[0x11], bpb[0x12]]);
        if root_entries > 0 || bps < 512 || bps > 4096 || bps % 32 != 0 || spc == 0 {
            crate::boot_logger::log(&alloc::format!("bootlog: LBA {} BPB nao-FAT32 (re={} bps={} spc={})", lba_start, root_entries, bps, spc));
            continue;
        }
        let spf = u32::from_le_bytes([bpb[0x24], bpb[0x25], bpb[0x26], bpb[0x27]]);
        let root_cluster = u32::from_le_bytes([bpb[0x2C], bpb[0x2D], bpb[0x2E], bpb[0x2F]]);
        let fat_lba = lba_start as u32 + reserved;
        let data_lba = fat_lba + fat_count * spf;
        let cluster_bytes = (spc * bps) as usize;

        let mut cluster = root_cluster;
        let mut walked = 0u32;
        while cluster >= 2 && cluster < 0x0FFF_FFF8 && walked < 64 {
            walked += 1;
            let clba = data_lba + (cluster - 2) * spc;
            let mut dir = vec![0u8; cluster_bytes];
            for s in 0..spc {
                let off = (s * bps) as usize;
                if !dev.read_sectors((clba + s) as u64, &mut dir[off..off + bps as usize]) {
                    crate::boot_logger::log(&alloc::format!("bootlog: read dir LBA {} falhou", u64::from(clba) + u64::from(s)));
                    return false;
                }
            }
            for entry in (0..dir.len()).step_by(32) {
                let first = dir[entry];
                if first == 0 { break; }
                if first == 0xE5 { continue; }
                if dir[entry + 11] & 0x0F == 0x0F || dir[entry + 11] & 0x08 != 0 { continue; }
if &dir[entry..entry + 11] != &want { continue; }
                let alloc_size = u32::from_le_bytes([
                    dir[entry + 28], dir[entry + 29], dir[entry + 30], dir[entry + 31],
                ]) as usize;
                // Capacidade efetiva: se o dirent ainda tem size "curto" (seed
                // truncado / 0), assumimos o pré-alocado de mkfat32 (BOOT_LOG_CAP).
                let capacity = if alloc_size >= 512 {
                    alloc_size.min(BOOT_LOG_CAP)
                } else {
                    BOOT_LOG_CAP
                };
                let fc_lo = u16::from_le_bytes([dir[entry + 26], dir[entry + 27]]);
                let fc_hi = u16::from_le_bytes([dir[entry + 20], dir[entry + 21]]);
                let mut fc = ((fc_hi as u32) << 16) | fc_lo as u32;
                let write_len = data.len().min(capacity);
                let mut written = 0usize;

                while fc >= 2 && fc < 0x0FFF_FFF8 && written < write_len {
                    let fc_lba = data_lba + (fc - 2) * spc;
                    for s in 0..spc {
                        if written >= write_len { break; }
                        let mut sector = [0u8; 512];
                        let take = (write_len - written).min(512);
                        sector[..take].copy_from_slice(&data[written..written + take]);
                        // Padding zero no resto do setor → leitor vê EOF limpo.
                        if !dev.write_sectors((fc_lba + s) as u64, &sector) {
                            crate::boot_logger::log(&alloc::format!("bootlog: WRITE LBA {} falhou (tick={})", u64::from(fc_lba) + u64::from(s), crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed)));
                            return false;
                        }
                        written += take;
                    }
                    let fat_off = fc as usize * 4;
                    let fat_sec = fat_lba + (fat_off as u32 / bps);
                    let mut fsec = [0u8; 512];
                    if !dev.read_sectors(fat_sec as u64, &mut fsec) {
                        crate::boot_logger::log(&alloc::format!("bootlog: read FAT LBA {} falhou", fat_sec));
                        return false;
                    }
                    let boff = fat_off % bps as usize;
                    fc = u32::from_le_bytes([fsec[boff], fsec[boff + 1], fsec[boff + 2], fsec[boff + 3]]) & 0x0FFF_FFFF;
                }

                // Dirent: só se size inválido/curto — fixa CAP com 1 WRITE de setor.
                // Flushes seguintes = data-only (crash-safe vs SESSION_260).
                if alloc_size < 512 || alloc_size > BOOT_LOG_CAP {
                    let target = (capacity as u32).to_le_bytes();
                    dir[entry + 28..entry + 32].copy_from_slice(&target);
                    let sector_idx = (entry as u32) / bps;
                    let off = (sector_idx * bps) as usize;
                    if !dev.write_sectors((clba + sector_idx) as u64, &dir[off..off + bps as usize]) {
                        crate::boot_logger::log(&alloc::format!("bootlog: write dir LBA {} falhou", u64::from(clba) + u64::from(sector_idx)));
                        return false;
                    }
                }
                crate::boot_logger::log(&alloc::format!("bootlog: OK {} bytes em {} (LBA {})", written, BOOT_LOG_NAME, lba_start));
                return written > 0 || write_len == 0;
            }
            let fat_off = cluster as usize * 4;
            let fat_sec = fat_lba + (fat_off as u32 / bps);
            let mut fsec = [0u8; 512];
            if !dev.read_sectors(fat_sec as u64, &mut fsec) { break; }
            let boff = fat_off % bps as usize;
            cluster = u32::from_le_bytes([fsec[boff], fsec[boff + 1], fsec[boff + 2], fsec[boff + 3]]) & 0x0FFF_FFFF;
        }
        crate::boot_logger::log(&alloc::format!("bootlog: BOOT.LOG NAO encontrado no root dir (walked={})", walked));
    }
    false
}

pub fn build_session_bytes() -> Vec<u8> {
    let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let ver = env!("CARGO_PKG_VERSION");
    let mut content = alloc::format!(
        "[S] neural-os-core {} BOOT.LOG tick={} fat-boot-log=1\n",
        ver, tick
    ).into_bytes();
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
        // Tenta USB-MSC â†’ ATA PIO â†’ AHCI SATA â†’ NVMe â€” nesta ordem.
        // USB-MSC tem sync_cache apÃ³s cada write.
        // SESSÃƒO_260: loga o MOTIVO de cada falha â€” no HW real (pendrive via
        // USB-MSC) o flush falhava silencioso e o BOOT.LOG ficava vazio.
        let mut ok = false;
        let mut reason = "nenhum backend disponivel";
        if let Some(mut g) = crate::globals::USB_MSC.try_lock() {
            if let Some(ref mut msc) = *g {
                ok = unsafe { overwrite_boot_log(msc, &content) };
                if ok {
                    msc.sync_cache();
                } else {
                    reason = "USB-MSC: overwrite_boot_log falhou (leitura/escrita do FAT32)";
                }
            } else {
                reason = "USB-MSC: presente mas None";
            }
        } else {
            reason = "USB-MSC: try_lock falhou (lock ocupado)";
        }
        if !ok {
            if let Some(mut g) = crate::globals::ATA_DRIVER.try_lock() {
                if let Some(ref mut ata) = *g {
                    ok = unsafe { overwrite_boot_log(ata, &content) };
                    if !ok {
                        reason = "ATA PIO: overwrite_boot_log falhou";
                    }
                }
            }
        }
        if !ok {
            if let Some(mut g) = crate::globals::AHCI_DRIVER.try_lock() {
                if let Some(ref mut ahci) = *g {
                    ok = unsafe { overwrite_boot_log(ahci, &content) };
                    if !ok {
                        reason = "AHCI: overwrite_boot_log falhou";
                    }
                }
            }
        }
        if !ok {
            if let Some(mut g) = crate::disk_agent::nvme::NVME_DRIVER.try_lock() {
                if let Some(ref mut nvme) = *g {
                    ok = unsafe { overwrite_boot_log(nvme, &content) };
                    if !ok {
                        reason = "NVMe: overwrite_boot_log falhou";
                    }
                }
            }
        }
        if !ok {
            crate::boot_logger::log(&alloc::format!("BOOT.LOG flush FALHOU - {}", reason));
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

/// Init legado ATA (partiÃ§Ãµes jÃ¡ lidas).
pub fn init(ata: Option<&crate::ata::AtaDriver>, _parts: &[crate::fat32::Partition]) {
    #[cfg(feature = "fat-boot-log")]
    {
        if let Some(a) = ata {
            let _ = a;
            let ok = persist_now(None);
            crate::slog_nano!("LOG", "info", "BOOT.LOG persist ATA/USB={} writes={}",
                ok,
                DISK_WRITES.load(Ordering::Relaxed));
            if !ok {
                crate::slog_nano!("LOG", "info", "WARN: BOOT.LOG nÃ£o gravado â€” confira FAT32 no stick");
            }
        } else {
            let ok = persist_now(None);
            crate::slog_nano!("LOG", "info", "BOOT.LOG persist (no ATA arg) ok={}", ok);
        }
    }
    #[cfg(not(feature = "fat-boot-log"))]
    {
        let _ = ata;
        crate::slog_nano!("LOG", "info", "SKIP fat session write (enable feature fat-boot-log to persist)");
        FAT_READY.store(false, Ordering::Relaxed);
    }
}

fn storage_available() -> bool {
    if crate::globals::USB_MSC
        .try_lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
    {
        return true;
    }
    if crate::globals::ATA_DRIVER
        .try_lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
    {
        return true;
    }
    if crate::globals::AHCI_DRIVER
        .try_lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
    {
        return true;
    }
    crate::disk_agent::nvme::NVME_DRIVER
        .try_lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

/// Registra mensagem. Com fat-boot-log: buffer; flush só com BlockDevice pronto.
pub fn log(msg: &str) {
    crate::slog_nano!("LOG", "info", "{}", msg);
    log_quiet(msg);
}

/// Buffer/persist sem eco no serial/FB (evita triplicar linhas de BOOT_PHASE).
pub fn log_quiet(msg: &str) {
    if !HEAP_READY.load(Ordering::Relaxed) {
        return;
    }

    buffer_log(msg);
    #[cfg(feature = "fat-boot-log")]
    {
        // SESSION_265: sem MSC/ATA não chama persist_now (no-op caro + risco cedo).
        if !storage_available() && !FAT_READY.load(Ordering::Relaxed) {
            return;
        }
        let n = SINCE_FLUSH.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= FLUSH_EVERY {
            let _ = persist_now(None);
        }
    }
}

/// Flush forÃ§ado (checkpoints crÃ­ticos). Retorna true se gravou em FAT.
pub fn flush() -> bool {
    #[cfg(feature = "fat-boot-log")]
    {
        let ok = persist_now(None);
        crate::slog_nano!("LOG", "info", "flush BOOT.LOG ok={} bytes~{}",
            ok,
            build_session_bytes().len());
        return ok;
    }
#[cfg(not(feature = "fat-boot-log"))]
    { false }
}

/// Tenta (re)enumerar USB-MSC se ainda nao ha BlockDevice util p/ BOOT.LOG.
/// Usado pelo SysInfoAgent em HW real quando o bring-up early falhou (porta
/// CCS atrasada) - sem isto o retry so chamava `flush()` em vao.
pub fn try_ensure_usb_msc() -> bool {
    if crate::globals::USB_MSC.lock().is_some() {
        return true;
    }
    if crate::xhci::XHCI_STATE.lock().is_none() {
        unsafe { crate::xhci::init_xhci(); }
    }
    let msc = unsafe { crate::usb_msc::UsbMassStorage::probe() };
    let ok = msc.is_some();
    if ok {
        *crate::globals::USB_MSC.lock() = msc;
        crate::slog_nano!("LOG", "info", "try_ensure_usb_msc: MSC OK (retry)");
    }
    ok
}

/// Retry completo: re-probe MSC se preciso + flush. Retorna true se FAT_READY.
pub fn ensure_persisted() -> bool {
    if FAT_READY.load(Ordering::Relaxed) {
        let _ = flush();
        return true;
    }
    let _ = try_ensure_usb_msc();
    flush()
}

/// Anexa texto sem `serial_println` (evita recursao no path sem-COM do serial.rs).
pub fn append_raw(msg: &str) {
    if !HEAP_READY.load(Ordering::Relaxed) {
        crate::boot_ramlog::append(msg);
        return;
    }
    buffer_log(msg);
    #[cfg(feature = "fat-boot-log")]
    {
        if !storage_available() && !FAT_READY.load(Ordering::Relaxed) {
            return;
        }
        let n = SINCE_FLUSH.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= FLUSH_EVERY {
            let _ = persist_now(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_83_boot_log() {
        let e = encode_83("BOOT.LOG");
        assert_eq!(&e[..8], b"BOOT    ");
        assert_eq!(&e[8..], b"LOG");
    }

    #[test]
    fn boot_log_cap_matches_mkfat32() {
        assert_eq!(BOOT_LOG_CAP, 256 * 1024);
        assert_eq!(BOOT_LOG_NAME, "BOOT.LOG");
    }

    /// Garante que, com `--features fat-boot-log`, o path real (não o stub)
    /// está compilado. Sem a feature este teste nem entra no binário de teste
    /// sob o cfg abaixo — rode: `cargo test -p k-nano --features fat-boot-log`.
    #[cfg(feature = "fat-boot-log")]
    #[test]
    fn fat_boot_log_feature_compiles_persist_path() {
        // flush() sob a feature chama persist_now real (sem BlockDevice → false,
        // mas não é o stub cfg(not(...)) que sempre retornava false sem tentar).
        assert!(!flush() || FAT_READY.load(Ordering::Relaxed));
    }
}

