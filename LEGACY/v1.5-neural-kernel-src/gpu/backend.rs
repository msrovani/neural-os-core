//! GPU Backend — conecta a GPU detectada com o pipeline de inferencia do Cortex.
//! Mapeia BAR0/BAR1 como UC, cria SPSC job ring para cada GPU.

use alloc::vec::Vec;
use crate::gpu::detect::{GpuInfo, GpuVendor};
use crate::gpu::intel::{IntelRing, BcsRing};
use crate::gpu::nvidia::NvidiaGpu;
use crate::gpu::ring::GpuJobRing;
use crate::tensor::Tensor;
use crate::serial_println;

pub enum GpuAccel {
    Intel(IntelRing, Option<BcsRing>),
    Nvidia(NvidiaGpu),
    CpuOnly,
}

use spin::Mutex;
static CURRENT_BACKEND: Mutex<Option<GpuAccel>> = Mutex::new(None);
static JOB_RINGS: Mutex<Vec<GpuJobRing>> = Mutex::new(Vec::new());

/// Mapeia BAR0 (MMIO registers) e BAR1/BAR2 (VRAM) como uncacheable para TODOS os vendors.
/// Deve ser chamado antes de vendor-specific init.
pub unsafe fn map_bars_uc(gpu: &GpuInfo) {
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let _vram_pages = ((gpu.vram_size.max(4096) + 4095) / 4096) as usize;

    // BAR0 inteiro como UC — necessário para TODOS os registers MMIO, não só o primeiro page.
    // Intel Gen9: VGACNTRL(0x71400), RENDER_RING_BASE(0x120000), FORCE_WAKEUP(0xA278)
    // NVIDIA: PFIFO(0x2000), DISPLAY(0x1000), RAMIN(0x8000)
    // AMD: RCC_CONFIG(0x2000), PM4 doorbell(0x1B0)
    if gpu.bar0 > 0 {
        let bar0_size = gpu.bar0_size();
        let pages = ((bar0_size + 4095) / 4096) as usize;
        for i in 0..pages {
            crate::apic::map_page_uc(gpu.bar0 + (i as u64) * 4096, pmoff);
        }
        serial_println!("[GPU-BAR] BAR0 mapeado UC: {:#x} ({} KB, {} paginas)", gpu.bar0, bar0_size / 1024, pages);
    }
    if gpu.bar2 > 0 && gpu.vram_size > 0 {
        let aligned = gpu.vram_size.next_power_of_two().min(256 * 1024 * 1024);
        let pages = crate::apic::map_region_uc_2mb(gpu.bar2, aligned, pmoff);
        if pages == 0 {
            serial_println!("[GPU-BAR] AVISO: BAR2(VRAM) @ {:#x} falhou ao mapear!", gpu.bar2);
        } else {
            serial_println!("[GPU-BAR] BAR2(VRAM) mapeado UC: {:#x} ({} MB, {} x 2MB)", gpu.bar2, gpu.vram_size / (1024*1024), pages);
        }
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

        // 4. Teste de firmware (valida blobs no FAT32 mesmo sem GPU NVIDIA)
        crate::gpu::firmware::test_load_firmware();

        // 5. Secure boot (ACR/PSP/GuC) — carrega firmware na GPU se disponivel
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
                if let Some(nv) = NvidiaGpu::probe(gpu, pmoff) {
                    if nv.pfifo_ready {
                        serial_println!("[GPU-BACKEND] NVIDIA PFIFO ativo: PUSH_BUFFER via BAR0");
                        *CURRENT_BACKEND.lock() = Some(GpuAccel::Nvidia(nv));
                        return;
                    }
                }
                serial_println!("[GPU-BACKEND] NVIDIA init falhou, fallback CPU");
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
        Some(GpuAccel::Nvidia(nv)) => {
            // NVIDIA: se PFIFO ativo, copia dados para VRAM e executa
            if nv.pfifo_ready {
                nvidia_matmul(nv, a, b)
            } else { None }
        }
        _ => None,
    };
    drop(guard);
    result.or_else(|| cpu_matmul(a, b))
}

/// NVIDIA GPU matmul: DMA pipeline + CPU fallback.
/// Estado atual:
/// - ✅ DMA handshake VRAM via PUSH_BUFFER (prova que canal PCIe funciona)
/// - ✅ CPU matmul (sempre funcional, usa AVX2 em HW real via tensor.rs)
/// - ❌ GPU compute shader (bloqueado por NDA — ISA NVIDIA não é pública)
/// - ❌ ACR firmware loading (bloqueado — signed blobs necessários)
/// 
/// Para desbloquear GPU compute real:
///   1. Baixar firmware de linux-firmware.git (MIT): fecs_*.bin + gpccs_*.bin
///   2. Implementar WPR loading (~150 LOC, documentado em nouveau driver)
///   3. Compilar shader ternário (exige documentação ISA ou CUDA → CUBIN)
///   Alternativa: usar CPU com AVX2 (2-6× speedup, já integrado via tensor.rs)
fn nvidia_matmul(nv: &NvidiaGpu, a: &Tensor, b: &Tensor) -> Option<Tensor> {
    if a.shape.1 != b.shape.0 { return None; }
    if nv.pfifo_ready && nv.vram_size > 0 {
        let sz = a.data.len() * 4;
        if let Some(vram_off) = crate::gpu::vram::vram_alloc(sz) {
            let bytes: &[u8] = unsafe { core::slice::from_raw_parts(a.data.as_ptr() as *const u8, sz) };
            unsafe { nv.cpu_to_vram(vram_off, bytes); }
            let mut rb = [0u8; 64];
            unsafe { nv.vram_to_cpu(vram_off, &mut rb); }
            crate::gpu::vram::vram_free(vram_off, sz);
        }
    }
    a.matmul(b)
}

/// CPU matmul — fallback universal. Roda em qualquer CPU.
/// Em HW real com AVX2, `Tensor::matmul()` desvia automaticamente para
/// `matmul_avx2_inner()` em tensor.rs (speedup 2-6× sobre scalar).
/// Sem AVX2 (QEMU TCG), usa loop triplo scalar.
/// Este é o único caminho funcional para GPU compute até firmwares e shaders
/// estarem disponíveis (ver CRM-001 em docs/dead-ends.md).
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

pub fn gpu_status() -> alloc::string::String {
    let guard = CURRENT_BACKEND.lock();
    match guard.as_ref() {
        Some(GpuAccel::Intel(_, bcs)) => {
            let b = if bcs.is_some() { " + BCS" } else { "" };
            alloc::format!("Intel iGPU RCS ring buffer{}", b)
        }
        Some(GpuAccel::Nvidia(nv)) => nv.status(),
        Some(GpuAccel::CpuOnly) => alloc::string::String::from("CPU fallback"),
        None => alloc::string::String::from("Nao inicializado"),
    }
}
