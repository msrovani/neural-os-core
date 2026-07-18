//! GGTT Intel — pin de páginas sysmem com bias WOPCM (ADR-0050 P2).
//! GuC/LRC/firmware DMA devem ficar acima da região WOPCM.

const GMADR_BASE: u64 = 0x100000;
const GFX_FLSH_CNTL: u64 = 0x101008;
/// Default WOPCM se reg ilegível (Gen9 tipicamente 512KiB–2MiB).
const DEFAULT_WOPCM_BYTES: u64 = 0x20_0000;
const GUC_WOPCM_SIZE: u64 = 0xC050;
const MAX_GTT_ENTRIES: u32 = 512;

/// Alocador GGTT simples (índices de página).
pub struct GgttPin {
    pub mmio: u64,
    pub wopcm_bytes: u64,
    pub next_index: u32,
    pub pinned: u32,
}

impl GgttPin {
    pub unsafe fn new(mmio: u64) -> Self {
        let wopcm_reg = core::ptr::read_volatile((mmio + GUC_WOPCM_SIZE) as *const u32);
        let wopcm = if wopcm_reg == 0 || wopcm_reg == 0xffff_ffff {
            DEFAULT_WOPCM_BYTES
        } else {
            // Bits inferiores frequentemente tamanho em bytes ou páginas — clamp.
            let b = (wopcm_reg as u64) & 0x00ff_ffff;
            if b < 0x10000 || b > 0x40_0000 {
                DEFAULT_WOPCM_BYTES
            } else {
                b
            }
        };
        let start = ((wopcm + 4095) / 4096) as u32;
        k_nano::slog_hal!("GGTT", "info", "WOPCM={}B bias_index={} (pin acima WOPCM)", wopcm, start);
        GgttPin {
            mmio,
            wopcm_bytes: wopcm,
            next_index: start.max(1),
            pinned: 0,
        }
    }

    /// Mapeia `pages` contíguas de `phys` → GGTT; retorna offset GGTT (bytes) = index*4096.
    pub unsafe fn pin_sys(&mut self, phys: u64, pages: u32) -> Option<u64> {
        if pages == 0 || self.next_index + pages > MAX_GTT_ENTRIES {
            k_nano::slog_hal!("GGTT", "info", "pin fail idx={} pages={} max={}", self.next_index, pages, MAX_GTT_ENTRIES);
            return None;
        }
        let gtt_base = self.mmio + GMADR_BASE;
        let start = self.next_index;
        for i in 0..pages {
            let pa = phys + (i as u64) * 4096;
            let entry: u64 = (pa & !0xfff) | 0x1; // PRESENT
            core::ptr::write_volatile(
                (gtt_base + ((start + i) as u64) * 8) as *mut u64,
                entry,
            );
        }
        core::ptr::write_volatile((self.mmio + GFX_FLSH_CNTL) as *mut u32, 0);
        self.next_index += pages;
        self.pinned += pages;
        let gtt_off = (start as u64) * 4096;
        k_nano::slog_hal!("GGTT", "info", "pin phys={:#x} pages={} → gtt_off={:#x}", phys, pages, gtt_off);
        Some(gtt_off)
    }
}
