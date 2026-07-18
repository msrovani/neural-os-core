//! Cap HAL — MAP_BAR / device ops + AS R1 (ADR-0041 H5+).
//! Deny se caller_ring >= 3 sem Cap; BAR só no AS do HAL.
//! HalOffer::bind granta Cap lógica Fe*; FE sem bind → Deny.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

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
/// Caps lógicas concedidas por HalOffer::bind (bitmask).
static GRANTED: AtomicU32 = AtomicU32::new(0);

pub fn set_cap_enforce(on: bool) {
    CAP_ENFORCE.store(on, Ordering::SeqCst);
}

pub fn grant_fe(cap: HalCap) {
    let b = cap.bit();
    if b != 0 {
        GRANTED.fetch_or(b, Ordering::SeqCst);
        k_nano::slog_hal!("Cap", "grant", "{:?} granted", cap);
    }
}

pub fn revoke_fe(cap: HalCap) {
    let b = cap.bit();
    if b != 0 {
        GRANTED.fetch_and(!b, Ordering::SeqCst);
        k_nano::slog_hal!("Cap", "revoke", "{:?} revoked", cap);
    }
}

pub fn has_fe(cap: HalCap) -> bool {
    let b = cap.bit();
    b != 0 && (GRANTED.load(Ordering::SeqCst) & b) != 0
}

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

pub fn hal_as_bar0() -> Option<u64> {
    if HAL_AS_ACTIVE.load(Ordering::SeqCst) {
        Some(HAL_AS_BAR0.load(Ordering::SeqCst))
    } else {
        None
    }
}

pub fn clear_hal_as() {
    HAL_AS_ACTIVE.store(false, Ordering::SeqCst);
    HAL_AS_BAR0.store(0, Ordering::SeqCst);
}

/// `caller_ring`: 0=nano, 1=hal, 2=cortex/k_ai, 3=hermes/jarbas.
pub fn check_map_bar(caller_ring: u8, has_cap: bool) -> CapResult {
    if !CAP_ENFORCE.load(Ordering::SeqCst) {
        return CapResult::Allow;
    }
    if caller_ring <= 1 {
        return CapResult::Allow;
    }
    if caller_ring >= 3 && !has_cap {
        k_nano::slog_hal!("Cap", "MAP_BAR", "DENY ring={}", caller_ring);
        return CapResult::Deny;
    }
    if has_cap {
        CapResult::Allow
    } else {
        CapResult::Deny
    }
}

pub fn check_fe(caller_ring: u8, cap: HalCap, has_cap: bool) -> CapResult {
    if !CAP_ENFORCE.load(Ordering::SeqCst) {
        return CapResult::Allow;
    }
    if caller_ring <= 1 {
        return CapResult::Allow;
    }
    match cap {
        HalCap::FeNet
        | HalCap::FeDisplay
        | HalCap::FeAudio
        | HalCap::FeCompute
        | HalCap::FeVideo => {
            if has_cap || caller_ring == 2 {
                CapResult::Allow
            } else if caller_ring >= 3 && !has_cap {
                k_nano::slog_hal!("Cap", "FE", "DENY {:?} ring={}", cap, caller_ring);
                CapResult::Deny
            } else {
                CapResult::Deny
            }
        }
        _ => check_map_bar(caller_ring, has_cap),
    }
}

/// Gate FE R3: usa Cap concedida por HalOffer bind.
pub fn check_fe_bound(cap: HalCap) -> CapResult {
    let has = has_fe(cap);
    let r = check_fe(3, cap, has);
    if r == CapResult::Allow {
        k_nano::slog_hal!("Cap", "FE", "Allow {:?} (bound={})", cap, has);
    }
    r
}

/// Demo non-fatal H5+: R3 sem Cap → DENY; bind → Allow; R1 MAP_BAR OK.
pub fn demo_h5_deny() {
    let r1 = check_map_bar(1, false);
    let r3 = check_map_bar(3, false);
    let fe_before = check_fe_bound(HalCap::FeNet);
    k_nano::slog_hal!(
        "Cap",
        "h5_demo",
        "R1={:?} R3_no_cap={:?} FE_no_bind={:?} (expect Allow/Deny/Deny)",
        r1,
        r3,
        fe_before
    );
    // Simula HalOffer grant + FE Allow
    grant_fe(HalCap::FeNet);
    let fe_after = check_fe_bound(HalCap::FeNet);
    k_nano::slog_hal!(
        "Cap",
        "h5_demo",
        "FE_after_grant={:?} (expect Allow)",
        fe_after
    );
    revoke_fe(HalCap::FeNet);

    if let Some(bar) = discovery_first_gpu_bar() {
        bind_hal_as(bar);
    }
}

fn discovery_first_gpu_bar() -> Option<u64> {
    for c in crate::discovery::device_tree() {
        if c.id.class == crate::device_cap::DeviceClass::Gpu && c.id.bar0 != 0 {
            return Some(c.id.bar0);
        }
    }
    // Fallback: qualquer BAR0 do tree
    for c in crate::discovery::device_tree() {
        if c.id.bar0 != 0 {
            return Some(c.id.bar0);
        }
    }
    None
}
