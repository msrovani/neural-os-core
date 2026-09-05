//! Boot Logger — buffer RAM → flush FAT32 / VFS.
//!
//! - **DEV/TEST:** arquivo fixo `BOOT.LOG` 8.3 (feature `fat-boot-log`, Live/Install).
//! - **Produto (Installed):** padrão com **timestamp** → `/logs/boot_<tick7hex>.log`
//!   (ADR-0086 telemetria `neural-<stamp>.log` no server).
//!
//! Self-heal SESSION_269: na 1ª falha do canal DEV — diagnose, skip, HEALTH_ISSUE, backoff.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use spin::Mutex;

use crate::block_dev::BlockDevice;

/// Nome 8.3 fixo — **somente DEV/TEST** (Live stick / QEMU / early bring-up).
/// Produto (BootMode::Installed) usa `timestamped_session_name()` → `/logs/boot_<tick>.log`.
pub const BOOT_LOG_NAME: &str = "BOOT.LOG";
/// Capacidade do arquivo pré-alocado (mkfat32) — só canal DEV.
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
/// Mensagens desde o último flush bem-sucedido (USB MSC é lento: não reescreve a cada linha).
static SINCE_FLUSH: AtomicUsize = AtomicUsize::new(0);
const FLUSH_EVERY: usize = 16;

/// Circuit breaker / self-heal (SESSION_269).
/// Bits: 1=USB 2=ATA 4=AHCI 8=NVMe — backend sem BOOT.LOG ou inadequado.
static BACKEND_SKIP: AtomicU8 = AtomicU8::new(0);
const SKIP_USB: u8 = 1;
const SKIP_ATA: u8 = 2;
const SKIP_AHCI: u8 = 4;
const SKIP_NVME: u8 = 8;
/// Próximo tick em que persist pode tentar de novo após falha.
static NEXT_RETRY_TICK: AtomicU64 = AtomicU64::new(0);
/// Falhas consecutivas → backoff 50→3200 (padrão mesh).
static FAIL_STREAK: AtomicUsize = AtomicUsize::new(0);
/// Já publicou HEALTH_ISSUE + diagnóstico nesta sessão de falha.
static HEAL_FIRED: AtomicBool = AtomicBool::new(false);

/// Resultado tipado — self-heal distingue arquivo ausente (skip) de I/O (backoff).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverwriteResult {
    Ok,
    NoFatParts,
    BootLogMissing,
    IoFail,
}

impl OverwriteResult {
    fn is_ok(self) -> bool {
        matches!(self, OverwriteResult::Ok)
    }
    fn unsuitable(self) -> bool {
        matches!(
            self,
            OverwriteResult::NoFatParts | OverwriteResult::BootLogMissing
        )
    }
    fn as_reason(self) -> &'static str {
        match self {
            OverwriteResult::Ok => "ok",
            OverwriteResult::NoFatParts => "sem particao FAT32 util",
            OverwriteResult::BootLogMissing => "BOOT.LOG ausente no root",
            OverwriteResult::IoFail => "I/O FAT read/write falhou",
        }
    }
}

/// Backoff exponencial 50 → 100 → … → 3200 ticks (espelha mesh probe_node).
pub fn bootlog_backoff_ticks(streak: usize) -> u64 {
    let shift = streak.saturating_sub(1).min(6) as u32;
    50u64.saturating_mul(1u64 << shift).min(3200)
}

fn now_tick() -> u64 {
    crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64
}

fn persist_allowed_now() -> bool {
    now_tick() >= NEXT_RETRY_TICK.load(Ordering::Relaxed)
}

fn schedule_backoff() {
    let streak = FAIL_STREAK.fetch_add(1, Ordering::Relaxed) + 1;
    let delay = bootlog_backoff_ticks(streak);
    let next = now_tick().saturating_add(delay);
    NEXT_RETRY_TICK.store(next, Ordering::Relaxed);
    SINCE_FLUSH.store(0, Ordering::Relaxed);
}

fn clear_breaker_on_success() {
    FAIL_STREAK.store(0, Ordering::Relaxed);
    NEXT_RETRY_TICK.store(0, Ordering::Relaxed);
    HEAL_FIRED.store(false, Ordering::Relaxed);
}

