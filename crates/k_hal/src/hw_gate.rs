//! HW-GATE — residuals AWAITING_REAL_HW scaneáveis no serial.
//! Formato: `id=… status=AWAITING_REAL_HW|PARTIAL|CLOSED need=… pass_when=… grep=…`
//! Ler: `rg "HW-GATE"` no log metal/QEMU. Sem claim PASS inventado.
//!
//! SESSION_497 (metal dual-boot, `logs/BOOT_esp_now.txt`):
//! - CONFIRMADO: Limine smoke CLOSED; USB-MSC root live (PARTIAL em USB_HUB);
//!   NeuralFS mount; orb/UI; smp_online=16.
//! - NEGADO / ainda aberto: ath10k scan/assoc; GPU canary/GSP (bar0_validate_failed);
//!   i225 e2e; hub TT/route (`no_child_ccs`); NET rx; UAC.

use core::sync::atomic::{AtomicBool, Ordering};

/// Um gate de validação em hardware real (Note ou placa dedicada).
pub struct HwGate {
    pub id: &'static str,
    pub need: &'static str,
    pub pass_when: &'static str,
    pub grep: &'static str,
}

/// Catálogo pós Labor 4–8 — residual, PARTIAL (prova incompleta) ou CLOSED.
pub const GATES: &[HwGate] = &[
    HwGate {
        id: "ATH10K_A5_SCAN",
        need: "WMI scan SSID do ar (QCA6174) — metal s497: WIFI-HW SKIP no_WifiAssociated",
        pass_when: "step=scan RF + SSIDs reais",
        grep: "ATH10K|WIFI-HW",
    },
    HwGate {
        id: "ATH10K_ASSOC",
        need: "assoc/WPA2 link up — metal s497: sem WifiAssociated",
        pass_when: "WifiState Connected + smoltcp wifi",
        grep: "WIFI-HW",
    },
    HwGate {
        id: "GPU_CANARY_GOLDEN",
        need: "vector_add golden LegacyAcr — metal s497: GPU-HW bar0_validate_failed",
        pass_when: "VERDICT=PASS + CapToken GpuCompute",
        grep: "GPU-HW",
    },
    HwGate {
        id: "GPU_GSP",
        need: "GspBackend Ampere canario — s498: nvidia_gsp D0-D5 wired; metal Alive/Compute AWAITING reflash",
        pass_when: "VERDICT=PASS family=turing|ampere|ada",
        grep: "GPU-HW",
    },
    HwGate {
        id: "I225_E2E",
        need: "igc TX/RX real (QEMU sem emu) — metal s497: sem prova i225",
        pass_when: "DHCP/HTTP via i225",
        grep: "I225|NET",
    },
    HwGate {
        id: "USB_HUB",
        need: "xHCI hub TT/route — metal s497: MSC root OK; hub child no_child_ccs",
        pass_when: "hub enumerate (CLOSED) | MSC live (PARTIAL)",
        grep: "xHCI|USB",
    },
    HwGate {
        id: "BOOT_011_SMOKE",
        need: "boot past MemoryCore (Limine) — metal s497: CLOSED CapToken BootSmokeOk",
        pass_when: "BootSmokeOk + step=smoke",
        grep: "bootloader|Boot",
    },
];

static EMITTED: AtomicBool = AtomicBool::new(false);

fn usb_msc_live() -> bool {
    k_nano::globals::USB_MSC
        .try_lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

/// Status honesto por evidência de runtime (CapToken / xHCI / MSC).
fn gate_status(id: &str) -> &'static str {
    if id == "GPU_CANARY_GOLDEN"
        && crate::unlock_dag::has(crate::unlock_dag::CapToken::GpuCompute)
    {
        return "CLOSED";
    }
    if id == "BOOT_011_SMOKE"
        && crate::unlock_dag::has(crate::unlock_dag::CapToken::BootSmokeOk)
    {
        return "CLOSED";
    }
    if id == "ATH10K_ASSOC"
        && crate::unlock_dag::has(crate::unlock_dag::CapToken::WifiAssociated)
    {
        return "CLOSED";
    }
    if id == "USB_HUB" {
        if k_nano::xhci::hub_ok() || k_nano::xhci::hub_child_ok() {
            return "CLOSED";
        }
        // Metal: stick no root port → MSC live ≠ hub TT/route (SESSION_497).
        if usb_msc_live() {
            return "PARTIAL";
        }
        return "AWAITING_REAL_HW";
    }
    "AWAITING_REAL_HW"
}

fn status_sev(status: &str) -> &'static str {
    match status {
        "CLOSED" => "ok",
        "PARTIAL" => "warn",
        _ => "ok", // AWAITING deve aparecer no dmesg (nao Trace)
    }
}

/// Marca smoke OK (MemoryCore) + slog `step=smoke`.
pub fn mark_boot_smoke(boot_path: &str) {
    crate::unlock_dag::grant(crate::unlock_dag::CapToken::BootSmokeOk);
    k_nano::slog_bin!(
        "Boot",
        "ok",
        "step=smoke status=OK boot={} detail=MemoryCore",
        boot_path
    );
    k_nano::slog_bin!(
        "HW-GATE",
        "ok",
        "step=smoke status=OK boot={} CapToken=BootSmokeOk",
        boot_path
    );
}

/// Emite o bloco completo (QEMU e Note). **Idempotente** — 2ª chamada (WifiAgent) é no-op.
/// Early: pós-MemoryCore; late: `emit_all_refresh` pós-MSC/GPU CapTokens.
pub fn emit_all() {
    if EMITTED.swap(true, Ordering::Relaxed) {
        k_nano::slog_bin!("HW-GATE", "ok", "emit_all skip=already_emitted");
        return;
    }
    k_nano::slog_bin!(
        "HW-GATE",
        "ok",
        "begin count={} (rg HW-GATE no serial)",
        GATES.len()
    );
    for g in GATES {
        let status = gate_status(g.id);
        let sev = status_sev(status);
        k_nano::slog_bin!(
            "HW-GATE",
            sev,
            "id={} status={} need={} pass_when={} grep={}",
            g.id,
            status,
            g.need,
            g.pass_when,
            g.grep
        );
    }
    k_nano::slog_bin!("HW-GATE", "ok", "end");
}

/// Force re-emit (pós-MSC / GpuCompute) — atualiza PARTIAL/CLOSED.
pub fn emit_all_refresh() {
    EMITTED.store(false, Ordering::Relaxed);
    emit_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_seven_gates() {
        assert_eq!(GATES.len(), 7);
        assert_eq!(GATES[6].id, "BOOT_011_SMOKE");
        assert_eq!(GATES[5].id, "USB_HUB");
    }

    #[test]
    fn status_sev_maps() {
        assert_eq!(status_sev("CLOSED"), "ok");
        assert_eq!(status_sev("PARTIAL"), "warn");
        assert_eq!(status_sev("AWAITING_REAL_HW"), "ok");
    }
}
