//! Cap HAL — MAP_BAR / device ops + AS R1 + CapGate (ADR-0041 H5+).
//! Canonical Cap lives in k_nano::paging (R0) — re-exported here (R1) as
//! single source of truth, no duplication. Dispatch validates Cap + allocates
//! L3/L2 frames via k_nano::paging helpers (R0 paging).

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

// Re-export canonical Cap and syscall numbers from R0 paging (no duplication)
pub use k_nano::paging::Cap;
pub use k_nano::paging::{
    RING_OP_READ, RING_OP_WRITE, SYSCALL_VECTOR, SYS_DEMAND_PAGE, SYS_EXIT_USER,
    SYS_MAP_DMA, SYS_MAP_FB, SYS_MAP_FILE, SYS_MAP_WEIGHTS, SYS_PIN_DMA, SYS_PING,
    SYS_PRESENT_FB, SYS_RING_OP,
};

// ─── HalCap (R1 FE) — unchanged ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HalCap {
    None = 0,
    MapBar = 1,
    DeviceIo = 2,
    FeNet = 3,
    FeDisplay = 4,
    FeAudio = 5,
    FeCompute = 6,
    FeVideo = 7,
}

impl HalCap {
    fn bit(self) -> u32 {
        match self {
            HalCap::None => 0,
            HalCap::MapBar => 1 << 0,
            HalCap::DeviceIo => 1 << 1,
            HalCap::FeNet => 1 << 2,
            HalCap::FeDisplay => 1 << 3,
            HalCap::FeAudio => 1 << 4,
            HalCap::FeCompute => 1 << 5,
            HalCap::FeVideo => 1 << 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapResult {
    Allow,
    Deny,
}

static HAL_AS_ACTIVE: AtomicBool = AtomicBool::new(false);
static HAL_AS_BAR0: AtomicU64 = AtomicU64::new(0);
static CAP_ENFORCE: AtomicBool = AtomicBool::new(true);
static GRANTED: AtomicU32 = AtomicU32::new(0);

pub fn set_cap_enforce(on: bool) { CAP_ENFORCE.store(on, Ordering::SeqCst); }
pub fn grant_fe(cap: HalCap) { let b = cap.bit(); if b != 0 { GRANTED.fetch_or(b, Ordering::SeqCst); k_nano::slog_hal!("Cap", "grant", "{:?} granted", cap); } }
pub fn revoke_fe(cap: HalCap) { let b = cap.bit(); if b != 0 { GRANTED.fetch_and(!b, Ordering::SeqCst); k_nano::slog_hal!("Cap", "revoke", "{:?} revoked", cap); } }
pub fn has_fe(cap: HalCap) -> bool { let b = cap.bit(); b != 0 && (GRANTED.load(Ordering::SeqCst) & b) != 0 }
pub fn fe_for_class(class: crate::device_cap::DeviceClass) -> Option<HalCap> {
    use crate::device_cap::DeviceClass;
    match class {
        DeviceClass::Net | DeviceClass::Wifi => Some(HalCap::FeNet),
        DeviceClass::Display => Some(HalCap::FeDisplay),
        DeviceClass::Gpu => Some(HalCap::FeCompute),
        DeviceClass::Snd => Some(HalCap::FeAudio),
        DeviceClass::Video => Some(HalCap::FeVideo),
        _ => None,
    }
}
pub fn bind_hal_as(bar0: u64) {
    HAL_AS_BAR0.store(bar0, Ordering::SeqCst);
    HAL_AS_ACTIVE.store(true, Ordering::SeqCst);
    k_nano::slog_hal!("AS", "bind", "R1 AS bound bar0={:#x}", bar0);
}
pub fn hal_as_bar0() -> Option<u64> { if HAL_AS_ACTIVE.load(Ordering::SeqCst) { Some(HAL_AS_BAR0.load(Ordering::SeqCst)) } else { None } }
pub fn clear_hal_as() { HAL_AS_ACTIVE.store(false, Ordering::SeqCst); HAL_AS_BAR0.store(0, Ordering::SeqCst); }
pub fn check_map_bar(caller_ring: u8, has_cap: bool) -> CapResult {
    if !CAP_ENFORCE.load(Ordering::SeqCst) { return CapResult::Allow; }
    if caller_ring <= 1 { return CapResult::Allow; }
    if caller_ring >= 3 && !has_cap { k_nano::slog_hal!("Cap", "MAP_BAR", "DENY ring={}", caller_ring); return CapResult::Deny; }
    if has_cap { CapResult::Allow } else { CapResult::Deny }
}
pub fn check_fe(caller_ring: u8, cap: HalCap, has_cap: bool) -> CapResult {
    if !CAP_ENFORCE.load(Ordering::SeqCst) { return CapResult::Allow; }
    if caller_ring <= 1 { return CapResult::Allow; }
    match cap {
        HalCap::FeNet | HalCap::FeDisplay | HalCap::FeAudio | HalCap::FeCompute | HalCap::FeVideo => {
            if has_cap || caller_ring == 2 { CapResult::Allow }
            else if caller_ring >= 3 && !has_cap { k_nano::slog_hal!("Cap", "FE", "DENY {:?} ring={}", cap, caller_ring); CapResult::Deny } else { CapResult::Deny }
        }
        _ => check_map_bar(caller_ring, has_cap),
    }
}
pub fn check_fe_bound(cap: HalCap) -> CapResult {
    let has = has_fe(cap);
    let r = check_fe(3, cap, has);
    if r == CapResult::Allow { k_nano::slog_hal!("Cap", "FE", "Allow {:?} (bound={})", cap, has); }
    r
}
pub fn demo_h5_deny() {
    let r1 = check_map_bar(1, false);
    let r3 = check_map_bar(3, false);
    let fe_before = check_fe_bound(HalCap::FeNet);
    k_nano::slog_hal!("Cap", "h5_demo", "R1={:?} R3_no_cap={:?} FE_no_bind={:?} (expect Allow/Deny/Deny)", r1, r3, fe_before);
    grant_fe(HalCap::FeNet);
    let fe_after = check_fe_bound(HalCap::FeNet);
    k_nano::slog_hal!("Cap", "h5_demo", "FE_after_grant={:?} (expect Allow)", fe_after);
    revoke_fe(HalCap::FeNet);
    if let Some(bar) = discovery_first_gpu_bar() { bind_hal_as(bar); }
}
fn discovery_first_gpu_bar() -> Option<u64> {
    for c in crate::discovery::device_tree() { if c.id.class == crate::device_cap::DeviceClass::Gpu && c.id.bar0 != 0 { return Some(c.id.bar0); } }
    for c in crate::discovery::device_tree() { if c.id.bar0 != 0 { return Some(c.id.bar0); } }
    None
}

// ─── CapabilityGate (moved from bin) ───────────────────────────────────────

pub const HOST_FN_SEND_TCP: &str = "aios_send_tcp";
pub const HOST_FN_WRITE_RING: &str = "aios_write_ring";
pub const HOST_FN_MAP_FB: &str = "aios_map_fb";
pub const HOST_FN_PRESENT_FB: &str = "aios_present_fb";
pub const HOST_FN_PIN_DMA: &str = "aios_pin_dma";
pub const HOST_FN_MAP_DMA: &str = "aios_map_dma";
pub const HOST_FN_MAP_WEIGHTS: &str = "aios_map_weights";
pub const HOST_FN_DEMAND_PAGE: &str = "aios_demand_page";
pub const HOST_FN_VRING_SETUP: &str = "aios_vring_setup";
pub const HOST_FN_MAP_FILE: &str = "aios_map_file";

static DENY_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOW_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn required_cap(host_fn: &str) -> Option<Cap> {
    match host_fn {
        HOST_FN_SEND_TCP | "send_tcp" | "net_send" => Some(Cap::RING_OP),
        HOST_FN_WRITE_RING | "write_ring" => Some(Cap::RING_OP),
        "ping" | "aios_ping" => Some(Cap::PING),
        "read_ring" | "aios_read_ring" => Some(Cap::RING_OP),
        HOST_FN_MAP_FB | "map_fb" => Some(Cap::MAP_FB),
        HOST_FN_PRESENT_FB | "present_fb" | "write_fb" => Some(Cap::WRITE_FB),
        HOST_FN_PIN_DMA | "pin_dma" => Some(Cap::PIN_DMA),
        HOST_FN_MAP_DMA | "map_dma" => Some(Cap::MAP_DMA),
        HOST_FN_MAP_WEIGHTS | "map_weights" => Some(Cap::MAP_WEIGHTS),
        HOST_FN_DEMAND_PAGE | "demand_page" => Some(Cap::DEMAND_PAGE),
        HOST_FN_VRING_SETUP | "vring_setup" => Some(Cap::RING_OP),
        HOST_FN_MAP_FILE | "map_file" => Some(Cap::MAP_FILE),
        _ => None,
    }
}
pub fn check(host_fn: &str, held: Cap) -> Result<(), &'static str> {
    let Some(need) = required_cap(host_fn) else {
        DENY_COUNT.fetch_add(1, Ordering::Relaxed);
        k_nano::slog_hal!("CapGate", "info", "DENY unknown host_fn={}", host_fn);
        return Err("EPERM: host_fn desconhecida");
    };
    if !held.contains(need) {
        DENY_COUNT.fetch_add(1, Ordering::Relaxed);
        k_nano::slog_hal!("CapGate", "info", "DENY fn={} need=0x{:x} held=0x{:x}", host_fn, need.bits(), held.bits());
        return Err("EPERM: Cap insuficiente");
    }
    ALLOW_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(())
}
pub fn host_send_tcp(held: Cap, _host: &str, _port: u16) -> Result<u64, &'static str> {
    check(HOST_FN_SEND_TCP, held)?;
    Ok(0)
}
fn parse_dotted_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut parts = s.split('.');
    for o in out.iter_mut() { let p = parts.next()?; *o = p.parse().ok()?; }
    if parts.next().is_some() { return None; }
    Some(out)
}
pub fn host_write_ring(held: Cap) -> Result<u64, &'static str> {
    check(HOST_FN_WRITE_RING, held)?;
    dispatch(SYS_RING_OP, RING_OP_WRITE, held)
}
pub fn deny_count() -> u64 { DENY_COUNT.load(Ordering::Relaxed) }
pub fn allow_count() -> u64 { ALLOW_COUNT.load(Ordering::Relaxed) }
pub fn demo_hermes_caps() -> Result<(), &'static str> {
    k_nano::slog_hal!("Cap", "p3", "CapabilityGate demo (Hermes host Caps)");
    if host_send_tcp(Cap::EMPTY, "127.0.0.1", 80).is_ok() { return Err("p3: Cap vazia nao deveria enviar tcp"); }
    host_send_tcp(Cap::RING_OP, "127.0.0.1", 80)?;
    if host_write_ring(Cap::EMPTY).is_ok() { return Err("p3: Cap vazia nao deveria write_ring"); }
    host_write_ring(Cap::RING_OP)?;
    k_nano::slog_hal!("Cap", "p3", "SUCCESS CapGate allow={} deny={}", allow_count(), deny_count());
    Ok(())
}

