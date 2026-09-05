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

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

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
/// Bound técnico do array estático da RQ (não teto de produto AIOS).
/// Alinhado a LAPIC/`max_aps=255` metal: cabe até 256 lógicos; MADT > isto →
/// slog `fail` + HITL. Silício atual << 256 — o inventário MADT é que manda.
pub const MAX_CORES: usize = 256;

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

static RUN_QUEUES: [PerCoreRunQueue; MAX_CORES] = [const { PerCoreRunQueue::new() }; MAX_CORES];

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

static CPU_STATS: [CpuStats; MAX_CORES] = [const { CpuStats::new() }; MAX_CORES];

/// Overflow global (enqueue falhou — honesty, Tokio injector spirit).
static OVERFLOW_TOTAL: AtomicU32 = AtomicU32::new(0);

/// Inflight bitmap: agent_idx < 256 já enfileirado / em tick (Plinth ≠ CURRENT).
static INFLIGHT: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

static LAST_RQ_SLOG_TICK: AtomicU32 = AtomicU32::new(u32::MAX);
static LAST_DIST_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Gate de segurança para executar `Agent::tick` em APs.
///
/// A RQ distribui corretamente, mas `agent-core` ainda protege o Registry com
/// um único `AGENT_TICK_BUSY`. Um tick longo de Cortex/Hermes em AP segura esse
/// lock e congela Display/Input no BSP. APs continuam disponíveis para kernels
/// de compute; ticks de agents ficam BSP-only até haver isolamento por-agent.
pub fn agent_tick_offload_safe() -> bool {
    false
}

pub fn cpu_stats(core_id: usize) -> &'static CpuStats {
    &CPU_STATS[core_id.min(MAX_CORES - 1)]
}

pub fn overflow_total() -> u32 {
    OVERFLOW_TOTAL.load(Ordering::Relaxed)
}

fn inflight_test(idx: u32) -> bool {
    if idx >= 256 {
        return false;
    }
    let w = (idx / 64) as usize;
    let b = idx % 64;
    (INFLIGHT[w].load(Ordering::Acquire) & (1u64 << b)) != 0
}

fn inflight_set(idx: u32) {
    if idx >= 256 {
        return;
    }
    let w = (idx / 64) as usize;
    let b = idx % 64;
    INFLIGHT[w].fetch_or(1u64 << b, Ordering::AcqRel);
}

fn inflight_clear(idx: u32) {
    if idx >= 256 {
        return;
    }
    let w = (idx / 64) as usize;
    let b = idx % 64;
    INFLIGHT[w].fetch_and(!(1u64 << b), Ordering::AcqRel);
}

pub fn total_pending() -> usize {
    let mut total = 0;
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    for i in 0..cores.min(MAX_CORES) {
        total += RUN_QUEUES[i].len();
    }
    total
}

/// Imbalance max(len)-min(len) entre APs (core ≥1).
pub fn ap_imbalance() -> usize {
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    let n = cores.min(MAX_CORES);
    if n <= 2 {
        return 0;
    }
    let mut min_l = usize::MAX;
    let mut max_l = 0usize;
    for i in 1..n {
        let l = RUN_QUEUES[i].len();
        min_l = min_l.min(l);
        max_l = max_l.max(l);
    }
    max_l.saturating_sub(min_l)
}

/// Observe→Plan: redistribuir só se filas vazias, tick periódico, ou desbalance.
pub fn should_redistribute(tick_id: u32) -> bool {
    if total_pending() == 0 {
        return true;
    }
    if tick_id % 32 == 0 {
        return true;
    }
    ap_imbalance() > 2
}

