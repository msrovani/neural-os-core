//! ADR-0057 WS-D — GPU → cortex::compute (C1/C2 W2A8).
//!
//! Fluxo: pack W2A8 → KernelImage → layout device (`w2a8_device`) → stage upload
//! → despacho ISA (AWAITING_HW) → None → escada CPU.
//! Perfis: Dp4aW2A8 / MadInt8 / DotInt8; slog profile + fallback.

use crate::gpu::aios_adapt;
use crate::gpu::backend::{self, compute_state};
use crate::gpu::compute_abi::{BackendState, IsaTag, OpProfile};
use crate::gpu::detect::GpuVendor;
use crate::gpu::falcon3_w2a8;
use crate::gpu::kernel_image;
use crate::gpu::kernel_pack::{self, IrOrigin, PackOp};
use crate::gpu::w2a8_device;
use cortex::tensor::{PackedTernaryTensor, Tensor};
use k_nano::slog_hal;

fn vendor_for_isa(isa: IsaTag) -> Option<GpuVendor> {
    match isa {
        IsaTag::Sm52 | IsaTag::Sm61 | IsaTag::Sm70 | IsaTag::Sm75 | IsaTag::Sm80 | IsaTag::Sm86
        | IsaTag::Sm89 => {
            Some(GpuVendor::Nvidia)
        }
        IsaTag::Gen9 | IsaTag::Dg2 => Some(GpuVendor::Intel),
        IsaTag::Gfx90c | IsaTag::Gfx1030 | IsaTag::Gfx1036 | IsaTag::Gfx1103 => Some(GpuVendor::Amd),
        IsaTag::None => None,
    }
}

fn profile_ok(isa: IsaTag) -> OpProfile {
    if let Some(p) = aios_adapt::last_plan() {
        return p.op_profile;
    }
    crate::gpu::compute_abi::ComputeCaps::features_for_isa(isa).4
}

fn profile_name(p: OpProfile) -> &'static str {
    match p {
        OpProfile::ScalarInt8 => "scalar_int8",
        OpProfile::Dp4aW2A8 => "dp4a_w2a8",
        OpProfile::MadInt8 => "mad_int8",
        OpProfile::WmmaI8 => "wmma_i8",
        OpProfile::DotInt8 => "dot_int8",
    }
}

/// Tentativa de despacho device (C2 AWAITING_HW).
/// Preparação C1 completa; sem SASS/EU no blob → None (CPU ladder).
fn try_device_w2a8(
    w: &PackedTernaryTensor,
    x: &Tensor,
    isa: IsaTag,
    profile: OpProfile,
    img_regs: u32,
    img_code_len: usize,
) -> Option<Tensor> {
    let buf = w2a8_device::prepare_device_buffers(w, x, profile)?;
    if !w2a8_device::try_stage_upload(&buf) {
        slog_hal!(
            "COMPUTE",
            "warn",
            "W2A8 upload skip profile={} — fallback CPU",
            profile_name(profile)
        );
        return None;
    }
    // Sample host golden (verify path); device readback = metal residual.
    let _host = w2a8_device::host_gemv_signed(&buf)?;
    cortex::matmul_diag::note_gpu_disp_awaiting();
    slog_hal!(
        "COMPUTE",
        "warn",
        "W2A8 staged isa={} profile={} regs={} code={}B upload={}B — device dispatch AWAITING_HW fallback=CPU",
        isa.as_str(),
        profile_name(profile),
        img_regs,
        img_code_len,
        w2a8_device::upload_byte_len(&buf)
    );
    None
}

fn gpu_ternary(w: &PackedTernaryTensor, x: &Tensor) -> Option<Tensor> {
    let isa = backend::compute_isa_tag()?;
    let vendor = vendor_for_isa(isa)?;
    let pack = kernel_pack::find_active_pack(vendor, isa, PackOp::BitLinearW2A8)?;
    if !pack.verified {
        slog_hal!("COMPUTE", "trace", "W2A8 pack unsigned — fallback CPU");
        return None;
    }
    let img = kernel_image::from_pack(&pack)?;
    if img.is_stub || img.ir == IrOrigin::CpuStub {
        slog_hal!(
            "COMPUTE",
            "trace",
            "W2A8 KernelImage stub isa={} — fallback CPU (gere CUBIN/zebin/HSACO no host)",
            isa.as_str()
        );
        return None;
    }
    let (k, n) = w.shape;
    let (m, k2) = x.shape;
    if k != k2 || m == 0 {
        return None;
    }
    let shapes = falcon3_w2a8::falcon3_decode_gemv_shapes();
    let ok_shape = shapes.iter().any(|s| s.n as usize == n && s.k as usize == k)
        || (k >= 512 && n >= 512);
    if !ok_shape {
        slog_hal!(
            "COMPUTE",
            "warn",
            "W2A8 shape ({},{}) fora Falcon3 — fallback CPU",
            n,
            k
        );
        return None;
    }
    let profile = profile_ok(isa);
    try_device_w2a8(w, x, isa, profile, img.regs, img.code.len())
}

pub fn register_compute_if_ready() {
    if compute_state() == BackendState::Ready {
        cortex::compute::register_gpu_ternary(gpu_ternary);
        let dual = if backend::is_dual_gpu() { "dual" } else { "single" };
        let isa = backend::compute_isa_tag()
            .map(|i| i.as_str())
            .unwrap_or("none");
        let sku = aios_adapt::last_plan()
            .map(|p| p.sku.as_str())
            .unwrap_or_else(|| falcon3_w2a8::active_sku().as_str());
        let profile = backend::compute_isa_tag()
            .map(profile_ok)
            .map(profile_name)
            .unwrap_or("cpu");
        slog_hal!(
            "COMPUTE",
            "ok",
            "GPU Ready ({}) isa={} profile={} falcon3={} — ternary on; W2A8 device=AWAITING_HW until fence+golden",
            dual,
            isa,
            profile,
            sku
        );
        aios_adapt::remember_caps();
    } else {
        // ADR-0105 B1.3: QEMU/VirtIO/sem pack → CpuOnly honesto (nunca Ready falso).
        slog_hal!(
            "COMPUTE",
            "ok",
            "GPU CpuOnly/Quarantine ({:?}) — CPU W2A8 ladder se gaps; device=off",
            compute_state()
        );
    }
}
