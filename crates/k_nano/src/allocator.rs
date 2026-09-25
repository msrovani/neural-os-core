//! Tier 1 — Global allocator (Hermes / JARBAS / UI).
//! Lazy Bump Allocator — auto-inicializável na primeira alloc() via CAS.
//! zero init, zero chicken-and-egg. TALC pós-boot para resize.
//! Re-exported from neural-kernel to make k_nano the canonical location.

use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write;
use core::sync::atomic::{AtomicIsize, AtomicU64, AtomicUsize, Ordering};
use agent_core::{AgentKind, ScheduleKind};
use spin::Mutex;
use talc::{Span, Talc, Talck, ErrOnOom};
use x86_64::structures::paging::{FrameAllocator, PageTable, PageTableFlags};
use x86_64::{PhysAddr, VirtAddr};

// ─── LazyBumpAllocator ───

/// Lazy Bump Allocator — auto-inicializa na primeira alloc() lendo HEAP_BUFFER.
/// CAS loop garante alinhamento atômico sem locks. Zero init externo.
pub struct LazyBumpAllocator {
    offset: AtomicIsize,
}

impl LazyBumpAllocator {
    pub const fn new() -> Self { Self { offset: AtomicIsize::new(-1) } }

    /// Retorna true se já foi inicializado (alguém já alocou).
    pub fn is_initialized(&self) -> bool { self.offset.load(Ordering::Relaxed) >= 0 }
}

unsafe impl GlobalAlloc for LazyBumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let heap_start = HEAP_BUFFER.as_mut_ptr() as usize;
        let size = layout.size();
        let align = layout.align().max(1);

        // SESSION_351: refuse só o impossível (size > janela ~2GB) — classe wrap
        // 4.7GB. Cap 256MiB era falso positivo: load Falcon3 copia ~315MB (align=1)
        // e o OOM/hlt vinha no meio do parse (agente=? boot).
        let window = bump_max_offset();
        if size > window {
            let agent = agent_core::oom_agent_label();
            note_alloc_refused(size, window, agent);
            {
                let mut buf = [0u8; 72];
                let mut n = 0usize;
                for &b in b"ALLOC refuse " {
                    if n < buf.len() {
                        buf[n] = b;
                        n += 1;
                    }
                }
                for &b in agent.as_bytes() {
                    if n < buf.len() {
                        buf[n] = b;
                        n += 1;
                    }
                }
                crate::interrupts::exception_fb_stamp(&buf[..n]);
            }
            return core::ptr::null_mut();
        }

        // SESSION_366: reserva 8MB só no teto da janela wrap. Grow livre até lá.
        const CRITICAL_RESERVE: usize = 8 * 1024 * 1024;
        const CONTROL_PLANE_MAX: usize = 8192;
        let window_soft = window.saturating_sub(CRITICAL_RESERVE);

        let mut current_offset = self.offset.load(Ordering::Relaxed);
        loop {
            let real_offset = if current_offset < 0 { 0 } else { current_offset as usize };
            let current_ptr = match heap_start.checked_add(real_offset) {
                Some(p) => p,
                None => return core::ptr::null_mut(),
            };
            let aligned_ptr = (current_ptr + align - 1) & !(align - 1);
            let next_offset = aligned_ptr.wrapping_sub(heap_start).saturating_add(size);
            let hard_limit = HEAP_LIMIT.load(Ordering::Relaxed).min(window);

            let allow_upto = if size <= CONTROL_PLANE_MAX {
                window
            } else {
                window_soft
            };
            if next_offset > allow_upto {
                let agent = agent_core::oom_agent_label();
                note_alloc_refused(next_offset, allow_upto, agent);
                return core::ptr::null_mut();
            }
            if next_offset > hard_limit {
                if grow_bump_auto(next_offset) {
                    current_offset = self.offset.load(Ordering::Relaxed);
                    continue;
                }
                let agent = agent_core::oom_agent_label();
                note_alloc_refused(next_offset, window, agent);
                return core::ptr::null_mut();
            }

            match self.offset.compare_exchange_weak(
                current_offset,
                next_offset as isize,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return aligned_ptr as *mut u8,
                Err(actual) => current_offset = actual,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // bump allocator — sem free
    }
}

/// Janela endereçável do bump heap (Fix A, wrap 2^64): HEAP_BUFFER vive no
/// high-half no FIM da imagem (.kheap), então `heap_start + offset` só é
/// válido até `usize::MAX - heap_start` (~2.0-2.1 GB). Computado uma vez.
/// Guard de 1 página no fim da janela.
static BUMP_MAX_OFFSET: AtomicUsize = AtomicUsize::new(0);

fn bump_max_offset() -> usize {
    let cached = BUMP_MAX_OFFSET.load(Ordering::Relaxed);
    if cached != 0 { return cached; }
    let heap_start = unsafe { HEAP_BUFFER.as_mut_ptr() as usize };
    let w = usize::MAX - heap_start - 4096;
    BUMP_MAX_OFFSET.store(w, Ordering::Relaxed);
    w
}

