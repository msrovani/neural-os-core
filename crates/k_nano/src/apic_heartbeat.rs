//! APIC Timer Heartbeat — watchdog de beacons via timer local x86.
//! Dispara a cada ~100ms para detectar queda de roteador WiFi.
//!
//! Integração: WifiAgent chama `setup_heartbeat(vector)` durante init.
//! O ISR incrementa BEACON_MISS a cada tick. WifiAgent::tick() verifica
//! se BEACON_MISS cresceu — se não, roteador caiu —> reconexão.

use core::ptr::write_volatile;
use core::sync::atomic::{AtomicU32, Ordering};

const APIC_BASE: usize = 0xFEE0_0000;
const APIC_ICR: usize = APIC_BASE + 0x0380;
const APIC_LVT: usize = APIC_BASE + 0x0320;
const APIC_TDCR: usize = APIC_BASE + 0x03E0;
const APIC_EOI: usize = APIC_BASE + 0x00B0;

/// Contador de heartbeats perdidos.
pub static BEACON_MISS: AtomicU32 = AtomicU32::new(0);

/// Configura o timer APIC para gerar interrupções periódicas.
pub unsafe fn setup_heartbeat(vector: u8) {
    write_volatile(APIC_TDCR as *mut u32, 0x03);
    write_volatile(APIC_LVT as *mut u32, vector as u32 | (1 << 17));
    write_volatile(APIC_ICR as *mut u32, 10_000_000);
}

/// ISR do heartbeat — chamado pelo IDT.
#[no_mangle]
pub unsafe extern "x86-interrupt" fn isr_apic_heartbeat(_frame: *mut u8) {
    BEACON_MISS.fetch_add(1, Ordering::Relaxed);
    write_volatile(APIC_EOI as *mut u32, 0);
}
