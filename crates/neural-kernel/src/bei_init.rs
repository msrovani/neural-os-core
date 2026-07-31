//! BEI (BitNet Ecosystem Intelligence) Initialization
//! ADR-0060: Wire all 8 waves into the boot process.
//! This module creates and connects all BEI components.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::vec;
use spin::Mutex;
use k_nano::sync::mpmc::MpmcQueue;
use k_ai::{economy::BudgetManager, expert_lifecycle::ExpertLifecycleManager};
use cortex_crate::{cellular::{CellNetwork, CellType, CellId}, evolution::PlasticityController, moe::DynamicMoE};
use hermes_crate::{memory::{MemoryStore, MemoryLevel}, affect::{AffectRegulator, AffectVector, AffectEvent}, executive::{ExecutiveSupervisor, LoopPhase, SupervisorVerdict}};
use jarbas_crate::display::soul_mirror::SoulMirrorState;


/// Global BEI state container
pub struct BeiState {
    // Wave 0: Communication
    pub cell_message_queue: Arc<MpmcQueue<cortex_crate::cellular::CellMessage>>,
    
    // Wave 1: Economy + Lifecycle
    pub budget_manager: Arc<Mutex<BudgetManager>>,
    pub expert_lifecycle: Arc<Mutex<ExpertLifecycleManager>>,
    
    // Wave 2: Cellular + Evolution
    pub cell_network: Arc<Mutex<CellNetwork>>,
    pub plasticity_controller: Arc<Mutex<PlasticityController>>,
    
    // Wave 3: Dynamic MoE
    pub dynamic_moe: Arc<Mutex<DynamicMoE>>,
    
    // Wave 4: Memory L0-L7
    pub memory_store: Arc<Mutex<MemoryStore>>,
    
    // Wave 5: Affect
    pub affect_regulator: Arc<Mutex<AffectRegulator>>,
    
    // Wave 6: Supervisor
    pub executive_supervisor: Arc<Mutex<ExecutiveSupervisor>>,
    
    // Wave 7: Soul Mirror (state synced in tick; render owned by compositor)
    pub soul_mirror_state: Arc<Mutex<SoulMirrorState>>,
    
    // Current tick for synchronization
    pub current_tick: Arc<Mutex<u64>>,
}

impl BeiState {
    /// Initialize all BEI components (called after heap is ready)
    pub fn new() -> Self {
        k_nano::slog_bin!("BEI", "init", "Starting BEI initialization (8 waves)");
        
        // ─── Wave 0: MPMC Queue ───
        let cell_message_queue = Arc::new(
            MpmcQueue::<cortex_crate::cellular::CellMessage>::new(256)
                .expect("Failed to create MPMC queue for cell messages")
        );
        k_nano::slog_bin!("BEI", "wave0", "MPMC queue created (cap=256)");
        
        // ─── Wave 1: Economy + Expert Lifecycle ───
        let budget_manager = Arc::new(Mutex::new(
            BudgetManager::new(64 * 1024 * 1024) // 64MB budget
        ));
        let expert_lifecycle = Arc::new(Mutex::new(ExpertLifecycleManager::new()));
        k_nano::slog_bin!("BEI", "wave1", "BudgetManager + ExpertLifecycleManager created");
        
        // ─── Wave 2: Cellular Network + Plasticity ───
        let cell_network = Arc::new(Mutex::new(
            CellNetwork::new(64, 10) // inbox_cap=64, budget_per_tick=10
                .expect("Failed to create CellNetwork")
        ));
        let plasticity_controller = Arc::new(Mutex::new(
            PlasticityController::new(8, 0.7, 0.1) // 8 regions, growth=0.7, prune=0.1
        ));
        k_nano::slog_bin!("BEI", "wave2", "CellNetwork (8 regions) + PlasticityController created");
        
        // Spawn initial cells per region
        {
            let mut net = cell_network.lock();
            for region in 0..8 {
                // 1 Reasoning + 1 Memory cell per region
                let _ = net.spawn_cell(CellType::Reasoning, region);
                let _ = net.spawn_cell(CellType::Memory, region);
            }
            k_nano::slog_bin!("BEI", "wave2", "Spawned {} cells across 8 regions", net.cell_count());
        }
        
        // ─── Wave 3: Dynamic MoE ───
        // Note: DynamicMoE needs a base MoELayer. We'll create a minimal one.
        let dynamic_moe = Arc::new(Mutex::new(Self::create_dynamic_moe()));
        k_nano::slog_bin!("BEI", "wave3", "DynamicMoE created");
        
        // ─── Wave 4: Memory L0-L7 ───
        let memory_store = Arc::new(Mutex::new({
            let mut store = MemoryStore::new();
            store.init_default_tiers();
            store
        }));
        k_nano::slog_bin!("BEI", "wave4", "MemoryStore L0-L7 initialized");
        
        // ─── Wave 5: Affect Regulator ───
        let affect_regulator = Arc::new(Mutex::new(AffectRegulator::new()));
        k_nano::slog_bin!("BEI", "wave5", "AffectRegulator created (neutral state)");
        
        // ─── Wave 6: Executive Supervisor ───
        let executive_supervisor = Arc::new(Mutex::new(ExecutiveSupervisor::new()));
        k_nano::slog_bin!("BEI", "wave6", "ExecutiveSupervisor created (7-phase loop)");
        
        // ─── Wave 7: Soul Mirror ───
        let soul_mirror_state = Arc::new(Mutex::new(SoulMirrorState::neutral()));
        k_nano::slog_bin!("BEI", "wave7", "SoulMirrorState created");
        
        // ─── Cross-connections ───
        Self::connect_components(
            &executive_supervisor,
            &affect_regulator,
            &memory_store,
            &cell_network,
            &plasticity_controller,
            &dynamic_moe,
            &expert_lifecycle,
            &budget_manager,
        );
        
        k_nano::slog_bin!("BEI", "init", "All 8 waves initialized and connected");
        
        BeiState {
            cell_message_queue,
            budget_manager,
            expert_lifecycle,
            cell_network,
            plasticity_controller,
            dynamic_moe,
            memory_store,
            affect_regulator,
            executive_supervisor,
            soul_mirror_state,
            current_tick: Arc::new(Mutex::new(0)),
        }
    }
    