/// Auto-crescimento do bump heap (premissa AIOS: self-adapting heap).
/// Mapeia frames adicionais após o HEAP_BUFFER (.bss.heap) para acomodar
/// `need` bytes, em blocos de HEAP_GROW_STEP. VERIFICA presença real de cada
/// página após o mapeamento (map_page_direct falha silenciosamente quando
/// alloc_pt_frame retorna 0 — não deixar HEAP_LIMIT avançar sem páginas).
/// Retorna true se `need` ficou coberto; false = OOM real.
fn grow_bump_auto(need: usize) -> bool {
    let current_limit = HEAP_LIMIT.load(Ordering::Relaxed);
    if need <= current_limit {
        return true; // já coberto
    }
    // (3) Grow-gate entry: quotas + observe via ATOMICS ONLY — nunca
    // BOOT_LOG/EVENT_BUS/MHI/GLOBAL_ALLOCATOR locks aqui, nunca publish
    // (publish_heap_pressure_if_due roda FORA do grow — pode alocar).
    let entry_obs = heap_observe();
    let _entry_carve0 = core_quota_for(0);
    // Fix C: log do need na ENTRADA (o path wrap/refuse era cego).
    crate::slog_nano!("HEAP", "BUMP", "grow entry need={}MB limit={}MB",
        need / (1024 * 1024), current_limit / (1024 * 1024));
    // SESSION_287: HEAP_BUDGET_MB era escrito e nunca lido — grow ia até OOM.
    let budget_bytes = HEAP_BUDGET_MB
        .load(Ordering::Relaxed)
        .saturating_mul(1024 * 1024)
        .max(HEAP_SIZE);
    if current_limit >= budget_bytes {
        crate::slog_nano!("HEAP", "BUMP", "budget cap {}MB — recusa grow (need={}MB)",
            budget_bytes / (1024 * 1024), need / (1024 * 1024));
        return false;
    }
    let heap_start = unsafe { HEAP_BUFFER.as_mut_ptr() as usize };
    let base = VirtAddr::new(crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed));
    if base.as_u64() == 0 {
        return false;
    }

    // (3) Large-path gate lock-free: grows com ≥64MB descobertos precisam passar
    // em can_alloc_bytes (margem 8MB = CRITICAL_RESERVE); refuse → honesto, sem panic/halt.
    let uncovered = need.saturating_sub(current_limit);
    if uncovered >= 64 * 1024 * 1024 && !can_alloc_bytes(uncovered, 8) {
        note_alloc_refused(need, bump_max_offset(), "grow");
        crate::slog_nano!("HEAP", "fail", "grow refuse need={}MB uncovered={}MB headroom={}MB (advisory quota gate)",
            need / (1024 * 1024), uncovered / (1024 * 1024), entry_obs.headroom_mb);
        return false;
    }

    let want_raw = need.saturating_add(HEAP_GROW_STEP - 1) / HEAP_GROW_STEP * HEAP_GROW_STEP;
    // Fix A: clamp à janela endereçável (wrap 2^64) — nunca andar páginas
    // além do que heap_start + offset consegue endereçar.
    let window = bump_max_offset();
    if need > window {
        // Fix C: nomeia o agente requestor via seam FB (SESSION_316).
        let agent = agent_core::oom_agent_label();
        crate::slog_nano!("HEAP", "fail", "refuse need={}MB window=~{}MB (agente={})",
            need / (1024 * 1024), window / (1024 * 1024), agent);
        // AIOS Observe (SESSION_350): só atomics — NÃO alocar (EventBus) aqui.
        note_alloc_refused(need, window, agent);
        return false;
    }
    let want = want_raw.min(budget_bytes).min(window);
    if want <= current_limit {
        return false;
    }
    let extra = want.saturating_sub(current_limit);
    let diff_pages = extra.div_ceil(4096);

    let mut allocated = 0usize;
    for _i in 0..diff_pages {
        // Lock só para allocate_frame — solta ANTES de map_page_direct
        // (map_page_direct → alloc_pt_frame re-locka; TicketLock não é reentrante).
        let phys = {
            let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
            match g.as_mut().and_then(|a| a.allocate_frame()) {
                Some(f) => f.start_address().as_u64(),
                None => break,
            }
        };
        let virt_usize = match heap_start.checked_add(current_limit + allocated * 4096) {
            Some(v) => v,
            None => {
                crate::slog_nano!("HEAP", "fail", "grow wrap 2^64 — abort");
                break;
            }
        };
        let virt = VirtAddr::new(virt_usize as u64);
        // SESSION_252 diagnóstico (ora-1): loga o 1º frame físico alocado ao
        // heap — para comparar com os RX buffers do e1000 (corrupção OTA).
        if allocated < 4 || allocated % 512 == 0 {
            crate::slog_nano!("HEAP", "BUMP", "frame[{}] phys={:#x} virt={:#x}", allocated, phys, virt.as_u64());
        }
        unsafe {
            map_page_direct(base, virt, phys);
            // Verificação real (AIOS): mapa apenas se a página ficou PRESENT.
            if heap_pte_present(base, virt) {
                allocated += 1;
            }
        }
    }
    if allocated > 0 {
        let new_limit = current_limit + allocated * 4096;
        HEAP_LIMIT.store(new_limit, Ordering::Release);
        let new_mb = (new_limit + 1024 * 1024 - 1) / (1024 * 1024);
        CURRENT_HEAP_MB.store(new_mb, Ordering::SeqCst);
        crate::slog_nano!("HEAP", "BUMP", "auto-grow {} MB → {} MB (need={}MB, {} páginas, AIOS)",
            current_limit / (1024 * 1024), new_mb, need / (1024 * 1024), allocated);
    }
    // Retorna true SÓ se o novo limite cobre `need` (senão o alloc re-tenta).
    // (3) Exit: re-lê quotas + observe via atomics (sem publish — fora do grow).
    let _exit_obs = heap_observe();
    let _exit_carve0 = core_quota_for(0);
    need <= HEAP_LIMIT.load(Ordering::Relaxed)
}

/// Tamanho mínimo do bloco de auto-crescimento do heap.
const HEAP_GROW_STEP: usize = 256 * 1024 * 1024; // 256MB por passo

#[cfg(feature = "global-alloc")]
#[global_allocator]
static HEAP_ALLOC: LazyBumpAllocator = LazyBumpAllocator::new();

#[cfg(not(feature = "global-alloc"))]
static HEAP_ALLOC: LazyBumpAllocator = LazyBumpAllocator::new();

/// Returns the actual heap usage in bytes from the LazyBumpAllocator.
pub fn heap_used_bytes() -> usize {
    let offset = HEAP_ALLOC.offset.load(Ordering::Relaxed);
    if offset < 0 { 0 } else { offset as usize }
}

/// Janela endereçável do bump (~2GB) — Observe AIOS.
pub fn heap_window_bytes() -> usize {
    bump_max_offset()
}

/// Headroom real: window − used (nunca o HUD “RAM guest”).
pub fn heap_headroom_bytes() -> usize {
    bump_max_offset().saturating_sub(heap_used_bytes())
}

