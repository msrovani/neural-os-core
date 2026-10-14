//! BEI (BitNet Ecosystem Intelligence) Initialization
//! ADR-0060: Wire all 8 waves into the boot process.
//! This module creates and connects all BEI components.
//! P0.2: BeiState alocado em frame fixo via HHDM (fora da região de heap)
//! para eliminar REMAP race (map_page_direct) que corrompia Arc pointers (s452).
//!
//! CONTRATO DE TICK (bei_tick):
//! - Chamado a CADA tick do scheduler (BSP, ~18Hz PIT / 60Hz target) via
//!   `agent_core::set_bei_tick_hook` → `maybe_log_sched_metrics`.
//! - Ordem de locks (deadlock-free): CellNetwork → PlasticityController
//!   (coleta grow/prune) → CellNetwork (spawn/mark_dead) → MemoryStore
//!   → AffectRegulator → ExecutiveSupervisor → DynamicMoE →
//!   ExpertLifecycleManager → BudgetManager.
//! - Gates de heap (BeiHeapPolicy): low_mem=128MB (pula blocos não-essenciais),
//!   critical=48MB (só AFFECT_SNAPSHOT). Re-check bounded (SESSION_411) em
//!   Supervisor, PromoteSkill, MoE lifecycle.
//! - Fail-closed observável: Arc corruption detector (CORRUPT_TICKS),
//!   REMAP detector (BOOT_PA), watchpoint DR0 em affect_regulator (s452).
//! - Mesh/FL/CRDT roda APÓS cognição local, gated por TOFU settled.
//! - Métricas reais via `record_result(domain, success, confidence, latency)`
//!   alimentam ExecutiveSupervisor (H9 gap: wire real pendente).

use alloc::string::String;
use alloc::sync::Arc;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use spin::Mutex;
use x86_64::structures::paging::FrameAllocator;
use k_nano::sync::mpmc::MpmcQueue;
use k_ai::{economy::BudgetManager, expert_lifecycle::ExpertLifecycleManager};
use cortex::{cellular::{CellNetwork, CellType}, evolution::PlasticityController, moe::DynamicMoE};
use crate::{memory::{MemoryStore, MemoryLevel}, affect::{AffectRegulator, AffectVector, AffectEvent}, executive::{ExecutiveSupervisor, SupervisorVerdict}};


/// Política de heap do BEI — centraliza thresholds (P4: unifica 4 gates espalhados).
/// low_mem = 128MB (pula blocos não-essenciais), critical = 48MB (só AFFECT roda).
/// Valores vêm de s429-lab: janela bump ~2030MB, wasmi/cranelift allocam GRANDE.
#[derive(Copy, Clone)]
pub struct BeiHeapPolicy {
    pub low_mem_mb: usize,
    pub critical_mb: usize,
}

impl Default for BeiHeapPolicy {
    fn default() -> Self {
        Self { low_mem_mb: 128, critical_mb: 48 }
    }
}

impl BeiHeapPolicy {
    /// true se headroom < low_mem_mb — pula blocos não-essenciais (CellNet, Plasticity, Supervisor, MoE lifecycle).
    pub fn is_low(&self) -> bool {
        let obs = k_nano::allocator::heap_observe();
        obs.headroom_mb < self.low_mem_mb
    }
    /// true se headroom < critical_mb — só AFFECT_SNAPSHOT roda; resto espera.
    pub fn is_critical(&self) -> bool {
        let obs = k_nano::allocator::heap_observe();
        obs.headroom_mb < self.critical_mb
    }
    /// Re-check bounded (padrão lost-wakeup SESSION_411): headroom pode cair entre gate externo e lock.
    pub fn recheck_critical(&self) -> bool {
        k_nano::allocator::heap_headroom_critical()
    }
}

/// Helper de log throttled — substitui 7+ statics ad-hoc (LAST_LOG, LAST_BUDGET, LAST_FL_LOG, etc.).
/// Uso: `ThrottledLog::new("BEI", "warn", 64).log(|| "msg")` — loga se passou `min_ticks` desde último.
pub struct ThrottledLog {
    subsystem: &'static str,
    severity: &'static str,
    min_ticks: u64,
    last_tick: AtomicU64,
}