fn slog_aggregate_stats(tick_id: u32) {
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    let n = cores.min(MAX_CORES);
    let mut enq = 0u32;
    let mut stolen = 0u32;
    let mut running = 0u32;
    let mut blocked = 0u32;
    for i in 0..n {
        let s = &CPU_STATS[i];
        enq = enq.wrapping_add(s.enqueued.load(Ordering::Relaxed));
        stolen = stolen.wrapping_add(s.stolen.load(Ordering::Relaxed));
        running = running.wrapping_add(s.running.load(Ordering::Relaxed));
        blocked = blocked.wrapping_add(s.blocked.load(Ordering::Relaxed));
    }
    let ov = OVERFLOW_TOTAL.load(Ordering::Relaxed);
    crate::slog_nano!(
        "SMP",
        "ok",
        "stats tick={} enq={} stolen={} running={} blocked={} overflow={} pending={}",
        tick_id,
        enq,
        stolen,
        running,
        blocked,
        ov,
        total_pending()
    );
}

// ─── BSP: distribui agents para run-queues ─────────────────────────────────

/// Limpa todas as run-queues (para testes).
pub fn clear_all_queues() {
    for i in 0..MAX_CORES {
        RUN_QUEUES[i].clear();
    }
    for w in &INFLIGHT {
        w.store(0, Ordering::Release);
    }
}

pub fn enqueue_agent(core_id: usize, task: AgentTask) -> bool {
    if core_id >= MAX_CORES {
        return false;
    }
    let ok = RUN_QUEUES[core_id].enqueue(task);
    if ok {
        CPU_STATS[core_id].enqueued.fetch_add(1, Ordering::Relaxed);
    }
    ok
}

pub fn dequeue_agent(core_id: usize) -> Option<AgentTask> {
    if core_id >= MAX_CORES {
        return None;
    }
    RUN_QUEUES[core_id].dequeue()
}

/// Soft affinity: ring0 nunca; ring1 Compute; ring2 Worker/Memory/Compute; ring3 Memory|Worker.
fn affinity_allows(core_id: usize, affinity_ring: u8) -> bool {
    if affinity_ring == 0 {
        return core_id == 0;
    }
    let role = core_role(core_id);
    match affinity_ring {
        1 => matches!(role, CoreRole::Compute | CoreRole::Worker),
        2 => matches!(role, CoreRole::Worker | CoreRole::Memory | CoreRole::Compute),
        3 => matches!(role, CoreRole::Memory | CoreRole::Worker),
        _ => role != CoreRole::System,
    }
}

/// Work-stealing: só se vítima `len>1` **e** affinity permite no ladrão (Redox-like soft).
pub fn steal_agent(core_id: usize) -> Option<AgentTask> {
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    let n = cores.min(MAX_CORES);
    if n <= 1 {
        return None;
    }
    for offset in 1..n {
        let victim = (core_id + offset) % n;
        if victim == core_id {
            continue;
        }
        if RUN_QUEUES[victim].len() <= 1 {
            continue;
        }
        if let Some(task) = RUN_QUEUES[victim].dequeue() {
            if !affinity_allows(core_id, task.affinity_ring) {
                let _ = RUN_QUEUES[victim].enqueue(task);
                continue;
            }
            CPU_STATS[victim].stolen.fetch_add(1, Ordering::Relaxed);
            return Some(task);
        }
    }
    None
}

/// Steal half∩4 para a fila local (Tokio/st3/smp-nostd); só se local vazia.
/// Retorna quantas tasks movidas. Não rouba slot em execução (só Ready na RQ).
pub fn steal_burst(thief: usize) -> usize {
    if thief >= MAX_CORES {
        return 0;
    }
    if !RUN_QUEUES[thief].is_empty() {
        return 0;
    }
    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    let n = cores.min(MAX_CORES);
    if n <= 1 {
        return 0;
    }
    for offset in 1..n {
        let victim = (thief + offset) % n;
        if victim == thief {
            continue;
        }
        let vlen = RUN_QUEUES[victim].len();
        if vlen <= 1 {
            continue;
        }
        let want = (vlen / 2).min(4).max(1);
        let mut moved = 0usize;
        for _ in 0..want {
            if RUN_QUEUES[victim].len() <= 1 {
                break;
            }
            let Some(task) = RUN_QUEUES[victim].dequeue() else {
                break;
            };
            if !affinity_allows(thief, task.affinity_ring) {
                let _ = RUN_QUEUES[victim].enqueue(task);
                continue;
            }
            if enqueue_agent(thief, task) {
                CPU_STATS[victim].stolen.fetch_add(1, Ordering::Relaxed);
                moved += 1;
            } else {
                let _ = RUN_QUEUES[victim].enqueue(task);
                break;
            }
        }
        if moved > 0 {
            return moved;
        }
    }
    0
}