/// Tópicos EventBus (consumidor publica fora do grow — grow é alloc-free).
pub const TOPIC_HEAP_PRESSURE: &str = "HEAP_PRESSURE";
pub const TOPIC_ALLOC_REFUSED: &str = "ALLOC_REFUSED";

/// 0=ok 1=warn (headroom baixo) 2=critical (refuse recente).
static HEAP_PRESSURE_LEVEL: AtomicUsize = AtomicUsize::new(0);
static LAST_REFUSE_NEED: AtomicUsize = AtomicUsize::new(0);
static LAST_REFUSE_WINDOW: AtomicUsize = AtomicUsize::new(0);
static REFUSE_COUNT: AtomicU64 = AtomicU64::new(0);
static PRESSURE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Snapshot Observe (lock-free).
#[derive(Clone, Copy, Debug)]
pub struct HeapObserve {
    pub used_mb: usize,
    pub window_mb: usize,
    pub headroom_mb: usize,
    pub pressure: u8,
    pub last_refuse_need_mb: usize,
    pub refuse_count: u64,
    pub seq: u64,
}

pub fn heap_observe() -> HeapObserve {
    let used = heap_used_bytes();
    let window = bump_max_offset();
    let headroom = window.saturating_sub(used);
    // Warn proativo: <256MB headroom com modelo heavy já carregado.
    let mut pressure = HEAP_PRESSURE_LEVEL.load(Ordering::Acquire) as u8;
    if pressure < 1 && headroom < 256 * 1024 * 1024 {
        pressure = 1;
    }
    HeapObserve {
        used_mb: used / (1024 * 1024),
        window_mb: window / (1024 * 1024),
        headroom_mb: headroom / (1024 * 1024),
        pressure,
        last_refuse_need_mb: LAST_REFUSE_NEED.load(Ordering::Relaxed) / (1024 * 1024),
        refuse_count: REFUSE_COUNT.load(Ordering::Relaxed),
        seq: PRESSURE_SEQ.load(Ordering::Relaxed),
    }
}

/// Chamado no refuse do grow — **zero alloc**.
pub fn note_alloc_refused(need: usize, window: usize, _agent: &str) {
    LAST_REFUSE_NEED.store(need, Ordering::Release);
    LAST_REFUSE_WINDOW.store(window, Ordering::Release);
    REFUSE_COUNT.fetch_add(1, Ordering::Relaxed);
    HEAP_PRESSURE_LEVEL.store(2, Ordering::Release);
    PRESSURE_SEQ.fetch_add(1, Ordering::Relaxed);
}

/// InferQueue / Tensor: pedir bytes e ver se cabe com margem.
pub fn can_alloc_bytes(size: usize, margin_mb: usize) -> bool {
    let headroom = heap_headroom_bytes();
    let margin = margin_mb.saturating_mul(1024 * 1024);
    size.saturating_add(margin) <= headroom
}

/// Publica HEAP_PRESSURE no EventBus (chamar fora de grow — pode alocar).
pub fn publish_heap_pressure_if_due() {
    let obs = heap_observe();
    if obs.pressure == 0 {
        return;
    }
    use alloc::format;
    use event_bus::{CapabilityToken, Event};
    let payload = format!(
        "pressure={} used_mb={} window_mb={} headroom_mb={} refuse_need_mb={} refuse_n={} seq={}",
        obs.pressure,
        obs.used_mb,
        obs.window_mb,
        obs.headroom_mb,
        obs.last_refuse_need_mb,
        obs.refuse_count,
        obs.seq
    );
    let _ = crate::EVENT_BUS.publish(Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_HEAP_PRESSURE),
        payload: payload.into_bytes(),
        token: CapabilityToken::Legacy(1),
    });
}

/// Limpa critical → warn após plan degradar com sucesso.
pub fn clear_critical_pressure() {
    let cur = HEAP_PRESSURE_LEVEL.load(Ordering::Acquire);
    if cur >= 2 {
        HEAP_PRESSURE_LEVEL.store(1, Ordering::Release);
    }
}

pub fn set_pressure_warn() {
    let _ = HEAP_PRESSURE_LEVEL.compare_exchange(0, 1, Ordering::SeqCst, Ordering::Relaxed);
}

/// Buffer de heap estático — seção própria `.bss.heap` colocada no FIM da
/// imagem (limine.ld). Extensão alem dele (resize_bump_heap) só toca espaço
/// livre — NUNCA corrompe outras statics .bss (GLOBAL_ALLOCATOR, etc).
/// SESSION_233: sem isso, extender HEAP_LIMIT alem de HEAP_SIZE sobrescrevia
/// statics adjacentes e zerava total_frames (falsa exaustao de frames).
#[link_section = ".kheap"]
pub static mut HEAP_BUFFER: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

/// TALC allocator usado APÓS o boot para resize_heap (não é o global_allocator).
static TALC_ALLOC: Talck<spin::Mutex<()>, ErrOnOom> = Talck::new(Talc::new(ErrOnOom));
static CLAIMED_HEAP: Mutex<Option<Span>> = Mutex::new(None);

pub const HEAP_START: usize = 0x_4000_0000_0000;
pub const HEAP_SIZE: usize = 512 * 1024 * 1024; // 512MB .bss
pub static CURRENT_HEAP_MB: AtomicUsize = AtomicUsize::new(512);

/// Budget máximo do heap em MB. grow_bump_auto para ao atingir este limite.
/// Definido em main.rs baseado na RAM detectada (min(75% RAM, 1536MB)).
pub static HEAP_BUDGET_MB: AtomicUsize = AtomicUsize::new(1536);

/// Define o budget máximo do heap (chamado de main.rs no boot).
/// Fix A: clamp à janela endereçável — budget em MB-de-RAM não pode exceder
/// o offset máximo antes do wrap 2^64 (política e telemetria coerentes).
pub fn set_heap_budget_mb(mb: usize) {
    let window_mb = bump_max_offset() / (1024 * 1024);
    let mb = mb.min(window_mb);
    HEAP_BUDGET_MB.store(mb, Ordering::Release);
    crate::slog_nano!("HEAP", "BUDGET", "budget={}MB (window=~{}MB)", mb, window_mb);
}

