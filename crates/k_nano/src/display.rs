//! k_nano display shim — boot_ckpt observability without jarbas dependency.
//! Mirrors `jarbas::display::fb::boot_ckpt` semantics (K*: FB console + ramlog)
//! but in Ring0 the FB may not be probed yet, so we log to slog + ramlog.
//! Keeps SMP observable on Core 7 240H hybrid when display crate not yet wired.

pub mod fb {
    /// Checkpoint K<n>: slog + ramlog (FB console is no-op if not yet probed).
    /// Ponytail: oportunista flush para pendrive (USB-MSC ou ATA fallback) sem hang.
    pub fn boot_ckpt(n: u8, msg: &str) {
        boot_ckpt_noflush(n, msg);
        // Ponytail: tenta pendrive se já houver backend (USB-MSC ou ATA) sem bloquear.
        // try_lock + backoff → nunca hang em K22 (SMP) nem K137 (TSC).
        let _ = crate::boot_logger::try_flush_ramlog();
    }

    /// Checkpoint SEM flush oportunista — uso DENTRO do path de persistência
    /// (boot_ckpt → try_flush → persist_now → boot_ckpt = recursão/stack overflow,
    /// mesma classe de log() em persist_now — SESSION_265).
    pub fn boot_ckpt_noflush(n: u8, msg: &str) {
        crate::slog_nano!("SMP", "ckpt", "K{}: {}", n, msg);
        // Persist for BOOT.LOG / ramlog dump (HW without COM)
        crate::boot_ramlog::set_last_ckpt(n);
        // Format like jarbas fb::boot_ckpt "K<n>: <msg>" for parser compatibility
        let mut buf = [0u8; 100];
        let mut pos = 0usize;
        buf[pos] = b'K';
        pos += 1;
        if n >= 100 {
            buf[pos] = b'0' + (n / 100);
            pos += 1;
        }
        if n >= 10 {
            buf[pos] = b'0' + ((n / 10) % 10);
            pos += 1;
        }
        buf[pos] = b'0' + (n % 10);
        pos += 1;
        buf[pos] = b':';
        pos += 1;
        buf[pos] = b' ';
        pos += 1;
        for &b in msg.as_bytes() {
            if pos >= buf.len() - 1 {
                break;
            }
            buf[pos] = b;
            pos += 1;
        }
        if let Ok(s) = core::str::from_utf8(&buf[..pos]) {
            crate::boot_ramlog::append(s);
        }
    }
}
