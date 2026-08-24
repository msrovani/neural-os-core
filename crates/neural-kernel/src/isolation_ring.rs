//! Facade — isolation ring seam (R0 paging + R1 cap_gate + hermes wire).
//! Logic lives in k_nano::paging (R0 blob exec) + k_hal::cap_gate (R1 safe check).
//! Bin keeps only `register_native_ring` seam (no Cap privilege escalation static).
use k_hal::cap_gate::{ring3_is_safe, ring3_run_native as hal_run};

pub use k_hal::cap_gate::ring3_is_safe as ring3_is_safe_pub;

/// Called at boot. Registers native ring iff hypervisor is safe (KVM).
/// Otherwise wasmi (A) stays active — port-safe gating.
pub fn init_connectors() {
    if ring3_is_safe() {
        k_nano::slog_bin!("ISO-RING", "info", "Ring3 SAFE — registering native ring (B/C gated by HITL)");
        hermes_crate::app_factory::register_native_ring(ring3_run_native);
    } else {
        k_nano::slog_bin!("ISO-RING", "info", "Ring3 UNSAFE — native ring NOT registered; wasmi (A) active");
    }
}

/// Native execution entry — delegates to R0 paging for blob, ELF via bin loader.
pub fn ring3_run_native(code: &[u8], _caps: u32) -> Result<i64, &'static str> {
    if crate::elf_loader::ElfLoader::is_valid_elf(code) {
        let pid = crate::elf_loader::load_and_spawn(code, "sandbox")?;
        k_nano::slog_bin!("ISO-RING", "info", "ring3_run_native: ELF pid={}", pid);
        return match crate::user_mode::run_process(pid) { Ok(()) => Ok(0), Err(e) => Err(e) };
    }
    hal_run(code, _caps)
}
