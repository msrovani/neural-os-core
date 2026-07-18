//! AMD RDNA/GCN compute — IP Discovery → KiQ (≤GFX10) | Mes (≥GFX11).
//! Sem ROCm no alvo; Kernel Pack = HSACO/ELF amdgcn offline (ADR-0049).

use crate::gpu::amd_discovery::{self, AmdIpId, DiscoverySource};
use crate::gpu::amd_kiq;
use crate::gpu::amd_mes;
use crate::gpu::amd_psp;
use crate::gpu::compute_abi::ComputeBackendKind;
use crate::gpu::detect::{GpuArch, GpuInfo};

pub use crate::gpu::amd_discovery::AmdIpId as AmdIpSnapshot;

pub struct AmdGpu {
    pub mmio: u64,
    pub vram: u64,
    pub vram_size: u64,
    pub backend: ComputeBackendKind,
    pub ip: Option<AmdIpId>,
    pub compute_ready: bool,
}

impl AmdGpu {
    pub fn probe(gpu: &GpuInfo, pmoff: u64) -> Option<Self> {
        let mmio = gpu.bar0 + pmoff;
        unsafe {
            k_nano::apic::map_page_uc(gpu.bar0, pmoff);
        }

        let test = unsafe { core::ptr::read_volatile(mmio as *const u32) };
        if test == 0xFFFFFFFF {
            k_nano::slog_hal!("AMD", "info", "GPU nao respondeu.");
            return None;
        }

        let mut vram_va = 0u64;
        if gpu.bar2 > 0 && gpu.vram_size > 0 {
            let aligned = gpu.vram_size.next_power_of_two().min(256 * 1024 * 1024);
            let pages = unsafe { k_nano::apic::map_region_uc_2mb(gpu.bar2, aligned, pmoff) };
            vram_va = gpu.bar2 + pmoff;
            k_nano::slog_hal!("GPU", "amd", "{} VRAM {} MB mapeada ({} x 2MB pages).",
                gpu.name,
                gpu.vram_mb(),
                pages);
        }

        let ip = unsafe { amd_discovery::probe_ip(gpu, vram_va, gpu.vram_size) };
        let backend = if ip.source == DiscoverySource::Failed {
            ComputeBackendKind::CpuFallback
        } else {
            ip.backend_kind()
        };

        unsafe {
            let _psp = amd_psp::bring_up_psp(gpu, mmio);
        }

        if ip.has_mes {
            let st = amd_mes::check_mes_blobs(&ip);
            k_nano::slog_hal!("AMD", "info", "MES fw stage={:?}", st);
        }

        k_nano::slog_hal!("GPU", "amd", "backend={:?} (gpu_table={:?}) igpu={} isa={} IP=GC{}.{} src={:?}",
            backend,
            gpu.backend_kind,
            gpu.is_integrated,
            gpu.isa_tag.as_str(),
            ip.gfx_major,
            ip.gfx_minor,
            ip.source);

        Some(AmdGpu {
            mmio,
            vram: vram_va,
            vram_size: gpu.vram_size,
            backend,
            ip: Some(ip),
            compute_ready: false,
        })
    }

    /// Canário com handle vivo — KIQ ou MES conforme IP.
    pub unsafe fn try_vector_add(
        &mut self,
        hsaco: &[u8],
        a: &[f32],
        b: &[f32],
        expect: &[f32],
    ) -> bool {
        let Some(ip) = self.ip else {
            return false;
        };
        if hsaco.is_empty() {
            return false;
        }
        match self.backend {
            ComputeBackendKind::KiQ => {
                amd_kiq::dispatch_vector_add_kiq(self.mmio, &ip, hsaco, a, b, expect)
            }
            ComputeBackendKind::Mes => {
                amd_mes::dispatch_vector_add_mes(self.mmio, &ip, hsaco, a, b, expect)
            }
            _ => {
                k_nano::slog_hal!("AMD", "info", "CpuFallback — sem dispatch");
                false
            }
        }
    }
}

/// Canário sem handle (fallback genérico).
pub unsafe fn try_vector_add(
    gpu: &GpuInfo,
    hsaco: &[u8],
    a: &[f32],
    b: &[f32],
    expect: &[f32],
) -> bool {
    let _ = (a, b, expect);
    if hsaco.is_empty() {
        return false;
    }
    k_nano::slog_hal!("GPU", "amd", "standalone {:?}/{:?}: pack {}B — precisa AmdGpu::probe",
        gpu.backend_kind,
        gpu.arch,
        hsaco.len());
    false
}

/// Features honestas por GC (sem inventar WMMA em GFX9).
pub fn feature_flags(ip: &AmdIpId) -> (bool, bool, bool) {
    // (dot4, wmma_i8, mfma_i8)
    match ip.gfx_major {
        9 => (false, false, false),
        10 => (true, false, false),  // RDNA2: int8/dot genérico
        11 | 12 => (true, true, false), // WMMA i8 potencial
        _ => (false, false, false),
    }
}

pub fn arch_label(arch: GpuArch) -> &'static str {
    match arch {
        GpuArch::AmdGcn => "gfx9",
        GpuArch::AmdRdna1 => "gfx10.1",
        GpuArch::AmdRdna2 => "gfx10.3",
        GpuArch::AmdRdna3 => "gfx11",
        GpuArch::AmdRdna4 => "gfx12",
        _ => "unknown",
    }
}
