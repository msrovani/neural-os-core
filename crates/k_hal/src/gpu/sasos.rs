//! SASOS-lite: unified RAM+VRAM address space view (ADR-0047-GPU G3).
//! PoC maps logical UnifiedAddr → DramPtr | VramOff without full IOMMU.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnifiedKind {
    Dram,
    Vram,
}

#[derive(Clone, Copy, Debug)]
pub struct UnifiedAddr {
    pub kind: UnifiedKind,
    pub offset: u64,
    pub len: usize,
}

pub struct SasosMap {
    pub entries: alloc::vec::Vec<UnifiedAddr>,
    pub next_va: u64,
}

impl SasosMap {
    pub fn new() -> Self {
        SasosMap {
            entries: alloc::vec::Vec::new(),
            next_va: 0xA000_0000_0000,
        }
    }

    pub fn map_dram(&mut self, ptr: u64, len: usize) -> u64 {
        let va = self.next_va;
        self.next_va = self.next_va.wrapping_add(len as u64).wrapping_add(0x1000);
        self.entries.push(UnifiedAddr {
            kind: UnifiedKind::Dram,
            offset: ptr,
            len,
        });
        va
    }

    pub fn map_vram(&mut self, vram_off: u64, len: usize) -> u64 {
        let va = self.next_va;
        self.next_va = self.next_va.wrapping_add(len as u64).wrapping_add(0x1000);
        self.entries.push(UnifiedAddr {
            kind: UnifiedKind::Vram,
            offset: vram_off,
            len,
        });
        va
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

lazy_static::lazy_static! {
    static ref SASOS: spin::Mutex<SasosMap> = spin::Mutex::new(SasosMap::new());
}

/// Boot smoke: map one DRAM + one VRAM slot (VRAM may be 0 if absent).
pub fn gate_status(vram_available: bool) -> &'static str {
    let mut m = SASOS.lock();
    if m.count() == 0 {
        let _ = m.map_dram(0x1000, 4096);
        if vram_available {
            let _ = m.map_vram(0, 4096);
        }
    }
    k_nano::slog_hal!("ADR", "0047-G3", "sasos entries={} vram={}",
        m.count(),
        vram_available as u8);
    if m.count() > 0 {
        "OK"
    } else {
        "ABSENT"
    }
}