impl ThrottledLog {
    pub const fn new(subsystem: &'static str, severity: &'static str, min_ticks: u64) -> Self {
        Self { subsystem, severity, min_ticks, last_tick: AtomicU64::new(0) }
    }
    /// Loga se passou `min_ticks` desde último log. `msg_fn` é lazy (só executa se for logar).
    pub fn log<F: FnOnce() -> alloc::string::String>(&self, msg_fn: F) {
        let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        let last = self.last_tick.load(Ordering::Relaxed);
        if last == 0 || now.wrapping_sub(last) >= self.min_ticks {
            self.last_tick.store(now, Ordering::Release);
            k_nano::slog_bin!(self.subsystem, self.severity, "{}", msg_fn());
        }
    }
    /// Loga one-shot (só primeira vez).
    pub fn log_once<F: FnOnce() -> alloc::string::String>(&self, msg_fn: F) {
        let last = self.last_tick.load(Ordering::Relaxed);
        if last == 0 {
            self.last_tick.store(1, Ordering::Release);
            k_nano::slog_bin!(self.subsystem, self.severity, "{}", msg_fn());
        }
    }
    /// Loga a cada `min_ticks` (rate-limited) — para logs periódicos como FL/health.
    pub fn log_periodic<F: FnOnce() -> alloc::string::String>(&self, msg_fn: F) {
        self.log(msg_fn);
    }
}


/// Global BEI state container
pub struct BeiState {
    // Wave 0: Communication
    pub cell_message_queue: Arc<MpmcQueue<cortex::cellular::CellMessage>>,
    
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
    
    // Current tick for synchronization
    pub current_tick: Arc<Mutex<u64>>,
}

impl BeiState {
    /// Initialize all BEI components (called after heap is ready).
    /// `None` = OOM/capacidadefalha — boot continua DEGRADED (sem panic).
    pub fn try_new() -> Option<Self> {
        k_nano::slog_bin!("BEI", "ok", "Starting BEI initialization (8 waves)");
        
        // ─── Wave 0: MPMC Queue ───
        let cell_message_queue = Arc::new(
            MpmcQueue::<cortex::cellular::CellMessage>::new(256)?
        );
        k_nano::slog_bin!("BEI", "ok", "wave0 MPMC queue created (cap=256)");
        
        // ─── Wave 1: Economy + Expert Lifecycle ───
        let budget_manager = Arc::new(Mutex::new(
            BudgetManager::new(64 * 1024 * 1024) // 64MB budget
        ));
        let expert_lifecycle = Arc::new(Mutex::new(ExpertLifecycleManager::new()));
        k_nano::slog_bin!("BEI", "ok", "wave1 BudgetManager + ExpertLifecycleManager created");
        
        // ─── Wave 2: Cellular Network + Plasticity ───
        let cell_network = Arc::new(Mutex::new(
            CellNetwork::new(64, 10)? // inbox_cap=64, budget_per_tick=10
        ));
        let plasticity_controller = Arc::new(Mutex::new(
            PlasticityController::new(8, 0.7, 0.1) // 8 regions, growth=0.7, prune=0.1
        ));
        k_nano::slog_bin!("BEI", "ok", "wave2 CellNetwork (8 regions) + PlasticityController created");
        
        // Spawn initial cells per region
        {
            let mut net = cell_network.lock();
            for region in 0..8 {
                // 1 Reasoning + 1 Memory cell per region
                let _ = net.spawn_cell(CellType::Reasoning, region);
                let _ = net.spawn_cell(CellType::Memory, region);
            }
            k_nano::slog_bin!("BEI", "ok", "wave2 Spawned {} cells across 8 regions", net.cell_count());
        }
        
        // ─── Wave 3: Dynamic MoE ───
        // Note: DynamicMoE needs a base MoELayer. We'll create a minimal one.
        let dynamic_moe = Arc::new(Mutex::new(Self::create_dynamic_moe()));
        // M13: honesto — scaffold com pesos zero, NÃO é um MoE treinado.
        k_nano::slog_bin!("BEI", "warn", "wave3 DynamicMoE DEGRADED (zero_weights scaffold — sem pesos treinados)");
        
        // ─── Wave 4: Memory L0-L7 ───
        let memory_store = Arc::new(Mutex::new({
            let mut store = MemoryStore::new();
            store.init_default_tiers();
            store
        }));
        k_nano::slog_bin!("BEI", "ok", "wave4 MemoryStore L0-L7 initialized");
        
        // ─── Wave 5: Affect Regulator ───
        let affect_regulator = Arc::new(Mutex::new(AffectRegulator::new()));
        k_nano::slog_bin!("BEI", "ok", "wave5 AffectRegulator created (neutral state)");
        
        // ─── Wave 6: Executive Supervisor ───
        let executive_supervisor = Arc::new(Mutex::new(ExecutiveSupervisor::new()));
        k_nano::slog_bin!("BEI", "ok", "wave6 ExecutiveSupervisor created (7-phase loop)");
        
        // ─── Wave 7: Soul Mirror ───
        // Estado do orb mora no compositor (jarbas::display::soul_mirror);
        // o BEI só publica o affect via AFFECT_SNAPSHOT (bridge abaixo).
        k_nano::slog_bin!("BEI", "ok", "wave7 Soul Mirror: affect via AFFECT_SNAPSHOT (compositor owns render)");
        
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
        
        k_nano::slog_bin!("BEI", "ok", "All 8 waves allocated (wire cross-component = tick-time)");
        
        Some(BeiState {
            cell_message_queue,
            budget_manager,
            expert_lifecycle,
            cell_network,
            plasticity_controller,
            dynamic_moe,
            memory_store,
            affect_regulator,
            executive_supervisor,
            current_tick: Arc::new(Mutex::new(0)),
        })
    }
    
