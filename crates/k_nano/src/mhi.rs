//! Memory Hierarchy Index — alocacao inteligente por tier com migracao soft.
//! MHI Ativo: mhi_tick() executa 1 migracao/tick (metadata + memcpy DRAM quando seguro).
//! DMA NVMe/VRAM peer = AWAITING_HW (ADR-0040 / IDEA #420/#423) — logs `[MHI-DMA]`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use x86_64::PhysAddr;

/// Hook opcional: k_hal/vram registra apos `init_vram_tier` (IDEA #67).
static mut VRAM_ALLOC_HOOK: Option<fn(usize) -> Option<u64>> = None;
static MHI_DMA_LOGGED: AtomicBool = AtomicBool::new(false);

/// Registra alocador VRAM (BAR buddy). Sem hook = Vram tier retorna None.
pub fn register_vram_allocator(hook: fn(usize) -> Option<u64>) {
    unsafe { VRAM_ALLOC_HOOK = Some(hook) };
    crate::slog_bin!("MHI", "info", "VRAM alloc hook registered (IDEA #67)");
}

fn log_mhi_dma_awaiting(reason: &str) {
    if MHI_DMA_LOGGED.swap(true, Ordering::Relaxed) {
        return;
    }
    crate::slog_bin!(
        "MHI-DMA",
        "info",
        "step=peer_dma status=UNSUPPORTED detail={}",
        reason
    );
    crate::slog_bin!(
        "MHI-DMA",
        "info",
        "VERDICT=AWAITING_REAL_HW reason={}",
        reason
    );
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AllocTier {
    Dram, Vram, Nvme, Hdd, UsbMsc,
}

impl AllocTier {
    pub fn name(&self) -> &'static str {
        match self {
            AllocTier::Dram => "DRAM",
            AllocTier::Vram => "VRAM",
            AllocTier::Nvme => "NVMe",
            AllocTier::Hdd => "HDD",
            AllocTier::UsbMsc => "USB",
        }
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
            phys_addr: addr,
            size_bytes: size,
            tier,
            access_count: 0,
            last_access_tick: 0,
            owner: String::from(owner),
        }
    }
    pub fn record_access(&mut self, tick: u64) {
        self.access_count += 1;
        self.last_access_tick = tick;
    }
}

/// ZFS-ARC-style tier suggestion
pub fn arc_suggest_tier(profile: &AllocProfile, now: u64, _weight: f32) -> AllocTier {
    let freq = profile.access_count;
    let recency = now.saturating_sub(profile.last_access_tick);
    if freq > 10 && recency < 500 {
        return AllocTier::Dram;
    }
    if recency < 1000 {
        return AllocTier::Nvme;
    }
    if profile.size_bytes > 1024 * 1024 {
        return AllocTier::Hdd;
    }
    if freq > 3 {
        return AllocTier::Dram;
    }
    AllocTier::Hdd
}

pub struct MhiRegistry {
    pub allocations: BTreeMap<u64, AllocProfile>,
}

