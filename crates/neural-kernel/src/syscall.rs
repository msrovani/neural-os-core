//! Facade — syscall/Cap dispatch lives in k_hal::cap_gate (R1) + k_nano::paging (R0).
//! k_nano R0 = paging/TSS/IST, k_hal R1 = CapGate/HalOffer with int 0x90 (ADR-0041 §11).
//! No duplicate Cap definition; canonical is k_nano::paging::Cap re-exported by k_hal.

pub use k_hal::cap_gate::{
    dispatch, init_syscall_fast_path, ping_count, soft_syscall, stage_syscall,
    syscall_int_handler, Cap, RING_OP_READ, RING_OP_WRITE, SYSCALL_VECTOR, SYS_DEMAND_PAGE,
    SYS_EXIT_USER, SYS_MAP_DMA, SYS_MAP_FB, SYS_MAP_FILE, SYS_MAP_WEIGHTS, SYS_PIN_DMA,
    SYS_PING, SYS_PRESENT_FB, SYS_RING_OP,
};