// ─── (1) Per-core carve accounting (advisory auto-fractioning) ───────────────
// Fracionamento consultivo do budget por core: BSP 40%, APs dividem 60% igual.
// Computado UMA vez no boot via init_core_carves() (chamado de
// smp::runqueue::init_roles_from_pools — topologia final: budget veio de
// heap_budget_mb(RAM), divisor é CPU_COUNT). Pure atomics: nunca BOOT_LOG /
// EVENT_BUS / MHI / GLOBAL_ALLOCATOR locks aqui.
// ponytail: fair-share consultivo — o path de alloc não carimba cpu, então uso
// por-core NÃO é rastreado; a conta é quota (fatia justa) vs heap_used_bytes()
// global. Upgrade path: carimbar allocs por cpu se pressão por-core virar política.
// ─── (2) Per-agent quota table (advisory auto-fractioning) ───────────────────
// Mapa SEPARADO (não alarga AgentManifest): quota de headroom por ScheduleKind.
// Inference (Cortex/Hermes) despeja p/ MHI/arquivo, nunca bump (quota 0 no alloc).
// Lookup puro p/ o spawn path (runqueue::enqueue_agent*): over-quota → overflow
// global existente, nunca panic.

// (1) Carve máximo espelha smp::runqueue::MAX_CORES sem acoplar const cross-mod.
const CORE_CARVE_MAX: usize = 256;
static CORE_QUOTA: [AtomicUsize; CORE_CARVE_MAX] = [const { AtomicUsize::new(0) }; CORE_CARVE_MAX];
static CORE_CARVE_DONE: AtomicUsize = AtomicUsize::new(0);

/// Matemática pura do split (host-testável): quota em bytes p/ `cpu`.
/// BSP (cpu 0) = 40%; APs dividem 60% igualmente (resto do arredondamento no último AP).
fn split_core_carve(budget_bytes: usize, n_cores: usize, cpu: usize) -> usize {
    if n_cores == 0 || cpu >= n_cores {
        return 0;
    }
    if n_cores == 1 {
        return if cpu == 0 { budget_bytes } else { 0 };
    }
    let bsp = budget_bytes * 2 / 5;
    if cpu == 0 {
        return bsp;
    }
    let rem = budget_bytes.saturating_sub(bsp);
    let aps = n_cores - 1;
    let per = rem / aps;
    if cpu == n_cores - 1 {
        rem.saturating_sub(per.saturating_mul(aps - 1))
    } else {
        per
    }
}

/// Quota (fatia justa) do core em bytes. Pós-carve lê o static; pré-carve
/// (boot inicial, topologia ainda aberta) deriva on-the-fly dos atomics atuais
/// sem armazenar nem slogar — leitura pura, sem locks.
pub fn core_quota_for(cpu: usize) -> usize {
    if CORE_CARVE_DONE.load(Ordering::Acquire) == 1 {
        return CORE_QUOTA[cpu.min(CORE_CARVE_MAX - 1)].load(Ordering::Relaxed);
    }
    let budget = HEAP_BUDGET_MB.load(Ordering::Relaxed).saturating_mul(1024 * 1024).max(HEAP_SIZE);
    let n = (crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize).max(1).min(CORE_CARVE_MAX);
    split_core_carve(budget, n, cpu.min(CORE_CARVE_MAX - 1))
}

/// Headroom consultivo de um core: quota menos a fatia igual do uso global.
/// Contabiliza contra o heap_used_bytes() existente. Atomics only.
pub fn core_carve_headroom_bytes(cpu: usize) -> usize {
    let n = (crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize).max(1);
    core_quota_for(cpu).saturating_sub(heap_used_bytes() / n)
}

/// Carve único no boot (idempotente — primeiro caller vence). Emite
/// `HEAP BUMP carve cpu<N>=<MB>` uma vez. Só atomics + slog.
pub fn init_core_carves() {
    if CORE_CARVE_DONE.compare_exchange(0, 1, Ordering::SeqCst, Ordering::Relaxed).is_err() {
        return;
    }
    let budget = HEAP_BUDGET_MB.load(Ordering::Relaxed).saturating_mul(1024 * 1024).max(HEAP_SIZE);
    let n = (crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed) as usize).max(1).min(CORE_CARVE_MAX);
    for i in 0..n {
        CORE_QUOTA[i].store(split_core_carve(budget, n, i), Ordering::Relaxed);
    }
    for i in 0..n {
        crate::slog_nano!("HEAP", "BUMP", "carve cpu{}={}MB", i, CORE_QUOTA[i].load(Ordering::Relaxed) / (1024 * 1024));
    }
}

// (2) Quotas por ScheduleKind: Continuous=8MB, PollEvery=2MB, Oneshot/EventDriven=512KB.
/// Mapa estático separado indexado por discriminante de ScheduleKind:
/// [Oneshot, Continuous, PollEvery, EventDriven]. Não mexer no AgentManifest.
pub static AGENT_MEM_QUOTA: [usize; 4] = [
    512 * 1024,       // Oneshot
    8 * 1024 * 1024,  // Continuous
    2 * 1024 * 1024,  // PollEvery
    512 * 1024,       // EventDriven
];
/// Menor quota — backpressure genérico do spawn path (slot não custa bump,
/// mas freia spawn sob pressão real). Atomics only no caller.
pub const AGENT_QUOTA_MIN_BYTES: usize = 512 * 1024;

/// Lookup puro: quota de headroom p/ um ScheduleKind. Sem locks, sem alloc.
pub fn agent_mem_quota_for(schedule: &ScheduleKind) -> usize {
    match schedule {
        ScheduleKind::Oneshot => AGENT_MEM_QUOTA[0],
        ScheduleKind::Continuous => AGENT_MEM_QUOTA[1],
        ScheduleKind::PollEvery(_) => AGENT_MEM_QUOTA[2],
        ScheduleKind::EventDriven => AGENT_MEM_QUOTA[3],
    }
}