fn publish_bootlog_health(reason: &str) {
    let payload = alloc::format!("HEALTH_ISSUE:I5:boot_log:{}", reason);
    let _ = crate::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from("HEALTH_ISSUE"),
        payload: payload.into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

/// `BOOT.LOG` fixo = canal **DEV/TEST** (Live/Install/early).
/// Residente (`Installed`) usa log com timestamp — padrão ADR-0086 / LogFs.
pub fn fixed_boot_log_dev_only() -> bool {
    #[cfg(not(feature = "fat-boot-log"))]
    {
        return false;
    }
    #[cfg(feature = "fat-boot-log")]
    {
        match crate::boot_mode::peek() {
            Some(crate::boot_mode::BootMode::Installed) => false,
            // Live, Install, ou ainda Unknown (early stick antes do probe) → DEV OK.
            _ => true,
        }
    }
}

/// Nome de sessão com timestamp (tick hex) — padrão on-device.
/// Espelha `BootLogAgent` (`boot_{:07X}.log`) e o server `neural-<stamp>-<seq>.log`.
pub fn timestamped_session_name() -> String {
    let tick = now_tick();
    alloc::format!("boot_{:07X}.log", (tick as u32) & 0x0FFF_FFFF)
}

fn session_name_for_persist() -> String {
    let mut g = SESSION_FILENAME.lock();
    if let Some(ref n) = *g {
        // Early DEV gravou BOOT.LOG; depois Installed → migra p/ timestamp.
        if n.as_str() == BOOT_LOG_NAME && !fixed_boot_log_dev_only() {
            let neu = timestamped_session_name();
            *g = Some(neu.clone());
            return neu;
        }
        return n.clone();
    }
    let n = if fixed_boot_log_dev_only() {
        String::from(BOOT_LOG_NAME)
    } else {
        timestamped_session_name()
    };
    *g = Some(n.clone());
    n
}

/// Persistência produto: `/logs/<boot_TICK.log>` (timestamp), sem tocar BOOT.LOG.
fn persist_timestamped_vfs(content: &[u8]) -> bool {
    let name = session_name_for_persist();
    let path = alloc::format!("/logs/{}", name);
    match crate::fs::write_vfs(&path, content) {
        Ok(()) => {
            log_no_flush(&alloc::format!(
                "bootlog: OK {} bytes em {} (timestamped; BOOT.LOG=DEV-only)",
                content.len(),
                path
            ));
            true
        }
        Err(e) => {
            log_no_flush(&alloc::format!("bootlog: VFS {} falhou: {}", path, e));
            false
        }
    }
}

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

/// Sobrescreve payload de `BOOT.LOG` pré-alocado via BlockDevice (USB ou ATA).
///
/// SESSION_260: dir rasgado → data-only após 1º dirent; logs de diagnóstico.
/// SESSION_269: retorna `OverwriteResult` tipado p/ self-heal (skip vs backoff).
unsafe fn overwrite_boot_log(dev: &mut dyn BlockDevice, data: &[u8]) -> OverwriteResult {
    let want = encode_83(BOOT_LOG_NAME);
    let mut parts = fat32_parts(dev);
    if parts.is_empty() {
        return OverwriteResult::NoFatParts;
    }
    // Prefere volume de dados (0x0C/0x0B) sobre ESP (0xEF) — BOOT.LOG vive no NEURAL-OS.
    parts.sort_by_key(|p| match p.type_code {
        0x0C | 0x0B | 0x1C => 0u8,
        0x73 => 1u8,
        0xEF => 2u8,
        _ => 3u8,
    });
    let mut saw_fat = false;
    let mut saw_missing = false;
    let mut saw_io = false;
    for part in &parts {
        // 0xEF = ESP GPT (Fat32Reader aceita); BOOT.LOG costuma estar em 0x0C.
        if !matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0x73 | 0xEF) {
            continue;
        }
        saw_fat = true;
        let lba_start = part.lba_start as u64;
        let mut bpb = [0u8; 512];
        if !dev.read_sectors(lba_start, &mut bpb) {
            saw_io = true;
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
        if root_entries > 0 || bps < 512 || bps > 4096 || bps % 32 != 0 || spc == 0 {
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
                    return OverwriteResult::IoFail;
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
                        if written >= write_len {
                            break;
                        }
                        let mut sector = [0u8; 512];
                        let take = (write_len - written).min(512);
                        sector[..take].copy_from_slice(&data[written..written + take]);
                        if !dev.write_sectors((fc_lba + s) as u64, &sector) {
                            return OverwriteResult::IoFail;
                        }
                        written += take;
                    }
                    let fat_off = fc as usize * 4;
                    let fat_sec = fat_lba + (fat_off as u32 / bps);
                    let mut fsec = [0u8; 512];
                    if !dev.read_sectors(fat_sec as u64, &mut fsec) {
                        return OverwriteResult::IoFail;
                    }
                    let boff = fat_off % bps as usize;
                    fc = u32::from_le_bytes([
                        fsec[boff],
                        fsec[boff + 1],
                        fsec[boff + 2],
                        fsec[boff + 3],
                    ]) & 0x0FFF_FFFF;
                }

                if alloc_size < 512 || alloc_size > BOOT_LOG_CAP {
                    let target = (capacity as u32).to_le_bytes();
                    dir[entry + 28..entry + 32].copy_from_slice(&target);
                    let sector_idx = (entry as u32) / bps;
                    let off = (sector_idx * bps) as usize;
                    if !dev.write_sectors(
                        (clba + sector_idx) as u64,
                        &dir[off..off + bps as usize],
                    ) {
                        return OverwriteResult::IoFail;
                    }
                }
                log_no_flush(&alloc::format!(
                    "bootlog: OK {} bytes em {} (LBA {})",
                    written,
                    BOOT_LOG_NAME,
                    lba_start
                ));
                if written > 0 || write_len == 0 {
                    return OverwriteResult::Ok;
                }
                return OverwriteResult::IoFail;
            }
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
        // Chegou ao fim do root sem achar BOOT.LOG nesta partição.
        saw_missing = true;
    }
    if !saw_fat {
        OverwriteResult::NoFatParts
    } else if saw_io {
        OverwriteResult::IoFail
    } else if saw_missing {
        OverwriteResult::BootLogMissing
    } else {
        OverwriteResult::NoFatParts
    }
}

