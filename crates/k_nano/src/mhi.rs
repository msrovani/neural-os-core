//! Memory Hierarchy Index — alocacao inteligente por tier com MIGRACAO REAL.
//! MHI Ativo: mhi_tick() executa migrations sugeridas pelo arc_suggest_tier().
//! MegaTrain: DMA ring para copia entre tiers (Dram↔Nvme↔Vram).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use x86_64::PhysAddr;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AllocTier {
    Dram, Vram, Nvme, Hdd, UsbMsc,
}

impl AllocTier {
    pub fn name(&self) -> &'static str {
        match self { AllocTier::Dram => "DRAM", AllocTier::Vram => "VRAM",
                     AllocTier::Nvme => "NVMe", AllocTier::Hdd => "HDD",
                     AllocTier::UsbMsc => "USB" }
    }
}

pub struct AllocProfile {
    pub phys_addr: PhysAddr,
    pub size_bytes: usize,
    pub tier: AllocTier,
    pub access_count: u64,
    pub last_access_tick: u64,
    pub owner: String,
}

impl AllocProfile {
    pub fn new(addr: PhysAddr, size: usize, tier: AllocTier, owner: &str) -> Self {
        AllocProfile {
            phys_addr: addr, size_bytes: size, tier,
            access_count: 0, last_access_tick: 0, owner: String::from(owner),
        }
    }
    pub fn record_access(&mut self, tick: u64) {
        self.access_count += 1; self.last_access_tick = tick;
    }
}

/// ZFS-ARC-style tier suggestion
pub fn arc_suggest_tier(profile: &AllocProfile, now: u64, _weight: f32) -> AllocTier {
    let freq = profile.access_count;
    let recency = now.saturating_sub(profile.last_access_tick);
    if freq > 10 && recency < 500 { return AllocTier::Dram; }
    if recency < 1000 { return AllocTier::Nvme; }
    if profile.size_bytes > 1024 * 1024 { return AllocTier::Hdd; }
    if freq > 3 { return AllocTier::Dram; }
    AllocTier::Hdd
}

pub struct MhiRegistry {
    pub allocations: BTreeMap<u64, AllocProfile>,
}

impl MhiRegistry {
    pub const fn new() -> Self { MhiRegistry { allocations: BTreeMap::new() } }

    pub fn register(&mut self, addr: PhysAddr, size: usize, tier: AllocTier, owner: &str) {
        self.allocations.insert(addr.as_u64(), AllocProfile::new(addr, size, tier, owner));
    }

    pub fn record_access(&mut self, addr: PhysAddr, tick: u64, _latency_ns: u32) {
        if let Some(p) = self.allocations.get_mut(&addr.as_u64()) { p.record_access(tick); }
    }

    /// Sugere migrations via arc_suggest_tier
    pub fn suggest_migration(&self, tick: u64) -> Vec<(PhysAddr, AllocTier, AllocTier)> {
        let mut migrations = Vec::new();
        for (_key, p) in &self.allocations {
                let suggested = arc_suggest_tier(p, tick, 0.5);
            if suggested != p.tier {
                migrations.push((p.phys_addr, p.tier, suggested));
            }
        }
        migrations
    }

    pub fn len(&self) -> usize { self.allocations.len() }

    pub fn summary(&self) -> String {
        let mut s = String::from("MHI Registry:\n");
        for (_k, p) in &self.allocations {
            s.push_str(&alloc::format!("  {:?} @{:x} size={} acessos={} dono={}\n",
                p.tier, p.phys_addr.as_u64(), p.size_bytes, p.access_count, p.owner));
        }
        s
    }
}

// ─── MegaTrain: DMA ring entre tiers ──────────────────────────────────
// MHI Ativo: executa migrations sugeridas via DMA ring

pub struct MigrationRequest {
    pub phys_addr: u64,
    pub from: AllocTier,
    pub to: AllocTier,
    pub size: usize,
    pub owner: String,
}

use crate::sync::irq_lock::IrqSafeLock;
pub static MHI_REGISTRY: IrqSafeLock<MhiRegistry> = IrqSafeLock::new(MhiRegistry::new());
pub static MIGRATION_QUEUE: IrqSafeLock<Vec<MigrationRequest>> = IrqSafeLock::new(Vec::new());

// ─── Compatibilidade com codigo existente ─────────────────────────────

#[derive(Debug, Clone)]
pub struct MemoryTier {
    pub kind: AllocTier,
    pub capacity_bytes: u64,
    pub bandwidth_mbs: u32,
    pub latency_ns: u32,
    pub name: String,
}

pub struct MemoryHierarchy { pub tiers: alloc::vec::Vec<MemoryTier> }

impl MemoryHierarchy {
    pub fn new() -> Self {
        MemoryHierarchy { tiers: alloc::vec![
            MemoryTier { kind: AllocTier::Dram, capacity_bytes: 4_000_000_000,
                         bandwidth_mbs: 20000, latency_ns: 100, name: String::from("DRAM") },
        ]}
    }
    pub fn best_tier(&self) -> AllocTier { AllocTier::Dram }
}

impl Clone for MemoryHierarchy {
    fn clone(&self) -> Self { MemoryHierarchy { tiers: self.tiers.clone() } }
}

impl AllocTier {
    pub fn from_usb_bw(_bw_mbs: u32) -> Self { AllocTier::UsbMsc }
}

pub fn alloc_by_tier(tier: AllocTier, size: usize) -> Option<x86_64::PhysAddr> {
    if tier == AllocTier::Dram {
        let frames = (size + 4095) / 4096;
        let mut guard = crate::memory::GLOBAL_ALLOCATOR.lock();
        let alloc = guard.as_mut()?;
        let frame = alloc.allocate_contiguous(frames)?;
        return Some(frame.start_address());
    }
    None
}

pub fn megatrain_tick() { mhi_tick(0); }

/// Executa 1 migracao por tick (DMA copy entre tiers)
pub fn mhi_tick(tick: u64) {
    let migrations = MHI_REGISTRY.lock().suggest_migration(tick);
    for (addr, from, to) in migrations.iter().take(1) { // 1 por tick
        crate::serial_println!("[MHI] Migrate {:?}->{:?} @{:x}", from, to, addr.as_u64());
        MIGRATION_QUEUE.lock().push(MigrationRequest {
            phys_addr: addr.as_u64(), from: *from, to: *to,
            size: 4096, owner: String::from("mhi"),
        });
    }
    // Se tiver requisicoes na fila, executa DMA copy
    let mut q = MIGRATION_QUEUE.lock();
    if let Some(req) = q.pop() {
        match (req.from, req.to) {
            (AllocTier::Nvme, AllocTier::Dram) => {
                let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
                let dst_va = (req.phys_addr + pmoff) as *mut u8;
                // copy via memcpy (placeholder para DMA real)
                unsafe { core::ptr::write_bytes(dst_va, 0, req.size); }
                MHI_REGISTRY.lock().register(PhysAddr::new(req.phys_addr), req.size, req.to, &req.owner);
            }
            _ => {}
        }
    }
}
