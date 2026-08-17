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

/// ADR-0087 Fase 4b→5 (SESSION_274): copier tier1→tier0 via engine (CE DMA).
/// k_hal registra quando o canário CE passa (`ce_ready`); sem hook a promoção
/// Dram→Vram continua metadata-only + AWAITING (comportamento QEMU inalterado).
static mut TIER0_COPY_HOOK: Option<fn(u64, u64, usize) -> bool> = None;
static mut VRAM_FREE_HOOK: Option<fn(u64, usize)> = None;

pub fn register_tier0_copier(copy: fn(u64, u64, usize) -> bool, free: fn(u64, usize)) {
    unsafe {
        TIER0_COPY_HOOK = Some(copy);
        VRAM_FREE_HOOK = Some(free);
    }
    crate::slog_bin!("MHI", "info", "tier0 copier (CE DMA) registrado — Dram→Vram com dados reais");
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

/// Janela "quente" em ticks: acessos dentro desta janela contam para o streak
/// de histerese (mesma ordem dos thresholds de recency da escada).
const HOT_WINDOW_TICKS: u64 = 500;
/// Streak mínimo de acessos quentes para sugerir promoção (ADR-0087 §3, LWN 898766).
const HOT_HITS_PROMOTE: u32 = 2;

pub struct AllocProfile {
    pub phys_addr: PhysAddr,
    pub size_bytes: usize,
    pub tier: AllocTier,
    pub access_count: u64,
    pub last_access_tick: u64,
    /// Streak de acessos dentro da janela quente (histerese — zera no frio).
    pub hot_hits: u32,
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
            hot_hits: 0,
            owner: String::from(owner),
        }
    }
    pub fn record_access(&mut self, tick: u64) {
        self.access_count += 1;
        // Histerese (ADR-0087 §3): acesso dentro da janela quente incrementa o
        // streak; acesso frio (gap > janela) reinicia em 1. Promoção exige >= 2.
        if tick.saturating_sub(self.last_access_tick) < HOT_WINDOW_TICKS {
            self.hot_hits = self.hot_hits.saturating_add(1);
        } else {
            self.hot_hits = 1;
        }
        self.last_access_tick = tick;
    }
}

/// ADR-0087 §3: ids de tier (maior = mais rápido). SSD não existe no enum — omitido.
pub fn tier_id(tier: AllocTier) -> u32 {
    match tier {
        AllocTier::Vram => 300,
        AllocTier::Dram => 200,
        AllocTier::Nvme => 100,
        AllocTier::Hdd => 25,
        AllocTier::UsbMsc => 10,
    }
}

/// ADR-0087 §3: ordem de demotion explícita (quente → frio), NÃO hardcoded no
/// ladder de sugestão. Policy estilo Linux: a demotion segue esta lista quando
/// o tier de origem não comporta mais (evita saltos arbitrários Vram→Hdd).
pub const DEMOTION_ORDER: [AllocTier; 5] = [
    AllocTier::Vram,
    AllocTier::Dram,
    AllocTier::Nvme,
    AllocTier::Hdd,
    AllocTier::UsbMsc,
];

/// Próximo tier mais frio na ordem de demotion. None se já no mais frio.
pub fn demote_to(tier: AllocTier) -> Option<AllocTier> {
    DEMOTION_ORDER
        .iter()
        .position(|t| *t == tier)
        .and_then(|i| DEMOTION_ORDER.get(i + 1).copied())
}

/// ADR-0087 §3 (LWN 898766): rate limit da migração — evita thrash. Janela de
/// ticks + bytes máximos migrados por janela. Promoção async: excedeu o budget
/// → skip no tick atual, sem stall no path crítico.
const MIGRATION_RATE_WINDOW_TICKS: u64 = 100;
const MIGRATION_RATE_MAX_BYTES: u64 = 64 * 1024 * 1024; // 64MB / janela
static MIGRATION_WINDOW_START: AtomicU64 = AtomicU64::new(0);
static MIGRATION_WINDOW_BYTES: AtomicU64 = AtomicU64::new(0);

