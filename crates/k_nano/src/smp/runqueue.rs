//! ADR-0089: Per-CPU Run-Queues para agents — distribuição cooperativa entre cores.
//!
//! ## Visão
//! O scheduler BSP-only agenda todos os agents em loop cooperativo.
//! Com `smp-runqueue`, o BSP distribui agents elegíveis para run-queues
//! por-CPU. APs ociosos roubam trabalho de outras queues (work-stealing).
//!
//! ## Elegibilidade
//! - `affinity_ring >= 1` (R1/R2) → elegível a migrar para APs.
//! - `affinity_ring == 0` (BSP/critical) → NUNCA migra.
//! - `coherence_partner` → tenta ficar no mesmo core.
//!
//! ## Feature Gate: `smp-runqueue` (default OFF). Zero regressão quando OFF.
//!
//! ## Padrão: slot-based MPMC (mesmo de `ap_work.rs`) — SyncCell + HEAD/TAIL
//! atômicos + CAS. Sem heap, sem const-fn restriction, compatível com static.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

// ─── SyncCell pattern (from ap_work.rs) ────────────────────────────────────

/// Wrapper over UnsafeCell that implements Sync for single-writer/single-reader
/// access patterns guarded by atomic HEAD/TAIL indices.
struct SyncCell<T>(UnsafeCell<T>);
unsafe impl<T> Sync for SyncCell<T> {}

use core::cell::UnsafeCell;

/// Tarefa de agent enfileirada para execução em um core.
#[derive(Clone, Copy, Debug)]
pub struct AgentTask {
    pub agent_idx: u32,
    pub tick_id: u32,
    pub priority: u8,
    pub affinity_ring: u8,
    pub goal_urgency: u8,
    _reserved: u8,
}

impl Default for AgentTask {
    fn default() -> Self {
        Self { agent_idx: u32::MAX, tick_id: 0, priority: 0, affinity_ring: 0, goal_urgency: 0, _reserved: 0 }
    }
}

impl AgentTask {
    pub const fn new(agent_idx: u32, tick_id: u32, priority: u8, affinity_ring: u8, goal_urgency: u8) -> Self {
        Self { agent_idx, tick_id, priority, affinity_ring, goal_urgency, _reserved: 0 }
    }

    pub fn score(&self) -> u16 {
        (self.goal_urgency as u16) * 4 + (self.priority as u16) * 2
    }

    pub fn is_valid(&self) -> bool {
        self.agent_idx != u32::MAX
    }
}

const RQ_CAP: usize = 128;
const MAX_CORES: usize = 16;

/// Slot de run-queue.
#[derive(Copy, Clone)]
struct RqSlot {
    task: AgentTask,
}

impl RqSlot {
    const fn new() -> Self {
        Self { task: AgentTask { agent_idx: u32::MAX, tick_id: 0, priority: 0, affinity_ring: 0, goal_urgency: 0, _reserved: 0 } }
    }
}

