//! Facade — Ring3 iretq/TSS.RSP0 lives in k_nano::paging (R0).
//! ADR-0041 §11: paging/TSS/IST in R0, CapGate in R1. No static Cap privilege escalation here.
pub use k_nano::paging::{
    demo_active, demo_ring3, demo_ring3_capgate_dma_mmio, demo_ring3_fault_containment,
    demo_ring3_softfloat_sse, enter_user_mode, fault_abort, mailbox_syscalls,
    note_sandbox_cap_deny, return_from_user, ring3_can_iretq, ring3_can_register_native,
    ring3_self_test_iretq, sandbox_dma_denies, sandbox_mmio_denies, sandbox_syscalls, Cap,
    RING3_MAGIC, TRY_ENTER_RING3, USER_CODE_VA, USER_MARKER_VA, USER_STACK_VA,
};
pub use k_nano::ring3::{ring3_mark_hw_gate_passed, USER_MAILBOX_VA};

pub use k_nano::paging::{SYS_EXIT_USER, SYS_MAP_FB, SYS_PIN_DMA};

pub fn run_process(pid: u64) -> Result<(), &'static str> {
    let (entry, stack_top, l4_frame) = {
        let pm = crate::process::PROCESS_MANAGER.lock();
        let p = pm.get(pid).ok_or("process: PID not found")?;
        (p.entry, p.stack_top, p.address_space.l4_frame)
    };
    {
        let mut pm = crate::process::PROCESS_MANAGER.lock();
        if let Some(p) = pm.get_mut(pid) {
            p.state = crate::process::ProcessState::Running;
        }
    }
    crate::interrupts_ext::switch_to_proc_tss((pid % 8) as usize);
    let result = unsafe {
        x86_64::instructions::interrupts::without_interrupts(|| {
            enter_user_mode(entry, stack_top, l4_frame, Cap::ENTER_USER)
        })
    };
    {
        let mut pm = crate::process::PROCESS_MANAGER.lock();
        if let Some(p) = pm.get_mut(pid) {
            p.state = crate::process::ProcessState::Exited(if result.is_ok() { 0 } else { 1 });
        }
    }
    result
}