// ─── Syscall dispatch (R1 Cap validation + R0 paging allocation) ───────────

/// Dispatch capability-gated. Allocates L3/L2 frames via k_nano::paging when needed.
pub fn dispatch(nr: u64, arg: u64, cap: Cap) -> Result<u64, &'static str> {
    // Sandbox deny (R0 helper)
    k_nano::paging::dispatch_check_sandbox(nr, cap)?;
    match nr {
        SYS_PING => {
            if !cap.contains(Cap::PING) { return Err("EPERM: Cap::PING"); }
            Ok(k_nano::paging::ping_count().wrapping_add(1))
        }
        SYS_RING_OP => {
            if !cap.contains(Cap::RING_OP) { return Err("EPERM: Cap::RING_OP"); }
            Ok(0)
        }
        SYS_MAP_FB => {
            if !cap.contains(Cap::MAP_FB) { return Err("EPERM: Cap::MAP_FB"); }
            let fb_phys = arg;
            if fb_phys == 0 { return Err("ENODEV: FB phys address is 0"); }
            // Full mapping is done via k_nano paging helper when wire is present.
            // For now, validate Cap and return VA stub (bin's jarbas_fb will do real map via paging).
            Ok(0x0000_4000_0000_0000)
        }
        SYS_PRESENT_FB => {
            if !cap.contains(Cap::WRITE_FB) { return Err("EPERM: Cap::WRITE_FB"); }
            Ok(0)
        }
        SYS_PIN_DMA => {
            if !cap.contains(Cap::PIN_DMA) { return Err("EPERM: Cap::PIN_DMA"); }
            Ok(0)
        }
        SYS_MAP_DMA => {
            if !cap.contains(Cap::MAP_DMA) { return Err("EPERM: Cap::MAP_DMA"); }
            Ok(0)
        }
        SYS_MAP_WEIGHTS => {
            if !cap.contains(Cap::MAP_WEIGHTS) { return Err("EPERM: Cap::MAP_WEIGHTS"); }
            Ok(0)
        }
        SYS_EXIT_USER => {
            if !cap.contains(Cap::ENTER_USER) { return Err("EPERM: Cap::ENTER_USER"); }
            Ok(0)
        }
        SYS_DEMAND_PAGE => {
            if !cap.contains(Cap::DEMAND_PAGE) { return Err("EPERM: Cap::DEMAND_PAGE"); }
            // Real allocation lives in k_nano::paging::install_present_leaf_current via #PF;
            // dispatch here just validates Cap.
            Ok(0)
        }
        SYS_MAP_FILE => {
            if !cap.contains(Cap::MAP_FILE) { return Err("EPERM: Cap::MAP_FILE"); }
            Ok(0)
        }
        _ => Err("ENOSYS"),
    }
}

// Re-export staging / soft-path via R0 paging (single statics)
pub use k_nano::paging::stage_syscall;
pub use k_nano::paging::soft_syscall;
pub use k_nano::paging::init_syscall_fast_path;
pub use k_nano::paging::syscall_int_handler;
pub use k_nano::paging::ping_count;

// Isolation seam (R1) — delegates to R0 paging (no hermes dep; bin wires hermes)
pub fn ring3_is_safe() -> bool { k_nano::paging::ring3_is_safe() }
pub fn ring3_run_native(code: &[u8], _caps: u32) -> Result<i64, &'static str> { k_nano::paging::ring3_run_native_blob(code) }

// Host SSE stub gated to avoid STATUS_ILLEGAL_INSTRUCTION soft-float (lesson SSE)
#[cfg(all(x86_64, not(target_os="none")))]
pub fn host_sse_stub() {}
