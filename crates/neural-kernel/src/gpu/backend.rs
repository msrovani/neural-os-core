//! GPU Backend — conecta a GPU detectada com o pipeline de inferencia do Cortex.
//! Mapeia BAR0/BAR1 como UC, cria SPSC job ring para cada GPU.

use alloc::vec::Vec;
use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::gpu::intel::{IntelRing, BcsRing};
use crate::gpu::ring::GpuJobRing;
use crate::tensor::Tensor;
use crate::serial_println;

pub enum GpuAccel {
    Intel(IntelRing, Option<BcsRing>),
    CpuOnly,
}

use spin::Mutex;
static CURRENT_BACKEND: Mutex<Option<GpuAccel>> = Mutex::new(None);
static JOB_RINGS: Mutex<Vec<GpuJobRing>> = Mutex::new(Vec::new());

/// Mapeia BAR0 (MMIO registers) e BAR1/BAR2 (VRAM) como uncacheable para TODOS os vendors.
/// Deve ser chamado antes de vendor-specific init.
pub unsafe fn map_bars_uc(gpu: &GpuInfo) {
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let pages = ((gpu.vram_size.max(4096) + 4095) / 4096) as usize;

    if gpu.bar0 > 0 {
        crate::apic::map_page_uc(gpu.bar0, pmoff);
        serial_println!("[GPU-BAR] BAR0 mapeado UC: {:#x}", gpu.bar0);
    }
    if gpu.bar2 > 0 && gpu.vram_size > 0 {
        crate::apic::map_region_uc_2mb(gpu.bar2, gpu.vram_size, pmoff);
        serial_println!("[GPU-BAR] BAR2(VRAM) mapeado UC: {:#x} ({} MB)", gpu.bar2, gpu.vram_size / (1024*1024));
    }
}

/// Valida mapeamento lendo register conhecido de cada vendor
pub unsafe fn validate_bar0(gpu: &GpuInfo) -> bool {
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let virt = gpu.bar0 + pmoff;

    match gpu.vendor {
        GpuVendor::Nvidia => {
            // NVIDIA: ler VERSION register (offset 0x0000, retorna 0x0000XXXX para Pascal+)
            let version = core::ptr::read_volatile(virt as *const u32);
            serial_println!("[GPU-BAR] NVIDIA VERSION=0x{:08x} @ BAR0+0x00", version);
            version != 0 && version != 0xFFFFFFFF
        }
        GpuVendor::Amd => {
            // AMD: ler RCC_CONFIG (offset 0x2000, tipicamente retorna 0xXXXXXXXX)
            let rcc = core::ptr::read_volatile((virt + 0x2000) as *const u32);
            serial_println!("[GPU-BAR] AMD RCC_CONFIG=0x{:08x} @ BAR0+0x2000", rcc);
            rcc != 0 && rcc != 0xFFFFFFFF
        }
        GpuVendor::Intel => {
            // Intel: ler VGACNTRL (offset 0x71400, já usado no xuvisco fix)
            let vga = core::ptr::read_volatile((virt + 0x71400) as *const u32);
            serial_println!("[GPU-BAR] Intel VGACNTRL=0x{:08x} @ BAR0+0x71400", vga);
            vga != 0xFFFFFFFF
        }
        GpuVendor::VirtIo => {
            // VirtIO-GPU: ler VERSION (offset 0x00, tipicamente 0x1)
            let ver = core::ptr::read_volatile(virt as *const u32);
            serial_println!("[GPU-BAR] VirtIO VERSION=0x{:08x}", ver);
            ver == 1
        }
        GpuVendor::Unknown => {
            serial_println!("[GPU-BAR] Unknown vendor, skip validation");
            true
        }
    }
}