fn migration_rate_ok(tick: u64, size: usize) -> bool {
    let start = MIGRATION_WINDOW_START.load(Ordering::Relaxed);
    if tick.saturating_sub(start) >= MIGRATION_RATE_WINDOW_TICKS {
        // Nova janela
        MIGRATION_WINDOW_START.store(tick, Ordering::Relaxed);
        MIGRATION_WINDOW_BYTES.store(size as u64, Ordering::Relaxed);
        return size as u64 <= MIGRATION_RATE_MAX_BYTES;
    }
    let used = MIGRATION_WINDOW_BYTES.load(Ordering::Relaxed);
    if used.saturating_add(size as u64) > MIGRATION_RATE_MAX_BYTES {
        return false;
    }
    MIGRATION_WINDOW_BYTES.store(used + size as u64, Ordering::Relaxed);
    true
}

/// ZFS-ARC-style tier suggestion (ADR-0087 §3: VRAM na escada + histerese).
/// Promoção (sugerir tier mais quente que o atual) só quando o padrão de acesso
/// está ESTÁVEL (hot_hits >= 2 na janela quente) — evita thrash (LWN 898766).
pub fn arc_suggest_tier(profile: &AllocProfile, now: u64, _weight: f32) -> AllocTier {
    let freq = profile.access_count;
    let recency = now.saturating_sub(profile.last_access_tick);
    let stable_hot = profile.hot_hits >= HOT_HITS_PROMOTE;
    if stable_hot && freq > 10 && recency < 500 {
        return AllocTier::Vram; // working set quente → VRAM (peer DMA = Fase 4a HW)
    }
    if stable_hot && recency < 1000 {
        return AllocTier::Dram;
    }
    if stable_hot && recency < 3000 {
        return AllocTier::Nvme;
    }
    if profile.size_bytes > 1024 * 1024 {
        return AllocTier::Hdd;
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

    /// Remove alocação do registry (ex: vram_free) — evita crescimento infinito.
    pub fn unregister(&mut self, addr: u64) {
        self.allocations.remove(&addr);
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

/// ADR-0087 Fase 2 — wiring: call sites reais (disk I/O, msched) registram acesso.
/// Tick vem do contador global TIMER_TICKS (monotônico, incrementado no timer IRQ).
pub fn record_access(addr: u64, latency_ns: u32) {
    let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    MHI_REGISTRY.lock().record_access(PhysAddr::new(addr), tick, latency_ns);
}

/// Remove registro de alocação (ex: vram_free).
pub fn unregister(addr: u64) {
    MHI_REGISTRY.lock().unregister(addr);
}

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
/// - ADR-0087 §3: rate limit por janela (LWN 898766) — excedeu budget → skip
pub fn mhi_tick(tick: u64) {
    let migrations = {
        let reg = MHI_REGISTRY.lock();
        reg.suggest_migration(tick)
    };
    let mut budget_hit = false;
    for (addr, from, to) in migrations.iter().take(1) {
        let (size, owner) = {
            let reg = MHI_REGISTRY.lock();
            match reg.allocations.get(&addr.as_u64()) {
                Some(p) => (p.size_bytes, p.owner.clone()),
                None => (4096, String::from("mhi")),
            }
        };
        if !migration_rate_ok(tick, size) {
            budget_hit = true;
            continue;
        }
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
    if budget_hit {
        MHI_SKIPPED.fetch_add(1, Ordering::Relaxed);
    }

    let mut q = MIGRATION_QUEUE.lock();
    if let Some(req) = q.pop() {
        drop(q);
        execute_soft_migrate(req);
    }
}

/// Promoção Dram→Vram com DADOS (não só metadata): aloca no buddy VRAM,
/// copia via engine (hook CE) e re-registra no novo endereço. `false` = sem
/// hook / sem VRAM / cópia falhou — o caller segue no caminho metadata-only.
/// Rollback: VRAM alocada é devolvida se a cópia falhar (lição CoW F2).
fn try_tier0_promote(req: &MigrationRequest) -> bool {
    let copy = match unsafe { TIER0_COPY_HOOK } {
        Some(f) => f,
        None => return false,
    };
    let Some(dst) = alloc_by_tier(AllocTier::Vram, req.size) else {
        return false;
    };
    let dst_pa = dst.as_u64();
    if !copy(req.phys_addr, dst_pa, req.size) {
        if let Some(free) = unsafe { VRAM_FREE_HOOK } {
            free(dst_pa, req.size);
        }
        return false;
    }
    // vram_alloc (hook) já registrou dst como Vram/"vram" — re-registra com o
    // owner real da página promovida e remove o registro DRAM antigo.
    let mut reg = MHI_REGISTRY.lock();
    reg.allocations.remove(&req.phys_addr);
    reg.register(PhysAddr::new(dst_pa), req.size, AllocTier::Vram, &req.owner);
    drop(reg);
    MHI_SOFT_COPY.fetch_add(1, Ordering::Relaxed);
    crate::slog_nano!("MHI", "info", "tier0 promote Dram->Vram @{:x} -> @{:x} ({} B via CE)",
        req.phys_addr,
        dst_pa,
        req.size);
    true
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
        // Dram→Vram com engine real (CE) quando o hook está registrado —
        // fecha o seam morto `mhi_tier0_copy` (ADR-0087 F2: política só vale
        // com wiring). Falhou/ausente → cai no metadata-only abaixo.
        if req.to == AllocTier::Vram && try_tier0_promote(&req) {
            return;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> AllocProfile {
        AllocProfile::new(PhysAddr::new(0x1000), 4096, AllocTier::Hdd, "test")
    }

    #[test]
    fn tier_id_order() {
        assert!(tier_id(AllocTier::Vram) > tier_id(AllocTier::Dram));
        assert!(tier_id(AllocTier::Dram) > tier_id(AllocTier::Nvme));
        assert!(tier_id(AllocTier::Nvme) > tier_id(AllocTier::Hdd));
        assert!(tier_id(AllocTier::Hdd) > tier_id(AllocTier::UsbMsc));
        assert_eq!(tier_id(AllocTier::Vram), 300);
        assert_eq!(tier_id(AllocTier::Dram), 200);
        assert_eq!(tier_id(AllocTier::Nvme), 100);
        assert_eq!(tier_id(AllocTier::Hdd), 25);
        assert_eq!(tier_id(AllocTier::UsbMsc), 10);
    }

    #[test]
    fn hysteresis_no_thrash_single_access() {
        // Um acesso isolado NÃO promove (hot_hits=1 < 2).
        let mut p = profile();
        p.record_access(100);
        assert_eq!(arc_suggest_tier(&p, 110, 0.5), AllocTier::Hdd);
    }

    #[test]
    fn hysteresis_promotes_after_stable_hot() {
        // Dois acessos na janela quente → hot_hits=2 → promoção para Dram.
        let mut p = profile();
        p.record_access(100);
        p.record_access(200);
        assert_eq!(arc_suggest_tier(&p, 250, 0.5), AllocTier::Dram);
    }

    #[test]
    fn hysteresis_cold_gap_resets_streak() {
        // Acesso com gap > janela quente zera o streak (reinicia em 1).
        let mut p = profile();
        p.record_access(100);
        p.record_access(200); // hot_hits=2
        p.record_access(5000); // frio: gap 4800 > 500 → hot_hits=1
        assert_eq!(p.hot_hits, 1);
        assert_eq!(arc_suggest_tier(&p, 5100, 0.5), AllocTier::Hdd);
    }

    #[test]
    fn vram_suggested_for_hot_working_set() {
        let mut p = profile();
        for t in (100..=2000).step_by(50) {
            p.record_access(t);
        }
        assert!(p.access_count > 10);
        assert_eq!(arc_suggest_tier(&p, 2100, 0.5), AllocTier::Vram);
    }

    #[test]
    fn unregister_removes() {
        let mut reg = MhiRegistry::new();
        reg.register(PhysAddr::new(0x2000), 4096, AllocTier::Vram, "test");
        assert_eq!(reg.len(), 1);
        reg.unregister(0x2000);
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn demote_order_follows_list() {
        // Demotion segue DEMOTION_ORDER explícito (ADR-0087 §3), um degrau por vez.
        assert_eq!(demote_to(AllocTier::Vram), Some(AllocTier::Dram));
        assert_eq!(demote_to(AllocTier::Dram), Some(AllocTier::Nvme));
        assert_eq!(demote_to(AllocTier::Nvme), Some(AllocTier::Hdd));
        assert_eq!(demote_to(AllocTier::Hdd), Some(AllocTier::UsbMsc));
        assert_eq!(demote_to(AllocTier::UsbMsc), None);
    }

    #[test]
    fn rate_limit_blocks_over_budget() {
        // Budget 64MB/janela: 2x 40MB na mesma janela → 2ª bloqueada.
        let tick = 1000u64;
        assert!(migration_rate_ok(tick, 40 * 1024 * 1024));
        assert!(!migration_rate_ok(tick, 40 * 1024 * 1024));
        // Nova janela (gap ≥ 100 ticks) libera de novo.
        assert!(migration_rate_ok(tick + 100, 40 * 1024 * 1024));
    }

    #[test]
    fn rate_limit_small_accumulates() {
        let tick = 2000u64;
        assert!(migration_rate_ok(tick, 10 * 1024 * 1024));
        assert!(migration_rate_ok(tick, 10 * 1024 * 1024));
        assert!(migration_rate_ok(tick, 10 * 1024 * 1024));
        assert!(migration_rate_ok(tick, 10 * 1024 * 1024)); // 40MB < 64MB
        assert!(!migration_rate_ok(tick, 30 * 1024 * 1024)); // 70MB > 64MB
    }

    // ── SESSION_274: promoção tier0 (Dram→Vram) com engine registrado ──────

    const FAKE_VRAM_PA: u64 = 0xDEAD_0000;

    fn fake_vram_alloc(_size: usize) -> Option<u64> {
        Some(FAKE_VRAM_PA)
    }
    fn fake_ce_copy(_src: u64, dst: u64, _bytes: usize) -> bool {
        dst == FAKE_VRAM_PA
    }
    fn fake_ce_copy_fail(_src: u64, _dst: u64, _bytes: usize) -> bool {
        false
    }
    fn fake_vram_free(_addr: u64, _size: usize) {}

    #[test]
    fn tier0_promote_requires_hook_and_moves_registry() {
        let src = 0x7710_0000u64;
        let req = MigrationRequest {
            phys_addr: src,
            from: AllocTier::Dram,
            to: AllocTier::Vram,
            size: 4096,
            owner: String::from("kv_test"),
        };
        // Sem hook: honesto — false (caller cai no metadata-only/AWAITING).
        unsafe { TIER0_COPY_HOOK = None };
        assert!(!try_tier0_promote(&req));

        // Com hooks: registry migra Dram@src → Vram@dst preservando o owner.
        MHI_REGISTRY.lock().register(PhysAddr::new(src), 4096, AllocTier::Dram, "kv_test");
        register_vram_allocator(fake_vram_alloc);
        register_tier0_copier(fake_ce_copy, fake_vram_free);
        assert!(try_tier0_promote(&req));
        {
            let reg = MHI_REGISTRY.lock();
            assert!(reg.allocations.get(&src).is_none(), "registro DRAM antigo removido");
            let p = reg.allocations.get(&FAKE_VRAM_PA).expect("registrado na VRAM");
            assert_eq!(p.tier, AllocTier::Vram);
            assert_eq!(p.owner, "kv_test");
        }
        MHI_REGISTRY.lock().unregister(FAKE_VRAM_PA);

        // Cópia falhou → false (rollback via free hook; sem re-registro).
        register_tier0_copier(fake_ce_copy_fail, fake_vram_free);
        assert!(!try_tier0_promote(&req));
        assert!(MHI_REGISTRY.lock().allocations.get(&FAKE_VRAM_PA).is_none());
        unsafe {
            TIER0_COPY_HOOK = None;
            VRAM_FREE_HOOK = None;
            VRAM_ALLOC_HOOK = None;
        }
    }
}
