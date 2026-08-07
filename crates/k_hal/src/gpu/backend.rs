//! GPU Backend — plano display_coex dirige init; canário define has_compute; CPU fallback honesto.

use alloc::vec::Vec;
use crate::gpu::amd::AmdGpu;
use crate::gpu::blit::{init_blit, run_blit_canary};
use crate::gpu::canary::{self, CanaryResult};
use crate::gpu::compute_abi::{BackendState, TensorOp};
use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::gpu::display_coex::{self, GpuAssignment};
use crate::gpu::intel::{IntelRing, BcsRing};
use crate::gpu::nvidia::NvidiaGpu;
use crate::gpu::nvidia_pascal_ce;
use crate::gpu::ring::GpuJobRing;
use cortex::tensor::Tensor;
use spin::Mutex;

pub enum GpuAccel {
    Intel(IntelRing, Option<BcsRing>),
    Nvidia(NvidiaGpu),
    Amd(AmdGpu),
    CpuOnly,
}

static CURRENT_BACKEND: Mutex<Option<GpuAccel>> = Mutex::new(None);
static JOB_RINGS: Mutex<Vec<GpuJobRing>> = Mutex::new(Vec::new());
static COMPUTE_STATE: Mutex<BackendState> = Mutex::new(BackendState::CpuOnly);
static LAST_PLAN: Mutex<Option<GpuAssignment>> = Mutex::new(None);

