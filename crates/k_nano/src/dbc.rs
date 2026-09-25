//! xHCI Debug Capability (DbC) — detect-only skeleton.
//!
//! xHCI spec §7: extended capabilities hang off the HCCPARAMS1 xECP pointer
//! (CAP offset 0x10, bits 31:16 = dword offset from Cap base); each ext-cap
//! header is `id[7:0] | next[15:8]`, with `next` a dword stride from the
//! current entry (same walk as `claim_firmware_ownership` / `port_protocol_major`
//! in `xhci/mod.rs`). DbC = extended capability ID `0x0A` (precedent:
//! header-only `cjams/xue` C++ driver, Linux `xhci-dbgcap.c`).
//!
//! Scope: detect-only. No write path, no DbC doorbell/transfer-ring bring-up,
//! no slog/dispatch integration. `dbc_detect` does volatile MMIO reads only
//! (no alloc, no locks, no logging — returns bool, caller logs). `dbc_present`
//! is a cached lazy probe over the already-bound (already mapped) HC in
//! `crate::xhci::XHCI_STATE`; it never touches unmapped BARs, so a pre-init
//! call reports absent without caching a permanent false.

use core::sync::atomic::{AtomicBool, Ordering};

/// Extended-capability ID for the Debug Capability (xHCI §7).
pub const DBC_CAP_ID: u8 = 0x0A;
/// Hard bound on ext-cap walk steps (mirrors the `< 64` bound in xhci/mod.rs).
const DBC_WALK_MAX: usize = 64;
/// Byte-offset ceiling for the walk; xECP is a 16-bit dword index (max
/// ~256 KB) and real cap space is a few KB — anything past this is garbage.
const DBC_OFF_MAX: u64 = 0x4000;

#[inline]
pub fn dbc_cap_id(hdr: u32) -> u8 {
    (hdr & 0xFF) as u8
}

#[inline]
pub fn dbc_cap_next(hdr: u32) -> u64 {
    ((hdr >> 8) & 0xFF) as u64
}

#[inline]
pub fn dbc_xecp_off(hcc1: u32) -> u64 {
    (((hcc1 >> 16) & 0xFFFF) as u64).wrapping_mul(4)
}

/// Pure-logic cap walk over an abstract header reader.
///
/// `read` maps a byte offset (from Cap base) to the u32 header there, or
/// `None` to abort the walk. `start_off` is the xECP byte offset (0 = none).
/// This is the host-test seam: the MMIO reader in `dbc_detect` and the fake
/// buffer in tests both plug in here.
pub(crate) fn dbc_scan<F>(mut read: F, start_off: u64) -> bool
where
    F: FnMut(u64) -> Option<u32>,
{
    if start_off == 0 || start_off > DBC_OFF_MAX {
        return false;
    }
    let mut off = start_off;
    for _ in 0..DBC_WALK_MAX {
        if off == 0 || off > DBC_OFF_MAX {
            break;
        }
        let hdr = match read(off) {
            Some(h) => h,
            None => return false,
        };
        if hdr == 0 || hdr == 0xFFFF_FFFF {
            return false;
        }
        if dbc_cap_id(hdr) == DBC_CAP_ID {
            return true;
        }
        let next = dbc_cap_next(hdr);
        if next == 0 {
            break;
        }
        off = off.wrapping_add(next.wrapping_mul(4));
    }
    false
}

/// Walk xHCI extended capabilities from the HCCPARAMS1 xECP pointer looking
/// for ID `0x0A` (DbC).
///
/// `mmio_base_phys` is the raw BAR0 base (masked, as in `init_xhci_select`:
/// `bar0 & !0xF`); `pmoff` is the HHDM offset (`PHYS_MEM_OFFSET`). The caller
/// must have the HC pages mapped (the bound HC in `XHCI_STATE` already is —
/// see the 16-page `map_page_uc` loop in `init_xhci_select`).
/// Returns `true` iff a `0x0A` entry is found; never logs, allocates, or locks.
pub fn dbc_detect(mmio_base_phys: u64, pmoff: u64) -> bool {
    if mmio_base_phys == 0 || pmoff == 0 {
        return false;
    }
    let base = mmio_base_phys.wrapping_add(pmoff);
    let hcc1 = unsafe { (base.wrapping_add(0x10) as *const u32).read_volatile() };
    if hcc1 == 0 || hcc1 == 0xFFFF_FFFF {
        return false;
    }
    let start = dbc_xecp_off(hcc1);
    dbc_scan(
        |off| {
            if off > DBC_OFF_MAX {
                return None;
            }
            Some(unsafe { (base.wrapping_add(off) as *const u32).read_volatile() })
        },
        start,
    )
}

static DBC_DONE: AtomicBool = AtomicBool::new(false);
static DBC_HIT: AtomicBool = AtomicBool::new(false);

/// Cached lazy probe: `true` iff the already-bound HC exposes DbC.
///
/// Reuses the mapped base in `crate::xhci::XHCI_STATE` (no BAR remap, no port
/// I/O here — BAR0 masking via `k_nano::pci` stays in the xhci driver). A call
/// with no bound HC (or a contended lock) reports absent *without* caching,
/// so a later post-bind call still probes. The first completed probe caches
/// permanently (detect-only skeleton: no hotplug tracking).
pub fn dbc_present() -> bool {
    if DBC_DONE.load(Ordering::Relaxed) {
        return DBC_HIT.load(Ordering::Relaxed);
    }
    let Some(hit) = try_probe_once() else {
        return false;
    };
    DBC_HIT.store(hit, Ordering::Relaxed);
    DBC_DONE.store(true, Ordering::Relaxed);
    hit
}

/// One probe attempt: `None` = nothing safe to read (don't cache).
fn try_probe_once() -> Option<bool> {
    let (phys, pmoff) = {
        let g = crate::xhci::XHCI_STATE.try_lock()?;
        let st = g.as_ref()?;
        if st.pmoff == 0 {
            return None;
        }
        (st.base.wrapping_sub(st.pmoff), st.pmoff)
    };
    Some(dbc_detect(phys, pmoff))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbc_walk_finds_0x0a_over_fake_buffer() {
        // Fake Cap space as dwords indexed by byte_off / 4.
        let mut fake = alloc::vec![0u32; 0x100];
        let put = |fake: &mut alloc::vec::Vec<u32>, byte_off: u64, id: u8, next: u8| {
            fake[(byte_off / 4) as usize] = (id as u32) | ((next as u32) << 8);
        };
        // Chain with DbC: 0x100(id=1,next=2 -> 0x108) -> 0x108(id=0x0A,next=0).
        put(&mut fake, 0x100, 0x01, 0x02);
        put(&mut fake, 0x108, DBC_CAP_ID, 0x00);
        let read = |off: u64| fake.get((off / 4) as usize).copied();
        assert!(dbc_scan(read, 0x100));
        // Same chain without DbC: 0x100(id=1,next=2) -> 0x108(id=2,next=0).
        put(&mut fake, 0x108, 0x02, 0x00);
        let read = |off: u64| fake.get((off / 4) as usize).copied();
        assert!(!dbc_scan(read, 0x100));
        // Degenerate inputs never report present.
        assert!(!dbc_scan(|_| None, 0x100));
        assert!(!dbc_scan(|_| Some(0x0A), 0x00));
    }
}
