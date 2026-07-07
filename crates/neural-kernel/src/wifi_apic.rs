//! APIC Timer Heartbeat — watchdog de beacons 802.11 via timer local x86.
//! Dispara a cada ~100ms para detectar queda de roteador WiFi.

use core::ptr::write_volatile;

const APIC_BASE: usize = 0xFEE0_0000;
const APIC_ICR: usize = APIC_BASE + 0x0380;
const APIC_LVT: usize = APIC_BASE + 0x0320;
const APIC_TDCR: usize = APIC_BASE + 0x03E0;
const APIC_EOI: usize = APIC_BASE + 0x00B0;

pub static mut BEACON_MISS: u32 = 0;

pub unsafe fn setup_heartbeat(vector: u8) {
    write_volatile(APIC_TDCR as *mut u32, 0x03); // divide por 16
    write_volatile(APIC_LVT as *mut u32, vector as u32 | (1 << 17)); // periodico
    write_volatile(APIC_ICR as *mut u32, 10_000_000); // ~100ms
}

#[no_mangle]
pub unsafe extern "x86-interrupt" fn isr_apic_heartbeat(_frame: *mut u8) {
    BEACON_MISS = BEACON_MISS.wrapping_add(1);
    write_volatile(APIC_EOI as *mut u32, 0);
}