    fn create_dynamic_moe() -> DynamicMoE {
        use cortex::{moe::{MoELayer, MoEConfig, Int8Router}, nn::BitLinear, tensor::PackedTernaryTensor};
        
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
        _executive_supervisor: &Arc<Mutex<ExecutiveSupervisor>>,
        _affect_regulator: &Arc<Mutex<AffectRegulator>>,
        _memory_store: &Arc<Mutex<MemoryStore>>,
        _cell_network: &Arc<Mutex<CellNetwork>>,
        _plasticity_controller: &Arc<Mutex<PlasticityController>>,
        _dynamic_moe: &Arc<Mutex<DynamicMoE>>,
        _expert_lifecycle: &Arc<Mutex<ExpertLifecycleManager>>,
        _budget_manager: &Arc<Mutex<BudgetManager>>,
    ) {
        // A fiação real ocorre no bei_tick() com ordem de locks consistente:
        // 1. CellNetwork (net.lock)
        // 2. PlasticityController (pc.lock) → decisões grow/prune → net.lock p/ spawn/mark_dead
        // 3. MemoryStore (mem.lock)
        // 4. AffectRegulator (affect.lock)
        // 5. ExecutiveSupervisor (supervisor.lock)
        // 6. DynamicMoE + ExpertLifecycleManager + BudgetManager (dmoe→lifecycle→budget)
        // Não há graph edges no init — a cognição é tick-driven (ADR-0060).
        k_nano::slog_bin!(
            "BEI",
            "ok",
            "connect_components: wiring tick-driven (net→pc→mem→affect→supervisor→dmoe→lifecycle→budget)"
        );
    }
    
    /// s437: valida os ponteiros internos dos Arc ANTES de qualquer `.lock()`.
    /// Um stray write no objeto leaked pode zerar o ponteiro interno (visto em
    /// produção: `cell_network`→0x6) e o lock do `spin::Mutex` vira #PF storm
    /// no BSP — o park do handler (`loop{hlt}`, IF=0) congela o OS inteiro
    /// (scheduler/timer/display vivem no BSP). `Arc::as_ptr` só lê o campo,
    /// sem deref → seguro mesmo com o Arc corrompido.
    fn all_ptrs_valid(&self) -> bool {
        #[inline]
        fn ok<T>(a: &Arc<T>) -> bool {
            (Arc::as_ptr(a) as usize) >= 0xffff_0000_0000_0000
        }
        ok(&self.cell_message_queue)
            && ok(&self.budget_manager)
            && ok(&self.expert_lifecycle)
            && ok(&self.cell_network)
            && ok(&self.plasticity_controller)
            && ok(&self.dynamic_moe)
            && ok(&self.memory_store)
            && ok(&self.affect_regulator)
            && ok(&self.executive_supervisor)
            && ok(&self.current_tick)
    }

