//! VRAM Tier — mapeia BAR2 da GPU como AllocTier::Vram.
//! Toda GPU com BAR2 mapeavel tem sua VRAM disponivel como tier MHI.

use crate::gpu::detect::{GpuInfo, VENDOR_INTEL, VENDOR_NVIDIA, VENDOR_AMD};
use crate::mhi::{AllocTier, MHI_REGISTRY};
use crate::serial_println;

static VRAM_BASE: spin::Mutex<Option<GpuVram>> = spin::Mutex::new(None);

pub struct GpuVram {
    pub base: u64,
    pub size: u64,
    pub gpu: GpuInfo,
    pub page_map: fn(u64) -> Option<u64>, // função de mapeamento
}

/// Inicializa VRAM tier para a melhor GPU encontrada
pub unsafe fn init_vram_tier(gpu: &GpuInfo) -> bool {
    if gpu.bar2 == 0 || gpu.vram_size == 0 {
        serial_println!("[VRAM] {}: sem BAR2 mapeavel (usando DRAM compartilhada)", gpu.name);
        return false;
    }

    // Map BAR2 como uncacheable (VRAM)
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

    // Intel Arc usa BAR0 para MMIO + BAR2 para VRAM
    // NVIDIA/AMD usam BAR0 para MMIO, BAR2 para VRAM
    let vram_phys = gpu.bar2;
    let vram_size = gpu.vram_size;

    // Mapeia as primeiras páginas da VRAM para testar
    crate::apic::map_page_uc(vram_phys, pmoff);
    crate::apic::map_page_uc(vram_phys + 0x1000, pmoff);

    // Testa se VRAM é acessível (escreve/le)
    let test_addr = vram_phys + pmoff;
    let test_val: u32 = 0xDEADBEEF;
    unsafe { core::ptr::write_volatile(test_addr as *mut u32, test_val); }
    let read_back = unsafe { core::ptr::read_volatile(test_addr as *const u32) };

    if read_back != test_val {
        serial_println!("[VRAM] {}: teste de escrita/leitura falhou (endereco {:#x})", gpu.name, vram_phys);
        return false;
    }

    *VRAM_BASE.lock() = Some(GpuVram {
        base: vram_phys,
        size: vram_size,
        gpu: gpu.clone(),
        page_map: |_| None,
    });

    serial_println!("[VRAM] {} mapeado: {:#x} ({} MB)", gpu.name, vram_phys, vram_size / (1024*1024));
    true
}

/// Aloca na VRAM (simplificado: retorna um endereco na BAR2)
pub fn vram_alloc(size: usize) -> Option<u64> {
    let guard = VRAM_BASE.lock();
    let vram = guard.as_ref()?;
    if size as u64 > vram.size { return None; }
    // Alocacao simplificada: retorna base (bump allocator seria o ideal)
    Some(vram.base)
}

/// Libera endereco na VRAM (stub — sem free list ainda)
pub fn vram_free(_addr: u64) {}
