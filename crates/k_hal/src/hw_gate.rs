//! HW-GATE — residuals AWAITING_REAL_HW scaneáveis no serial.
//! Formato: `id=… status=AWAITING_REAL_HW|CLOSED need=… pass_when=… grep=…`
//! Ler: `rg "HW-GATE"` no log do Note/QEMU. Sem claim PASS inventado.
//! Labor 7: `GPU_CANARY_GOLDEN` → CLOSED se CapToken::GpuCompute.
//! Labor 8: `BOOT_011_SMOKE` → CLOSED se CapToken::BootSmokeOk; emit early + idempotente.

use core::sync::atomic::{AtomicBool, Ordering};

/// Um gate de validação em hardware real (Note ou placa dedicada).
pub struct HwGate {
    pub id: &'static str,
    pub need: &'static str,
    pub pass_when: &'static str,
    pub grep: &'static str,
}

/// Catálogo pós Labor 4–8 — residual ou CLOSED.
pub const GATES: &[HwGate] = &[
    HwGate {
        id: "ATH10K_A5_SCAN",
        need: "WMI scan SSID do ar (QCA6174)",
        pass_when: "step=scan RF + SSIDs reais",
        grep: "ATH10K|WIFI-HW",
    },
    HwGate {
        id: "ATH10K_ASSOC",
        need: "assoc/WPA2 link up",
        pass_when: "WifiState Connected + smoltcp wifi",
        grep: "WIFI-HW",
    },
    HwGate {
        id: "GPU_CANARY_GOLDEN",
        need: "vector_add golden LegacyAcr (Note GTX1050=teste)",
        pass_when: "VERDICT=PASS + CapToken GpuCompute",
        grep: "GPU-HW",
    },
    HwGate {
        id: "GPU_GSP",
        need: "GspBackend Turing+ canario",
        pass_when: "VERDICT=PASS family=turing|ampere|ada",
        grep: "GPU-HW",
    },
    HwGate {
        id: "I225_E2E",
        need: "igc TX/RX real (QEMU sem emu)",
        pass_when: "DHCP/HTTP via i225",
        grep: "I225|NET",
    },
    HwGate {
        id: "USB_HUB",
        need: "xHCI hub TT/route",
        pass_when: "hub enumerate (nao hub=AWAITING)",
        grep: "xHCI|USB",
    },
    HwGate {
        id: "BOOT_011_SMOKE",
        need: "boot past MemoryCore (Limine preferido; 0.11 se chegar)",
        pass_when: "BootSmokeOk + step=smoke",
        grep: "bootloader|Boot",
    },
];

static EMITTED: AtomicBool = AtomicBool::new(false);

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
    if id == "USB_HUB" && (k_nano::xhci::hub_ok() || k_nano::xhci::hub_child_ok()) {
        return "CLOSED";
    }
    "AWAITING_REAL_HW"
}

/// Marca smoke OK (MemoryCore) + slog `step=smoke`.
pub fn mark_boot_smoke(boot_path: &str) {
    crate::unlock_dag::grant(crate::unlock_dag::CapToken::BootSmokeOk);
    k_nano::slog_bin!(
        "Boot",
        "info",
        "step=smoke status=OK boot={} detail=MemoryCore",
        boot_path
    );
    k_nano::slog_bin!(
        "HW-GATE",
        "info",
        "step=smoke status=OK boot={} CapToken=BootSmokeOk",
        boot_path
    );
}

/// Emite o bloco completo (QEMU e Note). **Idempotente** — 2ª chamada (WifiAgent) é no-op.
/// Early: pós-MemoryCore; late: pós-WifiAgent (GPU CLOSED pode atualizar se re-emitíssemos —
/// L8 preferiu early para Limine; late skip se já emitido).
pub fn emit_all() {
    if EMITTED.swap(true, Ordering::Relaxed) {
        k_nano::slog_bin!("HW-GATE", "info", "emit_all skip=already_emitted");
        return;
    }
    k_nano::slog_bin!(
        "HW-GATE",
        "info",
        "begin count={} (rg HW-GATE no serial)",
        GATES.len()
    );
    for g in GATES {
        let status = gate_status(g.id);
        k_nano::slog_bin!(
            "HW-GATE",
            "await",
            "id={} status={} need={} pass_when={} grep={}",
            g.id,
            status,
            g.need,
            g.pass_when,
            g.grep
        );
    }
    k_nano::slog_bin!("HW-GATE", "info", "end");
}

/// Force re-emit (ex.: após GpuCompute late) — raramente necessário.
pub fn emit_all_refresh() {
    EMITTED.store(false, Ordering::Relaxed);
    emit_all();
}
