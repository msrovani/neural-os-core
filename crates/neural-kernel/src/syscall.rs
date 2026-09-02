//! Facade — syscall/Cap dispatch lives in k_hal::cap_gate (R1) + k_nano::paging (R0).
//! Handler int 0x90 fica no bin (mailbox USER + reg fallback SESSION_278).

use x86_64::structures::idt::InterruptStackFrame;

pub use k_hal::cap_gate::{
    dispatch, init_syscall_fast_path, ping_count, soft_syscall, stage_syscall, Cap,
    RING_OP_READ, RING_OP_WRITE, SYSCALL_VECTOR, SYS_DEMAND_PAGE, SYS_EXIT_USER, SYS_MAP_DMA,
    SYS_MAP_FB, SYS_MAP_FILE, SYS_MAP_WEIGHTS, SYS_PIN_DMA, SYS_PING, SYS_PRESENT_FB, SYS_RING_OP,
};

pub extern "x86-interrupt" fn syscall_int_handler(_stack: InterruptStackFrame) {
    if k_nano::paging::mailbox_syscalls() {
        k_nano::paging::syscall_stage_from_mailbox(0);
    } else {
        k_nano::paging::syscall_try_regs_fallback();
    }

    let (nr, arg, cap_bits) = k_nano::paging::syscall_staged();
    let cap = Cap::from_bits(cap_bits);

    if nr == SYS_EXIT_USER && crate::user_mode::demo_active() {
        match dispatch(nr, arg, cap) {
            Ok(v) => {
                k_nano::paging::syscall_finish_ok(v);
                crate::user_mode::return_from_user(true);
            }
            Err(_) => {
                k_nano::paging::syscall_finish_err();
                crate::user_mode::return_from_user(false);
            }
        }
    }

    match dispatch(nr, arg, cap) {
        Ok(v) => k_nano::paging::syscall_finish_ok(v),
        Err(_) => k_nano::paging::syscall_finish_err(),
    }
}
