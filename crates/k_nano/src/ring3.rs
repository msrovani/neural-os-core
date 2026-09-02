//! Ring3 isolation helpers (ADR-0102): opcode gate T-056, syscall mailbox, HW register gate, fault telemetry.

use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};

use event_bus::{CapabilityToken, Event};

use crate::EVENT_BUS;

/// Maintainer/HITL sets after T-053 HW checklist (ADR-0077 §6).
static HW_GATE_PASSED: AtomicBool = AtomicBool::new(false);

/// Página USER RW para ABI syscall (N4) — handler lê struct, não statics de kernel.
pub const USER_MAILBOX_VA: u64 = 0x0000_7000_0030_2000;

/// Layout canônico da mailbox USER (48 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SyscallMailbox {
    pub nr: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub cap: u64,
    pub result: u64,
    pub status: u64,
}

/// Marca T-053 passado (chamado explicitamente após checklist HW).
pub fn ring3_mark_hw_gate_passed() {
    HW_GATE_PASSED.store(true, Ordering::Release);
    crate::slog_nano!("ISO-RING", "ok", "Ring3 HW gate T-053 marked PASS");
}

/// Predicado separado de `ring3_is_safe` / `ring3_can_iretq` (ADR-0102 H3).
pub fn ring3_can_register_native() -> bool {
    HW_GATE_PASSED.load(Ordering::Acquire)
        && crate::paging::ring3_can_iretq()
        && crate::platform_probe::probe_done()
        && matches!(
            crate::platform_probe::hypervisor(),
            crate::platform_probe::HypervisorKind::None
        )
}

/// T-056 opção A: rejeita SSE/AVX/EVEX antes do `iretq`.
pub fn verify_blob_no_simd(code: &[u8]) -> Result<(), &'static str> {
    let mut i = 0usize;
    while i < code.len() {
        match code[i] {
            0xC4 | 0xC5 | 0x62 => return Err("ring3: VEX/EVEX denied (T-056)"),
            0x0F if i + 1 < code.len() => {
                let op2 = code[i + 1];
                if (0x57..=0x6F).contains(&op2) || matches!(op2, 0x76 | 0xAE | 0xAF) {
                    return Err("ring3: SSE opcode denied (T-056)");
                }
                i += 2;
            }
            0xF3 | 0xF2 | 0x66 if i + 2 < code.len() && code[i + 1] == 0x0F => {
                let op2 = code[i + 2];
                if (0x10..=0xEF).contains(&op2) {
                    return Err("ring3: SIMD prefix opcode denied (T-056)");
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    Ok(())
}

/// Lê mailbox no VA user (handler int 0x90 com CR3 sandbox).
#[inline]
pub unsafe fn read_user_mailbox() -> SyscallMailbox {
    core::ptr::read_volatile(USER_MAILBOX_VA as *const SyscallMailbox)
}

/// Escreve result/status na mailbox USER.
#[inline]
pub unsafe fn write_user_mailbox_result(result: u64, status: u64) {
    let m = USER_MAILBOX_VA as *mut SyscallMailbox;
    (*m).result = result;
    (*m).status = status;
}

/// Publica falha de sandbox no EventBus (Hermes / SelfHeal).
pub fn publish_sandbox_fault(reason: &'static str) {
    let payload = alloc::format!("HEALTH_ISSUE:ring3:sandbox_fault:{reason}").into_bytes();
    let _ = EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from("HEALTH_ISSUE"),
        payload,
        token: CapabilityToken::Legacy(1),
    });
}

/// T-051: classifica #GP para separar firmware/OVMF vs kernel vs Ring3 user.
pub fn gp_fault_class(ip: u64, cs: u64) -> &'static str {
    if cs & 3 == 3 {
        return "ring3_user";
    }
    if ip >= 0xffff_ffff_8000_0000 {
        return "kernel";
    }
    if ip < 0x0010_0000 {
        return "firmware_ovmf";
    }
    "unknown"
}

/// T-051: true se o #GP provavelmente veio do firmware UEFI/OVMF (não sandbox Ring3).
pub fn gp_likely_firmware(ip: u64, cs: u64) -> bool {
    gp_fault_class(ip, cs) == "firmware_ovmf"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_rejects_xorps() {
        let code = [0x0F, 0x57, 0xC0, 0xC3];
        assert!(verify_blob_no_simd(&code).is_err());
    }

    #[test]
    fn verify_allows_mov_ret() {
        let code = [0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];
        assert!(verify_blob_no_simd(&code).is_ok());
    }

    #[test]
    fn can_register_false_without_hw_gate() {
        assert!(!ring3_can_register_native());
    }

    #[test]
    fn gp_classifies_firmware_low_ip() {
        assert_eq!(gp_fault_class(0x8000, 0x08), "firmware_ovmf");
    }

    #[test]
    fn gp_classifies_ring3_user() {
        assert_eq!(gp_fault_class(0x7000_0030_0000, 0x1B), "ring3_user");
    }

    #[test]
    fn gp_classifies_kernel_high_ip() {
        assert_eq!(gp_fault_class(0xffff_ffff_8010_0000, 0x08), "kernel");
    }

    #[test]
    fn mailbox_layout_48_bytes() {
        assert_eq!(core::mem::size_of::<SyscallMailbox>(), 48);
    }
}
