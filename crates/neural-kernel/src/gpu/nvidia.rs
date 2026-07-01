//! NVIDIA PFIFO + FALCON firmware loader.
//! Pascal+ (GTX 1050 → RTX 5090).
//! Display via VBIOS, compute via PUSH_BUFFER, VRAM via BAR2.

use crate::gpu::detect::GpuInfo;
use crate::serial_println;

pub struct NvidiaGpu {
    pub mmio: u64,      // BAR0 virtual
    pub vram: u64,      // BAR2 virtual (VRAM)
    pub vram_size: u64,
    pub clock_p8: bool,
}

impl NvidiaGpu {
    pub fn probe(gpu: &GpuInfo, pmoff: u64) -> Option<Self> {
        let mmio = gpu.bar0 + pmoff;

        unsafe { crate::apic::map_page_uc(gpu.bar0, pmoff); }

        let version = unsafe { core::ptr::read_volatile((mmio + 0x000000) as *const u32) };
        serial_println!("[NVIDIA] {}: version={:#x} bar0={:#x} bar2={:#x}", gpu.name, version, gpu.bar0, gpu.bar2);

        if version == 0xFFFFFFFF || version == 0 {
            serial_println!("[NVIDIA] GPU nao respondeu.");
            return None;
        }

        // Mapeia BAR2 (VRAM): mapeia ate 1MB (256 paginas) como janela inicial
        if gpu.bar2 > 0 && gpu.vram_size > 0 {
            let pages = (gpu.vram_size.min(1024 * 1024) / 4096) as usize;
            for i in 0..pages {
                unsafe { crate::apic::map_page_uc(gpu.bar2 + (i as u64) * 4096, pmoff); }
            }

            let test = gpu.bar2 + pmoff;
            unsafe { core::ptr::write_volatile(test as *mut u32, 0xDEADBEEF); }
            let read = unsafe { core::ptr::read_volatile(test as *const u32) };
            if read == 0xDEADBEEF {
                serial_println!("[NVIDIA] VRAM {} MB acessivel ({} paginas mapeadas)!", gpu.vram_mb(), pages);
            } else {
                serial_println!("[NVIDIA] VRAM nao respondeu. Sem firmware = P8 mode.");
            }
        }

        serial_println!("[NVIDIA] P8 mode (405MHz). GPU compute via VRAM + PFIFO.");

        Some(NvidiaGpu { mmio, vram: gpu.bar2 + pmoff, vram_size: gpu.vram_size, clock_p8: true })
    }
}
