//! Boot RAM log — buffer físico espelhando checkpoints (FB/serial sem COM).
//!
//! Phys default `0x1000_0000` (256 MiB) — low mem (16–32 MiB) costuma ser
//! zerada no warm-reset de notebooks (ex.: Note 1050). CRC32 valida sobrevivência.
//!
//! Magic `NEURLOG!` = flush pendente (legado soft-reboot); `NEURDONE` = consumido.
//! Soft-reboot 0xCF9 é **opt-in** (`feature = "soft-reboot-bootlog"`) — default OFF
//! porque nenhum UEFI writer gravava `NEURDONE` → loop infinito em HW real.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// 256 MiB — acima da zona que firmware costuma limpar no reset.
pub const BOOT_RAMLOG_PHYS: u64 = 0x1000_0000;
pub const BOOT_RAMLOG_CAP: usize = 256 * 1024;
const HDR_SIZE: usize = 16;

pub const MAGIC_NEED_FLUSH: u64 = u64::from_le_bytes(*b"NEURLOG!");
pub const MAGIC_FLUSHED: u64 = u64::from_le_bytes(*b"NEURDONE");

static SKIP_FLUSH_REBOOT: AtomicBool = AtomicBool::new(false);
static INITED: AtomicBool = AtomicBool::new(false);
static LAST_CKPT: AtomicU8 = AtomicU8::new(0);

#[repr(C)]
pub struct BootRamLogHeader {
    pub magic: u64,
    pub len: u32,
    /// CRC32 IEEE dos `len` bytes de payload (ou 0 se vazio).
    pub crc_and_ckpt: u32,
}

#[inline]
fn va() -> u64 {
    crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) + BOOT_RAMLOG_PHYS
}

unsafe fn hdr_mut() -> *mut BootRamLogHeader {
    va() as *mut BootRamLogHeader
}

unsafe fn data_ptr() -> *mut u8 {
    (va() + HDR_SIZE as u64) as *mut u8
}

fn data_cap() -> usize {
    BOOT_RAMLOG_CAP.saturating_sub(HDR_SIZE)
}

fn pack_crc_ckpt(crc: u32, ckpt: u8) -> u32 {
    (crc & 0x00FF_FFFF) | ((ckpt as u32) << 24)
}

fn unpack_ckpt(v: u32) -> u8 {
    (v >> 24) as u8
}

/// CRC32 IEEE (poly 0xEDB88320), 24 bits baixos bastam p/ validar.
pub fn crc32_24(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (!(crc & 1)).wrapping_add(1); // 0 or 0xFFFFFFFF
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    (!crc) & 0x00FF_FFFF
}

pub fn set_last_ckpt(n: u8) {
    LAST_CKPT.store(n, Ordering::Relaxed);
}

pub fn last_ckpt() -> u8 {
    LAST_CKPT.load(Ordering::Relaxed)
}

/// Após map_phys: consome magic legado e **sempre** evita rearmar soft-reboot loop.
///
/// - `NEURDONE` → skip (flush consumido)
/// - `NEURLOG!` → tentativa incompleta do boot anterior (UEFI nunca escreveu DONE) → skip
/// - outro → zera buffer
pub unsafe fn init_from_phys() {
    if INITED.swap(true, Ordering::Relaxed) {
        return;
    }
    let h = &*hdr_mut();
    if h.magic == MAGIC_FLUSHED {
        SKIP_FLUSH_REBOOT.store(true, Ordering::Relaxed);
        let k = unpack_ckpt(h.crc_and_ckpt);
        if k != 0 {
            LAST_CKPT.store(k, Ordering::Relaxed);
        }
        core::ptr::write_volatile(&mut (*hdr_mut()).magic, 0);
        core::ptr::write_volatile(&mut (*hdr_mut()).len, 0);
        crate::slog_nano!(
            "RAMLOG",
            "info",
            "BOOT.LOG consumido (ckpt K{}) — skip soft-reboot",
            k
        );
    } else if h.magic == MAGIC_NEED_FLUSH {
        // Soft-reboot anterior deixou NEURLOG! sem UEFI writer → loop se não skiparmos.
        let k = unpack_ckpt(h.crc_and_ckpt);
        if k != 0 {
            LAST_CKPT.store(k, Ordering::Relaxed);
        }
        SKIP_FLUSH_REBOOT.store(true, Ordering::Relaxed);
        // Marca consumido localmente (não depende de UEFI fantasma).
        core::ptr::write_volatile(&mut (*hdr_mut()).magic, MAGIC_FLUSHED);
        crate::slog_nano!(
            "RAMLOG",
            "info",
            "NEURLOG! pendente (ckpt K{}) — skip soft-reboot; Runtime segue",
            k
        );
    } else {
        core::ptr::write_bytes(va() as *mut u8, 0, BOOT_RAMLOG_CAP);
    }
}

pub fn skip_flush_reboot() -> bool {
    SKIP_FLUSH_REBOOT.load(Ordering::Relaxed)
}

/// Marca esta sessão para não pedir soft-reboot de novo (continue boot).
pub fn mark_skip_flush_reboot() {
    SKIP_FLUSH_REBOOT.store(true, Ordering::Relaxed);
}

