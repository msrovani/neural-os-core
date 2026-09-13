//! Blit 2D acceleration — CPU fallback + Intel BCS ring.
//! Consumido via `blit_2d()` em backend.rs (padrão gpu_matmul).

use alloc::vec::Vec;
use crate::gpu::intel::BcsRing;
use crate::gpu::intel_gtt::GgttPin;
use crate::gpu::detect::GpuInfo;
use crate::unlock_dag::CapToken;
use crate::cap_gate::{check_map_bar, CapResult};
use k_nano::memory::PHYS_MEM_OFFSET;
use k_nano::slog_hal;
use core::sync::atomic::{AtomicU64, Ordering};

/// Engine de blit 2D.
pub enum BlitEngine {
    Cpu,
    IntelBcs(BcsRing),
}

/// Estado global do blit engine.
static BLIT_ENGINE: spin::Mutex<Option<BlitEngine>> = spin::Mutex::new(None);

/// MMIO base (BAR0 + pmoff) do engine Intel — para pin GGTT sob demanda.
static BLIT_MMIO: AtomicU64 = AtomicU64::new(0);

/// Cache PA→GGTT: BCS endereça por offset GGTT; repinar a cada blit esgotaria
/// as 512 entradas. Reusa regiões (PA base page-aligned + nº páginas).
struct PinEntry {
    pa: u64,
    pages: u32,
    gtt: u64,
}
static PIN_CACHE: spin::Mutex<Vec<PinEntry>> = spin::Mutex::new(Vec::new());
const PIN_CACHE_MAX: usize = 24;

/// Inicializa o blit engine baseado no backend atual.
/// Chamado de `init_backend()` após probe do BCS.
pub unsafe fn init_blit(gpu: &GpuInfo, pmoff: u64) {
    BLIT_MMIO.store(gpu.bar0 + pmoff, Ordering::Release);
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

/// Pina [pa, pa+bytes) no GGTT e devolve o offset GGTT do 1º byte.
/// `pin_sys` mapeia páginas **físicas contíguas**; o BCS (intel.rs) usa
/// pitch = w*bpp para src e dst, então o chamador deve fornecer um buffer
/// contíguo (rect de largura total). Fail → `None` → fallback CPU.
fn pin_region(pa: u64, bytes: usize) -> Option<u64> {
    let mmio = BLIT_MMIO.load(Ordering::Relaxed);
    if mmio == 0 || bytes == 0 {
        return None;
    }
    let offset = pa & 0xFFF;
    let base = pa & !0xFFF;
    let pages = (((offset as usize).saturating_add(bytes)).saturating_add(4095) / 4096) as u32;
    if pages == 0 {
        return None;
    }
    {
        let cache = PIN_CACHE.lock();
        for e in cache.iter() {
            if e.pa == base && e.pages == pages {
                return Some(e.gtt + offset);
            }
        }
    }
    let gtt_off = unsafe {
        let mut gtt = GgttPin::new(mmio);
        gtt.pin_sys(base, pages)?
    };
    {
        let mut cache = PIN_CACHE.lock();
        if cache.len() < PIN_CACHE_MAX {
            cache.push(PinEntry { pa: base, pages, gtt: gtt_off });
        } else {
            slog_hal!("BLIT", "pin", "cache cheio ({}), região não cacheada", PIN_CACHE_MAX);
        }
    }
    Some(gtt_off + offset)
}

/// Tenta o BCS p/ uma cópia tight (pitch = w*bpp). `None` = sem engine Intel
/// ou sem token `GpuBlitReady` (fora do canário) → caller faz CPU fallback.
/// `allow_unready` só é true dentro do próprio canário (chicken-and-egg do token).
fn try_bcs_blit(
    src_pa: u64,
    dst_pa: u64,
    w: u32,
    h: u32,
    bpp: u32,
    allow_unready: bool,
) -> Option<bool> {
    let mut guard = BLIT_ENGINE.lock();
    let bcs = match guard.as_mut() {
        Some(BlitEngine::IntelBcs(b)) => b,
        _ => return None,
    };
    if !allow_unready && !blit_ready() {
        return None;
    }
    if bpp != 4 || w == 0 || h == 0 {
        return None;
    }
    let bytes = (w as usize).saturating_mul(h as usize).saturating_mul(bpp as usize);
    let s = pin_region(src_pa, bytes)?;
    let d = pin_region(dst_pa, bytes)?;
    Some(bcs.blit(s, d, w, h, bpp))
}

/// Blit 2D genérico: src_pa → dst_pa, w×h, bpp (4 = BGRA/ARGB32).
/// Endereços são **físicos**; o backend BCS os pina no GGTT e passa offsets
/// (nunca PA/virtual cru). `blit_ready()` gateia o BCS; senão CPU.
pub fn blit_2d(src_pa: u64, dst_pa: u64, w: u32, h: u32, bpp: u32) -> bool {
    if let Some(true) = try_bcs_blit(src_pa, dst_pa, w, h, bpp, false) {
        return true;
    }
    cpu_blit(src_pa, dst_pa, w, h, bpp)
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

/// Fill retângulo com cor sólida (CPU).
/// NOTA: XY_COLOR_BLT (0x41) no BCS não é emitido — o encoding Gen9 precisa de
/// validação em metal (o canário só cobre XY_SRC_COPY_BLT). CPU é o default.
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
    
    // Executa blit via BCS DIRETO (canário roda antes do token existir).
    // Sem engine BCS o canário FALHA — nunca concede GpuBlitReady em CPU.
    let ok = match try_bcs_blit(src_pa, dst_pa, 64, 64, 4, true) {
        Some(v) => v,
        None => {
            slog_hal!("BLIT", "canary", "FAIL — engine BCS ausente (Cpu)");
            return false;
        }
    };
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