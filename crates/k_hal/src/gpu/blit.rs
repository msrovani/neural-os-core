//! Blit 2D acceleration — CPU fallback + Intel BCS ring.
//! Consumido via `blit_2d()` em backend.rs (padrão gpu_matmul).

use alloc::vec::Vec;
use crate::gpu::intel::BcsRing;
use crate::gpu::detect::GpuInfo;
use crate::unlock_dag::CapToken;
use crate::cap_gate::{check_map_bar, CapResult};
use k_nano::memory::PHYS_MEM_OFFSET;
use k_nano::slog_hal;
use core::sync::atomic::Ordering;

/// Engine de blit 2D.
pub enum BlitEngine {
    Cpu,
    IntelBcs(BcsRing),
}

/// Estado global do blit engine.
static BLIT_ENGINE: spin::Mutex<Option<BlitEngine>> = spin::Mutex::new(None);

/// Inicializa o blit engine baseado no backend atual.
/// Chamado de `init_backend()` após probe do BCS.
pub unsafe fn init_blit(gpu: &GpuInfo, pmoff: u64) {
    let bcs = BcsRing::probe(gpu.bar0 + pmoff);
    let engine = if let Some(bcs) = bcs {
        slog_hal!("BLIT", "init", "Intel BCS probe OK — blit acelerado ativo");
        BlitEngine::IntelBcs(bcs)
    } else {
        slog_hal!("BLIT", "init", "BCS não disponível — fallback CPU");
        BlitEngine::Cpu
    };
    *BLIT_ENGINE.lock() = Some(engine);
}

/// Blit 2D genérico: src_pa → dst_pa, w×h, bpp (bytes per pixel: 4 para BGRA32).
/// Retorna true se sucesso.
pub fn blit_2d(src_pa: u64, dst_pa: u64, w: u32, h: u32, bpp: u32) -> bool {
    let mut guard = BLIT_ENGINE.lock();
    match guard.as_mut() {
        Some(BlitEngine::IntelBcs(bcs)) => {
            // BCS blit requer endereços físicos (GTT pinned)
            bcs.blit(src_pa, dst_pa, w, h, bpp)
        }
        Some(BlitEngine::Cpu) | None => {
            cpu_blit(src_pa, dst_pa, w, h, bpp)
        }
    }
}

/// CPU fallback: memcpy via framebuffer virtual addresses.
/// Converte endereços físicos para virtuais usando PHYS_MEM_OFFSET.
fn cpu_blit(src_pa: u64, dst_pa: u64, w: u32, h: u32, bpp: u32) -> bool {
    let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let src_va = (src_pa + pmoff) as *const u8;
    let dst_va = (dst_pa + pmoff) as *mut u8;
    let row_bytes = (w * bpp) as usize;
    let total = (row_bytes * h as usize) as usize;
    
    // Copia linha a linha para respeitar pitch/stride
    for y in 0..h as usize {
        let src_row = unsafe { src_va.add(y * row_bytes) };
        let dst_row = unsafe { dst_va.add(y * row_bytes) };
        unsafe { core::ptr::copy_nonoverlapping(src_row, dst_row, row_bytes); }
    }
    true
}

/// Fill retângulo com cor sólida (CPU fallback).
pub fn fill_rect_2d(dst_pa: u64, w: u32, h: u32, bpp: u32, color: u32) -> bool {
    let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let dst_va = (dst_pa + pmoff) as *mut u32;
    let pixels = (w * h) as usize;
    for i in 0..pixels {
        unsafe { dst_va.add(i).write_volatile(color); }
    }
    true
}

/// Canário blit 2D: desenha gradiente 64×64 no BCS, compara com golden CPU.
/// Se PASS, grant CapToken::GpuBlitReady.
pub unsafe fn run_blit_canary(gpu: &GpuInfo) -> bool {
    let pmoff = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    
    // Aloca buffers de teste (64×64×4 = 16KB cada)
    let mut alloc = k_nano::memory::GLOBAL_ALLOCATOR.lock();
    let a = match alloc.as_mut() {
        Some(a) => a,
        None => {
            slog_hal!("BLIT", "canary", "FAIL — allocator not available");
            return false;
        }
    };
    let src_frame = match a.allocate_contiguous(4) {
        Some(f) => f,
        None => {
            slog_hal!("BLIT", "canary", "FAIL — src allocation failed");
            return false;
        }
    };
    let dst_frame = match a.allocate_contiguous(4) {
        Some(f) => f,
        None => {
            slog_hal!("BLIT", "canary", "FAIL — dst allocation failed");
            return false;
        }
    };
    drop(alloc);
    
    let src_pa = src_frame.start_address().as_u64();
    let dst_pa = dst_frame.start_address().as_u64();
    
    // Preenche src com gradiente CPU (golden)
    let src_va = (src_pa + pmoff) as *mut u32;
    for y in 0..64 {
        for x in 0..64 {
            let r = (x * 255 / 63) as u32;
            let g = (y * 255 / 63) as u32;
            let b = ((x + y) * 255 / 126) as u32;
            unsafe { src_va.add(y * 64 + x).write_volatile(0xFF000000 | (r << 16) | (g << 8) | b); }
        }
    }
    
    // Executa blit via engine ativo
    let ok = blit_2d(src_pa, dst_pa, 64, 64, 4);
    if !ok {
        slog_hal!("BLIT", "canary", "FAIL — blit_2d returned false");
        return false;
    }
    
    // Compara dst com golden
    let dst_va = (dst_pa + pmoff) as *const u32;
    let mut pass = true;
    for y in 0..64 {
        for x in 0..64 {
            let expected = {
                let r = (x * 255 / 63) as u32;
                let g = (y * 255 / 63) as u32;
                let b = ((x + y) * 255 / 126) as u32;
                0xFF000000 | (r << 16) | (g << 8) | b
            };
            let actual = unsafe { dst_va.add(y * 64 + x).read_volatile() };
            if actual != expected {
                slog_hal!("BLIT", "canary", "MISMATCH at ({},{}) expected={:#x} actual={:#x}", x, y, expected, actual);
                pass = false;
            }
        }
    }
    
    if pass {
        crate::unlock_dag::grant(CapToken::GpuBlitReady);
        slog_hal!("BLIT", "canary", "PASS — GpuBlitReady granted");
    } else {
        slog_hal!("BLIT", "canary", "FAIL — golden mismatch");
    }
    pass
}

/// Verifica se blit acelerado está disponível.
pub fn blit_ready() -> bool {
    crate::unlock_dag::has(CapToken::GpuBlitReady)
}