/// Política do path de alloc: kinds de inferência (Cortex/Hermes) despejam p/
/// MHI/arquivo e nunca consomem bump (quota 0 = sem orçamento bump).
pub fn agent_bump_quota(kind: &AgentKind, schedule: &ScheduleKind) -> usize {
    if matches!(kind, AgentKind::Inference) {
        return 0;
    }
    agent_mem_quota_for(schedule)
}

/// Teto atual do bump (redimensionável via grow_bump_auto/resize_bump_heap).
/// HEAP_SIZE (512MB) é só o tamanho INICIAL do array HEAP_BUFFER em `.bss.heap`
/// no FIM da imagem — crescer além dele é SEGURO (mapeia frames novos em espaço
/// livre via map_page_direct; SESSION_233 corrigido pelo link_section `.kheap`).
/// O limite REAL é a janela endereçável (~2GB, bump_max_offset) + HEAP_BUDGET_MB.
pub static HEAP_LIMIT: AtomicUsize = AtomicUsize::new(HEAP_SIZE);

/// Phys do `HEAP_BUFFER` (bump). 0 = ainda não reservado no PMM.
/// `alloc_pt_frame` recusa zerar um frame nesta faixa — alias HHDM sobre o
/// bump heap era o #PF-storm (PT escrita em nós BTree, CR2=0x16a).
static BUMP_HEAP_PHYS: AtomicU64 = AtomicU64::new(0);

/// Grava o phys do bump heap após `reserve_range` no boot. Idempotente.
pub fn set_bump_heap_phys(phys: u64) {
    BUMP_HEAP_PHYS.store(phys, Ordering::Release);
    crate::slog_nano!("HEAP", "info", "bump heap phys={:#x} len={}MB", phys, HEAP_SIZE / (1024 * 1024));
}

/// KERNEL_END virtual address (set from linker symbol at boot).
/// .kheap contains HEAP_BUFFER + statics placed by the linker after it.
/// The #PF handler needs to cover all pages up to KERNEL_END.
static KERNEL_VIRT_END: AtomicU64 = AtomicU64::new(0);

pub fn set_kernel_virt_end(addr: u64) {
    KERNEL_VIRT_END.store(addr, Ordering::Release);
}

/// Kernel physical base address (set from Limine handoff at boot).
/// Used by #PF handler to derive physical address for kernel virtual
/// addresses: phys = kernel_phys + (virt - kernel_virt_base).
static KERNEL_PHYS_BASE: AtomicU64 = AtomicU64::new(0);
static KERNEL_VIRT_BASE: AtomicU64 = AtomicU64::new(0);

pub fn set_kernel_phys_base(phys: u64, virt: u64) {
    KERNEL_PHYS_BASE.store(phys, Ordering::Release);
    KERNEL_VIRT_BASE.store(virt, Ordering::Release);
}

/// Returns (kernel_phys_base, kernel_virt_base) for diagnostics.
pub fn kernel_phys_virt() -> (u64, u64) {
    (KERNEL_PHYS_BASE.load(Ordering::Relaxed), KERNEL_VIRT_BASE.load(Ordering::Relaxed))
}

/// Diagnostic counters for #PF handler (lock-free).
pub static PF_DIAG_PMOFF_ZERO: AtomicU64 = AtomicU64::new(0);
pub static PF_DIAG_NO_RANGE: AtomicU64 = AtomicU64::new(0);
pub static PF_DIAG_ALLOC_FAIL: AtomicU64 = AtomicU64::new(0);
pub static PF_DIAG_MAP_FAIL: AtomicU64 = AtomicU64::new(0);
pub static PF_DIAG_OK: AtomicU64 = AtomicU64::new(0);
pub static PF_DIAG_P0: AtomicU64 = AtomicU64::new(0);

/// Returns all diagnostic counters as a tuple.
pub fn pf_diag() -> (u64, u64, u64, u64, u64, u64) {
    (
        PF_DIAG_PMOFF_ZERO.load(Ordering::Relaxed),
        PF_DIAG_NO_RANGE.load(Ordering::Relaxed),
        PF_DIAG_ALLOC_FAIL.load(Ordering::Relaxed),
        PF_DIAG_MAP_FAIL.load(Ordering::Relaxed),
        PF_DIAG_OK.load(Ordering::Relaxed),
        PF_DIAG_P0.load(Ordering::Relaxed),
    )
}

/// VA do `HEAP_BUFFER` (higher-half). O boot traduz para phys com virt_base
/// do Limine e reserva no PMM — não vazar `static mut` para o bin.
pub fn bump_heap_virt() -> u64 {
    core::ptr::addr_of!(HEAP_BUFFER) as u64
}

/// Slab zone: usa HEAP_BUFFER (em .bss, já identity-mapped pelo bootloader).
/// Não usa HEAP_START (0x4000_0000_0000) porque essa região não está mapeada
/// nas page tables durante o boot inicial — causava #PF → triple fault → boot loop.
pub const SLAB_START: usize = HEAP_START;
pub const SLAB_SIZE: usize = 8 * 65536;
pub const LARGE_HEAP_START: usize = HEAP_START + SLAB_SIZE;
pub const LARGE_HEAP_SIZE: usize = HEAP_SIZE - SLAB_SIZE;

pub fn try_alloc_check() -> bool {
    CLAIMED_HEAP.lock().is_some()
}

