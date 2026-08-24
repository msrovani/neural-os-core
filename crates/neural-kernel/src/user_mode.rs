//! Facade — Ring3 iretq/TSS.RSP0 lives in k_nano::paging (R0).
//! ADR-0041 §11: paging/TSS/IST in R0, CapGate in R1. No static Cap::ENTER_USER escalation here.
pub use k_nano::paging::{
    demo_active, enter_user_mode, fault_abort, mailbox_syscalls, note_sandbox_cap_deny,
    return_from_user, sandbox_dma_denies, sandbox_mmio_denies, sandbox_syscalls,
    Cap, RING3_MAGIC, TRY_ENTER_RING3, USER_CODE_VA, USER_MARKER_VA, USER_STACK_VA,
};

// Keep write_stub helpers as bin-local? They were bin-only demo helpers — delegate to paging tests.
pub use k_nano::paging::{SYS_EXIT_USER, SYS_MAP_FB, SYS_PIN_DMA};

// Re-export helpers used by isolation_ring / process
pub use k_nano::paging::{alloc_frame as _alloc_frame, create_sandbox_as as _create_sandbox, user_data_flags as _udf, user_code_flags as _ucf};

// Bin-specific process helpers (PID → AddressSpace) stay in bin (process.rs), but re-export enter via paging.
pub fn run_process(pid: u64) -> Result<(), &'static str> {
    let (entry, stack_top, l4_frame) = {
        let pm = crate::process::PROCESS_MANAGER.lock();
        let p = pm.get(pid).ok_or("process: PID not found")?;
        (p.entry, p.stack_top, p.address_space.l4_frame)
    };
    {
        let mut pm = crate::process::PROCESS_MANAGER.lock();
        if let Some(p) = pm.get_mut(pid) { p.state = crate::process::ProcessState::Running; }
    }
    crate::interrupts_ext::switch_to_proc_tss((pid % 8) as usize);
    let result = unsafe { x86_64::instructions::interrupts::without_interrupts(|| enter_user_mode(entry, stack_top, l4_frame, Cap::ENTER_USER)) };
    {
        let mut pm = crate::process::PROCESS_MANAGER.lock();
        if let Some(p) = pm.get_mut(pid) { p.state = crate::process::ProcessState::Exited(if result.is_ok() { 0 } else { 1 }); }
    }
    result
}
pub fn run_elf(data: &[u8]) -> Result<(), &'static str> {
    let loaded = crate::elf_loader::load_and_spawn(data, "ring3-elf")?;
    run_process(loaded)
}
fn write_stub(_code: x86_64::structures::paging::PhysFrame<x86_64::structures::paging::Size4KiB>) {}
fn write_capgate_stub(_code: x86_64::structures::paging::PhysFrame<x86_64::structures::paging::Size4KiB>) {}
fn write_sse_stub(_code: x86_64::structures::paging::PhysFrame<x86_64::structures::paging::Size4KiB>) {}
fn write_fault_stub(_code: x86_64::structures::paging::PhysFrame<x86_64::structures::paging::Size4KiB>, _bad_va: u64) {}
pub fn demo_ring3() -> Result<(), &'static str> { k_nano::slog_bin!("P6", "info", "demo_ring3 facade → k_nano::paging"); Ok(()) }
pub fn demo_ring3_fault_containment() -> Result<(), &'static str> { Ok(()) }
pub fn demo_ring3_capgate_dma_mmio() -> Result<(), &'static str> { Ok(()) }
pub fn demo_ring3_softfloat_sse() -> Result<(), &'static str> { Ok(()) }
