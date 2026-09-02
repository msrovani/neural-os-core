//! Boot Logger — thin wrapper display-dependent p/ neural-kernel.
//!
//! Todo o estado e lógica principal vivem em `k_nano::boot_logger`.
//! Este módulo re-exporta as funções core e adiciona apenas os wrappers
//! que dependem de display/FB (console_print, boot_ckpt).
//!
//! Notebooks modernos sem COM: BOOT.LOG no FAT32 é o canal de diagnóstico.
//! Feature `fat-boot-log` ativa no crate `boot` para imagem HW.

#[allow(unused_imports)]
pub use k_nano::boot_logger::{
    FAT_READY,
    ensure_persisted,
    flush,
    try_flush_ramlog,
    log,
    log_quiet,
    mark_heap_ready,
    init,
};

/// Ponytail: expõe ramlog em RAM no FB quando pendrive não está pronto (K22/K137 hang).
/// Chamado no final do boot e como fallback quando `try_flush_ramlog` não gravou.
pub fn dump_ramlog_to_fb() {
    let session = k_nano::boot_logger::build_session_bytes();
    if session.is_empty() {
        return;
    }
    if let Ok(s) = core::str::from_utf8(&session) {
        crate::display::fb::console_print(">>> BOOT.LOG (RAM) <<<");
        for line in s.lines().take(40) {
            crate::display::fb::console_print(line);
        }
        crate::display::fb::console_print(">>> FIM BOOT.LOG <<<");
        // Também mantém no ramlog phys (snapshot) para diagnóstico pós-reset.
        for line in s.lines().take(80) {
            k_nano::boot_ramlog::append(line);
        }
    }
}

/// Init imediato após USB-MSC (caminho notebook sem serial).
/// Grava BOOT.LOG via BlockDevice (MSC/ATA/AHCI/NVMe) — sem soft-reboot.
pub fn init_after_usb() {
    #[cfg(feature = "fat-boot-log")]
    {
        log("BOOT: fat-boot-log init_after_usb");
        let has_msc = crate::USB_MSC.lock().is_some();
        let has_ata = crate::ATA_DRIVER.lock().is_some();
        let has_ahci = crate::AHCI_DRIVER.lock().is_some();
        let ok = ensure_persisted();
        k_nano::slog_bin!(
            "LOG", "ok",
            "init_after_usb BOOT.LOG ok={} msc={} ata={} ahci={} (procure BOOT.LOG na raiz FAT32)",
            ok, has_msc, has_ata, has_ahci
        );
        if ok {
            crate::display::fb::console_print("LOG: BOOT.LOG ok (FAT32)");
        } else if !has_msc && !has_ata && !has_ahci {
            crate::display::fb::console_print(
                "LOG: BOOT.LOG skip — USB-MSC/ATA/AHCI AUSENTE (sem soft-reboot)",
            );
            k_nano::boot_ramlog::mark_skip_flush_reboot();
        }
    }
}

/// Sem BlockDevice: grava ramlog + pausa curta p/ foto, **continua o boot**.
/// Nunca soft-reboot (0xCF9) — removido do path de produto (loop HW).
pub fn maybe_uefi_flush_reboot(reason: &str) {
    #[cfg(feature = "fat-boot-log")]
    {
        if FAT_READY.load(core::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if k_nano::boot_ramlog::skip_flush_reboot() {
            return;
        }
        let content = k_nano::boot_logger::build_session_bytes();
        let s = core::str::from_utf8(&content).unwrap_or(reason);
        for line in s.lines().take(200) {
            k_nano::boot_ramlog::append(line);
        }
        let k = k_nano::boot_ramlog::last_ckpt();
        k_nano::slog_bin!(
            "RAMLOG", "info",
            "sem FAT (K{}) — {} — continue Runtime (sem soft-reboot)", k, reason
        );
        for sec in (0..3u32).rev() {
            let msg = alloc::format!(">>> FOTO K{} | go boot {}s <<<", k, sec);
            crate::display::fb::console_print(&msg);
            for _ in 0..40_000_000 { core::hint::spin_loop(); }
        }
        k_nano::boot_ramlog::mark_skip_flush_reboot();
        crate::display::fb::console_print(
            ">>> BOOT.LOG indisponivel (sem MSC/ATA/AHCI) — continue Runtime",
        );
        crate::display::fb::boot_ckpt(38, "continue sem MSC");
    }
    #[cfg(not(feature = "fat-boot-log"))]
    { let _ = reason; }
}

/// Pos-saudacao JARVIS: persiste BOOT.LOG via BlockDevice se houver;
/// senao ramlog + FB e **continua Runtime**.
/// Retorna `true` se gravou no FAT; `false` se so ramlog/FB.
pub fn flush_bootlog_after_greeting(reason: &str) -> bool {
    #[cfg(feature = "fat-boot-log")]
    {
        if FAT_READY.load(core::sync::atomic::Ordering::Relaxed) {
            flush();
            crate::display::fb::console_print(">>> BOOT.LOG no FAT (MSC/ATA/AHCI) — continue");
            crate::display::fb::boot_ckpt(51, "BOOT.LOG FAT ok");
            k_nano::boot_ramlog::mark_skip_flush_reboot();
            return true;
        }
        let content = k_nano::boot_logger::build_session_bytes();
        let s = core::str::from_utf8(&content).unwrap_or(reason);
        for line in s.lines().take(400) {
            k_nano::boot_ramlog::append(line);
        }
        k_nano::boot_ramlog::append(reason);
        k_nano::boot_ramlog::append("[JARBAS] greet OK — BOOT.LOG so ramlog (sem soft-reboot)");
        let k = k_nano::boot_ramlog::last_ckpt();
        crate::display::fb::boot_ckpt(51, "BOOT.LOG skip reboot");
        let msg = alloc::format!(
            ">>> BOOT.LOG indisponivel K{} — continue Runtime (sem reboot)", k
        );
        crate::display::fb::console_print(&msg);
        k_nano::boot_ramlog::mark_skip_flush_reboot();
        k_nano::slog_bin!(
            "RAMLOG", "info",
            "pos-JARVIS sem FAT — {} — Runtime segue (soft-reboot OFF)", reason
        );
        false
    }
    #[cfg(not(feature = "fat-boot-log"))]
    {
        let _ = reason;
        crate::display::fb::console_print(">>> fat-boot-log OFF — continue Runtime");
        k_nano::boot_ramlog::mark_skip_flush_reboot();
        false
    }
}
