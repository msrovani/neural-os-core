//! QEMU fw_cfg session persistence MVP (Labor 46).
//! Selector 0x510/0x511 — honesty SKIP se não QEMU.

use core::sync::atomic::{AtomicBool, Ordering};

static SEEN: AtomicBool = AtomicBool::new(false);

#[inline(always)]
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, preserves_flags));
}

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let v: u8;
    core::arch::asm!("in al, dx", out("al") v, in("dx") port, options(nostack, preserves_flags));
    v
}

/// Probe signature "QEMU" via fw_cfg file directory — best-effort.
pub fn probe() -> bool {
    unsafe {
        // FW_CFG_SIGNATURE = 0x0000
        outw(0x510, 0x0000);
        let mut sig = [0u8; 4];
        for b in &mut sig {
            *b = inb(0x511);
        }
        let ok = &sig == b"QEMU";
        SEEN.store(ok, Ordering::Relaxed);
        ok
    }
}

pub fn boot_smoke() -> bool {
    let ok = probe();
    if ok {
        crate::slog_nano!(
            "FWCFG",
            "info",
            "step=probe status=OK VERDICT=PARTIAL reason=qemu_signature"
        );
    } else {
        crate::slog_nano!(
            "FWCFG",
            "info",
            "step=probe status=SKIP VERDICT=SKIP reason=no_qemu_fw_cfg"
        );
    }
    true
}