pub fn resize_heap_to_mb(target_mb: usize) {
    let current = CURRENT_HEAP_MB.load(Ordering::SeqCst);
    if target_mb <= current {
        return;
    }
    // ponytail: gate window — TALC não bypassa a janela endereçável.
    let diff_bytes = target_mb.saturating_sub(current).saturating_mul(1024 * 1024);
    if !can_alloc_bytes(diff_bytes, 0) {
        return;
    }
    let diff_pages = (target_mb - current).saturating_mul(256);
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let base = VirtAddr::new(pmoff);
    let start_virt = HEAP_START as u64 + (current as u64 * 1024 * 1024);

    let mut allocated = 0usize;
    for i in 0..diff_pages {
        let phys = {
            let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
            match g.as_mut().and_then(|a| a.allocate_frame()) {
                Some(f) => f.start_address().as_u64(),
                None => break,
            }
        };
        let virt = VirtAddr::new(start_virt + (i as u64 * 4096));
        unsafe {
            map_page_direct(base, virt, phys);
        }
        allocated += 1;
    }

    if allocated > 0 {
        let new_mb = current + allocated / 256;
        let new_size = new_mb * 1024 * 1024;
        unsafe {
            let mut guard = TALC_ALLOC.lock();
            // Um lock só: o guard de `*CLAIMED_HEAP.lock()` vive no corpo do
            // if; o segundo lock = self-deadlock do TicketLock (mesma classe
            // do LAST em k_hal aios_adapt — congela o boot sem serial).
            {
                let mut claimed = CLAIMED_HEAP.lock();
                if let Some(old) = *claimed {
                    let req = Span::from_base_size(LARGE_HEAP_START as *mut u8, new_size - SLAB_SIZE);
                    *claimed = Some(guard.extend(old, req));
                }
            }
        }
        CURRENT_HEAP_MB.store(new_mb, Ordering::SeqCst);
        crate::slog_nano!("HEAP", "TALC", "{} MB → {} MB ({} pages added)",
            current,
            new_mb,
            allocated);
    }
}

