//! MhiScheduler — varre periodicamente os AllocProfiles e promove/demove
//! arquivos entre tiers MHI baseado em padroes de acesso.
//!
//! Aquecimento (promocao): access_count > 5 em 500 ticks → HDD→DRAM
//! Resfriamento (democao): sem acesso por 5000 ticks → DRAM→HDD

use core::sync::atomic::{AtomicU64, Ordering};

const PROMOTE_ACCESS_THRESHOLD: u64 = 5;
const PROMOTE_TICK_WINDOW: u64 = 500;
const DEMOTE_IDLE_TICKS: u64 = 5000;
const SCAN_INTERVAL: u64 = 1000;

static LAST_SCAN_TICK: AtomicU64 = AtomicU64::new(0);

/// Deve ser chamado a cada tick do OptimizerAgent
pub fn mhi_scheduler_tick(tick: u64) {
    let last = LAST_SCAN_TICK.load(Ordering::Relaxed);
    if tick < last + SCAN_INTERVAL { return; }
    LAST_SCAN_TICK.store(tick, Ordering::Relaxed);

    let reg = crate::mhi::MHI_REGISTRY.lock();
    let mut promotions = 0u64;
    let _demotions = 0u64;

    for (_addr, profile) in reg.allocations.iter() {
        let freq = profile.access_count;
        let idle = tick.saturating_sub(profile.last_access_tick);
        let (_cpu_w, gpu_w, _io_w) = crate::profile::ProfileManager::get().resource_weights();
        let profile_weight = gpu_w;
        let suggested = crate::mhi::arc_suggest_tier(profile, tick, profile_weight);

        if suggested != profile.tier {
            if freq > PROMOTE_ACCESS_THRESHOLD && idle < PROMOTE_TICK_WINDOW {
                // Promocao: tier mais quente
                crate::serial_println!("[MHI] Promover {:?} {}H{}.tier {:?}→{:?} (freq={} idle={})",
                    profile.phys_addr, profile.owner, freq, profile.tier, suggested, freq, idle);
                promotions += 1;
            } else if idle > DEMOTE_IDLE_TICKS {
                // Democao: tier mais frio
                crate::serial_println!("[MHI] Demover {:?} (freq={} idle={})",
                    profile.phys_addr, freq, idle);
            }
        }
    }
}
