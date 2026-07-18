//! OS-Emulation compat layer — mutex cooperativo + rdtsc calibrado para firmware blobs.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::arch::asm;

pub static TSC_PER_MS: AtomicU64 = AtomicU64::new(2400); // fallback, calibrado no boot

/// Mutex cooperativo para blobs WiFi (sem layout C — AtomicBool basta).
pub struct WifiMutex {
    pub locked: AtomicBool,
}

#[no_mangle]
pub static mut wifi_mutex: WifiMutex = WifiMutex {
    locked: AtomicBool::new(false),
};

#[no_mangle]
pub unsafe extern "C" fn wifi_os_mutex_lock(m: *mut WifiMutex) {
    if m.is_null() {
        return;
    }
    while (*m)
        .locked
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        asm!("pause", options(nostack));
    }
}

#[no_mangle]
pub unsafe extern "C" fn wifi_os_mutex_unlock(m: *mut WifiMutex) {
    if m.is_null() {
        return;
    }
    (*m).locked.store(false, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn wifi_os_get_time_ms() -> u64 {
    let lo: u32; let hi: u32;
    unsafe { asm!("rdtsc", out("eax") lo, out("edx") hi); }
    let tsc = (hi as u64) << 32 | lo as u64;
    let freq = TSC_PER_MS.load(Ordering::Relaxed);
    if freq == 0 { tsc / 2400 } else { tsc / freq }
}
