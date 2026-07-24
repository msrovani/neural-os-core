//! Boot chime PC speaker (Labor 39). Feature-gated `boot-chime` no k_nano.
//! Default OFF — ports 0x61/0x43 PIT.

use core::sync::atomic::AtomicBool;

static ARMED: AtomicBool = AtomicBool::new(false);

#[inline(always)]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nostack, preserves_flags));
}

/// C5 ≈ 523 Hz — PIT channel 2.
pub unsafe fn beep_hz(hz: u32, iters: u32) {
    if hz < 20 || hz > 20_000 {
        return;
    }
    let div = 1_193_182u32 / hz;
    outb(0x43, 0xB6);
    outb(0x42, (div & 0xFF) as u8);
    outb(0x42, ((div >> 8) & 0xFF) as u8);
    let mut t: u8;
    core::arch::asm!("in al, dx", out("al") t, in("dx") 0x61u16, options(nostack, preserves_flags));
    outb(0x61, t | 3);
    for _ in 0..iters {
        core::hint::spin_loop();
    }
    core::arch::asm!("in al, dx", out("al") t, in("dx") 0x61u16, options(nostack, preserves_flags));
    outb(0x61, t & !3);
}

/// C5-E5-G5 triad smoke — só se feature boot-chime.
pub fn play_triad_if_enabled() {
    #[cfg(feature = "boot-chime")]
    unsafe {
        ARMED.store(true, core::sync::atomic::Ordering::Relaxed);
        beep_hz(523, 80_000);
        beep_hz(659, 80_000);
        beep_hz(784, 80_000);
        crate::slog_nano!(
            "CHIME",
            "info",
            "step=boot_chime status=OK VERDICT=PASS reason=c5_e5_g5"
        );
    }
    #[cfg(not(feature = "boot-chime"))]
    {
        let _ = &ARMED;
        crate::slog_nano!(
            "CHIME",
            "info",
            "step=boot_chime status=SKIP VERDICT=SKIP reason=feature_off"
        );
    }
}

pub fn boot_smoke() {
    play_triad_if_enabled();
}