unsafe fn map_page_direct(base: VirtAddr, virt: VirtAddr, phys: u64) {
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4_virt = base + l4_frame.start_address().as_u64();
    let l4_tbl = &mut *(l4_virt.as_mut_ptr::<PageTable>());
    let e3 = &mut l4_tbl[virt.p4_index()];
    // HUGE_PAGE em TODOS os níveis (SESSION_250 §2.3/§4): não descer para um
    // walk se a entrada já é 1GB/2MB page — lê P2 garbage → páginas não-mapeadas
    // → #PF → reboot loop. Fix real do known-issue (commit 2662d50).
    if e3.flags().contains(PageTableFlags::HUGE_PAGE) {
        return;
    }
    if !e3.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 { return; }
        e3.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l3_virt = base + e3.addr().as_u64();
    let l3_tbl = &mut *(l3_virt.as_mut_ptr::<PageTable>());
    let e2 = &mut l3_tbl[virt.p3_index()];
    if e2.flags().contains(PageTableFlags::HUGE_PAGE) {
        return;
    }
    if !e2.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 { return; }
        e2.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l2_virt = base + e2.addr().as_u64();
    let l2_tbl = &mut *(l2_virt.as_mut_ptr::<PageTable>());
    let e1 = &mut l2_tbl[virt.p2_index()];
    if !e1.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 { return; }
        e1.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    if e1.flags().contains(PageTableFlags::HUGE_PAGE) {
        return;
    }
    let l1_virt = base + e1.addr().as_u64();
    let l1_tbl = &mut *(l1_virt.as_mut_ptr::<PageTable>());
    let pte = &mut l1_tbl[virt.p1_index()];
    if pte.flags().contains(PageTableFlags::PRESENT) {
        if !pte.flags().contains(PageTableFlags::WRITABLE) {
            // Add WRITABLE if page exists but is read-only
            let mut flags = pte.flags();
            flags.insert(PageTableFlags::WRITABLE);
            pte.set_flags(flags);
            x86_64::instructions::tlb::flush(virt);
        }
        return;
    }
    pte.set_addr(PhysAddr::new(phys), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    x86_64::instructions::tlb::flush(virt);
}

/// #PF cure: demand-map kernel pages on fault.
/// SESSION_293/299/300/301: cures #PF for:
/// 1. HEAP_START (0x_4000_0000_0000) — TALC allocator (pós-boot)
/// 2. HEAP_BUFFER linker addr — bump allocator (boot + runtime)
/// 3. Kernel virtual range — pages loaded by Limine but dropped from
///    kernel page tables, or pages in the LOAD segment beyond
///    KERNEL_END that code accesses (e.g., ATA buffers, statics).
///    Derives physical address from HHDM (identity map) so the
///    ORIGINAL frame is mapped (not a fresh allocation).
pub fn try_fault_in_heap(cr2: u64) -> bool {
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if pmoff == 0 {
        PF_DIAG_PMOFF_ZERO.fetch_add(1, Ordering::Relaxed);
        return false;
    }

    let virt = VirtAddr::new(cr2 & !0xFFF);
    let base = VirtAddr::new(pmoff);

    // Check 0: Already present? Just stale TLB after CR3 switch.
    if unsafe { heap_pte_present(base, virt) } {
        x86_64::instructions::tlb::flush(virt);
        return true;
    }

    // Determine which range the fault is in:
    // ponytail: range TALC canônico (LARGE_HEAP_*), não CURRENT_HEAP_MB (bump).
    let start = LARGE_HEAP_START as u64;
    let talc_end = start + LARGE_HEAP_SIZE as u64;
    let in_talc = cr2 >= start && cr2 < talc_end;

    let bump_start = unsafe { HEAP_BUFFER.as_mut_ptr() as u64 };
    let kvirt_end = KERNEL_VIRT_END.load(Ordering::Relaxed);
    let limit_end = bump_start + HEAP_LIMIT.load(Ordering::Relaxed) as u64;
    let bump_end = core::cmp::max(kvirt_end, limit_end);
    let in_bump = cr2 >= bump_start && cr2 < bump_end;

    // Range 3: kernel virtual range — pages in the kernel's LOAD segment
    // or beyond KERNEL_END that code accesses. The correct physical
    // address is kernel_phys + (cr2 - kernel_virt), NOT cr2 - HHDM.
    // Kernel virtual addresses (0xffffffff80000000+) are a separate
    // mapping from HHDM (0xffff800000000000+).
    // HHDM phys (for fallback in target_phys computation below).
    let hhdm_phys = cr2.wrapping_sub(pmoff);
    let kphys_check = KERNEL_PHYS_BASE.load(Ordering::Relaxed);
    let kvirt_check = KERNEL_VIRT_BASE.load(Ordering::Relaxed);
    let in_kernel_virt = if kphys_check != 0 && kvirt_check != 0 {
        let phys_k = kphys_check.wrapping_add(cr2.wrapping_sub(kvirt_check));
        cr2 >= kvirt_check && phys_k < 8 * 1024 * 1024 * 1024
    } else {
        // Fallback: assume any address in high half is kernel virt
        cr2 >= 0xffffffff80000000
    };

    if !in_talc && !in_bump && !in_kernel_virt {
        PF_DIAG_NO_RANGE.fetch_add(1, Ordering::Relaxed);
        return false;
    }

    if in_talc || in_bump {
        // Heap ranges: allocate a fresh frame (old behavior)
        if let Some(f) = crate::memory::alloc_physical_frame() {
            let p = f.start_address().as_u64();
            unsafe {
                map_page_direct(base, virt, p);
                x86_64::instructions::tlb::flush(virt);
                let ok = heap_pte_present(base, virt);
                if ok {
                    PF_DIAG_OK.fetch_add(1, Ordering::Relaxed);
                } else {
                    PF_DIAG_MAP_FAIL.fetch_add(1, Ordering::Relaxed);
                }
                return ok;
            }
        }
        PF_DIAG_ALLOC_FAIL.fetch_add(1, Ordering::Relaxed);
    }

    // Kernel virtual range: map the page using HHDM identity.
    // For addresses in the LOAD segment, the physical page exists in RAM
    // (mapped 1:1 via HHDM). For addresses beyond KERNEL_END that were
    // allocated by grow_bump_auto, we allocate a fresh frame.
    // Strategy: try to use the HHDM-derived physical address first;
    // if it fails (page table walk error), allocate a fresh frame.
    let kphys = KERNEL_PHYS_BASE.load(Ordering::Relaxed);
    let kvirt = KERNEL_VIRT_BASE.load(Ordering::Relaxed);
    let target_phys = if kphys != 0 && kvirt != 0 {
        kphys + (cr2 - kvirt)
    } else {
        // Fallback: HHDM phys
        hhdm_phys
    };
    // Choose the best physical frame to map:
    // 1. If within kernel image range (kphys..KERNEL_END), use kernel_phys+offset
    // 2. Otherwise, allocate a fresh frame
    let kvirt_end = KERNEL_VIRT_END.load(Ordering::Relaxed);
    let use_identity = kphys != 0 && kvirt != 0 && cr2 >= kvirt && cr2 < kvirt_end;
    let p = if use_identity {
        target_phys & !0xFFF
    } else if let Some(f) = crate::memory::alloc_physical_frame() {
        f.start_address().as_u64()
    } else {
        // Last resort: use kernel_phys+offset even outside kernel image
        target_phys & !0xFFF
    };
    if p == 0 {
        PF_DIAG_P0.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    unsafe {
        map_page_direct(base, virt, p);
        x86_64::instructions::tlb::flush(virt);
        let ok = heap_pte_present(base, virt);
        if ok {
            PF_DIAG_OK.fetch_add(1, Ordering::Relaxed);
        } else {
            PF_DIAG_MAP_FAIL.fetch_add(1, Ordering::Relaxed);
        }
        ok
    }
}

unsafe fn heap_pte_present(base: VirtAddr, virt: VirtAddr) -> bool {
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4 = &*((base + l4_frame.start_address().as_u64()).as_ptr::<PageTable>());
    let e3 = &l4[virt.p4_index()];
    if !e3.flags().contains(PageTableFlags::PRESENT) { return false; }
    let l3 = &*((base + e3.addr().as_u64()).as_ptr::<PageTable>());
    let e2 = &l3[virt.p3_index()];
    if !e2.flags().contains(PageTableFlags::PRESENT) { return false; }
    if e2.flags().contains(PageTableFlags::HUGE_PAGE) { return true; }
    let l2 = &*((base + e2.addr().as_u64()).as_ptr::<PageTable>());
    let e1 = &l2[virt.p2_index()];
    if !e1.flags().contains(PageTableFlags::PRESENT) { return false; }
    if e1.flags().contains(PageTableFlags::HUGE_PAGE) { return true; }
    let l1 = &*((base + e1.addr().as_u64()).as_ptr::<PageTable>());
    l1[virt.p1_index()].flags().contains(PageTableFlags::PRESENT)
}

unsafe fn alloc_pt_frame(base: VirtAddr) -> u64 {
    // Pool dedicada (se o boot chamou init_pt_pool). Fallback no geral.
    // NUNCA allocate_frame cru aqui: o geral pode devolver um frame do
    // `.kheap` se a reserva falhou — write_bytes via HHDM zera nós BTree.
    let pa = match crate::memory::alloc_pt_frame() {
        Some(f) => f.start_address().as_u64(),
        None => return 0,
    };
    let heap_phys = BUMP_HEAP_PHYS.load(Ordering::Relaxed);
    if heap_phys != 0
        && pa >= heap_phys
        && pa < heap_phys.saturating_add(HEAP_SIZE as u64)
    {
        crate::slog_nano!(
            "HEAP",
            "fail",
            "recusa PT frame {:#x} — alias do bump heap [{:#x}..{:#x}]",
            pa,
            heap_phys,
            heap_phys.saturating_add(HEAP_SIZE as u64)
        );
        return 0;
    }
    core::ptr::write_bytes((base + pa).as_mut_ptr::<u8>(), 0, 4096);
    pa
}

pub fn heap_stats() -> (usize, usize) {
    let claimed = CLAIMED_HEAP.lock();
    if let Some(span) = *claimed {
        (0, span.size())
    } else {
        (0, 0)
    }
}

#[cfg_attr(feature = "global-alloc", alloc_error_handler)]
fn oom(layout: core::alloc::Layout) -> ! {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b'O', options(nostack, preserves_flags));
    }
    // SESSION_339: carimba o agente que pediu o alloc (allocs grandes bypassam o
    // grow — TALC direto). tick_in_progress() é lock-free (seguro no OOM handler).
    let agent = agent_core::oom_agent_label();
    {
        let mut w = crate::vga_buffer::WRITER.lock();
        if let Some(ref mut w) = *w {
            let _ = write!(w, "[OOM/TALC] size={} align={} agente={}", layout.size(), layout.align(), agent);
        }
    }
    {
        let mut s = crate::serial::SERIAL.lock();
        if let Some(ref mut s) = *s {
            let _ = write!(s, "[OOM/TALC] sem memoria Tier 1. size={} align={} agente={} Verifique HEAP_SIZE.\n",
                layout.size(), layout.align(), agent);
        }
    }
    // FB: o serial é invisível no metal — carimba o canal FB (SESSION_316/330).
    {
        let mut buf = [0u8; 96];
        let mut n = 0usize;
        for &b in b"OOM agente=" { if n < buf.len() { buf[n] = b; n += 1; } }
        for &b in agent.as_bytes() { if n < buf.len() { buf[n] = b; n += 1; } }
        for &b in b" size=" { if n < buf.len() { buf[n] = b; n += 1; } }
        let mut v = layout.size();
        let mut digits = [0u8; 20];
        let mut d = 0usize;
        if v == 0 { digits[0] = b'0'; d = 1; }
        while v > 0 && d < 20 { digits[d] = b'0' + (v % 10) as u8; v /= 10; d += 1; }
        while d > 0 { d -= 1; if n < buf.len() { buf[n] = digits[d]; n += 1; } }
        crate::interrupts::exception_fb_stamp(&buf[..n]);
    }
    loop {
        x86_64::instructions::hlt();
    }
}

