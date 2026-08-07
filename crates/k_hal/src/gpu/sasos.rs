//! SASOS: espaço de endereço unificado RAM+VRAM (ADR-0047-GPU G3, ADR-0087
//! Fase 4a). Mapeia a aperture VRAM no range do heap (0x4020_0000_0000+) com
//! páginas UC (2MB huge) — o ponteiro unificado que `Tensor::location =
//! MemTier::Vram` (0047-GPU §7.4) usa. Consumidores: KV pages, tensores
//! pequenos, debug — acesso pontual por ponteiro; transfers bulk ficam no CE
//! (ADR-0087 §2.0.1: SASOS e CE são complementares).

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// VRAM no espaço do heap: 0x4020_0000_0000 — 0x4040_0000_0000 (128GB, ADR-0047-GPU §7.2).
pub const SASOS_VRAM_BASE: u64 = 0x4020_0000_0000;
/// Tamanho máximo do range SASOS VRAM.
pub const SASOS_VRAM_LIMIT: u64 = 0x0020_0000_0000;

static SASOS_VRAM_READY: AtomicBool = AtomicBool::new(false);
static SASOS_VRAM_PHYS: AtomicU64 = AtomicU64::new(0);
static SASOS_VRAM_SIZE: AtomicU64 = AtomicU64::new(0);

/// Mapeia a aperture VRAM física no espaço SASOS (UC, 2MB huge pages).
/// `vram_phys` deve ser 2MB-aligned (BARs são). Idempotente.
pub unsafe fn init_sasos_vram(vram_phys: u64, vram_size: u64, pmoff: u64) -> bool {
    if vram_phys == 0 || vram_size == 0 || SASOS_VRAM_READY.load(Ordering::Relaxed) {
        return SASOS_VRAM_READY.load(Ordering::Relaxed);
    }
    let size = vram_size.min(SASOS_VRAM_LIMIT);
    let pages = k_nano::apic::map_region_uc_2mb_at(SASOS_VRAM_BASE, vram_phys, size, pmoff);
    if pages == 0 {
        k_nano::slog_hal!("SASOS", "info", "init falhou: 0 pages mapeadas @ {:#x}", vram_phys);
        return false;
    }
    SASOS_VRAM_PHYS.store(vram_phys, Ordering::Release);
    SASOS_VRAM_SIZE.store(size, Ordering::Release);
    SASOS_VRAM_READY.store(true, Ordering::Release);
    k_nano::slog_hal!("SASOS", "info", "VRAM {:#x} ({:.1}MB) mapeada em {:#x}+ ({} x 2MB UC)",
        vram_phys, size as f64 / (1024.0 * 1024.0), SASOS_VRAM_BASE, pages);
    true
}

/// Converte offset na aperture VRAM → VA SASOS (ponteiro CPU para a VRAM).
/// None se SASOS não inicializado ou offset fora do range mapeado.
pub fn sasos_vram_ptr(vram_off: u64) -> Option<u64> {
    if !SASOS_VRAM_READY.load(Ordering::Acquire) {
        return None;
    }
    let size = SASOS_VRAM_SIZE.load(Ordering::Acquire);
    if vram_off + 1 > size {
        return None;
    }
    Some(SASOS_VRAM_BASE + vram_off)
}

/// Converte um endereço físico da aperture → VA SASOS (idem, por phys).
pub fn sasos_phys_to_ptr(vram_phys: u64) -> Option<u64> {
    let base = SASOS_VRAM_PHYS.load(Ordering::Acquire);
    if base == 0 || vram_phys < base {
        return None;
    }
    sasos_vram_ptr(vram_phys - base)
}

/// Gate de boot (main.rs) — reflete o mapeamento REAL (não o PoC simbólico).
pub fn gate_status(vram_available: bool) -> &'static str {
    k_nano::slog_hal!("ADR", "0047-G3", "sasos_vram_ready={} vram_available={}",
        SASOS_VRAM_READY.load(Ordering::Relaxed) as u8, vram_available as u8);
    if SASOS_VRAM_READY.load(Ordering::Relaxed) {
        "OK"
    } else if vram_available {
        "MAPPED-IDENTITY" // VRAM presente mas SASOS ainda não iniciado (HW)
    } else {
        "ABSENT"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ptr_range() {
        // Sem init, sasos_vram_ptr sempre None (honesto — não inventa ponteiro).
        assert!(sasos_vram_ptr(0).is_none());
        assert!(sasos_phys_to_ptr(0x1000).is_none());
    }
}
