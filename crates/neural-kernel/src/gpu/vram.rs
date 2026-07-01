//! VRAM Tier — mapeia BAR2 da GPU como AllocTier::Vram.
//! Toda GPU com BAR2 mapeavel tem sua VRAM disponivel como tier MHI.

use crate::gpu::detect::GpuInfo;
use crate::serial_println;

static VRAM_STATE: spin::Mutex<Option<GpuVram>> = spin::Mutex::new(None);

pub struct GpuVram {
    pub base: u64,
    pub size: u64,
    pub gpu: GpuInfo,
    pub next_offset: u64, // bump allocator offset
    pub page_map: fn(u64) -> Option<u64>,
}

/// Inicializa VRAM tier para a melhor GPU encontrada
pub unsafe fn init_vram_tier(gpu: &GpuInfo) -> bool {
    if gpu.bar2 == 0 || gpu.vram_size == 0 {
        serial_println!("[VRAM] {}: sem BAR2 mapeavel (usando DRAM compartilhada)", gpu.name);
        return false;
    }

    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

    let vram_phys = gpu.bar2;
    let vram_size = gpu.vram_size;

    crate::apic::map_page_uc(vram_phys, pmoff);
    crate::apic::map_page_uc(vram_phys + 0x1000, pmoff);

    let test_addr = vram_phys + pmoff;
    let test_val: u32 = 0xDEADBEEF;
    unsafe { core::ptr::write_volatile(test_addr as *mut u32, test_val); }
    let read_back = unsafe { core::ptr::read_volatile(test_addr as *const u32) };

    if read_back != test_val {
        serial_println!("[VRAM] {}: teste de escrita/leitura falhou (endereco {:#x})", gpu.name, vram_phys);
        return false;
    }

    *VRAM_STATE.lock() = Some(GpuVram {
        base: vram_phys,
        size: vram_size,
        gpu: gpu.clone(),
        next_offset: 0,
        page_map: |_| None,
    });

    serial_println!("[VRAM] {} mapeado: {:#x} ({} MB)", gpu.name, vram_phys, vram_size / (1024*1024));
    true
}

/// Aloca na VRAM (bump allocator simples)
pub fn vram_alloc(size: usize) -> Option<u64> {
    let mut guard = VRAM_STATE.lock();
    let vram = guard.as_mut()?;
    let aligned = (size as u64 + 0xFFF) & !0xFFF;
    if vram.next_offset + aligned > vram.size {
        serial_println!("[VRAM] alloc {} bytes falhou: sem espaco (usado={}/{})",
            size, vram.next_offset, vram.size);
        return None;
    }
    let addr = vram.base + vram.next_offset;
    vram.next_offset += aligned;
    Some(addr)
}

/// Libera endereco na VRAM (stub — sem free list ainda)
pub fn vram_free(_addr: u64) {}
