//! CapabilityGate — host-functions Hermes/WASM gated por Cap (ADR-0041 P3).
//! Sem POSIX: send_tcp / write_ring só passam com Cap explícita.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::serial_println;
use crate::syscall::{self, Cap, SYS_WRITE_RING};

static DENY_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOW_COUNT: AtomicU64 = AtomicU64::new(0);

/// Host Caps mínimas para sandbox Hermes (WASM / aios_*).
pub const HOST_FN_SEND_TCP: &str = "aios_send_tcp";
pub const HOST_FN_WRITE_RING: &str = "aios_write_ring";

/// Cap exigida por nome de host-function.
pub fn required_cap(host_fn: &str) -> Option<Cap> {
    match host_fn {
        HOST_FN_SEND_TCP | "send_tcp" | "net_send" => Some(Cap::SEND_TCP),
        HOST_FN_WRITE_RING | "write_ring" => Some(Cap::WRITE_RING),
        "ping" | "aios_ping" => Some(Cap::PING),
        "read_ring" | "aios_read_ring" => Some(Cap::READ_RING),
        _ => None,
    }
}

/// Verifica Cap; loga DENY no serial se falhar.
pub fn check(host_fn: &str, held: Cap) -> Result<(), &'static str> {
    let Some(need) = required_cap(host_fn) else {
        DENY_COUNT.fetch_add(1, Ordering::Relaxed);
        serial_println!("[CapGate] DENY unknown host_fn={}", host_fn);
        return Err("EPERM: host_fn desconhecida");
    };
    if !held.contains(need) {
        DENY_COUNT.fetch_add(1, Ordering::Relaxed);
        serial_println!(
            "[CapGate] DENY fn={} need=0x{:x} held=0x{:x}",
            host_fn,
            need.bits(),
            held.bits()
        );
        return Err("EPERM: Cap insuficiente");
    }
    ALLOW_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Stub host `aios_send_tcp` — só prova o gate (não abre socket real).
pub fn host_send_tcp(held: Cap, _host: &str, _port: u16) -> Result<u64, &'static str> {
    check(HOST_FN_SEND_TCP, held)?;
    Ok(0)
}

/// Host `aios_write_ring` — reutiliza SYS_WRITE_RING do trap Cap.
pub fn host_write_ring(held: Cap) -> Result<u64, &'static str> {
    check(HOST_FN_WRITE_RING, held)?;
    syscall::dispatch(SYS_WRITE_RING, 0, held)
}

pub fn deny_count() -> u64 {
    DENY_COUNT.load(Ordering::Relaxed)
}

pub fn allow_count() -> u64 {
    ALLOW_COUNT.load(Ordering::Relaxed)
}

/// Demo non-fatal: deny sem Cap, allow com Cap::SEND_TCP | WRITE_RING.
pub fn demo_hermes_caps() -> Result<(), &'static str> {
    serial_println!("[P3] CapabilityGate demo (Hermes host Caps)");

    if host_send_tcp(Cap::EMPTY, "127.0.0.1", 80).is_ok() {
        return Err("p3: Cap vazia nao deveria enviar tcp");
    }
    host_send_tcp(Cap::SEND_TCP, "127.0.0.1", 80)?;

    if host_write_ring(Cap::EMPTY).is_ok() {
        return Err("p3: Cap vazia nao deveria write_ring");
    }
    host_write_ring(Cap::WRITE_RING.union(Cap::SEND_TCP))?;

    serial_println!(
        "[P3] SUCCESS CapGate allow={} deny={}",
        allow_count(),
        deny_count()
    );
    Ok(())
}