    fn create_dynamic_moe() -> DynamicMoE {
        use cortex_crate::{moe::{MoELayer, MoEConfig, Int8Router}, nn::BitLinear, tensor::PackedTernaryTensor};
        
        let hidden = 64;
        let n = 4;
        let top_k = 2;
        
        let make_linear = || BitLinear::new(
            PackedTernaryTensor { shape: (hidden, hidden), packed_data: alloc::vec![0u8; (hidden * hidden + 3) / 4] },
            None,
        );
        let shared = make_linear();
        let router = Int8Router::new(hidden, n);
        let mut experts = alloc::vec::Vec::with_capacity(n);
        for _ in 0..n { experts.push(make_linear()); }
        
        let config = MoEConfig::new(n, top_k, hidden);
        let layer = MoELayer::new(config, shared, router, experts);
        DynamicMoE::new(layer)
    }
    
    /// Connect all BEI components together
    fn connect_components(
        executive_supervisor: &Arc<Mutex<ExecutiveSupervisor>>,
        affect_regulator: &Arc<Mutex<AffectRegulator>>,
        memory_store: &Arc<Mutex<MemoryStore>>,
        cell_network: &Arc<Mutex<CellNetwork>>,
        plasticity_controller: &Arc<Mutex<PlasticityController>>,
        dynamic_moe: &Arc<Mutex<DynamicMoE>>,
        expert_lifecycle: &Arc<Mutex<ExpertLifecycleManager>>,
        budget_manager: &Arc<Mutex<BudgetManager>>,
    ) {
        // ExecutiveSupervisor already contains EgoLayer, PonderNet, EntropyMonitor, AffectRegulator
        // The affect_regulator is shared - supervisor has its own but we sync them
        
        // Connect MemoryStore to ExecutiveSupervisor (for domain confidence tracking)
        // This is done via the supervisor's record_result which updates ego layer
        
        // Connect PlasticityController to CellNetwork (growth/pruning signals)
        // This will be done in the BEI tick function
        
        // Connect DynamicMoE to ExpertLifecycleManager (birth/merge/split)
        // This will be done in the BEI tick function
        
        // Connect BudgetManager to DynamicMoE (compression tier decisions)
        // This will be done in the BEI tick function
        
        k_nano::slog_bin!("BEI", "connect", "Cross-component connections established");
    }
    
