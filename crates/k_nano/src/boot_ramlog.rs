//! Boot RAM log — buffer físico espelhando checkpoints (FB/serial sem COM).
//!
//! Phys default `0x1000_0000` (256 MiB) — low mem (16–32 MiB) costuma ser
//! zerada no warm-reset de notebooks (ex.: Note 1050). CRC32 valida sobrevivência.
//!
//! Magic `NEURLOG!` = flush pendente (legado soft-reboot); `NEURDONE` = consumido.
//! Soft-reboot 0xCF9 foi **removido** (bughunt M4) — produto = seal + logwriter-efi.

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
            "ok",
            "BOOT.LOG do boot anterior gravado pelo logwriter (K{})",
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
            "warn",
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
        // Defesa: NEURDONE / magic estranho sem init_from_phys → zera (evita misturar sessões).
        if h.magic == MAGIC_FLUSHED
            || (h.magic != MAGIC_NEED_FLUSH && h.magic != 0)
        {
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

/// Finaliza CRC + magic `NEURLOG!` para o logwriter UEFI no próximo boot.
/// Não reinicia — o caller faz reboot ordenado (`shutdown` / panic).
///
/// Sem isto o stage-0 (`logwriter-efi`) vê magic 0 e não grava BOOT.LOG.
pub fn seal_for_next_boot() {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return;
    }
    unsafe {
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
            "ok",
            "seal_for_next_boot ckpt=K{} len={} crc={:#x} (logwriter)",
            ckpt,
            len,
            crc
        );
    }
}

/// Warm-reset (0x64/FE → 0xCF9) — sempre disponível p/ reboot ordenado / panic.
pub unsafe fn warm_reset() -> ! {
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

/// Soft-reboot BOOT.LOG removido (bughunt M4): produto = seal_for_next_boot +
/// orderly reboot + logwriter-efi. API antiga era stub `-> !` com spin infinito.
pub fn maybe_flush_reboot(reason: &str) {
    let _ = reason;
    mark_skip_flush_reboot();
    append("maybe_flush_reboot: soft-reboot removed — seal+logwriter path");
}

/// Ponytail: dump não-bloqueante do ramlog phys no FB/serial (K22/K137 hang).
/// Mantém em RAM quando pendrive não pronto; FB mostra foto para usuário.
pub fn dump() {
    for_each_line(40, |line| {
        crate::slog_nano!("RAMLOG", "dump", "{}", line);
    });
}

/// Emite linhas USB/hub/MSC do ramlog (foto no Alienware sem COM1).
/// Varre **todo** o buffer (não só as 1ªs 200 linhas — F3 false "probe nao chegou").
pub fn dump_usb_hint(mut emit: impl FnMut(&str)) {
    let mut n = 0usize;
    for_each_line(usize::MAX, |line| {
        if n >= 32 {
            return;
        }
        let l = line;
        let lower_ok = l.contains("USB")
            || l.contains("usb")
            || l.contains("hub")
            || l.contains("MSC")
            || l.contains("msc")
            || l.contains("xhci")
            || l.contains("XHCI")
            || l.contains("BOOT.LOG")
            || l.contains("CCS")
            || l.contains("PORTSC");
        if lower_ok {
            emit(l);
            n += 1;
        }
    });
    if n == 0 {
        emit("USB: ramlog sem linhas hub/MSC (slog pode ser serial-only)");
    }
}

fn for_each_line(max: usize, mut emit: impl FnMut(&str)) {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return;
    }
    unsafe {
        let h = &*hdr_mut();
        let len = (h.len as usize).min(data_cap());
        if len == 0 {
            return;
        }
        let slice = core::slice::from_raw_parts(data_ptr(), len);
        if let Ok(s) = core::str::from_utf8(slice) {
            for line in s.lines().take(max) {
                emit(line);
            }
        }
    }
}

/// Snapshot do conteúdo como String (leitura pública p/ SGDB ingest — IDEA #539c).
/// None se PHYS_MEM_OFFSET ainda 0, buffer vazio ou payload não-UTF8.
pub fn snapshot() -> Option<alloc::string::String> {
    if crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed) == 0 {
        return None;
    }
    unsafe {
        let h = &*hdr_mut();
        let len = (h.len as usize).min(data_cap());
        if len == 0 {
            return None;
        }
        let slice = core::slice::from_raw_parts(data_ptr(), len);
        core::str::from_utf8(slice).ok().map(alloc::string::String::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_24_stable_contract_for_logwriter() {
        // Contrato com crates/logwriter-efi — NÃO mudar sem atualizar o EFI.
        assert_eq!(crc32_24(b""), 0);
        assert_eq!(crc32_24(b"NEURLOG!"), 0x00c3_b463);
        assert_eq!(MAGIC_NEED_FLUSH, u64::from_le_bytes(*b"NEURLOG!"));
        assert_eq!(MAGIC_FLUSHED, u64::from_le_bytes(*b"NEURDONE"));
        assert_eq!(BOOT_RAMLOG_PHYS, 0x1000_0000);
        assert_eq!(BOOT_RAMLOG_CAP, 256 * 1024);
    }
}