// ─── IPI de reschedule ────────────────────────────────────────────────────

/// Envia IPI de reschedule (vetor 0x80) para um AP.
///
/// # Safety
/// Requer LAPIC habilitada e AP com IDT carregada.
pub unsafe fn send_reschedule_ipi_to(lapic_id: u32) {
    crate::apic::send_ipi_reschedule_to(lapic_id);
}

/// Acorda o AP **somente** se a fila passou de vazia→não-vazia (Tokio/ArceOS).
/// `was_empty`: estado da fila **antes** do enqueue bem-sucedido.
pub fn wake_core_if_needed(core_id: usize, was_empty: bool) {
    if core_id == 0 || !was_empty {
        return;
    }
    if !crate::smp::ap_pollable() {
        return;
    }
    if RUN_QUEUES[core_id].is_empty() {
        return;
    }
    let ap_index = core_id - 1;
    let Some(p) = crate::smp::percpu::ap_pcpu_ptr_mut(ap_index) else {
        return;
    };
    let lapic_id = unsafe { (*p).lapic_id };
    if lapic_id == 0 {
        return;
    }
    unsafe {
        send_reschedule_ipi_to(lapic_id);
    }
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
/// Default: core 0 = System; demais Worker até `init_roles_from_pools`.
static CORE_ROLES: [core::sync::atomic::AtomicU8; MAX_CORES] =
    [const { core::sync::atomic::AtomicU8::new(CoreRole::Worker as u8) }; MAX_CORES];

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

/// Papéis proporcionais a N + CorePools (ADR-0088 / SESSION_279).
/// Sem magic index (`core 3 = Memory`). Memory = fração dos Workers se N≥4.
pub fn init_roles_from_pools(n_cores: usize) {
    let n = if n_cores == 0 {
        crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize
    } else {
        n_cores
    };
    let n = n.max(1);
    if n > MAX_CORES {
        crate::slog_nano!(
            "SMP",
            "fail",
            "roles: MADT/n={} > MAX_CORES={} — clamp + HITL (array RQ, não teto de silício)",
            n,
            MAX_CORES
        );
    }
    let usable = n.min(MAX_CORES);

    for i in 0..MAX_CORES {
        set_core_role(i, if i == 0 { CoreRole::System } else { CoreRole::Idle });
    }
    set_core_role(0, CoreRole::System);

    let mut compute_ids: alloc::vec::Vec<usize> = alloc::vec::Vec::new();
    let mut worker_ids: alloc::vec::Vec<usize> = alloc::vec::Vec::new();

    if let Some(pools) = crate::smp::corepools::pools() {
        for &cpu_id in pools.ring1.iter() {
            let id = cpu_id as usize;
            if id > 0 && id < usable {
                compute_ids.push(id);
            }
        }
        for &cpu_id in pools.ring2.iter() {
            let id = cpu_id as usize;
            if id > 0 && id < usable {
                worker_ids.push(id);
            }
        }
    }

    // Sem pools (ou APs sem tipo): metade dos APs = Compute, resto Worker.
    if compute_ids.is_empty() && worker_ids.is_empty() && usable > 1 {
        let aps = usable - 1;
        let n_compute = if usable == 2 {
            1
        } else {
            (aps + 1) / 2
        };
        for i in 1..usable {
            if compute_ids.len() < n_compute {
                compute_ids.push(i);
            } else {
                worker_ids.push(i);
            }
        }
    }

    // N=2: BSP System + 1 Compute (garante mesmo se pools botaram Worker).
    if usable == 2 {
        compute_ids.clear();
        worker_ids.clear();
        compute_ids.push(1);
    }

    for &id in &compute_ids {
        set_core_role(id, CoreRole::Compute);
    }
    for &id in &worker_ids {
        set_core_role(id, CoreRole::Worker);
    }

    // Memory: só N≥5 (1); N≥8 floor(N/8). N=4 mantém ≥1 Worker (não come o único).
    let memory_n = if usable >= 8 {
        (usable / 8).max(1)
    } else if usable >= 5 {
        1
    } else {
        0
    };
    let mut memory_set = 0usize;
    // Preferir Workers, mas deixar ≥1 Worker se usable≥4 e havia workers.
    let keep_one_worker = usable >= 4 && !worker_ids.is_empty();
    let max_from_workers = if keep_one_worker {
        worker_ids.len().saturating_sub(1)
    } else {
        worker_ids.len()
    };
    let mut candidates: alloc::vec::Vec<usize> = alloc::vec::Vec::new();
    for &id in worker_ids.iter().rev() {
        if candidates.len() >= max_from_workers.min(memory_n) {
            break;
        }
        candidates.push(id);
    }
    if memory_n > candidates.len() {
        for &id in compute_ids.iter().rev() {
            if candidates.len() >= memory_n {
                break;
            }
            // Não zerar Compute se só resta 1.
            if compute_ids.len() > 1 || candidates.is_empty() {
                candidates.push(id);
            }
        }
    }
    for &id in &candidates {
        if memory_set >= memory_n {
            break;
        }
        set_core_role(id, CoreRole::Memory);
        memory_set += 1;
    }

    let mut n_sys = 0u32;
    let mut n_comp = 0u32;
    let mut n_work = 0u32;
    let mut n_mem = 0u32;
    for i in 0..usable {
        match core_role(i) {
            CoreRole::System => n_sys += 1,
            CoreRole::Compute => n_comp += 1,
            CoreRole::Worker => n_work += 1,
            CoreRole::Memory => n_mem += 1,
            CoreRole::Idle => {}
        }
    }
    crate::slog_nano!(
        "SMP",
        "ok",
        "roles n={} sys={} compute={} worker={} memory={}",
        usable,
        n_sys,
        n_comp,
        n_work,
        n_mem
    );
}

/// Alias legado — delega para `init_roles_from_pools`.
pub fn init_default_roles() {
    init_roles_from_pools(0);
}

/// Resolve core alvo considerando papéis: agents de latência-crítica
/// (affinity_ring=0) ficam em System; compute em Compute; worker em Worker;
/// ring3 → Memory com fallback Worker.
pub fn resolve_target_core_for_role(
    affinity_ring: u8,
    _coherence_partner: Option<usize>,
    _agent_idx: usize,
) -> usize {
    if affinity_ring == 0 {
        return 0;
    }

    let target_role = match affinity_ring {
        1 => CoreRole::Compute,
        2 => CoreRole::Worker,
        3 => CoreRole::Memory,
        _ => CoreRole::Worker,
    };

    let cores = crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize;
    if cores <= 1 {
        return 0;
    }

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

    // Fallback Memory → Worker; depois menor carga geral.
    if best_len == usize::MAX && target_role == CoreRole::Memory {
        for c in 1..cores.min(MAX_CORES) {
            if core_role(c) == CoreRole::Worker {
                let len = RUN_QUEUES[c].len();
                if len < best_len {
                    best_len = len;
                    best_core = c;
                }
            }
        }
    }

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

/// Distribui batch de agents para run-queues (por papel). Retorna quantos distribuídos.
/// Gate: `should_redistribute`; skip inflight; IPI só 0→1; slog rate-limit; overflow contado.
pub fn distribute_batch(
    agents: &[(u32, u8, u8, Option<usize>, u8)],
    tick_id: u32,
) -> usize {
    if tick_id % 64 == 0 {
        slog_aggregate_stats(tick_id);
    }
    if !should_redistribute(tick_id) {
        return 0;
    }

    let mut distributed = 0usize;
    for &(idx, affinity_ring, priority, coherence_partner, urgency) in agents {
        if affinity_ring == 0 {
            continue;
        }
        if inflight_test(idx) {
            continue;
        }
        let target = resolve_target_core_for_role(affinity_ring, coherence_partner, idx as usize);
        let task = AgentTask::new(idx, tick_id, priority, affinity_ring, urgency);
        let was_empty = RUN_QUEUES[target.min(MAX_CORES - 1)].is_empty();
        if enqueue_agent(target, task) {
            inflight_set(idx);
            distributed += 1;
            wake_core_if_needed(target, was_empty);
        } else {
            CPU_STATS[target.min(MAX_CORES - 1)]
                .blocked
                .fetch_add(1, Ordering::Relaxed);
            OVERFLOW_TOTAL.fetch_add(1, Ordering::Relaxed);
        }
    }
    if distributed > 0 {
        let last_t = LAST_RQ_SLOG_TICK.load(Ordering::Relaxed);
        let last_n = LAST_DIST_COUNT.load(Ordering::Relaxed);
        let elapsed = if last_t == u32::MAX {
            32
        } else {
            tick_id.wrapping_sub(last_t)
        };
        if elapsed >= 32 || distributed != last_n {
            crate::slog_nano!("SMP", "ok", "runqueue: {} agents → APs", distributed);
            LAST_RQ_SLOG_TICK.store(tick_id, Ordering::Relaxed);
            LAST_DIST_COUNT.store(distributed, Ordering::Relaxed);
        }
    }
    distributed
}

/// Fn: tick agent by index (instalada pelo bin com lock curto no registry).
/// Invariante (ArceOS wake_handoff): spinlock `AGENT_TICK_BUSY` no agent-core
/// deve ser curto — sem spin infinito esperando remote `on_cpu`.
pub type AgentTickFn = fn(agent_idx: u32, tick_id: u32) -> bool;
static AGENT_TICK_FN: AtomicUsize = AtomicUsize::new(0);

pub fn register_agent_tick_fn(f: AgentTickFn) {
    AGENT_TICK_FN.store(f as usize, Ordering::Release);
}

/// AP: dequeue local; se vazio steal_burst + dequeue; senão steal 1 → tick.
pub fn try_run_one_agent(core_id: usize) -> bool {
    let tick_ptr = AGENT_TICK_FN.load(Ordering::Acquire);
    if tick_ptr == 0 {
        return false;
    }
    let tick: AgentTickFn = unsafe { core::mem::transmute(tick_ptr) };
    let task = dequeue_agent(core_id).or_else(|| {
        let _ = steal_burst(core_id);
        dequeue_agent(core_id).or_else(|| steal_agent(core_id))
    });
    let Some(task) = task else {
        return false;
    };
    CPU_STATS[core_id.min(MAX_CORES - 1)]
        .running
        .fetch_add(1, Ordering::Relaxed);
    let ok = tick(task.agent_idx, task.tick_id);
    inflight_clear(task.agent_idx);
    ok
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
        // Soft affinity: ladrão precisa papel compatível com ring 1.
        set_core_role(4, CoreRole::Compute);
        set_core_role(5, CoreRole::Compute);
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
        let _lock = TEST_LOCK.lock();
        set_core_role(0, CoreRole::System);
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
        init_default_roles();
    }

    #[test]
    fn init_roles_n2_no_memory_on_core3() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(2, Ordering::SeqCst);
        init_roles_from_pools(2);
        assert_eq!(core_role(0), CoreRole::System);
        assert_eq!(core_role(1), CoreRole::Compute);
        // N=2: nunca Memory no índice 3 (inexistente) nem hardcode
        assert_ne!(core_role(1), CoreRole::Memory);
    }

    #[test]
    fn init_roles_n4_keeps_worker() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(4, Ordering::SeqCst);
        init_roles_from_pools(4);
        assert_eq!(core_role(0), CoreRole::System);
        let mut mem = 0u32;
        let mut work = 0u32;
        let mut comp = 0u32;
        for i in 1..4 {
            match core_role(i) {
                CoreRole::Memory => mem += 1,
                CoreRole::Worker => work += 1,
                CoreRole::Compute => comp += 1,
                _ => {}
            }
        }
        assert_eq!(mem, 0, "N=4 → Memory=0 (só N≥5)");
        assert!(work >= 1, "N=4 deve manter ≥1 Worker");
        assert!(comp >= 1, "deve restar Compute");
    }

    #[test]
    fn init_roles_n5_has_memory() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(5, Ordering::SeqCst);
        init_roles_from_pools(5);
        let mut mem = 0u32;
        for i in 1..5 {
            if core_role(i) == CoreRole::Memory {
                mem += 1;
            }
        }
        assert_eq!(mem, 1, "N≥5 → exatamente 1 Memory");
    }

    #[test]
    fn should_redistribute_when_empty() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(4, Ordering::SeqCst);
        assert!(should_redistribute(1));
    }

    #[test]
    fn should_redistribute_periodic() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(4, Ordering::SeqCst);
        enqueue_agent(1, AgentTask::new(0, 1, 0, 1, 0));
        assert!(!should_redistribute(1));
        assert!(should_redistribute(32));
    }

    #[test]
    fn inflate_skip_on_second_distribute() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(4, Ordering::SeqCst);
        set_core_role(0, CoreRole::System);
        set_core_role(1, CoreRole::Compute);
        set_core_role(2, CoreRole::Worker);
        set_core_role(3, CoreRole::Worker);
        let agents = vec![(7u32, 1u8, 0u8, None, 0u8)];
        let d1 = distribute_batch(&agents, 0);
        assert_eq!(d1, 1);
        let d2 = distribute_batch(&agents, 32);
        assert_eq!(d2, 0, "inflight skip");
    }

    #[test]
    fn steal_burst_moves_half_capped() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(4, Ordering::SeqCst);
        set_core_role(1, CoreRole::Compute);
        set_core_role(2, CoreRole::Compute);
        for i in 0..8u32 {
            enqueue_agent(1, AgentTask::new(i, 1, 0, 1, 0));
        }
        let moved = steal_burst(2);
        assert_eq!(moved, 4, "half∩4 de 8 = 4");
        assert_eq!(RUN_QUEUES[1].len(), 4);
        assert_eq!(RUN_QUEUES[2].len(), 4);
    }

    #[test]
    fn resolve_target_core_for_role_memory_ring3() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(5, Ordering::SeqCst);
        set_core_role(0, CoreRole::System);
        set_core_role(1, CoreRole::Compute);
        set_core_role(2, CoreRole::Worker);
        set_core_role(3, CoreRole::Memory);
        set_core_role(4, CoreRole::Worker);
        assert_eq!(resolve_target_core_for_role(3, None, 10), 3);
    }

    #[test]
    fn resolve_target_core_for_role_memory_fallback_worker() {
        let _lock = TEST_LOCK.lock();
        clear_all_queues();
        crate::smp::percpu::CPU_COUNT.store(4, Ordering::SeqCst);
        set_core_role(0, CoreRole::System);
        set_core_role(1, CoreRole::Compute);
        set_core_role(2, CoreRole::Worker);
        set_core_role(3, CoreRole::Worker);
        // Sem Memory: ring3 → Worker
        let t = resolve_target_core_for_role(3, None, 10);
        assert!(t == 2 || t == 3);
    }
}
