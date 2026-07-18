//! Compute ABI compartilhado — host↔device POD, fences, caps honestas (ADR-0048/49/50).
//! Sem trait cross-vendor até ≥2 backends passarem canário vector_add.

use crate::gpu::detect::{GpuArch, GpuVendor};

/// Backend selecionado após probe de família (não após canário).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeBackendKind {
    LegacyAcr,
    Gsp,
    Gen9Ring,
    XeHpgCcs,
    KiQ,
    Mes,
    VirtIoDisplay,
    CpuFallback,
    Unknown,
}

/// Estado do caminho de compute após bring-up / canário.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendState {
    /// Detectado; BARs ok; compute ainda não validado.
    Probed,
    /// Firmware + channel mínimos ok; canário ainda não rodou.
    BringingUp,
    /// vector_add golden passou — `has_compute` pode ser true.
    Ready,
    /// Falha honesta; usar CPU; display intocado.
    Quarantine,
    CpuOnly,
}

/// Submission Intel (ADR-0050).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntelSubmission {
    None,
    Ring,
    GuC,
}

/// Capabilities publicadas no boot. `has_compute` só true após golden.
#[derive(Debug, Clone, Copy)]
pub struct ComputeCaps {
    pub vendor: GpuVendor,
    pub arch: GpuArch,
    pub backend: ComputeBackendKind,
    pub state: BackendState,
    pub has_compute: bool,
    pub has_display: bool,
    pub is_integrated: bool,
    pub vram_bytes: u64,
    pub isa_tag: IsaTag,
    /// CCS fused (Arc/Xe2); Gen9 = false.
    pub has_ccs: bool,
    pub intel_submission: IntelSubmission,
    /// Host MAD/INT8 golden ok (≠ GPU Ready).
    pub mad_int8_host: bool,
}

impl ComputeCaps {
    pub const fn cpu_only() -> Self {
        ComputeCaps {
            vendor: GpuVendor::Unknown,
            arch: GpuArch::Unknown,
            backend: ComputeBackendKind::CpuFallback,
            state: BackendState::CpuOnly,
            has_compute: false,
            has_display: false,
            is_integrated: false,
            vram_bytes: 0,
            isa_tag: IsaTag::None,
            has_ccs: false,
            intel_submission: IntelSubmission::None,
            mad_int8_host: false,
        }
    }
}

/// Tag de ISA do KernelPack / canário.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IsaTag {
    None = 0,
    Sm61 = 1,
    Sm75 = 2,
    Sm89 = 3,
    Gfx90c = 4,
    Gfx1036 = 5,
    Gfx1103 = 6,
    Gfx1030 = 7,
    Gen9 = 8,
    Dg2 = 9,
}

impl IsaTag {
    pub fn as_str(self) -> &'static str {
        match self {
            IsaTag::None => "none",
            IsaTag::Sm61 => "sm_61",
            IsaTag::Sm75 => "sm_75",
            IsaTag::Sm89 => "sm_89",
            IsaTag::Gfx90c => "gfx90c",
            IsaTag::Gfx1036 => "gfx1036",
            IsaTag::Gfx1103 => "gfx1103",
            IsaTag::Gfx1030 => "gfx1030",
            IsaTag::Gen9 => "gen9",
            IsaTag::Dg2 => "dg2",
        }
    }
}

/// Identificador de golden canário.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum GoldenId {
    VectorAdd = 1,
    BitLinearW2A8 = 2,
}

/// Handle opaco de buffer GPU (não expõe `&mut [T]` na ABI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct GpuBufferHandle {
    pub pa: u64,
    pub len: u64,
    pub align: u32,
    pub flags: u32,
}

impl GpuBufferHandle {
    pub const FLAG_DEVICE: u32 = 1;
    pub const FLAG_HOST_VISIBLE: u32 = 2;

    pub fn validate_launch(&self) -> bool {
        self.len > 0 && self.align.is_power_of_two() && self.pa % self.align as u64 == 0
    }
}

/// Fence ID — buffer imutável até complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct FenceId(pub u64);

/// Ops tensores roteáveis (Cortex → work_queue → backend Ready).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensorOp {
    Nop,
    VectorAdd,
    MatmulTernary,
    BitLinearW2A8,
    Fence,
}

/// Job de compute com contrato de launch.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ComputeJob {
    pub op: TensorOp,
    pub golden: GoldenId,
    pub isa: IsaTag,
    pub fence: FenceId,
    pub workgroup_x: u32,
    pub workgroup_y: u32,
    pub workgroup_z: u32,
    pub shared_mem_bytes: u32,
    pub a: GpuBufferHandle,
    pub b: GpuBufferHandle,
    pub c: GpuBufferHandle,
}

impl ComputeJob {
    /// Valida handles/workgroup antes de qualquer MMIO de dispatch.
    pub fn validate(&self) -> bool {
        if self.workgroup_x == 0 || self.workgroup_y == 0 || self.workgroup_z == 0 {
            return false;
        }
        match self.op {
            TensorOp::Nop | TensorOp::Fence => true,
            TensorOp::VectorAdd | TensorOp::MatmulTernary | TensorOp::BitLinearW2A8 => {
                self.a.validate_launch()
                    && self.b.validate_launch()
                    && self.c.validate_launch()
                    && self.isa != IsaTag::None
            }
        }
    }
}

/// Parâmetros POD canário vector_add (CPU golden + device).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct VectorAddParams {
    pub n: u32,
    pub _pad: u32,
    pub a_pa: u64,
    pub b_pa: u64,
    pub c_pa: u64,
}

/// Golden CPU: c[i] = a[i] + b[i] (f32).
pub fn vector_add_cpu(a: &[f32], b: &[f32], c: &mut [f32]) -> bool {
    if a.len() != b.len() || a.len() != c.len() || a.is_empty() {
        return false;
    }
    for i in 0..a.len() {
        c[i] = a[i] + b[i];
    }
    true
}

/// Compara resultado vs golden com tolerância absoluta.
pub fn vector_add_check(got: &[f32], expect: &[f32], eps: f32) -> bool {
    if got.len() != expect.len() || got.is_empty() {
        return false;
    }
    for i in 0..got.len() {
        let d = if got[i] > expect[i] {
            got[i] - expect[i]
        } else {
            expect[i] - got[i]
        };
        if d > eps {
            return false;
        }
    }
    true
}