/// Mapeia BAR0/BAR2 como uncacheable (só R1 / Cap MAP_BAR).
pub unsafe fn map_bars_uc(gpu: &GpuInfo) {
    if crate::cap_gate::check_map_bar(1, true) == crate::cap_gate::CapResult::Deny {
        k_nano::slog_hal!("GPU", "BAR", "DENY map_bars_uc (Cap)");
        return;
    }
    // VirtIO display: FE compositor usa UEFI GOP; não mapear BAR cold (P8).
    if gpu.vendor == GpuVendor::VirtIo {
        k_nano::slog_hal!("GPU", "BAR", "VirtIO skip map_bars (display FE=GOP; BE deferred)");
        return;
    }
    // QEMU std VGA (vendor Unknown, DID 0x1111) — map BAR causa #PF storm.
    if gpu.vendor == GpuVendor::Unknown {
        k_nano::slog_hal!(
            "GPU",
            "BAR",
            "skip Unknown DID={:04x} (QEMU VGA / avoid #PF)",
            gpu.device_id
        );
        return;
    }
    if gpu.bar0 == 0 || (gpu.bar0 >> 48) != 0 {
        k_nano::slog_hal!("GPU", "BAR", "skip — bar0 invalido {:#x}", gpu.bar0);
        return;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

    if gpu.bar0 > 0 {
        let bar0_size = gpu.bar0_size();
        let pages = ((bar0_size + 4095) / 4096) as usize;
        for i in 0..pages {
            k_nano::apic::map_page_uc(gpu.bar0 + (i as u64) * 4096, pmoff);
        }
        k_nano::slog_hal!("GPU", "bar", "BAR0 mapeado UC: {:#x} ({} KB, {} paginas)",
            gpu.bar0,
            bar0_size / 1024,
            pages);
    }
    if gpu.bar2 > 0 && gpu.vram_size > 0 {
        let aligned = gpu.vram_size.next_power_of_two().min(256 * 1024 * 1024);
        let pages = k_nano::apic::map_region_uc_2mb(gpu.bar2, aligned, pmoff);
        if pages == 0 {
            k_nano::slog_hal!("GPU", "BAR", "AVISO: BAR2(VRAM) @ {:#x} falhou ao mapear!", gpu.bar2);
        } else {
            k_nano::slog_hal!("GPU", "bar", "BAR2(VRAM) mapeado UC: {:#x} ({} MB, {} x 2MB)",
                gpu.bar2,
                gpu.vram_size / (1024 * 1024),
                pages);
        }
    }
}

pub unsafe fn validate_bar0(gpu: &GpuInfo) -> bool {
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let virt = gpu.bar0 + pmoff;

    match gpu.vendor {
        GpuVendor::Nvidia => {
            let version = core::ptr::read_volatile(virt as *const u32);
            k_nano::slog_hal!("GPU", "BAR", "NVIDIA VERSION=0x{:08x} @ BAR0+0x00", version);
            version != 0 && version != 0xFFFFFFFF
        }
        GpuVendor::Amd => {
            let rcc = core::ptr::read_volatile((virt + 0x2000) as *const u32);
            k_nano::slog_hal!("GPU", "BAR", "AMD RCC_CONFIG=0x{:08x} @ BAR0+0x2000", rcc);
            rcc != 0 && rcc != 0xFFFFFFFF
        }
        GpuVendor::Intel => {
            let vga = core::ptr::read_volatile((virt + 0x71400) as *const u32);
            k_nano::slog_hal!("GPU", "BAR", "Intel VGACNTRL=0x{:08x} @ BAR0+0x71400", vga);
            vga != 0xFFFFFFFF
        }
        GpuVendor::VirtIo => {
            k_nano::slog_hal!("GPU", "BAR", "VirtIO BAR validate skipped (GOP FE)");
            true
        }
        GpuVendor::Unknown => {
            k_nano::slog_hal!("GPU", "BAR", "Unknown vendor, skip validation");
            true
        }
    }
}

/// Init dirigido pelo plano de coex (display vs compute).
pub unsafe fn init_backend(gpus: &[GpuInfo]) {
    init_backend_with_plan(gpus, &display_coex::plan_assignment(gpus));
}

pub unsafe fn init_backend_with_plan(gpus: &[GpuInfo], plan: &GpuAssignment) {
    *LAST_PLAN.lock() = Some(*plan);
    k_nano::slog_hal!("GPU", "BACKEND", "{}", display_coex::assignment_status(plan, gpus));

    if gpus.is_empty() || matches!(plan, GpuAssignment::CpuOnly) {
        k_nano::slog_hal!("GPU", "BACKEND", "Sem GPU compute. Fallback CPU.");
        *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
        *COMPUTE_STATE.lock() = BackendState::CpuOnly;
        log_gpu_hw_verdict("no_compute_gpu");
        let _ = crate::gpu::direct_storage::probe_gds();
        return;
    }

    // Mapear display GPU (BARs) sem soft-reset em falha de compute
    if let Some(di) = plan.display_index() {
        if let Some(dg) = gpus.get(di) {
            map_bars_uc(dg);
            let _ = validate_bar0(dg);
            k_nano::slog_hal!("GPU", "BACKEND", "DisplayOwner={}: BARs mapped (intocado por falha AI)", dg.name);
        }
    }

    let compute_idx = plan.compute_index();
    let Some(ci) = compute_idx else {
        *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
        *COMPUTE_STATE.lock() = BackendState::CpuOnly;
        log_gpu_hw_verdict("no_compute_index");
        let _ = crate::gpu::direct_storage::probe_gds();
        return;
    };
    let gpu = match gpus.get(ci) {
        Some(g) => g,
        None => {
            *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
            *COMPUTE_STATE.lock() = BackendState::CpuOnly;
            log_gpu_hw_verdict("compute_gpu_missing");
            return;
        }
    };

    // Se display != compute, mapear compute também
    if plan.display_index() != Some(ci) {
        map_bars_uc(gpu);
    }
    if !validate_bar0(gpu) {
        k_nano::slog_hal!("GPU", "BACKEND", "{}: BAR0 validation FAILED → CPU", gpu.name);
        *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
        *COMPUTE_STATE.lock() = BackendState::Quarantine;
        log_gpu_hw_verdict("bar0_validate_failed");
        return;
    }

    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if let Some(job_ring) = GpuJobRing::new(gpu, pmoff) {
        JOB_RINGS.lock().push(job_ring);
    }

    crate::gpu::firmware::test_load_firmware();
    // ACR → sw_* (dentro de nvidia_acr_load) → depois D2/D3 no probe.
    let sb = crate::gpu::firmware::secure_boot_gpu(gpu, pmoff);
    if let Some(acr) = crate::gpu::firmware::last_acr_report() {
        k_nano::slog_hal!("GPU", "BACKEND", "ACR stage={:?} (Ok=HsBooted only; ≠ has_compute)", acr.stage);
        if acr.hs_booted() {
            crate::unlock_dag::grant(crate::unlock_dag::CapToken::GpuAcrBooted);
        }
    } else if sb == crate::gpu::firmware::SecureBootResult::Ok {
        // Outros vendors (ex. GuC/PSP) — não grant Nvidia ACR token
    }

    *COMPUTE_STATE.lock() = BackendState::BringingUp;

    match gpu.vendor {
        GpuVendor::Intel => {
            if let Some(g) = crate::gpu::intel_guc::last_guc_report() {
                k_nano::slog_hal!("GPU", "BACKEND", "GuC stage={:?} (via secure_boot)", g.stage);
            }
            if let Some(ring) = IntelRing::probe(gpu, pmoff) {
                let bcs = BcsRing::probe(gpu.bar0 + pmoff);
                k_nano::slog_hal!("GPU", "BACKEND", "Intel probe OK: {}", gpu.name);
                *CURRENT_BACKEND.lock() = Some(GpuAccel::Intel(ring, bcs));
                
                // Initialize blit engine and run canary
                unsafe { init_blit(gpu, pmoff); }
                let blit_ok = unsafe { run_blit_canary(gpu) };
                if blit_ok {
                    k_nano::slog_hal!("GPU", "BACKEND", "blit canary PASS");
                } else {
                    k_nano::slog_hal!("GPU", "BACKEND", "blit canary FAIL — CPU fallback");
                }
            } else {
                *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
            }
        }
        GpuVendor::Nvidia => {
            if let Some(nv) = NvidiaGpu::probe(gpu, pmoff) {
                k_nano::slog_hal!("GPU", "BACKEND", "NVIDIA probe: {}", nv.status());
                // ADR-0087 Fase 4b — Copy Engine (DMA bulk RAM↔VRAM): channel CE
                // + canário 64KB. HW-gated; ready só com golden (honesto).
                nvidia_pascal_ce::probe_global(gpu);
                *CURRENT_BACKEND.lock() = Some(GpuAccel::Nvidia(nv));
            } else {
                k_nano::slog_hal!("GPU", "BACKEND", "NVIDIA init falhou, fallback CPU");
                *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
            }
        }
        GpuVendor::Amd => {
            if let Some(amd) = AmdGpu::probe(gpu, pmoff) {
                k_nano::slog_hal!("GPU", "BACKEND", "AMD probe OK: {}", gpu.name);
                *CURRENT_BACKEND.lock() = Some(GpuAccel::Amd(amd));
            } else {
                *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
            }
        }
        GpuVendor::VirtIo => {
            k_nano::slog_hal!("GPU", "BACKEND", "VirtIO-GPU: display apenas (sem compute)");
            *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
            *COMPUTE_STATE.lock() = BackendState::CpuOnly;
            log_gpu_verdict_unified(gpu, "CPU_FALLBACK", "virtio_display_only");
            return;
        }
        _ => {
            *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
        }
    }

    // Canário: NVIDIA D4 / Intel ring vivo; demais vendors genérico.
    let canary = {
        let mut guard = CURRENT_BACKEND.lock();
        match guard.as_mut() {
            Some(GpuAccel::Nvidia(nv)) => unsafe { canary::run_vector_add_canary_nv(gpu, nv) },
            Some(GpuAccel::Intel(ring, _)) => {
                let mad_ok = crate::gpu::intel_mad::run_mad_int8_host_canary();
                k_nano::slog_hal!("GPU", "BACKEND", "MAD/INT8 host={}", mad_ok);
                unsafe { canary::run_vector_add_canary_intel(gpu, ring) }
            },
            Some(GpuAccel::Amd(amd)) => {
                let _ = crate::gpu::amd_mad::run_dot_int8_host_canary();
                unsafe { canary::run_vector_add_canary_amd(gpu, amd) }
            },
            _ => unsafe { canary::run_vector_add_canary(gpu) },
        }
    };
    let st = canary::state_after(canary);
    *COMPUTE_STATE.lock() = st;
    match canary {
        CanaryResult::Pass => {
            if let Some(GpuAccel::Nvidia(ref mut nv)) = CURRENT_BACKEND.lock().as_mut() {
                nv.compute_ready = true;
            }
            if let Some(GpuAccel::Amd(ref mut a)) = CURRENT_BACKEND.lock().as_mut() {
                a.compute_ready = true;
            }
            crate::unlock_dag::grant(crate::unlock_dag::CapToken::GpuCompute);
            k_nano::slog_hal!("GPU", "BACKEND", "compute Ready (canário PASS)");
            log_gpu_verdict_unified(gpu, "PASS", "canary_vector_add_golden");
        }
        CanaryResult::SkipVirtIo | CanaryResult::SkipCpu => {
            k_nano::slog_hal!("GPU", "BACKEND", "compute skip ({:?}) — CPU_FALLBACK", canary);
            log_gpu_verdict_unified(gpu, "CPU_FALLBACK", "skip_no_discrete_compute");
        }
        _ => {
            k_nano::slog_hal!("GPU", "BACKEND", "compute Quarantine/CPU — display owner preservado ({:?})", canary);
            let reason = match canary {
                CanaryResult::FailNoPack => "kernel_pack_missing",
                CanaryResult::FailUnsigned => "kernel_pack_unsigned",
                CanaryResult::FailDispatch => "canary_dispatch_fail",
                CanaryResult::FailGolden => "canary_golden_fail",
                _ => "canary_not_pass",
            };
            // NVIDIA Gsp scaffold → PARTIAL; resto AWAITING até HW golden
            let verdict = if gpu.vendor == GpuVendor::Nvidia
                && gpu.backend_kind == crate::gpu::compute_abi::ComputeBackendKind::Gsp
            {
                "PARTIAL"
            } else if gpu.vendor == GpuVendor::Nvidia {
                "AWAITING_REAL_HW"
            } else {
                "AWAITING_REAL_HW"
            };
            log_gpu_verdict_unified(gpu, verdict, reason);
        }
    }
    let _ = crate::gpu::direct_storage::probe_gds();
    crate::compute_port::sync_from_backend();
}

/// VERDICT unificado Labor 5 — family/isa/backend; nunca Ready implícito sem Pass.
fn log_gpu_verdict_unified(gpu: &GpuInfo, verdict: &str, reason: &str) {
    let family = if gpu.vendor == GpuVendor::Nvidia {
        crate::gpu::detect::nvidia_family_str(gpu.arch)
    } else {
        "n/a"
    };
    k_nano::slog_bin!(
        "GPU-HW",
        "info",
        "step=compute status={} detail={} family={} isa={} backend={:?} name={}",
        verdict,
        reason,
        family,
        gpu.isa_tag.as_str(),
        gpu.backend_kind,
        gpu.name
    );
    k_nano::slog_bin!(
        "GPU-HW",
        "info",
        "VERDICT={} reason={} family={} isa={} backend={:?}",
        verdict,
        reason,
        family,
        gpu.isa_tag.as_str(),
        gpu.backend_kind
    );
}

fn log_gpu_hw_verdict(reason: &str) {
    k_nano::slog_bin!(
        "GPU-HW",
        "info",
        "step=compute_ready status=UNSUPPORTED detail={}",
        reason
    );
    k_nano::slog_bin!(
        "GPU-HW",
        "info",
        "VERDICT=AWAITING_REAL_HW reason={}",
        reason
    );
}

pub fn compute_state() -> BackendState {
    *COMPUTE_STATE.lock()
}

pub fn gpu_matmul(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    let _ = crate::gpu::work_queue::submit_tensor(TensorOp::MatmulTernary);
    let ready = *COMPUTE_STATE.lock() == BackendState::Ready;
    let mut guard = CURRENT_BACKEND.lock();
    let result = if ready {
        match guard.as_mut() {
            Some(GpuAccel::Intel(ring, _)) => ring.gpu_matmul(a, b),
            Some(GpuAccel::Nvidia(nv)) if nv.compute_ready => nvidia_matmul(nv, a, b),
            _ => None,
        }
    } else {
        None
    };
    drop(guard);
    let _ = crate::gpu::work_queue::drain(ready && result.is_some());
    result.or_else(|| {
        let _ = crate::gpu::work_queue::drain(false);
        cpu_matmul(a, b)
    })
}

/// ADR-0047 gate: HW só após canário Ready — nunca por PFIFO NOP sozinho.
pub fn adr0047_compute_gate() -> &'static str {
    let hw = *COMPUTE_STATE.lock() == BackendState::Ready;
    crate::gpu::work_queue::gate_status(hw)
}