/// Inicializa TALC (SLAB adiado). LazyBumpAllocator auto-inicializa na primeira alloc().
pub fn init_heap() -> Result<(), &'static str> {
    // Safety: chamado uma vez no boot, single-threaded, antes de qualquer alocação.
    // TALC claim adiado: 0x4000_0000_0000 não está mapeado até init_memory.
    // O LazyBumpAllocator cobre as allocs iniciais do HEAP_BUFFER (.bss).
    // TALC será inicializado em talc_init_post_memory() após init_global_allocator.
    crate::slog_nano!("HEAP", "TALC", "Tier 1 deferred (call talc_init_post_memory after init_global_allocator)");
    Ok(())
}

/// Inicializa TALC com span em HEAP_START (0x4000_0000_0000). Deve ser chamado APÓS
/// init_global_allocator (global frame allocator disponível) e APÓS resize_bump_heap.
/// TALC gerencia páginas mapeadas via frame allocator — pool separado do bump allocator.
pub fn talc_init_post_memory() -> Result<(), &'static str> {
    let span = Span::from_base_size(LARGE_HEAP_START as *mut u8, LARGE_HEAP_SIZE);
    let claimed = unsafe {
        TALC_ALLOC.lock().claim(span).map_err(|_| "talc claim failed")?
    };
    *CLAIMED_HEAP.lock() = Some(claimed);
    crate::slog_nano!("HEAP", "TALC", "Tier 1 ready: virt={:#x} size={} MB",
        LARGE_HEAP_START,
        LARGE_HEAP_SIZE / (1024 * 1024));
    Ok(())
}

/// Estende o LazyBumpAllocator com mais páginas mapeadas após o .bss.
/// SEGURO (SESSION_233): HEAP_BUFFER agora fica em `.bss.heap` no FIM da
/// imagem (limine.ld) — a extensão além dele mapeia frames novos em espaço
/// livre, sem corromper statics .bss adjacentes (GLOBAL_ALLOCATOR, etc).
pub fn resize_bump_heap(target_mb: usize) {
    // ponytail: delega ao path canônico (budget/window/heap_pte_present); mantém API.
    grow_bump_auto(target_mb.saturating_mul(1024 * 1024));
}

#[cfg(test)]
mod auto_fractioning_tests {
    use super::*;

    #[test]
    fn agent_quota_table_lookup() {
        assert_eq!(agent_mem_quota_for(&ScheduleKind::Continuous), 8 * 1024 * 1024);
        assert_eq!(agent_mem_quota_for(&ScheduleKind::PollEvery(200)), 2 * 1024 * 1024);
        assert_eq!(agent_mem_quota_for(&ScheduleKind::Oneshot), 512 * 1024);
        assert_eq!(agent_mem_quota_for(&ScheduleKind::EventDriven), 512 * 1024);
        assert_eq!(AGENT_QUOTA_MIN_BYTES, 512 * 1024);
    }

    #[test]
    fn inference_spills_to_mhi_never_bump() {
        assert_eq!(agent_bump_quota(&AgentKind::Inference, &ScheduleKind::Continuous), 0);
        assert_eq!(agent_bump_quota(&AgentKind::Inference, &ScheduleKind::Oneshot), 0);
        assert_eq!(agent_bump_quota(&AgentKind::System, &ScheduleKind::Continuous), 8 * 1024 * 1024);
        assert_eq!(agent_bump_quota(&AgentKind::Driver, &ScheduleKind::PollEvery(1)), 2 * 1024 * 1024);
    }

    #[test]
    fn core_carve_split_math() {
        let b = 1536 * 1024 * 1024;
        // n=1: tudo no BSP
        assert_eq!(split_core_carve(b, 1, 0), b);
        // BSP = 40%
        assert_eq!(split_core_carve(b, 4, 0), b * 2 / 5);
        // soma fecha no budget (resto no último AP)
        let sum4: usize = (0..4).map(|c| split_core_carve(b, 4, c)).sum();
        assert_eq!(sum4, b);
        let sum2: usize = (0..2).map(|c| split_core_carve(b, 2, c)).sum();
        assert_eq!(sum2, b);
        // APs iguais entre si
        assert_eq!(split_core_carve(b, 4, 1), split_core_carve(b, 4, 2));
        // fora do range = 0
        assert_eq!(split_core_carve(b, 4, 9), 0);
        assert_eq!(split_core_carve(b, 0, 0), 0);
    }
}