pub fn build_session_bytes() -> Vec<u8> {
    let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let ver = env!("CARGO_PKG_VERSION");
    let session = session_name_for_persist();
    let channel = if fixed_boot_log_dev_only() {
        "dev"
    } else {
        "timestamped"
    };
    // BOM UTF-8: Notepad Windows (ANSI/GBK) sem BOM mostra mojibake/"chinês".
    let mut content: alloc::vec::Vec<u8> = alloc::vec![0xEF, 0xBB, 0xBF];
    content.extend_from_slice(
        alloc::format!(
            "[S] neural-os-core {} session={} channel={} tick={} fat-boot-log=1\n",
            ver, session, channel, tick
        )
        .as_bytes(),
    );
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

/// Self-heal na 1ª falha: re-probe USB-MSC + limpa skip USB se MSC voltar.
fn heal_on_first_failure(detail: &str) {
    if HEAL_FIRED.swap(true, Ordering::Relaxed) {
        return;
    }
    log_no_flush(&alloc::format!(
        "BOOT.LOG self-heal 1a falha: {} — re-probe MSC + skip backends sem arquivo",
        detail
    ));
    publish_bootlog_health(detail);
    // Live USB sem MSC: NÃO re-init xHCI / probe MSC (EnableSlot timeout = hang
    // em BOOT: self-tests via boot_ckpt→try_flush→heal). SESSION_309 HW.
    let no_msc = crate::globals::USB_MSC
        .try_lock()
        .map(|g| g.is_none())
        .unwrap_or(true);
    if internal_disk_skipped() && no_msc {
        log_no_flush("BOOT.LOG self-heal SKIP (live USB sem MSC — sem re-probe xHCI)");
        return;
    }
    let msc_ok = try_ensure_usb_msc();
    if msc_ok {
        BACKEND_SKIP.fetch_and(!SKIP_USB, Ordering::Relaxed);
        log_no_flush("BOOT.LOG self-heal: USB-MSC re-probe OK");
    } else {
        log_no_flush("BOOT.LOG self-heal: USB-MSC ainda ausente");
    }
}

fn mark_skip(bit: u8) {
    BACKEND_SKIP.fetch_or(bit, Ordering::Relaxed);
}

/// Pendrive Limine sem MSC: não tentar `BOOT.LOG` em ATA/AHCI/NVMe internos.
/// `overwrite_boot_log` = FAT walk PIO no HD errado → hang minutos (SESSION_296+).
pub fn skip_disk_persist_except_usb() {
    mark_skip(SKIP_ATA | SKIP_AHCI | SKIP_NVME);
    #[cfg(feature = "fat-boot-log")]
    {
        crate::slog_nano!("LOG", "info", "persist USB-only (live stick sem MSC)");
    }
}

/// Live USB sem MSC: bloqueia I/O em discos internos (BOOT.LOG, NSGDB, modelos).
pub fn internal_disk_skipped() -> bool {
    let skip = BACKEND_SKIP.load(Ordering::Relaxed);
    skip & (SKIP_ATA | SKIP_AHCI | SKIP_NVME) != 0
}

fn is_skipped(bit: u8) -> bool {
    BACKEND_SKIP.load(Ordering::Relaxed) & bit != 0
}

#[cfg(feature = "fat-boot-log")]
fn persist_now(dev: Option<&mut dyn BlockDevice>) -> bool {
    if !persist_allowed_now() {
        return false;
    }
    let content = build_session_bytes();

    // Produto (Installed): NÃO usa BOOT.LOG fixo — padrão com timestamp em /logs/.
    if !fixed_boot_log_dev_only() {
        let ok = persist_timestamped_vfs(&content);
        if ok {
            DISK_WRITES.fetch_add(1, Ordering::Relaxed);
            FAT_READY.store(true, Ordering::Relaxed);
            SINCE_FLUSH.store(0, Ordering::Relaxed);
            clear_breaker_on_success();
        } else {
            let first = !HEAL_FIRED.load(Ordering::Relaxed);
            if first {
                HEAL_FIRED.store(true, Ordering::Relaxed);
                publish_bootlog_health("timestamped_vfs_fail");
                log_no_flush(
                    "BOOT.LOG skipped (Installed=produto); persist timestamped /logs/ falhou (backoff)",
                );
            }
            schedule_backoff();
        }
        return ok;
    }

    let ok = if let Some(d) = dev {
        unsafe { overwrite_boot_log(d, &content) }.is_ok()
    } else {
        let skip = BACKEND_SKIP.load(Ordering::Relaxed);
        let mut ok = false;
        let mut last = OverwriteResult::NoFatParts;
        let mut last_name = "nenhum";
        let mut any_tried = false;

        if skip & SKIP_USB == 0 {
            if let Some(mut g) = crate::globals::USB_MSC.try_lock() {
                if let Some(ref mut msc) = *g {
                    any_tried = true;
                    last_name = "USB-MSC";
                    last = unsafe { overwrite_boot_log(msc, &content) };
                    if last.is_ok() {
                        msc.sync_cache();
                        ok = true;
                    } else if last.unsuitable() {
                        mark_skip(SKIP_USB);
                    }
                }
            }
        }

        // QEMU gate: virtio-blk data disk (disk_qemu.raw) antes de ATA IDE (uefi.img).
        if !ok {
            if let Some(mut g) = crate::virtio_blk::VIRTIO_BLK_DEV.try_lock() {
                if let Some(ref mut vb) = *g {
                    any_tried = true;
                    last_name = "virtio-blk";
                    last = unsafe { overwrite_boot_log(vb, &content) };
                    if last.is_ok() {
                        vb.sync_cache();
                        ok = true;
                    }
                }
            }
        }

        if !ok
            && skip & SKIP_ATA == 0
            && !crate::storage_bw::skip_measure()
            && crate::boot_bind::storage_includes(crate::boot_bind::StorageKind::Ata)
        {
            if let Some(mut g) = crate::globals::ATA_DRIVER.try_lock() {
                if let Some(ref mut ata) = *g {
                    any_tried = true;
                    last_name = "ATA-PIO";
                    last = unsafe { overwrite_boot_log(ata, &content) };
                    if last.is_ok() {
                        ok = true;
                    } else if last.unsuitable() {
                        // Disco ATA sem BOOT.LOG (HD interno) → nunca martelar de novo.
                        mark_skip(SKIP_ATA);
                    }
                }
            }
        }

        if !ok && skip & SKIP_AHCI == 0 {
            if let Some(mut g) = crate::globals::AHCI_DRIVER.try_lock() {
                if let Some(ref mut ahci) = *g {
                    any_tried = true;
                    last_name = "AHCI";
                    last = unsafe { overwrite_boot_log(ahci, &content) };
                    if last.is_ok() {
                        ok = true;
                    } else if last.unsuitable() {
                        mark_skip(SKIP_AHCI);
                    }
                }
            }
        }

        if !ok && skip & SKIP_NVME == 0 {
            if let Some(mut g) = crate::disk_agent::nvme::NVME_DRIVER.try_lock() {
                if let Some(ref mut nvme) = *g {
                    any_tried = true;
                    last_name = "NVMe";
                    last = unsafe { overwrite_boot_log(nvme, &content) };
                    if last.is_ok() {
                        ok = true;
                    } else if last.unsuitable() {
                        mark_skip(SKIP_NVME);
                    }
                }
            }
        }

        if !ok {
            let detail = if !any_tried {
                alloc::format!(
                    "nenhum backend tentavel (skip=0x{:02x} usb={} ata={} ahci={})",
                    skip,
                    crate::globals::USB_MSC.try_lock().map(|g| g.is_some()).unwrap_or(false),
                    crate::globals::ATA_DRIVER.try_lock().map(|g| g.is_some()).unwrap_or(false),
                    crate::globals::AHCI_DRIVER.try_lock().map(|g| g.is_some()).unwrap_or(false),
                )
            } else {
                alloc::format!("{}: {}", last_name, last.as_reason())
            };
            // Capturar ANTES do heal (heal seta HEAL_FIRED).
            let first = !HEAL_FIRED.load(Ordering::Relaxed);
            heal_on_first_failure(&detail);
            if first {
                // Retry imediato pós-heal (MSC pode ter voltado).
                if !is_skipped(SKIP_USB) {
                    if let Some(mut g) = crate::globals::USB_MSC.try_lock() {
                        if let Some(ref mut msc) = *g {
                            let r = unsafe { overwrite_boot_log(msc, &content) };
                            if r.is_ok() {
                                msc.sync_cache();
                                ok = true;
                            }
                        }
                    }
                }
            }
            if !ok {
                schedule_backoff();
                if first {
                    log_no_flush(&alloc::format!(
                        "BOOT.LOG flush FALHOU - {} (backoff {} ticks; sem spam)",
                        detail,
                        bootlog_backoff_ticks(FAIL_STREAK.load(Ordering::Relaxed))
                    ));
                }
            }
        }
        ok
    };
    if ok {
        DISK_WRITES.fetch_add(1, Ordering::Relaxed);
        FAT_READY.store(true, Ordering::Relaxed);
        SINCE_FLUSH.store(0, Ordering::Relaxed);
        clear_breaker_on_success();
        let _ = session_name_for_persist(); // grava BOOT.LOG ou timestamp no SESSION_FILENAME
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
    // Live USB: SKIP_ATA/AHCI/NVME — driver global pode existir, mas NÃO é
    // backend de BOOT.LOG (FAT walk no HD interno = hang).
    let skip = BACKEND_SKIP.load(Ordering::Relaxed);
    if skip & SKIP_ATA == 0
        && crate::globals::ATA_DRIVER
            .try_lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    {
        return true;
    }
    if skip & SKIP_AHCI == 0
        && crate::globals::AHCI_DRIVER
            .try_lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    {
        return true;
    }
    if skip & SKIP_NVME == 0
        && crate::disk_agent::nvme::NVME_DRIVER
            .try_lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    {
        return true;
    }
    false
}

/// Registra mensagem. Com fat-boot-log: buffer; flush só com BlockDevice pronto.
pub fn log(msg: &str) {
    crate::slog_nano!("LOG", "info", "{}", msg);
    log_quiet(msg);
}

/// Diagnóstico do path de persistência: serial + ramlog, SEM re-entrar no flush.
/// `log()` aqui causaria recursão infinita (persist_now → log → log_quiet →
/// SINCE_FLUSH≥16 → persist_now → ...) até stack overflow (#PF) quando o flush
/// falha (ex: QEMU sem USB-MSC/ATA). SESSION_265.
fn log_no_flush(msg: &str) {
    crate::slog_nano!("LOG", "info", "{}", msg);
    buffer_log(msg);
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
            // SESSION_269: em backoff não martela persist (nem loga de novo).
            if !persist_allowed_now() {
                SINCE_FLUSH.store(FLUSH_EVERY, Ordering::Relaxed);
                return;
            }
            let _ = persist_now(None);
        }
    }
}

