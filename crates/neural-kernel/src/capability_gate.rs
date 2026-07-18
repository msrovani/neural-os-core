//! CapabilityGate — host-functions Hermes/WASM gated por Cap (ADR-0041 P3).
//! Sem POSIX: send_tcp / write_ring só passam com Cap explícita.
//!
//! # Bootstrap de Caps (N1.2)
//! Demos P3–P9 no boot usam `Cap::empty()` / held parcial de propósito —
//! os DENY no serial são **esperados** (prova do gate), não falha de produto.
//! Authority de produção (Hermes AS + Caps iniciais) = follow-up N2/N4;
//! não inventar grant amplo no boot só para silenciar DENY.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::syscall::{self, Cap, SYS_WRITE_RING};

static DENY_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOW_COUNT: AtomicU64 = AtomicU64::new(0);

/// Host Caps mínimas para sandbox Hermes (WASM / aios_*).
pub const HOST_FN_SEND_TCP: &str = "aios_send_tcp";
pub const HOST_FN_WRITE_RING: &str = "aios_write_ring";
/// JARBAS FB (ADR-0041 P4) — também registradas no gate para deny uniforme.
pub const HOST_FN_MAP_FB: &str = "aios_map_fb";
pub const HOST_FN_PRESENT_FB: &str = "aios_present_fb";
/// K-IA / Cortex (ADR-0041 P5).
pub const HOST_FN_PIN_DMA: &str = "aios_pin_dma";
pub const HOST_FN_MAP_DMA: &str = "aios_map_dma";
pub const HOST_FN_MAP_WEIGHTS: &str = "aios_map_weights";
/// P7 demand-paging.
pub const HOST_FN_DEMAND_PAGE: &str = "aios_demand_page";
/// P8 VirtIO vring setup.
pub const HOST_FN_VRING_SETUP: &str = "aios_vring_setup";
/// P9 GGUF/FAT file-backed mmap.
pub const HOST_FN_MAP_FILE: &str = "aios_map_file";

/// Cap exigida por nome de host-function.
pub fn required_cap(host_fn: &str) -> Option<Cap> {
    match host_fn {
        HOST_FN_SEND_TCP | "send_tcp" | "net_send" => Some(Cap::SEND_TCP),
        HOST_FN_WRITE_RING | "write_ring" => Some(Cap::WRITE_RING),
        "ping" | "aios_ping" => Some(Cap::PING),
        "read_ring" | "aios_read_ring" => Some(Cap::READ_RING),
        HOST_FN_MAP_FB | "map_fb" => Some(Cap::MAP_FB),
        HOST_FN_PRESENT_FB | "present_fb" | "write_fb" => Some(Cap::WRITE_FB),
        HOST_FN_PIN_DMA | "pin_dma" => Some(Cap::PIN_DMA),
        HOST_FN_MAP_DMA | "map_dma" => Some(Cap::MAP_DMA),
        HOST_FN_MAP_WEIGHTS | "map_weights" => Some(Cap::MAP_WEIGHTS),
        HOST_FN_DEMAND_PAGE | "demand_page" => Some(Cap::DEMAND_PAGE),
        HOST_FN_VRING_SETUP | "vring_setup" => Some(Cap::VRING_SETUP),
        HOST_FN_MAP_FILE | "map_file" => Some(Cap::MAP_FILE),
        _ => None,
    }
}

/// Verifica Cap; loga DENY no serial se falhar.
pub fn check(host_fn: &str, held: Cap) -> Result<(), &'static str> {
    let Some(need) = required_cap(host_fn) else {
        DENY_COUNT.fetch_add(1, Ordering::Relaxed);
        k_nano::slog_bin!("CapGate", "info", "DENY unknown host_fn={}", host_fn);
        return Err("EPERM: host_fn desconhecida");
    };
    if !held.contains(need) {
        DENY_COUNT.fetch_add(1, Ordering::Relaxed);
        k_nano::slog_bin!("CapGate", "info", "DENY fn={} need=0x{:x} held=0x{:x}",
            host_fn,
            need.bits(),
            held.bits());
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
    k_nano::slog_bin!("Cap", "p3", "CapabilityGate demo (Hermes host Caps)");

    if host_send_tcp(Cap::EMPTY, "127.0.0.1", 80).is_ok() {
        return Err("p3: Cap vazia nao deveria enviar tcp");
    }
    host_send_tcp(Cap::SEND_TCP, "127.0.0.1", 80)?;

    if host_write_ring(Cap::EMPTY).is_ok() {
        return Err("p3: Cap vazia nao deveria write_ring");
    }
    host_write_ring(Cap::WRITE_RING.union(Cap::SEND_TCP))?;

    k_nano::slog_bin!("Cap", "p3", "SUCCESS CapGate allow={} deny={}",
        allow_count(),
        deny_count());
    Ok(())
}
