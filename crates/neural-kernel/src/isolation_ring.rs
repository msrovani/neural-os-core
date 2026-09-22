//! Facade — isolation ring seam (R0 paging + R1 cap_gate + hermes wire).
//! Logic lives in k_nano::paging (R0 blob exec) + k_hal::cap_gate (R1 safe check).
//! Bin keeps only `register_native_ring` seam (no Cap privilege escalation static).
use k_hal::cap_gate::{ring3_is_safe, ring3_run_native as hal_run};

pub use k_hal::cap_gate::ring3_is_safe as ring3_is_safe_pub;

/// Called at boot. Stash runner sempre; registra só com T-053 HW + metal + can_iretq.
/// TCG/WHPX nunca registram — wasmi (A) permanece default.
pub fn init_connectors() {
    let can_iretq = k_nano::paging::ring3_can_iretq();
    let can_reg = k_nano::paging::ring3_can_register_native();
    hermes_crate::app_factory::stash_native_runner(ring3_run_native);
    if can_reg {
        k_nano::slog_bin!("ISO-RING", "ok", "register_native_ring (T-053 HW + can_iretq)");
        hermes_crate::app_factory::register_native_ring(ring3_run_native);
    } else {
        k_nano::slog_bin!(
            "ISO-RING",
            "ok",
            "Ring3 gated (safe={} can_iretq={} can_reg={}) — wasmi (A); stash HITL /ring3 approve",
            ring3_is_safe(),
            can_iretq,
            can_reg
        );
    }
    k_nano::ring3::slog_onda6_boot_status();
}

/// Native execution entry — ELF64 ou blob JIT em sandbox CPL=3.
pub fn ring3_run_native(code: &[u8], caps: u32) -> Result<i64, &'static str> {
    if crate::elf_loader::ElfLoader::is_valid_elf(code) {
        // H10: path ELF entra em CPL=3 via load_and_spawn → run_process —
        // exige ENTER_USER explícito, nunca gate implícito só por parsing.
        if caps as u64 & k_nano::paging::Cap::ENTER_USER.bits() == 0 {
            k_nano::slog_bin!(
                "CapGate",
                "ok",
                "DENY ELF ring3 sem ENTER_USER caps=0x{:x}",
                caps
            );
            return Err("EPERM: Cap::ENTER_USER (ELF)");
        }
        let pid = crate::elf_loader::load_and_spawn(code, "sandbox")?;
        crate::user_mode::run_process(pid)?;
        // Honesty: não inventar Ok(0) sem ler estado — Exited(code) do process manager.
        let exit = {
            let pm = crate::process::PROCESS_MANAGER.lock();
            match pm.get(pid).map(|p| &p.state) {
                Some(crate::process::ProcessState::Exited(c)) => *c as i64,
                Some(_) => {
                    return Err("ring3: ELF process not Exited after run_process");
                }
                None => return Err("ring3: ELF pid lost after run_process"),
            }
        };
        return Ok(exit);
    }
    hal_run(code, caps)
}