/// Flush forÃ§ado (checkpoints crÃ­ticos). Retorna true se gravou em FAT.
pub fn flush() -> bool {
    #[cfg(feature = "fat-boot-log")]
    {
        let ok = persist_now(None);
        crate::slog_nano!("LOG", "ok", "flush BOOT.LOG ok={} bytes~{}",
            ok,
            build_session_bytes().len());
        return ok;
    }
#[cfg(not(feature = "fat-boot-log"))]
    { false }
}

/// Ponytail: flush oportunista não-bloqueante para pendrive em K22/K137.
///
/// Chamado após cada `boot_ckpt` crítico em `main.rs` e nos shims `display::fb`.
/// - `try_lock` em todos os backends (USB-MSC/ATA/AHCI/NVMe) → nunca hang.
/// - Respeita `HEAP_READY`, `PHYS_MEM_OFFSET` e backoff `NEXT_RETRY_TICK`.
/// - Fallback ATA automático quando USB ausente (storage_available).
/// - Se pendrive não pronto, só mantém ramlog em RAM (exposto via FB dump).
pub fn try_flush_ramlog() -> bool {
    if !HEAP_READY.load(Ordering::Relaxed) {
        return false;
    }
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return false;
    }
    // Live USB sem MSC: nada a gravar — não dispara flush/heal (hang xHCI).
    if internal_disk_skipped()
        && crate::globals::USB_MSC
            .try_lock()
            .map(|g| g.is_none())
            .unwrap_or(true)
    {
        return false;
    }
    #[cfg(not(feature = "fat-boot-log"))]
    {
        return false;
    }
    #[cfg(feature = "fat-boot-log")]
    {
        if !storage_available() && !FAT_READY.load(Ordering::Relaxed) {
            return false;
        }
        if !persist_allowed_now() {
            return false;
        }
        if SINCE_FLUSH.load(Ordering::Relaxed) == 0 && FAT_READY.load(Ordering::Relaxed) {
            return false;
        }
        flush()
    }
}

