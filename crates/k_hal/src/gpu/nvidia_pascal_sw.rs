//! Degrau ACR P2 — aplicação de `sw_*` pós-WPR/HS (ADR-0048).
//!
//! `sw_nonctx`: pares u32(addr,data) → MMIO BAR0 (bounds-checked).
//! `sw_bundle_init` / `sw_method_init` / `sw_ctx`: presença registrada;
//! apply completo exige FECS vivo (PresentNotApplied até silício).

use crate::gpu::firmware;
use crate::gpu::nvidia_pascal_acr::{AcrReport, AcrStage};
/// Limite de endereço MMIO relativo ao BAR0 (janela GR/falcon).
const MMIO_ADDR_MAX: u32 = 0x0100_0000;
/// Máximo de pares nonctx por boot (evita hang).
const NONCTX_MAX_PAIRS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwStatus {
    BlobsMissing,
    /// sw_nonctx escrito; bundle/method/ctx só catalogados.
    NonctxApplied,
    /// ACR não HsBooted — nonctx adiado ou parcial.
    Deferred,
    /// Blobs presentes mas formato inválido / não aplicados.
    PresentNotApplied,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct SwReport {
    pub status: SwStatus,
    pub nonctx_pairs: u32,
    pub bundle_present: bool,
    pub method_present: bool,
    pub ctx_present: bool,
}

impl SwReport {
    pub fn nonctx_ok(self) -> bool {
        self.status == SwStatus::NonctxApplied
    }
}

fn blob_len(name: &str) -> Option<usize> {
    firmware::load_firmware_file(name).map(|b| b.len())
}

/// Aplica sw_* conforme estágio ACR. Não-fatal.
pub unsafe fn apply_sw(mmio: u64, acr: &AcrReport) -> SwReport {
    let nonctx = firmware::load_firmware_file("sw_nonctx.bin");
    let bundle = blob_len("sw_bundle_init.bin");
    let method = blob_len("sw_method_init.bin");
    let ctx = blob_len("sw_ctx.bin");

    let bundle_present = bundle.is_some();
    let method_present = method.is_some();
    let ctx_present = ctx.is_some();

    let Some(nonctx) = nonctx else {
        k_nano::slog_hal!("NVIDIA", "SW", "BlobsMissing sw_nonctx (bundle={} method={} ctx={})",
            bundle_present,
            method_present,
            ctx_present);
        return SwReport {
            status: SwStatus::BlobsMissing,
            nonctx_pairs: 0,
            bundle_present,
            method_present,
            ctx_present,
        };
    };

    if nonctx.len() % 8 != 0 || nonctx.is_empty() {
        k_nano::slog_hal!("NVIDIA", "SW", "PresentNotApplied: sw_nonctx len={} (esperado múltiplo de 8)", nonctx.len());
        return SwReport {
            status: SwStatus::PresentNotApplied,
            nonctx_pairs: 0,
            bundle_present,
            method_present,
            ctx_present,
        };
    }

    // Sem HsBooted: ainda podemos tentar nonctx (MMIO GR) mas marcamos Deferred
    // se ACR nem chegou a WprBuilt.
    if !acr.wpr_ok() {
        k_nano::slog_hal!("NVIDIA", "SW", "Deferred — ACR stage={:?}; nonctx não escrito", acr.stage);
        return SwReport {
            status: SwStatus::Deferred,
            nonctx_pairs: 0,
            bundle_present,
            method_present,
            ctx_present,
        };
    }

    let pairs = (nonctx.len() / 8).min(NONCTX_MAX_PAIRS);
    let mut written = 0u32;
    let mut skipped = 0u32;
    for i in 0..pairs {
        let o = i * 8;
        let addr = u32::from_le_bytes([nonctx[o], nonctx[o + 1], nonctx[o + 2], nonctx[o + 3]]);
        let data = u32::from_le_bytes([
            nonctx[o + 4],
            nonctx[o + 5],
            nonctx[o + 6],
            nonctx[o + 7],
        ]);
        if addr == 0 || addr >= MMIO_ADDR_MAX || (addr & 3) != 0 {
            skipped += 1;
            continue;
        }
        core::ptr::write_volatile((mmio + addr as u64) as *mut u32, data);
        written += 1;
    }

    let defer_note = if acr.stage != AcrStage::HsBooted {
        " (ACR≠HsBooted — bundle/method/ctx PresentNotApplied)"
    } else {
        " (bundle/method/ctx PresentNotApplied até FECS mailbox)"
    };

    k_nano::slog_hal!("NVIDIA", "SW", "NonctxApplied pairs={} skipped={} bundle={}B method={}B ctx={}B{}",
        written,
        skipped,
        bundle.unwrap_or(0),
        method.unwrap_or(0),
        ctx.unwrap_or(0),
        defer_note);

    SwReport {
        status: SwStatus::NonctxApplied,
        nonctx_pairs: written,
        bundle_present,
        method_present,
        ctx_present,
    }
}
