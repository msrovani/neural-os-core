//! ADR-0057 WS-E — NPU (Neural Processing Unit): AMD XDNA / Intel NPU.
//!
//! **Camada Sponsor / firmware (Layer S).** O NPU é tratado como acelerador
//! **opcional** para o Ring 0 (intent routing / Trinity router). A premissa
//! arquitetural (IDEA #51) é que o Ring 0 roda em **software na CPU** sem NPU —
//! logo, na ausência de NPU (ou de firmware/overlay), o fallback é `Software` e
//! nada é registrado no dispatcher (`cortex::compute`).
//!
//! O que É implementado aqui (testável em QEMU): detecção por PCI + veredito
//! honesto + fallback software. O que é **Layer S / firmware** (requer HW real
//! + toolchain fechada, não testável sem patrocínio): a fila de comando +
//! doorbell + MSI-X + carga de overlay (AMD Vitis AI / Intel NCE) e o kernel de
//! matmul no NPU. Esses pontos estão marcados `LAYER-S` abaixo.

use k_nano::slog_hal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpuVendor {
    AmdXdna,
    IntelNpu,
    None,
}

/// ADR-0044: acelerador selecionado para o Ring 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accelerator {
    /// AMD XDNA (Ryzen AI). LAYER-S: firmware + overlay Vitis fechados.
    Xdna,
    /// Intel NPU (Meteor/Lunar Lake, LEON RT + NCE tiles). LAYER-S: firmware.
    IntelNpu,
    /// Fallback: MLP do Ring 0 roda inline na CPU (premissa IDEA #51).
    Software,
}

/// Detecção por PCI (VID/DID). IDs conhecidos:
/// - AMD XDNA: 0x1022:0x1502 (Phoenix), 0x1022:0x17F0 (Strix) — família Ryzen AI.
/// - Intel NPU: 0x8086:0x7D1D (Meteor Lake), 0x8086:0x643E (Lunar Lake).
pub fn detect_npu() -> NpuVendor {
    for dev in unsafe { k_nano::pci::scan_pci() } {
        match (dev.vendor_id, dev.device_id) {
            // AMD XDNA (Ryzen AI) — família Phoenix/Strix.
            (0x1022, 0x1502) | (0x1022, 0x17F0) => return NpuVendor::AmdXdna,
            // Intel NPU — Meteor Lake / Lunar Lake.
            (0x8086, 0x7D1D) | (0x8086, 0x643E) => return NpuVendor::IntelNpu,
            _ => {}
        }
    }
    NpuVendor::None
}

/// Estado global do acelerador de Ring 0 (para telemetria / `/cpuinfo`).
pub fn accelerator() -> Accelerator {
    match detect_npu() {
        // HW detectado, mas init real depende de firmware/overlay (LAYER-S).
        NpuVendor::AmdXdna | NpuVendor::IntelNpu => Accelerator::Software,
        NpuVendor::None => Accelerator::Software,
    }
}

/// Bring-up de boot (ADR-0057 WS-E). Detecta, emite veredito honesto e cai no
/// fallback software (não registra backend no dispatcher — Ring 0 na CPU).
pub fn init_npu() {
    match detect_npu() {
        NpuVendor::AmdXdna => {
            slog_hal!("NPU", "info", "AMD XDNA detectado (PCI)");
            // LAYER-S: try_init_xdna() exige firmware Ryzen AI + overlay Vitis
            // (blobs fechados) + fila/doorbell/MSI-X. Sem eles → software.
            slog_hal!(
                "NPU-HW",
                "info",
                "VERDICT=AWAITING_REAL_HW reason=xdna_firmware_overlay_layer_s"
            );
        }
        NpuVendor::IntelNpu => {
            slog_hal!("NPU", "info", "Intel NPU detectado (PCI)");
            // LAYER-S: firmware NCE + fila de comando. Sem eles → software.
            slog_hal!(
                "NPU-HW",
                "info",
                "VERDICT=AWAITING_REAL_HW reason=intel_npu_firmware_layer_s"
            );
        }
        NpuVendor::None => {
            // Sem NPU: Ring 0 MLP roda na CPU (premissa IDEA #51). Honesto.
            slog_hal!(
                "NPU-HW",
                "info",
                "VERDICT=SOFTWARE reason=no_npu ring0=cpu_mlp"
            );
        }
    }
    // Nada é registrado em `cortex::compute::register_npu_ternary` até um NPU
    // real + firmware passarem um golden (LAYER-S).
}