/// Tenta (re)enumerar USB-MSC se ainda nao ha BlockDevice util p/ BOOT.LOG.
/// Usado pelo SysInfoAgent / self-heal quando o bring-up early falhou.
/// Host/test: nunca toca xHCI (SEGv) — só reporta se o static já está populado.
pub fn try_ensure_usb_msc() -> bool {
    if crate::globals::USB_MSC.lock().is_some() {
        return true;
    }
    #[cfg(not(target_os = "none"))]
    {
        return false;
    }
    #[cfg(target_os = "none")]
    {
        if crate::xhci::XHCI_STATE.lock().is_none() {
            unsafe {
                crate::xhci::init_xhci();
            }
        }
        let msc = unsafe { crate::usb_msc::UsbMassStorage::probe() };
        let ok = msc.is_some();
        if ok {
            *crate::globals::USB_MSC.lock() = msc;
            crate::slog_nano!("LOG", "info", "try_ensure_usb_msc: MSC OK (retry)");
        }
        ok
    }
}

/// Retry completo: re-probe MSC se preciso + flush. Respeita backoff (SESSION_269).
/// Retorna true se FAT_READY.
pub fn ensure_persisted() -> bool {
    if FAT_READY.load(Ordering::Relaxed) {
        if persist_allowed_now() {
            let _ = flush();
        }
        return true;
    }
    if !persist_allowed_now() {
        return false;
    }
    // QEMU/TCG: virtio-blk data disk basta — re-probe MSC (xHCI) trava minutos.
    let skip_msc_retry = crate::storage_bw::skip_measure()
        && crate::virtio_blk::VIRTIO_BLK_DEV.lock().is_some();
    if !skip_msc_retry {
        let has_msc = crate::globals::USB_MSC
            .try_lock()
            .map(|g| g.is_some())
            .unwrap_or(false);
        // Live USB sem MSC: multi-porta xHCI é caro — no máx. 1 probe / ~200 ticks
        // (~11s @18Hz). Evita freeze no desktop (Bugbot / SESSION_310 HW).
        const MSC_PROBE_MIN_TICKS: u64 = 200;
        static LAST_MSC_PROBE_TICK: AtomicU64 = AtomicU64::new(0);
        let now = now_tick();
        let last = LAST_MSC_PROBE_TICK.load(Ordering::Relaxed);
        let due = has_msc
            || last == 0
            || now.saturating_sub(last) >= MSC_PROBE_MIN_TICKS;
        if due {
            LAST_MSC_PROBE_TICK.store(now, Ordering::Relaxed);
            let msc = try_ensure_usb_msc();
            if msc {
                BACKEND_SKIP.fetch_and(!SKIP_USB, Ordering::Relaxed);
                let _ = crate::storage::remount_after_usb_msc();
            }
        }
    } else {
        crate::slog_nano!("LOG", "ok", "ensure_persisted: skip MSC retry (TCG+virtio-blk)");
    }
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
            if !persist_allowed_now() {
                SINCE_FLUSH.store(FLUSH_EVERY, Ordering::Relaxed);
                return;
            }
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

    #[test]
    fn skip_disk_persist_blocks_internal_io() {
        let prev = BACKEND_SKIP.load(Ordering::Relaxed);
        skip_disk_persist_except_usb();
        assert!(internal_disk_skipped());
        BACKEND_SKIP.store(prev, Ordering::Relaxed);
    }

    #[test]
    fn bootlog_backoff_grows_to_cap() {
        assert_eq!(bootlog_backoff_ticks(1), 50);
        assert_eq!(bootlog_backoff_ticks(2), 100);
        assert_eq!(bootlog_backoff_ticks(3), 200);
        assert_eq!(bootlog_backoff_ticks(7), 3200);
        assert_eq!(bootlog_backoff_ticks(99), 3200);
    }

    #[test]
    fn overwrite_result_classifies_unsuitable() {
        assert!(OverwriteResult::BootLogMissing.unsuitable());
        assert!(OverwriteResult::NoFatParts.unsuitable());
        assert!(!OverwriteResult::IoFail.unsuitable());
        assert!(!OverwriteResult::Ok.unsuitable());
        assert_eq!(OverwriteResult::BootLogMissing.as_reason(), "BOOT.LOG ausente no root");
    }

    #[test]
    fn timestamped_session_name_has_boot_prefix() {
        let n = timestamped_session_name();
        assert!(n.starts_with("boot_"), "{}", n);
        assert!(n.ends_with(".log"), "{}", n);
    }

    #[test]
    fn fixed_boot_log_denied_when_installed() {
        crate::boot_mode::set_boot_mode(crate::boot_mode::BootMode::Installed);
        #[cfg(feature = "fat-boot-log")]
        {
            assert!(!fixed_boot_log_dev_only());
        }
        crate::boot_mode::set_boot_mode(crate::boot_mode::BootMode::Live);
        #[cfg(feature = "fat-boot-log")]
        {
            assert!(fixed_boot_log_dev_only());
        }
        crate::boot_mode::set_boot_mode(crate::boot_mode::BootMode::Unknown);
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

    /// SESSION_269: após falha simulada via NEXT_RETRY_TICK futuro,
    /// ensure_persisted não tenta de novo (circuit breaker).
    #[cfg(feature = "fat-boot-log")]
    #[test]
    fn ensure_persisted_respects_backoff() {
        FAT_READY.store(false, Ordering::Relaxed);
        NEXT_RETRY_TICK.store(u64::MAX, Ordering::Relaxed);
        assert!(!ensure_persisted());
        NEXT_RETRY_TICK.store(0, Ordering::Relaxed);
    }
}

