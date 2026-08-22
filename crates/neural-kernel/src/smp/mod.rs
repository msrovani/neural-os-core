//! SMP bring-up — ADR-0055; fonte única `k_nano::smp::init_smp` (map+flags+wake).

pub mod percpu;
pub mod trampoline;
pub mod spsc;
pub mod work_stealing;
pub mod parallel_matmul;

pub use k_nano::smp::AP_COUNT;

pub fn ap_entry_count() -> u64 {
    k_nano::smp::ap_entry_count()
}

pub unsafe fn init_smp() {
    crate::display::fb::boot_ckpt(22, "smp: k_nano");
    crate::display::fb::boot_ckpt(22, "smp: init start");
    k_nano::smp::init_smp();
    // granular: k_nano já fez map ok + tramp ok + antes wake (se hang, ver K22 no FB)
    crate::display::fb::boot_ckpt(22, "smp: map ok");
    crate::display::fb::boot_ckpt(23, "smp: sipi done");
    crate::display::fb::boot_ckpt(23, "smp: init done");
}