/// Run-queue por core — slot-based lock-free MPMC (padrão ap_work.rs).
/// HEAD/TAIL atômicos, CAS no dequeue, sem heap.
struct PerCoreRunQueue {
    slots: SyncCell<[RqSlot; RQ_CAP]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

/// SAFETY: Escrita guiada por TAIL (BSP), leitura guiada por HEAD (APs),
/// CAS garante exclusão por slot.
unsafe impl Send for PerCoreRunQueue {}
unsafe impl Sync for PerCoreRunQueue {}

impl PerCoreRunQueue {
    const fn new() -> Self {
        Self {
            slots: SyncCell(UnsafeCell::new([RqSlot::new(); RQ_CAP])),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Enfileira uma task (BSP). Retorna `false` se cheio.
    fn enqueue(&self, task: AgentTask) -> bool {
        let t = self.tail.load(Ordering::Relaxed);
        let h = self.head.load(Ordering::Acquire);
        if t.wrapping_sub(h) >= RQ_CAP {
            return false;
        }
        let idx = t & (RQ_CAP - 1);
        unsafe {
            (*self.slots.0.get())[idx].task = task;
        }
        self.tail.store(t.wrapping_add(1), Ordering::Release);
        true
    }

    /// Desenfileira uma task (consumer/AP). Retorna `None` se vazio.
    fn dequeue(&self) -> Option<AgentTask> {
        let h = self.head.load(Ordering::Relaxed);
        let t = self.tail.load(Ordering::Acquire);
        if h >= t {
            return None;
        }
        let idx = h & (RQ_CAP - 1);
        let task = unsafe { (*self.slots.0.get())[idx].task };
        if self.head.compare_exchange_weak(h, h.wrapping_add(1), Ordering::SeqCst, Ordering::Relaxed).is_ok() {
            Some(task)
        } else {
            None
        }
    }

    fn len(&self) -> usize {
        let t = self.tail.load(Ordering::Acquire);
        let h = self.head.load(Ordering::Acquire);
        t.saturating_sub(h)
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Limpa a queue (para testes).
    fn clear(&self) {
        self.head.store(0, Ordering::Release);
        self.tail.store(0, Ordering::Release);
    }
}

static RUN_QUEUES: [PerCoreRunQueue; MAX_CORES] = [
    PerCoreRunQueue::new(), PerCoreRunQueue::new(), PerCoreRunQueue::new(), PerCoreRunQueue::new(),
    PerCoreRunQueue::new(), PerCoreRunQueue::new(), PerCoreRunQueue::new(), PerCoreRunQueue::new(),
    PerCoreRunQueue::new(), PerCoreRunQueue::new(), PerCoreRunQueue::new(), PerCoreRunQueue::new(),
    PerCoreRunQueue::new(), PerCoreRunQueue::new(), PerCoreRunQueue::new(), PerCoreRunQueue::new(),
];

// ─── Telemetria por-CPU ────────────────────────────────────────────────────

#[repr(C, align(64))]
pub struct CpuStats {
    pub running: AtomicU32,
    pub blocked: AtomicU32,
    pub stolen: AtomicU32,
    pub enqueued: AtomicU32,
    _pad: [u8; 48],
}

impl CpuStats {
    const fn new() -> Self {
        Self { running: AtomicU32::new(0), blocked: AtomicU32::new(0), stolen: AtomicU32::new(0), enqueued: AtomicU32::new(0), _pad: [0; 48] }
    }
}

static CPU_STATS: [CpuStats; MAX_CORES] = [
    CpuStats::new(), CpuStats::new(), CpuStats::new(), CpuStats::new(),
    CpuStats::new(), CpuStats::new(), CpuStats::new(), CpuStats::new(),
    CpuStats::new(), CpuStats::new(), CpuStats::new(), CpuStats::new(),
    CpuStats::new(), CpuStats::new(), CpuStats::new(), CpuStats::new(),
];

pub fn cpu_stats(core_id: usize) -> &'static CpuStats {
    &CPU_STATS[core_id.min(MAX_CORES - 1)]
}

pub fn total_pending() -> usize {
    let mut total = 0;
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    for i in 0..cores.min(MAX_CORES) {
        total += RUN_QUEUES[i].len();
    }
    total
}

// ─── BSP: distribui agents para run-queues ─────────────────────────────────

/// Limpa todas as run-queues (para testes).
pub fn clear_all_queues() {
    for i in 0..MAX_CORES {
        RUN_QUEUES[i].clear();
    }
}

pub fn enqueue_agent(core_id: usize, task: AgentTask) -> bool {
    if core_id >= MAX_CORES { return false; }
    let ok = RUN_QUEUES[core_id].enqueue(task);
    if ok { CPU_STATS[core_id].enqueued.fetch_add(1, Ordering::Relaxed); }
    ok
}

pub fn dequeue_agent(core_id: usize) -> Option<AgentTask> {
    if core_id >= MAX_CORES { return None; }
    RUN_QUEUES[core_id].dequeue()
}

/// Work-stealing: rouba 1 task de outro core (round-robin, min 1 task na vítima).
pub fn steal_agent(core_id: usize) -> Option<AgentTask> {
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    let n = cores.min(MAX_CORES);
    if n <= 1 { return None; }
    for offset in 1..n {
        let victim = (core_id + offset) % n;
        if victim == core_id { continue; }
        if RUN_QUEUES[victim].len() > 1 {
            if let Some(task) = RUN_QUEUES[victim].dequeue() {
                CPU_STATS[victim].stolen.fetch_add(1, Ordering::Relaxed);
                return Some(task);
            }
        }
    }
    None
}

// ─── IPI de reschedule ────────────────────────────────────────────────────

/// Envia IPI de reschedule (vetor 0x80) para um AP.
///
/// # Safety
/// Requer LAPIC habilitada e AP com IDT carregada.
pub unsafe fn send_reschedule_ipi_to(lapic_id: u32) {
    crate::apic::send_ipi_reschedule_to(lapic_id);
}

/// Acorda o AP se tiver trabalho na run-queue.
pub fn wake_core_if_needed(core_id: usize) {
    if core_id == 0 { return; }
    if !crate::smp::ap_pollable() { return; }
    if RUN_QUEUES[core_id].is_empty() { return; }
    let ap_index = core_id - 1;
    let Some(p) = crate::smp::percpu::ap_pcpu_ptr_mut(ap_index) else { return; };
    let lapic_id = unsafe { (*p).lapic_id };
    if lapic_id == 0 { return; }
    unsafe { send_reschedule_ipi_to(lapic_id); }
}

// ─── Load balancing ────────────────────────────────────────────────────────

pub fn resolve_target_core(affinity_ring: u8, coherence_partner: Option<usize>, _agent_idx: usize) -> usize {
    if affinity_ring == 0 { return 0; }
    if let Some(partner_idx) = coherence_partner {
        let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
        if cores > 1 { return 1 + (partner_idx % (cores - 1)); }
        return 0;
    }
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    if cores <= 1 { return 0; }
    let mut best_core = 1;
    let mut best_len = RUN_QUEUES[1].len();
    for c in 2..cores.min(MAX_CORES) {
        let len = RUN_QUEUES[c].len();
        if len < best_len { best_len = len; best_core = c; }
    }
    best_core
}

// ─── Core Role Mapping (ADR-0057 CorePair + ADR-0089) ──────────────────────

/// Papel do core no sistema (espelha CoreRole de core_pair.rs).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreRole {
    /// System: k_nano I/O, drivers, VFS, agents críticos (ring0).
    System = 0,
    /// Compute: LLM decode, matmul, inferência.
    Compute = 1,
    /// Memory: SGDB, vector search, fact indexing.
    Memory = 2,
    /// Worker: WASM sandbox, orquestração, tarefas auxiliares.
    Worker = 3,
    /// Idle: dormindo em hlt/mwait.
    Idle = 4,
}

impl CoreRole {
    pub fn name(&self) -> &'static str {
        match self {
            Self::System => "SYS",
            Self::Compute => "COMP",
            Self::Memory => "MEM",
            Self::Worker => "WORK",
            Self::Idle => "IDLE",
        }
    }
}

/// Papel de cada core — configurável em boot.
/// Default: core 0 = System, cores 1-N = Worker (conservador).
static CORE_ROLES: [core::sync::atomic::AtomicU8; MAX_CORES] = [
    core::sync::atomic::AtomicU8::new(CoreRole::System as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
    core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8),
];

/// Retorna o papel de um core.
pub fn core_role(core_id: usize) -> CoreRole {
    let raw = CORE_ROLES[core_id.min(MAX_CORES - 1)].load(Ordering::Relaxed);
    match raw {
        0 => CoreRole::System,
        1 => CoreRole::Compute,
        2 => CoreRole::Memory,
        3 => CoreRole::Worker,
        _ => CoreRole::Idle,
    }
}

/// Define o papel de um core (chamado pelo boot após CorePools init).
pub fn set_core_role(core_id: usize, role: CoreRole) {
    if core_id < MAX_CORES {
        CORE_ROLES[core_id].store(role as u8, Ordering::Release);
    }
}

/// Configura papéis padrão baseado em CorePools (r0=System, r1=Compute, r2=Worker).
/// Chamado pelo boot após .
pub fn init_default_roles() {
    // Core 0 (BSP) = System (já setado no static)
    set_core_role(0, CoreRole::System);

    if let Some(pools) = crate::smp::corepools::pools() {
        // ring1 (P-cores) = Compute
        for &cpu_id in pools.ring1.iter() {
            set_core_role(cpu_id as usize, CoreRole::Compute);
        }
        for &cpu_id in pools.ring2.iter() {
            set_core_role(cpu_id as usize, CoreRole::Worker);
        }
    }

    // Core 3 (se existe) = Memory (SGDB + network)
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    if cores > 3 {
        set_core_role(3, CoreRole::Memory);
    }

    // Log
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    let mut roles_log = alloc::string::String::from("SMP: core roles = [");
    for i in 0..cores.min(MAX_CORES) {
        if i > 0 { roles_log.push(' '); }
        use core::fmt::Write;
        let _ = write!(roles_log, "{}:{}", i, core_role(i).name());
    }
    roles_log.push(']');
    crate::slog_nano!("SMP", "info", "{}", roles_log);
}

/// Resolve core alvo considerando papéis: agents de latência-crítica
/// (affinity_ring=0) ficam em System; compute em Compute; memória em Memory.
pub fn resolve_target_core_for_role(
    affinity_ring: u8,
    _coherence_partner: Option<usize>,
    _agent_idx: usize,
) -> usize {
    if affinity_ring == 0 { return 0; }

    let target_role = match affinity_ring {
        1 => CoreRole::Compute,
        2 => CoreRole::Worker,
        _ => CoreRole::Worker,
    };

    // Encontra core com o papel desejado e menor carga
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    if cores <= 1 { return 0; }

    let mut best_core = 1;
    let mut best_len = usize::MAX;

    for c in 1..cores.min(MAX_CORES) {
        if core_role(c) == target_role {
            let len = RUN_QUEUES[c].len();
            if len < best_len {
                best_len = len;
                best_core = c;
            }
        }
    }

    // Fallback: se nenhum core com o papel desejado, menor carga geral
    if best_len == usize::MAX {
        for c in 1..cores.min(MAX_CORES) {
            let len = RUN_QUEUES[c].len();
            if len < best_len {
                best_len = len;
                best_core = c;
            }
        }
    }

    best_core
}

/// Distribui batch de agents para run-queues. Retorna quantos distribuídos.
pub fn distribute_batch(
    agents: &[(u32, u8, u8, Option<usize>, u8)],
    tick_id: u32,
) -> usize {
    let mut distributed = 0usize;
    for &(idx, affinity_ring, priority, coherence_partner, urgency) in agents {
        if affinity_ring == 0 { continue; }
        let target = resolve_target_core(affinity_ring, coherence_partner, idx as usize);
        let task = AgentTask::new(idx, tick_id, priority, affinity_ring, urgency);
        if enqueue_agent(target, task) {
            distributed += 1;
            wake_core_if_needed(target);
        }
    }
    distributed
}

// ─── Testes host ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use spin::Mutex;
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn agent_task_score_ordering() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        let t1 = AgentTask::new(0, 1, 0, 1, 0);
        let t2 = AgentTask::new(1, 1, 0, 1, 200);
        let t3 = AgentTask::new(2, 1, 2, 1, 100);
        assert!(t2.score() > t3.score());
        assert!(t3.score() > t1.score());
    }

    #[test]
    fn enqueue_dequeue_basic() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        let task = AgentTask::new(5, 100, 0, 1, 0);
        assert!(enqueue_agent(1, task));
        let got = dequeue_agent(1).unwrap();
        assert_eq!(got.agent_idx, 5);
        assert_eq!(got.tick_id, 100);
        assert!(dequeue_agent(1).is_none());
    }

    #[test]
    fn run_queue_fifo_order() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        for i in 0..10u32 {
            enqueue_agent(2, AgentTask::new(i, 1, 0, 1, 0));
        }
        for i in 0..10u32 {
            let t = dequeue_agent(2).unwrap();
            assert_eq!(t.agent_idx, i);
        }
    }

    #[test]
    fn run_queue_fill_and_reject() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        for i in 0..RQ_CAP as u32 {
            assert!(enqueue_agent(3, AgentTask::new(i, 1, 0, 1, 0)));
        }
        assert!(!enqueue_agent(3, AgentTask::new(999, 1, 0, 1, 0)));
        for _ in 0..RQ_CAP { assert!(dequeue_agent(3).is_some()); }
        assert!(dequeue_agent(3).is_none());
    }

    #[test]
    fn steal_from_busy_core() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(8, Ordering::SeqCst);
        for i in 0..5u32 {
            enqueue_agent(4, AgentTask::new(i, 1, 0, 1, 0));
        }
        let stolen = steal_agent(5);
        assert!(stolen.is_some());
        assert_eq!(stolen.unwrap().agent_idx, 0);
        assert_eq!(RUN_QUEUES[4].len(), 4);
    }

    #[test]
    fn steal_respects_min_one() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(8, Ordering::SeqCst);
        enqueue_agent(6, AgentTask::new(0, 1, 0, 1, 0));
        let stolen = steal_agent(7);
        assert!(stolen.is_none());
        assert_eq!(RUN_QUEUES[6].len(), 1);
    }

    #[test]
    fn total_pending_counts_all_queues() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(8, Ordering::SeqCst);
        enqueue_agent(0, AgentTask::new(0, 1, 0, 0, 0));
        enqueue_agent(1, AgentTask::new(1, 1, 0, 1, 0));
        enqueue_agent(2, AgentTask::new(2, 1, 0, 1, 0));
        assert!(total_pending() >= 3);
    }

    #[test]
    fn resolve_target_core_ring0_is_bsp() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        assert_eq!(resolve_target_core(0, None, 42), 0);
    }

    #[test]
    fn resolve_target_core_coherence_partner() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        let target = resolve_target_core(1, Some(42), 10);
        let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
        if cores > 1 {
            assert_eq!(target, 1 + (42 % (cores - 1)));
        }
    }

    #[test]
    fn cpu_stats_enqueued_increments() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        let before = CPU_STATS[1].enqueued.load(Ordering::Relaxed);
        enqueue_agent(1, AgentTask::new(0, 1, 0, 1, 0));
        let after = CPU_STATS[1].enqueued.load(Ordering::Relaxed);
        assert_eq!(after, before + 1);
    }

    #[test]
    fn distribute_batch_skips_ring0() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        let agents = vec![
            (0u32, 0u8, 0u8, None, 0u8),
            (1u32, 1u8, 0u8, None, 0u8),
            (2u32, 2u8, 1u8, None, 50u8),
        ];
        let dist = distribute_batch(&agents, 42);
        assert!(dist >= 1);
    }

    #[test]
    fn agent_task_default_is_invalid() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        let t = AgentTask::default();
        assert!(!t.is_valid());
    }

    #[test]
    fn core_role_default_system() {
        // Core 0 should be System by default
        assert_eq!(core_role(0), CoreRole::System);
    }

    #[test]
    fn core_role_set_and_get() {
        let _lock = TEST_LOCK.lock();
        set_core_role(5, CoreRole::Compute);
        assert_eq!(core_role(5), CoreRole::Compute);
        set_core_role(5, CoreRole::Memory);
        assert_eq!(core_role(5), CoreRole::Memory);
    }

    #[test]
    fn core_role_name() {
        assert_eq!(CoreRole::System.name(), "SYS");
        assert_eq!(CoreRole::Compute.name(), "COMP");
        assert_eq!(CoreRole::Memory.name(), "MEM");
        assert_eq!(CoreRole::Worker.name(), "WORK");
        assert_eq!(CoreRole::Idle.name(), "IDLE");
    }

    #[test]
    fn resolve_target_core_for_role_compute() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(4, Ordering::SeqCst);
        set_core_role(0, CoreRole::System);
        set_core_role(1, CoreRole::Compute);
        set_core_role(2, CoreRole::Worker);
        set_core_role(3, CoreRole::Memory);
        // ring 1 (Compute) -> core 1
        let target = resolve_target_core_for_role(1, None, 10);
        assert_eq!(target, 1);
    }

    #[test]
    fn resolve_target_core_for_role_worker() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(4, Ordering::SeqCst);
        set_core_role(0, CoreRole::System);
        set_core_role(1, CoreRole::Compute);
        set_core_role(2, CoreRole::Worker);
        set_core_role(3, CoreRole::Memory);
        // ring 2 (Worker) -> core 2
        let target = resolve_target_core_for_role(2, None, 10);
        assert_eq!(target, 2);
    }

    #[test]
    fn resolve_target_core_for_role_ring0_stays_bsp() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        let target = resolve_target_core_for_role(0, None, 10);
        assert_eq!(target, 0);
    }

    #[test]
    fn init_default_roles_doesnt_panic() {
        let _lock = TEST_LOCK.lock();
        // Just call it — shouldn't panic
        init_default_roles();
    }
}