    /// Main BEI tick - called every scheduler tick
    pub fn tick(&self) {
        let mut tick = self.current_tick.lock();
        *tick += 1;
        let current_tick = *tick;
        drop(tick);
        
        // 1. Advance CellNetwork scheduler
        {
            let mut net = self.cell_network.lock();
            net.tick_advance();
            
            // Round-robin schedule cells
            while let Some((cell_id, messages)) = net.round_robin() {
                // Process messages for this cell
                // In a real implementation, this would invoke the cell's compute
                net.mark_processed(cell_id);
            }
            
            // Reap dead cells
            net.reap_dead();
        }
        
        // 2. Advance PlasticityController
        {
            let mut pc = self.plasticity_controller.lock();
            pc.tick_advance();
            
            // Check for growth/pruning per region
            for region in 0..pc.num_regions() {
                if pc.should_grow(region) {
                    k_nano::slog_bin!("BEI", "plasticity", "Region {} should GROW (entropy={:.2})", region, pc.region_entropy(region));
                    // Spawn new cell in this region
                    let mut net = self.cell_network.lock();
                    let _ = net.spawn_cell(CellType::Reasoning, region);
                }
                if pc.should_prune(region) {
                    k_nano::slog_bin!("BEI", "plasticity", "Region {} should PRUNE (activation={:.2})", region, pc.region_activation[region]);
                    // Mark lowest-activation cell in region for death
                    let dead_id = {
                        let net = self.cell_network.lock();
                        net.cells().iter()
                            .filter(|c| c.region == region && c.state != cortex_crate::cellular::CellState::Dead)
                            .min_by(|a, b| a.unprocessed().cmp(&b.unprocessed()))
                            .map(|c| c.id)
                    };
                    if let Some(id) = dead_id {
                        self.cell_network.lock().mark_dead(id);
                    }
                }
            }
        }
        
        // 3. Advance MemoryStore (TTL, promotion)
        {
            let mut mem = self.memory_store.lock();
            mem.tick_advance();
        }
        
        // 4. Decay AffectRegulator
        {
            let mut affect = self.affect_regulator.lock();
            affect.decay();
        }
        
        // 5. ExecutiveSupervisor tick (7-phase loop)
        {
            let mut supervisor = self.executive_supervisor.lock();
            let verdict = supervisor.tick_supervise(10); // base budget = 10
            
            // Handle verdict
            match verdict {
                SupervisorVerdict::Proceed => {}
                SupervisorVerdict::ProceedWithBudget(budget) => {
                    k_nano::slog_bin!("BEI", "supervisor", "ProceedWithBudget: {}", budget);
                }
                SupervisorVerdict::Ponder(steps) => {
                    k_nano::slog_bin!("BEI", "supervisor", "Ponder: {} steps", steps);
                }
                SupervisorVerdict::Delay { reason, until_tick } => {
                    k_nano::slog_bin!("BEI", "supervisor", "Delay: {} until {}", reason, until_tick);
                }
                SupervisorVerdict::Preempt { reason } => {
                    k_nano::slog_bin!("BEI", "supervisor", "Preempt: {}", reason);
                }
                SupervisorVerdict::Escalate { reason } => {
                    k_nano::slog_bin!("BEI", "supervisor", "ESCALATE: {}", reason);
                }
                SupervisorVerdict::Train { domain, reason } => {
                    k_nano::slog_bin!("BEI", "supervisor", "Train domain: {} ({})", domain, reason);
                    // Trigger BitNetTrainer for this domain
                }
                SupervisorVerdict::PromoteSkill { skill_name } => {
                    k_nano::slog_bin!("BEI", "supervisor", "PromoteSkill: {}", skill_name);
                }
            }
        }
        
        // 6. Sync SoulMirrorState with AffectVector
        {
            let affect = self.affect_regulator.lock().affect;
            let _state = SoulMirrorState::from_affect(&affect, 0, None);
            *self.soul_mirror_state.lock() = _state;
        }
        
        // 7. DynamicMoE lifecycle (birth/merge/split)
        if current_tick % 100 == 0 {
            let mut dmoe = self.dynamic_moe.lock();
            let mut lifecycle = self.expert_lifecycle.lock();
            let mut budget = self.budget_manager.lock();
            
            // Check for expert births (high entropy regions)
            let high_entropy = dmoe.high_entropy_indices(0.8);
            for idx in high_entropy {
                lifecycle.update_entropy(idx as u64, dmoe.expert_entropy[idx]);
                if lifecycle.candidates_for_split(current_tick).contains(&idx) {
                    dmoe.queue_split(idx);
                    k_nano::slog_bin!("BEI", "dmoe", "Queued split for expert {}", idx);
                }
            }
            
            // Check for merges (low entropy experts)
            let merge_candidates = lifecycle.candidates_for_merge(current_tick);
            for (i, j) in merge_candidates {
                dmoe.queue_merge(i, j);
                k_nano::slog_bin!("BEI", "dmoe", "Queued merge: {} + {}", i, j);
            }
            
            // Check for stale experts
            let stale = lifecycle.stale_experts(current_tick);
            for idx in stale {
                k_nano::slog_bin!("BEI", "dmoe", "Stale expert {} marked for removal", idx);
            }
            
            // Flush all pending changes
            dmoe.flush_all();
        }
        
        // 8. Budget pressure check
        if current_tick % 50 == 0 {
            let budget = self.budget_manager.lock();
            let pressure = budget.pressure();
            if pressure > 0.8 {
                k_nano::slog_bin!("BEI", "budget", "HIGH PRESSURE: {:.0}%", pressure * 100.0);
                // Signal plasticity controller to prune more aggressively
                let mut pc = self.plasticity_controller.lock();
                pc.prune_threshold = 0.2; // Increase pruning
            } else {
                let mut pc = self.plasticity_controller.lock();
                pc.prune_threshold = 0.1; // Normal
            }
        }
    }
    
