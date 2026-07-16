//! AMD RDNA PM4 Compute — PM4 packet submission via ring buffer.
//! RX 6000/7000/9000 series + AMD APUs com RDNA iGPU.

use crate::gpu::detect::GpuInfo;
use k_nano::serial_println;

pub struct AmdGpu {
    pub mmio: u64,
    pub vram: u64,
    pub vram_size: u64,
}

impl AmdGpu {
    pub fn probe(gpu: &GpuInfo, pmoff: u64) -> Option<Self> {
        let mmio = gpu.bar0 + pmoff;
        unsafe { k_nano::apic::map_page_uc(gpu.bar0, pmoff); }

        let test = unsafe { core::ptr::read_volatile(mmio as *const u32) };
        if test == 0xFFFFFFFF {
            serial_println!("[AMD] GPU nao respondeu.");
            return None;
        }

        if gpu.bar2 > 0 && gpu.vram_size > 0 {
            let aligned = gpu.vram_size.next_power_of_two().min(256 * 1024 * 1024);
            let pages = unsafe { k_nano::apic::map_region_uc_2mb(gpu.bar2, aligned, pmoff) };
            serial_println!("[AMD] {} VRAM {} MB mapeada ({} x 2MB pages).", gpu.name, gpu.vram_mb(), pages);
        }

        serial_println!("[AMD] PM4 compute futuro.");
        Some(AmdGpu { mmio, vram: gpu.bar2 + pmoff, vram_size: gpu.vram_size })
    }
}
