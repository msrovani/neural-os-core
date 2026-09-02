//! Facade — isolation ring seam (R0 paging + R1 cap_gate + hermes wire).
//! Logic lives in k_nano::paging (R0 blob exec) + k_hal::cap_gate (R1 safe check).
//! Bin keeps only `register_native_ring` seam (no Cap privilege escalation static).
use k_hal::cap_gate::{ring3_is_safe, ring3_run_native as hal_run};

pub use k_hal::cap_gate::ring3_is_safe as ring3_is_safe_pub;

/// Called at boot. Registers native ring iff T-053 HW gate + metal + can_iretq.
/// TCG/WHPX nunca registram — wasmi (A) permanece default.
pub fn init_connectors() {
    let can_iretq = k_nano::paging::ring3_can_iretq();
    let can_reg = k_nano::paging::ring3_can_register_native();
    if can_reg {
        k_nano::slog_bin!("ISO-RING", "ok", "register_native_ring (T-053 HW + can_iretq)");
        hermes_crate::app_factory::register_native_ring(ring3_run_native);
    } else {
        k_nano::slog_bin!(
            "ISO-RING",
            "info",
            "Ring3 gated (safe={} can_iretq={} can_reg={}) — wasmi (A) active",
            ring3_is_safe(),
            can_iretq,
            can_reg
        );
    }
}

/// Native execution entry — ELF64 ou blob JIT em sandbox CPL=3.
pub fn ring3_run_native(code: &[u8], caps: u32) -> Result<i64, &'static str> {
    if crate::elf_loader::ElfLoader::is_valid_elf(code) {
        let pid = crate::elf_loader::load_and_spawn(code, "sandbox")?;
        crate::user_mode::run_process(pid)?;
        let _ = caps;
        return Ok(0);
    }
    hal_run(code, caps)
}