    /// Main BEI tick - called every scheduler tick
    pub fn tick(&self) {
        // s437: fail-closed observável — nunca dar deref de Arc corrompido.
        // O tick é pulado e o sistema segue vivo (auto-cura pela próxima volta).
        if !self.all_ptrs_valid() {
            static CORRUPT_TICKS: core::sync::atomic::AtomicU32 =
                core::sync::atomic::AtomicU32::new(0);
            let n = CORRUPT_TICKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            // s452 diag (one-shot): endereco do BeiState + campos — alimenta o
            // watchpoint (affect_regulator @ self+0x38; cr2 = ArcInner+0x10).
            if n == 0 {
                k_nano::slog_bin!(
                    "BEI",
                    "warn",
                    "corrupt diag self=0x{:x} pa=0x{:x} cmq=0x{:x} bm=0x{:x} el=0x{:x} cn=0x{:x} pc=0x{:x} dm=0x{:x} ms=0x{:x} ar=0x{:x} es=0x{:x} ct=0x{:x}",
                    self as *const BeiState as usize,
                    k_nano::memory::page_leaf_phys(self as *const BeiState as u64).unwrap_or(0),
                    Arc::as_ptr(&self.cell_message_queue) as usize,
                    Arc::as_ptr(&self.budget_manager) as usize,
                    Arc::as_ptr(&self.expert_lifecycle) as usize,
                    Arc::as_ptr(&self.cell_network) as usize,
                    Arc::as_ptr(&self.plasticity_controller) as usize,
                    Arc::as_ptr(&self.dynamic_moe) as usize,
                    Arc::as_ptr(&self.memory_store) as usize,
                    Arc::as_ptr(&self.affect_regulator) as usize,
                    Arc::as_ptr(&self.executive_supervisor) as usize,
                    Arc::as_ptr(&self.current_tick) as usize,
                );
            }
            if n % 64 == 0 {
                k_nano::slog_bin!(
                    "BEI",
                    "warn",
                    "tick pulado: ponteiro Arc corrompido (stray heap write); sistema segue"
                );
            }
            return;
        }
        // s452: detector de REMAP da VA do BeiState (race em map_page_direct).
        // PA estavel = OK; PA mudou => a VA foi remapeada p/ um frame novo
        // (nao houve write — por isso o watchpoint de escrita nao dispara).
        {
            static BOOT_PA: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
            static CTR: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
            static LOGGED: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
            if CTR.fetch_add(1, core::sync::atomic::Ordering::Relaxed) % 512 == 0 {
                let pa = k_nano::memory::page_leaf_phys(self as *const BeiState as u64).unwrap_or(0);
                let b = BOOT_PA.load(core::sync::atomic::Ordering::Acquire);
                if b == 0 {
                    BOOT_PA.store(pa, core::sync::atomic::Ordering::Release);
                } else if pa != 0 && pa != b && LOGGED.swap(1, core::sync::atomic::Ordering::AcqRel) == 0 {
                    k_nano::slog_bin!("BEI", "warn", "REMAP bei pa=0x{:x} != boot 0x{:x} (race map_page_direct?)", pa, b);
                }
            }
        }
        let mut tick = self.current_tick.lock();
        *tick += 1;
        let current_tick = *tick;
        drop(tick);
        
        // M12: BEI state tick (cognição local) — bloco distinto do mesh abaixo.
        // s429-lab rodada 8: 48→128MB — wasmi/cranelift allocam GRANDE no PromoteSkill
        // (bypassam o grow; janela ~2030MB com headroom 48-127MB ainda falha).
        // Mesmo piso dos gates submit/claim do InferQueue (s429-lab).
        let heap_policy = BeiHeapPolicy::default();
        let low_mem = heap_policy.is_low();
        // M20: sob headroom crítico só o snapshot (AFFECT) roda; o resto espera.

        // 1. Advance CellNetwork scheduler (skip sob heap critico — MPMC #PF)
        if !low_mem {
            let mut net = self.cell_network.lock();
            net.tick_advance();
            while let Some((cell_id, _messages)) = net.round_robin() {
                net.mark_processed(cell_id);
            }
            net.reap_dead();
        }

        // 2. Advance PlasticityController
        // M21: coleta sob lock do pc, dropa, e só então toca o cell_network
        // (ordem de lock pc→net dentro do loop era inversão potencial).
        if !low_mem {
            let mut grows = alloc::vec::Vec::new();
            let mut prunes = alloc::vec::Vec::new();
            {
                let mut pc = self.plasticity_controller.lock();
                pc.tick_advance();
                for region in 0..pc.num_regions() {
                    if pc.should_grow(region) {
                        grows.push((region, pc.region_entropy(region)));
                    }
                    if pc.should_prune(region) {
                        prunes.push((region, pc.region_activation[region]));
                    }
                }
            }
            for (region, ent) in grows {
                k_nano::slog_bin!("BEI", "warn", "Region {} should GROW (entropy={:.2})", region, ent);
                let _ = self.cell_network.lock().spawn_cell(CellType::Reasoning, region);
            }
            for (region, act) in prunes {
                k_nano::slog_bin!("BEI", "warn", "Region {} should PRUNE (activation={:.2})", region, act);
                let dead_id = {
                    let net = self.cell_network.lock();
                    net.cells().iter()
                        .filter(|c| c.region == region && c.state != cortex::cellular::CellState::Dead)
                        .min_by(|a, b| a.unprocessed().cmp(&b.unprocessed()))
                        .map(|c| c.id)
                };
                if let Some(id) = dead_id {
                    self.cell_network.lock().mark_dead(id);
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
        
        static LAST_PHASE_DEG: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
        // 5. ExecutiveSupervisor tick (7-phase loop) — skip sob heap crítico,
        // mantendo o último phase p/ o snapshot (M20).
        if !low_mem {
        {
            // s429-lab rodada 9: re-check bounded (padrão lost-wakeup SESSION_411) —
            // o prefill concorrente come o headroom ENTRE o snapshot do início do
            // tick e aqui; iterar BTreeMap do supervisor com nós demand-paged sob
            // teto = #PF (evidência: LazyLeafRange cr2=0x702a00a8, T+28k).
            if heap_policy.is_low() {
                k_nano::slog_bin!("BEI", "warn", "supervisor tick pulado: heap caiu no meio do tick (headroom low)");
            } else {
            let mut supervisor = self.executive_supervisor.lock();
            let verdict = supervisor.tick_supervise(10); // base budget = 10
            
            // Handle verdict
            match verdict {
                SupervisorVerdict::Proceed => {}
                SupervisorVerdict::ProceedWithBudget(budget) => {
                    if heap_policy.is_critical() {
                        // Rate-limit: slog sob headroom=0 também aloca.
                        static LAST_LOG: core::sync::atomic::AtomicU64 =
                            core::sync::atomic::AtomicU64::new(0);
                        let now = current_tick;
                        let prev = LAST_LOG.load(core::sync::atomic::Ordering::Relaxed);
                        if now.saturating_sub(prev) >= 64 {
                            LAST_LOG.store(now, core::sync::atomic::Ordering::Relaxed);
                            let obs = k_nano::allocator::heap_observe();
                            k_nano::slog_bin!(
                                "BEI",
                                "warn",
                                "heap headroom={}MB - suppress ProceedWithBudget({})",
                                obs.headroom_mb,
                                budget
                            );
                        }
                    } else {
                        // H8: só loga quando o budget muda — log por tick é spam.
                        static LAST_BUDGET: core::sync::atomic::AtomicU64 =
                            core::sync::atomic::AtomicU64::new(0);
                        let prev = LAST_BUDGET.swap(budget, core::sync::atomic::Ordering::Relaxed);
                        if prev != budget {
                            k_nano::slog_bin!("BEI", "ok", "ProceedWithBudget: {}", budget);
                        }
                    }
                }
                SupervisorVerdict::Ponder(steps) => {
                    k_nano::slog_bin!("BEI", "ok", "Ponder: {} steps", steps);
                }
                SupervisorVerdict::Delay { reason, until_tick } => {
                    k_nano::slog_bin!("BEI", "warn", "Delay: {} until {}", reason, until_tick);
                }
                SupervisorVerdict::Preempt { reason } => {
                    k_nano::slog_bin!("BEI", "warn", "Preempt: {}", reason);
                }
                SupervisorVerdict::Escalate { reason } => {
                    k_nano::slog_bin!("BEI", "fail", "ESCALATE: {}", reason);
                }
                SupervisorVerdict::Train { domain, reason } => {
                    k_nano::slog_bin!("BEI", "warn", "Train domain: {} ({})", domain, reason);
                    // Trigger BitNetTrainer for this domain
                }
                SupervisorVerdict::PromoteSkill { skill_name } => {
                    // ADR-0059 F5: age, não só loga — promove a skill efêmera
                    // comprovada (≥3 runs, ≥70%) para WASM via sandbox wasmi
                    // (mesmo fluxo do hw_pnp). Falha → log (não derruba o boot).
                    // s429-lab rodada 8: re-check bounded (padrão SESSION_411) —
                    // o heap cai ENTRE o gate externo e o wasmi (cmpxchg em obj NULL
                    // = #PF cr2=0x10; evidência rodada 8 T+25k pós teto 2030MB).
                    if heap_policy.is_low() {
                        k_nano::slog_bin!("BEI", "warn", "PromoteSkill pulado: heap crítico (wasmi alloc)");
                    } else {
                        // s440: NÃO carimbar dummy — publica o pedido de geração
                        // real (forja model-born via LLM op-IR); dummy não é solução
                        // (paridade com hw_pnp; o elo morto agora publica).
                        crate::self_evolve::publish_skill_gen_request(&skill_name, "");
                        k_nano::slog_bin!(
                            "BEI",
                            "ok",
                            "PromoteSkill: {} → pedido de geração WASM (LLM op-IR)",
                            skill_name
                        );
                    }
                }
            }
            // H9: supervisor não recebe entradas de resultado reais — confidence/
            // latency verdadeiros não existem neste escopo (veredito é derivado,
            // não medido). Registrar aqui sem medição = métrica inventada.
            // Gap: wire quando o tick observar latency/domínio reais.
            // Capture phase before supervisor lock is dropped
            LAST_PHASE_DEG.store(supervisor.phase.rotation_deg(), core::sync::atomic::Ordering::Relaxed);
            } // else (re-check bounded — supervisor pulado)
        }
        }

        // 6. Sync AFFECT_SNAPSHOT for compositor (hermes::globals bridge) —
        // o OrbState do compositor lê daqui; sem cópia local no BEI.
        {
            let affect = self.affect_regulator.lock().affect;
            let phase_deg = LAST_PHASE_DEG.load(core::sync::atomic::Ordering::Relaxed);
            crate::globals::sync_affect_snapshot(&affect, phase_deg);
        }
        
        // 7. DynamicMoE lifecycle (birth/merge/split)
        // H4: sem experts registrados o lifecycle é morto — gate cedo.
        // SESSION_416 fail-closed: merge/split/birth clonam experts (merge_pair,
        // clone_with_noise) → sob heap crítico o alloc devolve null → deref →
        // #PF no AP (evidência: flush_merges cr2=0x8, T+27.8s pós teto 2030MB).
        // Lifecycle MoE é trabalho não-essencial: pula o ciclo inteiro.
        if current_tick % 100 == 0
            && !heap_policy.is_critical()
            && !self.dynamic_moe.lock().base.experts.is_empty()
        {
            let mut dmoe = self.dynamic_moe.lock();
            let mut lifecycle = self.expert_lifecycle.lock();
            let _budget = self.budget_manager.lock();
            
            // Re-check bounded (padrão lost-wakeup SESSION_411): o headroom pode
            // ter caído entre o gate externo e o lock.
            if heap_policy.recheck_critical() {
                k_nano::slog_bin!("BEI", "warn", "lifecycle MoE pulado: heap crítico");
            } else {
            // Check for expert births (high entropy regions)
            let high_entropy = dmoe.high_entropy_indices(0.8);
            for idx in high_entropy {
                lifecycle.update_entropy(idx as u64, dmoe.expert_entropy[idx]);
                if lifecycle.candidates_for_split(current_tick).contains(&idx) {
                    dmoe.queue_split(idx);
                k_nano::slog_bin!("BEI", "warn", "Queued split for expert {}", idx);
                }
            }
            
            // Check for merges (low entropy experts)
            let merge_candidates = lifecycle.candidates_for_merge(current_tick);
            for (i, j) in merge_candidates {
                dmoe.queue_merge(i, j);
                k_nano::slog_bin!("BEI", "warn", "Queued merge: {} + {}", i, j);
            }
            
            // Check for stale experts
            let stale = lifecycle.stale_experts(current_tick);
            for idx in stale {
                k_nano::slog_bin!("BEI", "warn", "Stale expert {} marked for removal", idx);
            }
            
            // Flush all pending changes
            dmoe.flush_all();
            } // else (heap crítico — lifecycle pulado)
        }
        
        // 8. Budget pressure check
        if current_tick % 50 == 0 {
            let budget = self.budget_manager.lock();
            let pressure = budget.pressure();
            if pressure > 0.8 {
                k_nano::slog_bin!("BEI", "warn", "HIGH PRESSURE: {:.0}%", pressure * 100.0);
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

/// Global BEI state (initialized during boot).
/// After init_bei(), the Box is leaked and the AtomicPtr points to the leaked
/// allocation, guaranteeing 'static lifetime for subsequent read-only access.
static BEI_STATE: AtomicPtr<BeiState> = AtomicPtr::new(core::ptr::null_mut());

/// Initialize BEI (call after heap init in main.rs). Fail-closed: OOM → skip, no panic.
/// P0.2: aloca BeiState em frame fixo via HHDM (fora da região de heap) para
/// evitar REMAP race do map_page_direct que corrompia Arc pointers (s452).
pub fn init_bei() {
    // 1) Tenta construir o BeiState (pode falhar OOM nos Arc::new internos)
    let Some(state) = BeiState::try_new() else {
        k_nano::slog_bin!(
            "BEI",
            "fail",
            "init SKIP (MPMC/CellNetwork alloc fail) — boot DEGRADED sem BEI"
        );
        return;
    };

    // 2) Aloca 1 frame físico (4KB) via PMM — BeiState cabe em <1KB
    let frame = k_nano::memory::GLOBAL_ALLOCATOR
        .lock()
        .as_mut()
        .and_then(|a| a.allocate_frame());
    let Some(frame) = frame else {
        k_nano::slog_bin!(
            "BEI",
            "fail",
            "init SKIP (PMM frame alloc fail) — boot DEGRADED sem BEI"
        );
        return;
    };

    // 3) VA fixa no HHDM (phys_mem_offset + frame_phys) — fora da região de heap
    //    heap = 0x_4000_0000_0000.. ; HHDM = phys_mem_offset + phys
    //    map_page_direct só toca heap_start + offset → HHDM imune a REMAP.
    let pm_off = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    let bei_va = pm_off + frame.start_address().as_u64();

    // 4) Constrói in-place no frame (ptr::write — sem Box, sem heap para o struct)
    unsafe {
        ptr::write(bei_va as *mut BeiState, state);
    }

    // 5) Publica o ponteiro
    BEI_STATE.store(bei_va as *mut BeiState, Ordering::Release);

    k_nano::slog_bin!(
        "BEI",
        "ok",
        "BEI 8 waves up (connect PARTIAL — wire no tick) @HHDM 0x{:x}",
        bei_va
    );
}

/// Get BEI state (call from agents/ticks)
pub fn bei_state() -> Option<&'static BeiState> {
    let ptr = BEI_STATE.load(Ordering::Acquire);
    if ptr.is_null() { None } else { Some(unsafe { &*ptr }) }
}

/// s452 diag: endereco do campo `affect_regulator` (Arc ptr) do BeiState leakado.
/// Alvo do watchpoint de escrita (DR0) — o stray write o corrompe e o #PF
/// resultante tem `cr2 = ArcInner + 0x10 = affect_regulator + 0x10` (0x11).
pub fn affect_regulator_addr() -> Option<usize> {
    let ptr = BEI_STATE.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &(*ptr).affect_regulator as *const _ as usize })
    }
}

/// BEI tick function (call from scheduler or timer interrupt)
pub fn bei_tick(_tick: u64) {
    if let Some(state) = bei_state() {
        state.tick();
    }
    // P5/Item 10: offload mesh/FL/CRDT to AP workers via IPI.
    // BSP submits job; AP workers execute in idle loop (k_nano::smp::ap_work).
    crate::ap_jobs::submit_mesh_fl_crdt_tick();

}
