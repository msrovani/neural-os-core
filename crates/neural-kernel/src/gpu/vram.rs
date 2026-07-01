//! VRAM Tier — mapeia BAR2 da GPU como AllocTier::Vram.
//! Free list allocator com first-fit e coalescing de blocos adjacentes.

use alloc::collections::BTreeMap;
use crate::gpu::detect::GpuInfo;
use crate::serial_println;

static VRAM_STATE: spin::Mutex<Option<GpuVram>> = spin::Mutex::new(None);

pub struct GpuVram {
    pub base: u64,
    pub size: u64,
    pub gpu: GpuInfo,
    pub free_blocks: BTreeMap<u64, u64>, // start → size
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

    // Mapeia VRAM com Huge Pages 2MB (muito mais rapido que 4KB)
    let pages = unsafe { crate::apic::map_region_uc_2mb(vram_phys, vram_size, pmoff) };
    serial_println!("[VRAM] Mapeados {} x 2MB pages para VRAM", pages);

    let test_addr = vram_phys + pmoff;
    let test_val: u32 = 0xDEADBEEF;
    core::ptr::write_volatile(test_addr as *mut u32, test_val);
    let read_back = core::ptr::read_volatile(test_addr as *const u32);

    if read_back != test_val {
        serial_println!("[VRAM] {}: teste de escrita/leitura falhou (endereco {:#x})", gpu.name, vram_phys);
        return false;
    }

    let mut free_blocks = BTreeMap::new();
    free_blocks.insert(vram_phys + (1 << 20), vram_size - (1 << 20)); // reserva 1MB inicial

    *VRAM_STATE.lock() = Some(GpuVram {
        base: vram_phys,
        size: vram_size,
        gpu: gpu.clone(),
        free_blocks,
        page_map: |_| None,
    });

    serial_println!("[VRAM] {} mapeado: {:#x} ({} MB) free list ativa", gpu.name, vram_phys, vram_size / (1024*1024));
    true
}

/// Aloca na VRAM (first-fit free list)
pub fn vram_alloc(size: usize) -> Option<u64> {
    let mut guard = VRAM_STATE.lock();
    let vram = guard.as_mut()?;
    let aligned = (size as u64 + 0xFFF) & !0xFFF;

    // First-fit: busca o primeiro bloco livre com tamanho suficiente
    let mut found_start = None;
    for (&start, &block_size) in &vram.free_blocks {
        if block_size >= aligned {
            found_start = Some(start);
            break;
        }
    }

    let start = found_start?;
    let block_size = vram.free_blocks.remove(&start).unwrap();

    if block_size > aligned {
        vram.free_blocks.insert(start + aligned, block_size - aligned);
    }

    serial_println!("[VRAM] alloc {} bytes @ {:#x} (restam {} blocos livres)", size, start, vram.free_blocks.len());
    Some(start)
}

/// Libera endereco na VRAM com coalescing de adjacentes
pub fn vram_free(addr: u64, size: usize) {
    let mut guard = VRAM_STATE.lock();
    let vram = guard.as_mut().expect("VRAM not initialized");
    let aligned = (size as u64 + 0xFFF) & !0xFFF;

    // Coalesce com bloco anterior (addr_prev + size_prev == addr)
    let prev = vram.free_blocks.range(..addr).last().map(|(&k, &v)| (k, v));
    let merged_start = if let Some((prev_start, prev_size)) = prev {
        if prev_start + prev_size == addr {
            vram.free_blocks.remove(&prev_start);
            prev_start
        } else { addr }
    } else { addr };

    // Coalesce com próximo bloco (addr + aligned == next_start)
    let merged_size = if let Some((&next_start, &next_size)) = vram.free_blocks.range(addr..).next() {
        if addr + aligned == next_start {
            vram.free_blocks.remove(&next_start);
            aligned + next_size
        } else { aligned }
    } else { aligned };

    vram.free_blocks.insert(merged_start, merged_size);
    serial_println!("[VRAM] free {:#x} ({}) -> merged @ {:#x} size {}", addr, size, merged_start, merged_size);
}

/// Status da VRAM para debug
pub fn vram_status() -> alloc::string::String {
    let guard = VRAM_STATE.lock();
    if let Some(vram) = guard.as_ref() {
        let used = vram.size - vram.free_blocks.values().sum::<u64>();
        alloc::format!("VRAM: {}/{} MB used, {} free blocks",
            used / (1024*1024), vram.size / (1024*1024), vram.free_blocks.len())
    } else {
        alloc::string::String::from("VRAM: not initialized")
    }
}
