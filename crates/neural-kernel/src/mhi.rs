//! Memory Hierarchy Index — alocacao inteligente por tier.
//! Suporta DRAM, VRAM (stub), NVMe (stub), HDD (stub).
//! Perfil de uso por AllocProfile: acesso, latencia, tamanho.
//! Auto-migracao entre tiers baseada em padroes de uso.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use x86_64::PhysAddr;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AllocTier {
    Dram,
    Vram,
    Nvme,
    Hdd,
    UsbMsc,
}

#[derive(Debug, Clone)]
pub struct MemoryTier {
    pub kind: AllocTier,
    pub capacity_bytes: u64,
    pub bandwidth_mbs: u32,
    pub latency_ns: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct MemoryHierarchy {
    pub tiers: Vec<MemoryTier>,
}

impl MemoryHierarchy {
    pub fn new() -> Self {
        let total_ram = estimate_total_ram();
        MemoryHierarchy {
            tiers: alloc::vec![
                MemoryTier {
                    kind: AllocTier::Dram,
                    capacity_bytes: total_ram,
                    bandwidth_mbs: 20000,
                    latency_ns: 100,
                    name: String::from("DRAM"),
                },
            ],
        }
    }

    pub fn best_tier(&self) -> AllocTier {
        AllocTier::Dram
    }
}

fn estimate_total_ram() -> u64 {
    let guard = crate::memory::GLOBAL_ALLOCATOR.lock();
    guard.as_ref().map_or(0, |a| a.usable_memory_bytes())
}

// ---------------------------------------------------------------------------
// AllocProfile — metadados por alocacao
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AllocProfile {
    pub phys_addr: PhysAddr,
    pub size_bytes: usize,
    pub tier: AllocTier,
    pub access_count: u64,
    pub last_access_tick: u64,
    pub avg_latency_ns: u32,
    pub owner: String,
}

impl AllocProfile {
    pub fn new(addr: PhysAddr, size: usize, tier: AllocTier, owner: &str) -> Self {
        AllocProfile {
            phys_addr: addr,
            size_bytes: size,
            tier,
            access_count: 0,
            last_access_tick: 0,
            avg_latency_ns: 0,
            owner: String::from(owner),
        }
    }

    /// Registra um acesso e atualiza metadados
    pub fn record_access(&mut self, tick: u64, latency_ns: u32) {
        self.access_count += 1;
        self.last_access_tick = tick;
        self.avg_latency_ns = (self.avg_latency_ns + latency_ns) / 2;
    }
}

/// ZFS-ARC-style tier suggestion based on access patterns.
pub fn arc_suggest_tier(profile: &AllocProfile, now: u64, profile_weight: f32) -> AllocTier {
    let freq = profile.access_count;
    let recency = now.saturating_sub(profile.last_access_tick);
    let size = profile.size_bytes;
    let profile_w = profile_weight;

    // MFU (Most Frequently Used) → DRAM ou VRAM
    if freq > 10 && recency < 500 {
        if profile_w > 0.7 { return AllocTier::Vram; }
        return AllocTier::Dram;
    }

    // MRU (Most Recently Used) → NVMe (morno)
    if recency < 1000 {
        if profile_w > 0.5 { return AllocTier::Vram; }
        return AllocTier::Nvme;
    }

    // Size-aware: grande e frio → HDD
    if size > 1024 * 1024 { return AllocTier::Hdd; }

    // Pequeno e frio → DRAM
    if freq > 3 { return AllocTier::Dram; }

    AllocTier::Hdd
}

// ---------------------------------------------------------------------------
// MhiRegistry — gerenciamento centralizado
// ---------------------------------------------------------------------------
// MhiRegistry — gerenciamento centralizado
// ---------------------------------------------------------------------------

pub struct MhiRegistry {
    allocations: BTreeMap<u64, AllocProfile>, // PhysAddr.as_u64() -> profile
    next_id: u64,
}

impl MhiRegistry {
    pub const fn new() -> Self {
        MhiRegistry { allocations: BTreeMap::new(), next_id: 0 }
    }

    pub fn register(&mut self, addr: PhysAddr, size: usize, tier: AllocTier, owner: &str) {
        let profile = AllocProfile::new(addr, size, tier, owner);
        self.allocations.insert(addr.as_u64(), profile);
    }

    pub fn record_access(&mut self, addr: PhysAddr, tick: u64, latency_ns: u32) {
        if let Some(profile) = self.allocations.get_mut(&addr.as_u64()) {
            profile.record_access(tick, latency_ns);
        }
    }

    pub fn suggest_migration(&self, tick: u64) -> Vec<(PhysAddr, AllocTier, AllocTier)> {
        let profile = crate::profile::ProfileManager::get();
        let (cpu_w, gpu_w, _io_w) = profile.resource_weights();
        let mut migrations = Vec::new();
        for (_key, profile) in &self.allocations {
            let suggested = crate::mhi::arc_suggest_tier(profile, tick, gpu_w);
            if suggested != profile.tier {
                migrations.push((profile.phys_addr, profile.tier, suggested));
            }
        }
        migrations
    }

    pub fn len(&self) -> usize {
        self.allocations.len()
    }

    /// Resumo para display
    pub fn summary(&self) -> String {
        let mut s = String::from("MHI Registry:\n");
        for (_key, p) in &self.allocations {
            s.push_str(&alloc::format!("  {:?} @{:x} size={} acessos={} dono={}\n",
                p.tier, p.phys_addr.as_u64(), p.size_bytes, p.access_count, p.owner));
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Alloc by tier
// ---------------------------------------------------------------------------

pub fn alloc_by_tier(tier: AllocTier, size: usize) -> Option<PhysAddr> {
    match tier {
        AllocTier::Dram => {
            let num_frames = (size + 4095) / 4096;
            if num_frames == 0 { return None; }
            let mut guard = crate::memory::GLOBAL_ALLOCATOR.lock();
            let alloc = guard.as_mut()?;
            if num_frames == 1 {
                let frame = alloc.allocate_frame()?;
                return Some(frame.start_address());
            }
            let contiguous = alloc.allocate_contiguous(num_frames);
            if let Some(frame) = contiguous {
                return Some(frame.start_address());
            }
            let mut frames = alloc::vec::Vec::new();
            for _ in 0..num_frames {
                match alloc.allocate_frame() {
                    Some(f) => frames.push(f),
                    None => {
                        for f in frames {
                            unsafe { alloc.deallocate_frame(f); }
                        }
                        return None;
                    }
                }
            }
            Some(frames[0].start_address())
        }
        AllocTier::Vram => {
            crate::serial_println!("[MHI] Vram alloc not implemented (no GPU driver)");
            None
        }
        AllocTier::Nvme => {
            crate::serial_println!("[MHI] Nvme alloc not implemented (no NVMe driver)");
            None
        }
        AllocTier::Hdd => {
            crate::serial_println!("[MHI] Hdd alloc not implemented (no storage driver)");
            None
        }
        AllocTier::UsbMsc => {
            crate::serial_println!("[MHI] UsbMsc alloc delegated to UsbMscAgent");
            None
        }
    }
}

// Global registry
use spin::Mutex;

pub static MHI_REGISTRY: Mutex<MhiRegistry> = Mutex::new(MhiRegistry::new());