impl MhiRegistry {
    pub const fn new() -> Self {
        MhiRegistry {
            allocations: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, addr: PhysAddr, size: usize, tier: AllocTier, owner: &str) {
        self.allocations
            .insert(addr.as_u64(), AllocProfile::new(addr, size, tier, owner));
    }

    pub fn record_access(&mut self, addr: PhysAddr, tick: u64, _latency_ns: u32) {
        if let Some(p) = self.allocations.get_mut(&addr.as_u64()) {
            p.record_access(tick);
        }
    }

    /// Update tier in-place (soft migrate metadata).
    pub fn set_tier(&mut self, addr: PhysAddr, tier: AllocTier) -> bool {
        if let Some(p) = self.allocations.get_mut(&addr.as_u64()) {
            p.tier = tier;
            true
        } else {
            false
        }
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

    pub fn len(&self) -> usize {
        self.allocations.len()
    }

    pub fn summary(&self) -> String {
        let mut s = String::from("MHI Registry:\n");
        for (_k, p) in &self.allocations {
            s.push_str(&alloc::format!(
                "  {:?} @{:x} size={} acessos={} dono={}\n",
                p.tier,
                p.phys_addr.as_u64(),
                p.size_bytes,
                p.access_count,
                p.owner
            ));
        }
        s
    }
}

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

/// Soft-migrate counters (honest MVP — not full DMA).
pub static MHI_SOFT_META: AtomicU64 = AtomicU64::new(0);
pub static MHI_SOFT_COPY: AtomicU64 = AtomicU64::new(0);
pub static MHI_SKIPPED: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct MemoryTier {
    pub kind: AllocTier,
    pub capacity_bytes: u64,
    pub bandwidth_mbs: u32,
    pub latency_ns: u32,
    pub name: String,
}

pub struct MemoryHierarchy {
    pub tiers: alloc::vec::Vec<MemoryTier>,
}

impl MemoryHierarchy {
    pub fn new() -> Self {
        MemoryHierarchy {
            tiers: alloc::vec![MemoryTier {
                kind: AllocTier::Dram,
                capacity_bytes: 4_000_000_000,
                bandwidth_mbs: 20000,
                latency_ns: 100,
                name: String::from("DRAM"),
            }],
        }
    }
    pub fn best_tier(&self) -> AllocTier {
        AllocTier::Dram
    }
}

impl Clone for MemoryHierarchy {
    fn clone(&self) -> Self {
        MemoryHierarchy {
            tiers: self.tiers.clone(),
        }
    }
}

impl AllocTier {
    pub fn from_usb_bw(_bw_mbs: u32) -> Self {
        AllocTier::UsbMsc
    }
}

pub fn alloc_by_tier(tier: AllocTier, size: usize) -> Option<x86_64::PhysAddr> {
    match tier {
        AllocTier::Dram => {
            let frames = (size + 4095) / 4096;
            let mut guard = crate::memory::GLOBAL_ALLOCATOR.lock();
            let alloc = guard.as_mut()?;
            let frame = alloc.allocate_contiguous(frames)?;
            Some(frame.start_address())
        }
        AllocTier::Vram => {
            let hook = unsafe { VRAM_ALLOC_HOOK };
            if let Some(f) = hook {
                if let Some(pa) = f(size) {
                    return Some(PhysAddr::new(pa));
                }
            }
            log_mhi_dma_awaiting("vram_tier_unavailable");
            None
        }
        AllocTier::Nvme | AllocTier::Hdd | AllocTier::UsbMsc => {
            log_mhi_dma_awaiting("block_tier_alloc_needs_dma");
            None
        }
    }
}

pub fn megatrain_tick() {
    mhi_tick(0);
}

/// Max bytes for DRAM→DRAM soft copy (avoid huge partition stubs).
const SOFT_COPY_MAX: usize = 4 * 1024 * 1024;

/// Executa 1 migracao por tick.
/// - Dram↔Dram (paginas reais, size limitado): memcpy + re-register
/// - Demais: update de tier metadata only (DMA NVMe/VRAM deferido)
/// - NUNCA zera memoria (bug antigo do placeholder write_bytes)
pub fn mhi_tick(tick: u64) {
    let migrations = {
        let reg = MHI_REGISTRY.lock();
        reg.suggest_migration(tick)
    };
    for (addr, from, to) in migrations.iter().take(1) {
        let (size, owner) = {
            let reg = MHI_REGISTRY.lock();
            match reg.allocations.get(&addr.as_u64()) {
                Some(p) => (p.size_bytes, p.owner.clone()),
                None => (4096, String::from("mhi")),
            }
        };
        crate::slog_nano!("MHI", "info", "Queue migrate {:?}->{:?} @{:x} size={}",
            from,
            to,
            addr.as_u64(),
            size);
        MIGRATION_QUEUE.lock().push(MigrationRequest {
            phys_addr: addr.as_u64(),
            from: *from,
            to: *to,
            size,
            owner,
        });
    }

    let mut q = MIGRATION_QUEUE.lock();
    if let Some(req) = q.pop() {
        drop(q);
        execute_soft_migrate(req);
    }
}

fn execute_soft_migrate(req: MigrationRequest) {
    // Partition stubs register LBA*sector as "phys" — never memcpy those.
    let looks_like_ram_page = req.from == AllocTier::Dram
        && req.size > 0
        && req.size <= SOFT_COPY_MAX
        && (req.phys_addr % 4096 == 0);

    if looks_like_ram_page && req.to == AllocTier::Dram {
        // Same-tier noop
        MHI_SKIPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }

    if looks_like_ram_page && req.to != AllocTier::Dram {
        // Demote Dram→cold: metadata only (no disk/VRAM peer DMA yet)
        let needs_peer = matches!(req.to, AllocTier::Vram | AllocTier::Nvme);
        if needs_peer {
            log_mhi_dma_awaiting("dram_to_peer_dma");
        }
        let ok = MHI_REGISTRY
            .lock()
            .set_tier(PhysAddr::new(req.phys_addr), req.to);
        if ok {
            MHI_SOFT_META.fetch_add(1, Ordering::Relaxed);
            crate::slog_nano!("MHI", "info", "soft-demote Dram->{:?} @{:x} size={} (DMA disk deferred)",
                req.to,
                req.phys_addr,
                req.size);
        } else {
            MHI_SKIPPED.fetch_add(1, Ordering::Relaxed);
        }
        return;
    }

    if req.to == AllocTier::Dram && req.from == AllocTier::Dram {
        // Promote within DRAM working set: allocate + memcpy (proves path non-destructive)
        if req.size == 0 || req.size > SOFT_COPY_MAX {
            MHI_SKIPPED.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        if pmoff == 0 {
            MHI_SKIPPED.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if let Some(new_pa) = alloc_by_tier(AllocTier::Dram, req.size) {
            let src = (req.phys_addr + pmoff) as *const u8;
            let dst = (new_pa.as_u64() + pmoff) as *mut u8;
            unsafe {
                core::ptr::copy_nonoverlapping(src, dst, req.size);
            }
            let mut reg = MHI_REGISTRY.lock();
            reg.allocations.remove(&req.phys_addr);
            reg.register(new_pa, req.size, AllocTier::Dram, &req.owner);
            MHI_SOFT_COPY.fetch_add(1, Ordering::Relaxed);
            crate::slog_nano!("MHI", "info", "soft-copy Dram page @{:x} -> @{:x} ({} bytes)",
                req.phys_addr,
                new_pa.as_u64(),
                req.size);
            return;
        }
        MHI_SKIPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }

    // Hdd/Nvme/Usb/Vram <-> *: metadata-only (block/GPU peer DMA not wired)
    if matches!(req.from, AllocTier::Vram | AllocTier::Nvme)
        || matches!(req.to, AllocTier::Vram | AllocTier::Nvme)
    {
        log_mhi_dma_awaiting("cross_tier_peer_dma");
    }
    let ok = MHI_REGISTRY
        .lock()
        .set_tier(PhysAddr::new(req.phys_addr), req.to);
    if ok {
        MHI_SOFT_META.fetch_add(1, Ordering::Relaxed);
        crate::slog_nano!("MHI", "info", "soft-meta {:?}->{:?} @{:x} (no DMA; ADR-0040 defer)",
            req.from,
            req.to,
            req.phys_addr);
    } else {
        MHI_SKIPPED.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn migration_stats() -> (u64, u64, u64) {
    (
        MHI_SOFT_META.load(Ordering::Relaxed),
        MHI_SOFT_COPY.load(Ordering::Relaxed),
        MHI_SKIPPED.load(Ordering::Relaxed),
    )
}