/// Anexa linha (sem alloc). No-op se PHYS_MEM_OFFSET ainda 0.
pub fn append(msg: &str) {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return;
    }
    unsafe {
        let h = &mut *hdr_mut();
        if h.magic != MAGIC_NEED_FLUSH && h.magic != 0 && h.magic != MAGIC_FLUSHED {
            core::ptr::write_bytes(va() as *mut u8, 0, BOOT_RAMLOG_CAP);
        }
        let mut len = h.len as usize;
        if len >= data_cap() {
            return;
        }
        let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        let mut line = [0u8; 320];
        let mut pos = 0usize;
        let pfx = b"[T+";
        for &b in pfx {
            line[pos] = b;
            pos += 1;
        }
        let mut t = tick;
        let mut tmp = [0u8; 20];
        let mut ti = 0usize;
        if t == 0 {
            tmp[0] = b'0';
            ti = 1;
        } else {
            while t > 0 && ti < 20 {
                tmp[ti] = b'0' + (t % 10) as u8;
                t /= 10;
                ti += 1;
            }
        }
        while ti > 0 {
            ti -= 1;
            line[pos] = tmp[ti];
            pos += 1;
        }
        line[pos] = b']';
        pos += 1;
        line[pos] = b' ';
        pos += 1;
        for &b in msg.as_bytes() {
            if pos >= line.len() - 1 {
                break;
            }
            line[pos] = b;
            pos += 1;
        }
        line[pos] = b'\n';
        pos += 1;
        let n = pos.min(data_cap() - len);
        core::ptr::copy_nonoverlapping(line.as_ptr(), data_ptr().add(len), n);
        len += n;
        core::ptr::write_volatile(&mut h.len, len as u32);
        let ckpt = LAST_CKPT.load(Ordering::Relaxed);
        // CRC parcial atualizado no flush final; aqui so guarda ckpt.
        core::ptr::write_volatile(&mut h.crc_and_ckpt, pack_crc_ckpt(0, ckpt));
        if h.magic != MAGIC_NEED_FLUSH {
            core::ptr::write_volatile(&mut h.magic, 0);
        }
    }
}

/// Finaliza CRC + magic e warm-reset — **somente** com `soft-reboot-bootlog`.
/// Builds de produto não ligam essa feature (evita loop HW).
#[cfg(feature = "soft-reboot-bootlog")]
pub unsafe fn request_flush_and_reboot(reason: &str) -> ! {
    append(reason);
    append("=== RAMLOG flush via soft-reboot UEFI ===");
    let h = &mut *hdr_mut();
    let len = h.len as usize;
    let slice = core::slice::from_raw_parts(data_ptr(), len.min(data_cap()));
    let crc = crc32_24(slice);
    let ckpt = LAST_CKPT.load(Ordering::Relaxed);
    core::ptr::write_volatile(&mut h.crc_and_ckpt, pack_crc_ckpt(crc, ckpt));
    core::ptr::write_volatile(&mut h.magic, MAGIC_NEED_FLUSH);
    core::arch::asm!("sfence", options(nostack, preserves_flags));
    crate::slog_nano!(
        "RAMLOG",
        "info",
        "soft-reboot flush BOOT.LOG ckpt=K{} len={} crc={:#x}",
        ckpt,
        len,
        crc
    );
    for _ in 0..2_000_000 {
        core::hint::spin_loop();
    }
    soft_reboot()
}

/// Stub produto: nunca reinicia; marca skip e gira (nao deve ser chamado).
#[cfg(not(feature = "soft-reboot-bootlog"))]
pub unsafe fn request_flush_and_reboot(reason: &str) -> ! {
    append(reason);
    append("=== soft-reboot DISABLED (product) — nao reinicia ===");
    mark_skip_flush_reboot();
    crate::slog_nano!(
        "RAMLOG",
        "warn",
        "request_flush_and_reboot chamado sem feature soft-reboot-bootlog — spin"
    );
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(feature = "soft-reboot-bootlog")]
unsafe fn soft_reboot() -> ! {
    for _ in 0..1000 {
        core::arch::asm!(
            "mov al, 0xFE",
            "out 0x64, al",
            options(nostack, nomem, preserves_flags)
        );
        core::hint::spin_loop();
    }
    core::arch::asm!(
        "mov al, 0x06",
        "mov dx, 0xCF9",
        "out dx, al",
        options(nostack, nomem, preserves_flags)
    );
    loop {
        core::hint::spin_loop();
    }
}

/// Soft-reboot opt-in. Sem feature: no-op (produto).
pub fn maybe_flush_reboot(reason: &str) {
    if SKIP_FLUSH_REBOOT.load(Ordering::Relaxed) {
        return;
    }
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return;
    }
    #[cfg(feature = "soft-reboot-bootlog")]
    {
        unsafe { request_flush_and_reboot(reason) }
    }
    #[cfg(not(feature = "soft-reboot-bootlog"))]
    {
        let _ = reason;
        mark_skip_flush_reboot();
        append("maybe_flush_reboot: soft-reboot OFF — continue");
    }
}