    /// Record a task result for the supervisor
    pub fn record_result(&self, domain: &str, success: bool, confidence: f32, latency: f32) {
        let mut supervisor = self.executive_supervisor.lock();
        supervisor.record_result_full(domain, success, confidence, latency);
    }
    
    /// Incorporate an affect event
    pub fn incorporate_affect(&self, event: AffectEvent) {
        let mut affect = self.affect_regulator.lock();
        affect.incorporate(event);
    }
    
    /// Get current affect vector for display
    pub fn current_affect(&self) -> AffectVector {
        self.affect_regulator.lock().affect
    }
    
    /// Get supervisor status string
    pub fn supervisor_status(&self) -> String {
        self.executive_supervisor.lock().status()
    }
    
    /// Get memory stats
    pub fn memory_stats(&self) -> alloc::vec::Vec<(MemoryLevel, usize, usize)> {
        self.memory_store.lock().stats()
    }
}

use core::sync::atomic::{AtomicPtr, Ordering};

/// Global BEI state (initialized during boot).
/// After init_bei(), the Box is leaked and the AtomicPtr points to the leaked
/// allocation, guaranteeing 'static lifetime for subsequent read-only access.
static BEI_STATE: AtomicPtr<BeiState> = AtomicPtr::new(core::ptr::null_mut());

/// Initialize BEI (call after heap init in main.rs)
pub fn init_bei() {
    let state = BeiState::new();
    let ptr = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(state));
    BEI_STATE.store(ptr, Ordering::Release);
}

/// Get BEI state (call from agents/ticks)
pub fn bei_state() -> Option<&'static BeiState> {
    let ptr = BEI_STATE.load(Ordering::Acquire);
    if ptr.is_null() { None } else { Some(unsafe { &*ptr }) }
}

/// BEI tick function (call from scheduler or timer interrupt)
pub fn bei_tick(_tick: u64) {
    if let Some(state) = bei_state() {
        state.tick();
    }
    // SESSION_234: transporte P2P movido para k_nano (ADR-0081) — roda aqui
    // (hook do scheduler, sempre chamado) em vez de depender do NetAgent agent
    // (rate-limited). k_nano envia heartbeats, drena o RX 42069 e publica
    // não-heartbeats no EVENT_BUS ("P2P_PACKET").
    k_nano::net::mesh::p2p_tick(_tick);
    // Com peer presente, ativa sync de skills + marketplace (idempotente).
    // O bin orquestra k_nano↔hermes — k_nano não conhece hermes.
    if k_nano::net::mesh::MESH_ENGINE.lock().as_ref().map_or(false, |e| e.node_count() >= 1) {
        hermes_crate::skill_sync::activate_global();
        hermes_crate::skill_marketplace::activate_global();
    }
    // Consumo dos pacotes P2P não-heartbeat (EventBus) — lazy subscribe + dreno.
    hermes_crate::skill_sync::poll_p2p();
    hermes_crate::skill_marketplace::poll_p2p();
    // Sync de skills (Master push / diff) + marketplace broadcast periódico.
    let now = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    hermes_crate::skill_sync::sync_skills(now);
    hermes_crate::skill_marketplace::marketplace_tick(k_nano::net::mesh::node_id());
    // SESSION_235 (ADR-0081 item 4): matmul distribuído — o Master responde
    // requests "MW\0" vindos de Workers (EventBus P2P_PACKET, pós p2p_tick).
    cortex_crate::compute::poll_mesh_requests();
    // Self-test do matmul distribuído: o Worker exercita o round-trip MW→MR
    // com o Master (DIAG do boot roda antes da eleição — role Undecided, então
    // nunca pegava o caminho P2P). Retry até 5x: sob TCG o Master pode ainda
    // não ter eleito quando o 1º request chega (timeout curto ~200 ticks).
    static MESH_SELFTEST_TRIES: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    if MESH_SELFTEST_TRIES.load(core::sync::atomic::Ordering::Relaxed) < 5
        && k_nano::net::mesh::local_role() == k_nano::net::mesh::NodeRole::Worker
        && k_nano::net::mesh::MESH_ENGINE.lock().as_ref().map_or(false, |e| e.node_count() >= 1)
    {
        MESH_SELFTEST_TRIES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        cortex_crate::compute::mesh_matmul_self_test();
    }
}