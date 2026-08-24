//! Facade — logic lives in k_nano::paging (R0).
//! ADR-0041 §11: paging/TSS/IST in R0. Bin only wires via pub use.
pub use k_nano::paging::{
    alloc_frame, create_sandbox_as, hhdm_mut, install_present_leaf_current, kernel_cr3,
    restore_cr3, rw_flags, user_code_flags, user_data_flags, AddressSpace, MVP_REGION_BASE,
    PRIVATE_PAGE_VA, SHARED_RING_VA,
};
pub use k_nano::paging::{Cap, SYS_DEMAND_PAGE, SYS_MAP_FILE, SYS_MAP_WEIGHTS};

/// Demo retained as wire into k_hal/k_nano where possible; kept here for boot log.
pub fn demo_as_r1_r3_shallow() {
    k_nano::slog_bin!("AS", "r1", "demo shallow start (facade → k_nano::paging + k_hal::cap_gate)");
    let bar0 = match k_hal::cap_gate::hal_as_bar0() {
        Some(b) if b != 0 && b != 0xffff_ffff_ffff_ffff => b,
        _ => {
            let fallback = k_hal::discovery::device_tree().into_iter().find(|c| c.id.bar0 != 0).map(|c| c.id.bar0);
            match fallback { Some(b) => { k_hal::cap_gate::bind_hal_as(b); b } None => { k_nano::slog_bin!("AS", "r1", "skip — sem BAR"); return; } }
        }
    };
    let r3 = k_hal::cap_gate::check_map_bar(3, false);
    k_nano::slog_bin!("AS", "r3", "MAP_BAR {:?} (expect Deny)", r3);
    if k_hal::cap_gate::check_map_bar(1, true) != k_hal::cap_gate::CapResult::Allow { k_nano::slog_bin!("AS", "r1", "unexpected Deny"); return; }
    let (kernel_l4, kernel_flags) = kernel_cr3();
    let mut as_r1 = match AddressSpace::clone_current() { Ok(a) => a, Err(e) => { k_nano::slog_bin!("AS", "r1", "clone_current fail: {}", e); return; } };
    const AS_BAR_VA: u64 = MVP_REGION_BASE + 0x2000;
    let bar_frame = x86_64::structures::paging::PhysFrame::<x86_64::structures::paging::Size4KiB>::containing_address(x86_64::PhysAddr::new(bar0 & !0xFFF));
    let flags = rw_flags() | x86_64::structures::paging::PageTableFlags::NO_CACHE;
    let map_ok = unsafe { as_r1.map_page(x86_64::VirtAddr::new(AS_BAR_VA), bar_frame, flags) };
    match map_ok { Ok(()) => k_nano::slog_bin!("AS", "r1", "BAR {:#x} mapped @ {:#x} UC", bar0, AS_BAR_VA), Err(e) => k_nano::slog_bin!("AS", "r1", "map note: {} — continua", e), }
    unsafe { as_r1.activate(); }
    let touch = AS_BAR_VA as *const u32;
    let val = unsafe { core::ptr::read_volatile(touch) };
    k_nano::slog_bin!("AS", "r1", "touch BAR ok val={:#x}", val);
    unsafe { restore_cr3(kernel_l4, kernel_flags); }
    k_nano::slog_bin!("AS", "r1", "restore CR3 OK");
}