fn nvidia_matmul(nv: &NvidiaGpu, a: &Tensor, b: &Tensor) -> Option<Tensor> {
    if a.shape.1 != b.shape.0 {
        return None;
    }
    // DMA handshake opcional; matmul real exige KernelPack + QMD Ready.
    if nv.pfifo_ready && nv.vram_size > 0 && nv.compute_ready {
        let sz = a.data.len() * 4;
        if let Some(pa) = crate::gpu::vram::vram_alloc(sz) {
            if let Some(off) = nv.vram_rel(pa) {
                let bytes: &[u8] =
                    unsafe { core::slice::from_raw_parts(a.data.as_ptr() as *const u8, sz) };
                unsafe { nv.cpu_to_vram(off, bytes); }
                let mut rb = [0u8; 64];
                unsafe { nv.vram_to_cpu(off, &mut rb); }
            }
            crate::gpu::vram::vram_free(pa, sz);
        }
    }
    a.matmul(b)
}

fn cpu_matmul(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    a.matmul(b)
}

pub fn gpu_forward(
    _model: &cortex::cortex::TransformerModel,
    _tokens: &[u16],
) -> Option<(Tensor, Tensor)> {
    None
}

pub fn job_ring_info() -> alloc::string::String {
    let rings = JOB_RINGS.lock();
    if rings.is_empty() {
        alloc::string::String::from("Nenhum job ring")
    } else {
        rings
            .iter()
            .map(|r| r.status())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn gpu_status() -> alloc::string::String {
    let st = *COMPUTE_STATE.lock();
    let guard = CURRENT_BACKEND.lock();
    let base = match guard.as_ref() {
        Some(GpuAccel::Intel(_, bcs)) => {
            let b = if bcs.is_some() { " + BCS" } else { "" };
            alloc::format!("Intel ring{}", b)
        }
        Some(GpuAccel::Nvidia(nv)) => nv.status(),
        Some(GpuAccel::Amd(_)) => alloc::string::String::from("AMD KiQ/Mes probe"),
        Some(GpuAccel::CpuOnly) => alloc::string::String::from("CPU fallback"),
        None => alloc::string::String::from("Nao inicializado"),
    };
    alloc::format!("{} | state={:?}", base, st)
}

/// Desliga plano VGA Intel (VGACNTRL) — MMIO só em k-hal (R1).
pub unsafe fn disable_intel_vga_plane() {
    if crate::cap_gate::check_map_bar(1, true) == crate::cap_gate::CapResult::Deny {
        k_nano::slog_hal!("GPU", "vga", "DENY disable_intel_vga (Cap)");
        return;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    for g in crate::gpu::detect::detect_all() {
        if g.vendor != GpuVendor::Intel || !g.has_display_engine || g.bar0 == 0 {
            continue;
        }
        let vga_cntrl = (g.bar0 + 0x71400 + pmoff) as *mut u32;
        let val = core::ptr::read_volatile(vga_cntrl);
        if val & 0x8000_0000 == 0 {
            core::ptr::write_volatile(vga_cntrl, val | 0x8000_0000);
            k_nano::slog_hal!(
                "GPU",
                "vga",
                "Intel VGA plane DISABLED via VGACNTRL ({}:{}.{})",
                g.pci_bus,
                g.pci_dev,
                g.pci_fn
            );
        } else {
            k_nano::slog_hal!("GPU", "vga", "Intel VGA plane ja desligado");
        }
        return;
    }
    k_nano::slog_hal!("GPU", "vga", "Intel GPU display nao encontrada");
}