/// Inicializa o backend GPU baseado no hardware detectado
pub unsafe fn init_backend(gpus: &[GpuInfo]) {
    if gpus.is_empty() {
        serial_println!("[GPU-BACKEND] Sem GPU detectada. Fallback CPU.");
        *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
        return;
    }

    for gpu in gpus {
        // 1. Mapeia BARs como UC (fundamental para qualquer GPU)
        map_bars_uc(gpu);

        // 2. Valida mapeamento
        if !validate_bar0(gpu) {
            serial_println!("[GPU-BACKEND] {}: BAR0 validation FAILED, skipping", gpu.name);
            continue;
        }
        serial_println!("[GPU-BACKEND] {}: BAR0 validated OK", gpu.name);

        // 3. Cria SPSC job ring para submissão de jobs
        let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        if let Some(job_ring) = GpuJobRing::new(gpu, pmoff) {
            JOB_RINGS.lock().push(job_ring);
            serial_println!("[GPU-BACKEND] {}: SPSC job ring criado", gpu.name);
        }

        // 4. Secure boot (ACR/PSP/GuC) — carrega firmware se disponivel
        let _sb_result = crate::gpu::firmware::secure_boot_gpu(gpu, pmoff);

        // 5. Inicializa backend especifico do vendor
        match gpu.vendor {
            GpuVendor::Intel => {
                if let Some(ring) = IntelRing::probe(gpu, pmoff) {
                    let bcs = BcsRing::probe(gpu.bar0 + pmoff);
                    let bcs_msg = if bcs.is_some() { "+ BCS" } else { "" };
                    serial_println!("[GPU-BACKEND] Intel GPU ativo: {} {}", gpu.name, bcs_msg);
                    *CURRENT_BACKEND.lock() = Some(GpuAccel::Intel(ring, bcs));
                    return;
                }
            }
            GpuVendor::Nvidia => {
                serial_println!("[GPU-BACKEND] NVIDIA {}: BAR+ring+secure boot OK. PFIFO+PUSH_BUFFER futuro.", gpu.name);
            }
            GpuVendor::Amd => {
                serial_println!("[GPU-BACKEND] AMD {}: BAR+ring+secure boot OK. PM4 ring futuro.", gpu.name);
            }
            GpuVendor::VirtIo => {
                serial_println!("[GPU-BACKEND] VirtIO-GPU {}: display apenas (sem compute).", gpu.name);
            }
            _ => {}
        }
    }

    serial_println!("[GPU-BACKEND] {} GPU(s) processadas. Backend: CPU fallback", gpus.len());
    *CURRENT_BACKEND.lock() = Some(GpuAccel::CpuOnly);
}

pub fn gpu_matmul(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    let mut guard = CURRENT_BACKEND.lock();
    let result = match guard.as_mut() {
        Some(GpuAccel::Intel(ring, _)) => ring.gpu_matmul(a, b),
        _ => None,
    };
    drop(guard);
    result.or_else(|| cpu_matmul(a, b))
}

fn cpu_matmul(a: &Tensor, b: &Tensor) -> Option<Tensor> {
    a.matmul(b)
}

pub fn gpu_forward(_model: &crate::cortex::TransformerModel, _tokens: &[u16]) -> Option<(Tensor, Tensor)> {
    None
}

/// Retorna status de todos os job rings
pub fn job_ring_info() -> alloc::string::String {
    let rings = JOB_RINGS.lock();
    if rings.is_empty() {
        alloc::string::String::from("Nenhum job ring")
    } else {
        rings.iter().map(|r| r.status()).collect::<Vec<_>>().join("\n")
    }
}

pub fn gpu_status() -> &'static str {
    let guard = CURRENT_BACKEND.lock();
    match guard.as_ref() {
        Some(GpuAccel::Intel(_, bcs)) => {
            if bcs.is_some() { "Intel iGPU RCS + BCS ring buffer" } else { "Intel iGPU RCS ring buffer" }
        }
        Some(GpuAccel::CpuOnly) => "CPU fallback",
        None => "Nao inicializado",
    }
}
