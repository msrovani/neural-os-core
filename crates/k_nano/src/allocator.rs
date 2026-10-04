//! Tier 1 — Global allocator (Hermes / JARBAS / UI).
//! Lazy Bump Allocator — auto-inicializável na primeira alloc() via CAS.
//! zero init, zero chicken-and-egg. TALC pós-boot para resize.
//! Re-exported from neural-kernel to make k_nano the canonical location.

use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, AtomicUsize, Ordering};
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
            // s437: guarda de wrap — `current_ptr + align - 1` perto de
            // usize::MAX envolve para VA baixa; escrita nesse ponteiro =
            // #PF em endereço baixo (storm → park). Refusa honestamente.
            if aligned_ptr < heap_start {
                return core::ptr::null_mut();
            }
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
///
/// Tick sob o qual TODO slice de inferência executa: manifest `infer_worker`
/// (hermes::agents::INFER_WORKER_MANIFEST) + stamp `note_background_agent` em
/// `poll_slice_stamped` (cobre BSP fallback e AP idle). Lido via agent_core
/// (cortex::INFER_IN_FLIGHT seria dependência circular). Residual honesto:
/// slice no AP enquanto o BSP ticka outro agente não carimba (stamp global,
/// não per-CPU) → grow passa (status quo); cobertura total exigiria seam
/// cortex→k_nano.
const INFER_SLICE_TICK: &str = "infer_worker";

/// Último HEAP_LIMIT já logado no refuse-in-slice (anti-spam: cada alloc além
/// do LIMIT re-chamaria o grow; o TALC serve os spills no intervalo).
static SLICE_GROW_REFUSED_AT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Último HEAP_LIMIT já logado no cap de orçamento do bump (anti-spam s437:
/// com o bump no teto, cada alloc que estoura re-entrava no grow e logava
/// `grow entry`+`budget cap` 2× — 44k logs/10min — antes do TALC servir o
/// spill; loga 1× por valor de HEAP_LIMIT, como o refuse-in-slice).
static CAP_REFUSED_LOG_AT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// True se um slice de inferência está em curso (qualquer core). Só atomics +
/// comparação de bytes — seguro no path de alloc (nunca aloca/publica).
fn infer_slice_in_progress() -> bool {
    match agent_core::tick_in_progress() {
        Some((n, _)) => n.as_bytes() == INFER_SLICE_TICK.as_bytes(),
        None => false,
    }
}

/// Passo 2MB no grow (tarefa b): gate default-OFF. Viável onde a plataforma
/// permite — PMM `allocate_huge_2mb` entrega runs 2MB-alinhados e
/// `heap_pte_present`/`map_page_direct` já entendem PDE HUGE (SESSION_250).
/// OFF porque: (1) HEAP_BUFFER 2MB-alinhado não garantido pelo limine.ld;
/// (2) PDE ocupada aborta o chunk; (3) ganho (TLB reach) não medido no alvo.
/// Fallback = loop 4KB intacto abaixo. Nunca no fast path (só aqui no grow).
/// ponytail: sem teardown parcial — chunk falho cai inteiro no 4KB; PDE só é
/// instalada em slot livre, nunca demolida. Upgrade: medir tok/s com gate ON.
const HEAP_GROW_HUGE_2MB: bool = false;
const HUGE_2MB: usize = 2 * 1024 * 1024;
const FRAMES_PER_2MB: usize = HUGE_2MB / 4096; // 512

/// Pré-condições puras p/ cobrir `len` bytes a partir de `virt` com PDEs 2MB:
/// alinhamento do início (chunks seguintes herdam, passo 2MB) + ≥1 chunk.
/// Pura e host-testável. Alinhamento físico vem de `allocate_huge_2mb`.
fn huge2m_candidate(virt: usize, len: usize) -> bool {
    len >= HUGE_2MB && virt % HUGE_2MB == 0
}

/// Leitura: PDE do `virt` livre p/ HUGE 2MB? PD ausente = criável no map;
/// PDE presente (tabela 4K ou huge) = ocupada. Nunca aloca/mapeia aqui.
unsafe fn huge2m_slot_free(base: VirtAddr, virt: VirtAddr) -> bool {
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4 = &*((base + l4_frame.start_address().as_u64()).as_ptr::<PageTable>());
    let e3 = &l4[virt.p4_index()];
    if e3.flags().contains(PageTableFlags::HUGE_PAGE) {
        return false;
    }
    if !e3.flags().contains(PageTableFlags::PRESENT) {
        return true;
    }
    let l3 = &*((base + e3.addr().as_u64()).as_ptr::<PageTable>());
    let e2 = &l3[virt.p3_index()];
    if e2.flags().contains(PageTableFlags::HUGE_PAGE) {
        return false;
    }
    if !e2.flags().contains(PageTableFlags::PRESENT) {
        return true;
    }
    let l2 = &*((base + e2.addr().as_u64()).as_ptr::<PageTable>());
    !l2[virt.p2_index()].flags().contains(PageTableFlags::PRESENT)
}

/// Instala 1 PDE 2MB WB de heap (PRESENT|WRITABLE|HUGE_PAGE — SEM UC/WT do
/// MMIO). Pré: slot livre + `phys` 2MB-alinhado. True se ficou PRESENT.
/// Falha (slot corrido em SMP) = caller cai no 4KB; o run alocado vaza
/// (mesma classe do leak 4K pré-existente em map concomitante).
unsafe fn map_page_2mb(base: VirtAddr, virt: VirtAddr, phys: u64) -> bool {
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4_tbl = &mut *((base + l4_frame.start_address().as_u64()).as_mut_ptr::<PageTable>());
    let e3 = &mut l4_tbl[virt.p4_index()];
    if e3.flags().contains(PageTableFlags::HUGE_PAGE) {
        return false;
    }
    if !e3.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 {
            return false;
        }
        e3.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l3_tbl = &mut *((base + e3.addr().as_u64()).as_mut_ptr::<PageTable>());
    let e2 = &mut l3_tbl[virt.p3_index()];
    if e2.flags().contains(PageTableFlags::HUGE_PAGE) {
        return false;
    }
    if !e2.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 {
            return false;
        }
        e2.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l2_tbl = &mut *((base + e2.addr().as_u64()).as_mut_ptr::<PageTable>());
    let pde = &mut l2_tbl[virt.p2_index()];
    if pde.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }
    pde.set_addr(
        PhysAddr::new(phys),
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::HUGE_PAGE,
    );
    x86_64::instructions::tlb::flush(virt);
    heap_pte_present(base, virt)
}

/// Cobre o head contíguo de `extra` com PDEs 2MB. Retorna frames-4K
/// equivalentes (soma direto no `allocated` do grow — contabilidade intacta).
/// Primeiro chunk falho (PDE ocupada, sem run contíguo, PT sem frame) = break:
/// o loop 4KB cobre o restante contiguamente (sem buracos na contagem).
unsafe fn grow_huge_2mb(base: VirtAddr, heap_start: usize, current_limit: usize, extra: usize) -> usize {
    let start = match heap_start.checked_add(current_limit) {
        Some(v) => v,
        None => return 0,
    };
    if !huge2m_candidate(start, extra) {
        return 0;
    }
    let mut mapped_4k = 0usize;
    let mut off = 0usize;
    while off + HUGE_2MB <= extra {
        // `start` 2MB-alinhado + `off` múltiplo de 2MB: sem wrap (off < extra ≤ window).
        let virt = VirtAddr::new((start + off) as u64);
        if !huge2m_slot_free(base, virt) {
            break;
        }
        // Lock só p/ alocar o run — solto ANTES do map (mesma disciplina do
        // loop 4KB: TicketLock não é reentrante).
        let phys = {
            let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
            match g.as_mut().and_then(|a| a.allocate_huge_2mb(FRAMES_PER_2MB)) {
                Some(f) => f.start_address().as_u64(),
                None => break,
            }
        };
        if !map_page_2mb(base, virt, phys) {
            break;
        }
        mapped_4k += FRAMES_PER_2MB;
        off += HUGE_2MB;
    }
    mapped_4k
}

fn grow_bump_auto(need: usize) -> bool {
    let current_limit = HEAP_LIMIT.load(Ordering::Relaxed);
    if need <= current_limit {
        return true; // já coberto
    }
    // SESSION_432(a): nunca estender o bump de dentro de um slice de
    // inferência — o auto-grow marchava 256MB/slice até o wrap (~2030MB)
    // (boot_whpx_20260930: 8 grows 512→1536 + OOM/TALC). Recusa honesta no
    // padrão existente: HybridAllocator cai no TALC (free real + demand-page).
    // heap_headroom_low() barra a ENTRADA do slice; isto barra o MEIO.
    if infer_slice_in_progress() {
        // 1 slog por valor de HEAP_LIMIT (cada alloc além do LIMIT re-chamaria
        // o grow; o TALC serve os spills no intervalo — sem o gate, spam/alloc).
        if SLICE_GROW_REFUSED_AT.load(Ordering::Relaxed) != current_limit {
            SLICE_GROW_REFUSED_AT.store(current_limit, Ordering::Relaxed);
            note_alloc_refused(need, bump_max_offset(), INFER_SLICE_TICK);
            crate::slog_nano!("HEAP", "warn",
                "grow refused (infer in-flight) need={}MB limit={}MB — spill→TALC, HITL",
                need / (1024 * 1024), current_limit / (1024 * 1024));
        }
        return false;
    }
    // SESSION_287: HEAP_BUDGET_MB era escrito e nunca lido — grow ia até OOM.
    let budget_bytes = BUMP_BUDGET_CLAMPED
        .load(Ordering::Relaxed)
        .saturating_mul(1024 * 1024)
        .max(HEAP_SIZE);
    // Cap ANTES do entry-observe/log: com o bump no orçamento o grow NUNCA
    // sucede (o TALC serve o spill). Sem esta ordem, cada alloc que estoura
    // logava `grow entry`+`budget cap` e pagava heap_observe à toa — flood
    // s437 (44k logs/10min). Log único por HEAP_LIMIT, como o refuse-in-slice.
    if current_limit >= budget_bytes {
        if CAP_REFUSED_LOG_AT.load(Ordering::Relaxed) != current_limit {
            CAP_REFUSED_LOG_AT.store(current_limit, Ordering::Relaxed);
            crate::slog_nano!("HEAP", "BUMP", "budget cap {}MB — recusa grow (need={}MB); spill→TALC",
                budget_bytes / (1024 * 1024), need / (1024 * 1024));
        }
        return false;
    }
    // (3) Grow-gate entry: quotas + observe via ATOMICS ONLY — nunca
    // BOOT_LOG/EVENT_BUS/MHI/GLOBAL_ALLOCATOR locks aqui, nunca publish
    // (publish_heap_pressure_if_due roda FORA do grow — pode alocar). Só chega
    // aqui quando o grow PODE suceder (abaixo do cap).
    let entry_obs = heap_observe();
    let _entry_carve0 = core_quota_for(0);
    // Fix C: log do need na ENTRADA (o path wrap/refuse era cego).
    crate::slog_nano!("HEAP", "BUMP", "grow entry need={}MB limit={}MB",
        need / (1024 * 1024), current_limit / (1024 * 1024));
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
    // (b) Caminho 2MB (default-OFF): cobre o head em PDEs; o loop 4KB cobre o
    // restante. `allocated` em unidades 4K — o loop abaixo começa exatamente
    // onde o huge parou (virt = heap_start + current_limit + allocated*4096).
    let mut allocated = 0usize;
    if HEAP_GROW_HUGE_2MB {
        allocated = unsafe { grow_huge_2mb(base, heap_start, current_limit, extra) };
    }
    let diff_pages = extra.saturating_sub(allocated * 4096).div_ceil(4096);

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

// ─── SESSION_415: Hybrid TALC-first allocator ───────────────────────────────
// O bump sem free ("dealloc = no-op") satura a janela de ~2GB com o runtime
// vivo (decode LLM + TTS + bus + memória) → alloc NULL → caller deref → #PF →
// AP hlt (evidência 2 boots: teto 2030MB + heap-fail). O TALC (heap canônico
// por AGENTS.md) tem free real e já é inicializado pós-boot
// (talc_init_post_memory) com demand-page cobrindo o range in_talc.
// Política: pós-init, TALC é primário; bump continua cobrindo allocs de boot
// (antes do claim) e é fallback honesto se TALC ainda não foi claimado.
struct HybridAllocator;

static TALC_READY: AtomicBool = AtomicBool::new(false);

// ─── Diagnóstico TALC NULL (idea #630, s434) ────────────────────────────────
// O OOM `infer_worker` com span de 6911MB é o residual de 3 sessões. O counter
// TALC_OVERFLOW_NULL diz QUE falhou, não POR QUÊ. Candidatas distinguíveis:
//   (1) PMM exausto — demand-page do gap-node tocado não conseguiu frame;
//   (2) gap-node fora do span demand-pagado (cr2 >= TALC_SPAN_END) → #PF
//       sem handler de heap → o talc lê lixo como tamanho do chunk;
//   (3) fragmentação real — bins sem chunk ≥ required (span útil, memória
//       viva em pedaços menores que o pedido);
//   (4) corrupção — talc feature "counters" (4µs/alloc) ou sanity-check
//       manual dos bins na morte (custo zero no caminho feliz).
// Tudo ATOMIC, zero-alloc (chamável de dentro do path de OOM).

/// Counter (2): último cr2 de fault no range TALC que ficou FORA do span
/// demand-pagado (end = TALC_SPAN_END). 0 = nenhum.
static TALC_PF_OUTSIDE_SPAN: AtomicU64 = AtomicU64::new(0);
/// Counter (3): allocs NULL no TALC com bins não-vazios (fragmentação
/// provável — havia memória registrada, nenhum chunk grande o bastante).
static TALC_NULL_BINS_NONEMPTY: AtomicU64 = AtomicU64::new(0);
/// Counter (3b): allocs NULL com bins VAZIOS (nada foi dado ao talc — span
/// nunca se materializou em gaps: causam (1)/(2) ou claim sem gaps).
static TALC_NULL_BINS_EMPTY: AtomicU64 = AtomicU64::new(0);
/// Snapshot one-shot no 1º NULL: availability_low/high dos bins do talc
/// (64 bits de bitmap cada; 0/0 = bins vazios) + requested/required.
static TALC_NULL_AVAIL_LOW: AtomicU64 = AtomicU64::new(0);
static TALC_NULL_AVAIL_HIGH: AtomicU64 = AtomicU64::new(0);
static TALC_NULL_REQ_SIZE: AtomicUsize = AtomicUsize::new(0);
static TALC_NULL_REQ_CHUNK: AtomicUsize = AtomicUsize::new(0);

pub fn talc_null_diag() -> (u64, u64, u64, u64, u64, usize, usize) {
    (
        TALC_OVERFLOW_NULL.load(Ordering::Relaxed),
        TALC_PF_OUTSIDE_SPAN.load(Ordering::Relaxed),
        TALC_NULL_BINS_NONEMPTY.load(Ordering::Relaxed),
        TALC_NULL_BINS_EMPTY.load(Ordering::Relaxed),
        TALC_PF_OUTSIDE_SPAN.load(Ordering::Relaxed), // (legado: alias p/ logs)
        TALC_NULL_REQ_SIZE.load(Ordering::Relaxed),
        TALC_NULL_REQ_CHUNK.load(Ordering::Relaxed),
    )
}

pub fn talc_pf_outside_span_cr2() -> u64 {
    TALC_PF_OUTSIDE_SPAN.load(Ordering::Relaxed)
}

/// Sanity-check dos bins do talc na morte (custo zero no caminho feliz —
/// só roda no 1º NULL): 0/0 = bins vazios (nada claimado/materializado);
/// !=0 com NULL = fragmentação ou gap-nodes ilegíveis (cr2 fora do span).
/// Lê `availability_low/high` privados via raw pointer (módulo irmão de
/// confiança; scan_for_errors do talc pode PANICAR — nunca no path de OOM).
fn snapshot_talc_bins(talc: &talc::Talc<talc::ErrOnOom>) {
    // Layout real de talc-4.4.3 (src/talc.rs): availability_low @0,
    // availability_high @8, bins @16, oom_handler (ZST, 0B) no fim.
    // read_volatile = o compilador não reordena/otimiza a leitura.
    unsafe {
        let base = talc as *const _ as *const u64;
        let avail_low = base.read_volatile();
        let avail_high = base.add(1).read_volatile();
        let bins = base.add(2).read_volatile();
        TALC_NULL_AVAIL_LOW.store(avail_low, Ordering::Relaxed);
        TALC_NULL_AVAIL_HIGH.store(avail_high, Ordering::Relaxed);
        if bins == 0 {
            TALC_NULL_BINS_EMPTY.fetch_add(1, Ordering::Relaxed);
        } else {
            TALC_NULL_BINS_NONEMPTY.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn talc_null_avail_snapshot() -> (u64, u64) {
    (
        TALC_NULL_AVAIL_LOW.load(Ordering::Relaxed),
        TALC_NULL_AVAIL_HIGH.load(Ordering::Relaxed),
    )
}

// ─── s435: Telemetria de uso REAL do TALC (idea #630 residual) ─────────────
// talc 4.4 não expõe free-bytes — até aqui o span INTEIRO contava como
// headroom (estimativa generosa) e o HUB não via fragmentação. Caminho
// honesto: percorrer os gap-nodes dos bins (memória LIVRE registrada) e
// derivar used = span − free. Layout confirmado no fonte talc-4.4.3:
//   bins: *mut Bin @16 — array de 128 sentinelas Option<NonNull<LlistNode>>;
//   gap-node: next @0 (NULL = fim — register_gap insere com next=old head),
//   size @16 (GAP_LOW_SIZE_OFFSET = NODE_SIZE = 2 ptr); MIN_CHUNK_SIZE=24B.
// Custo O(128 + gaps), sem alloc. Chamado a 2 Hz (refresh_hub_health) sob o
// lock do Talck — NUNCA no caminho de alloc (a exceção é o seed pós-claim,
// boot single-core). read_volatile: escritor é o talc sob o mesmo lock.

/// Uso real do span TALC (bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TalcUsage {
    /// Bytes LIVRES registrados nos bins (soma dos gap-nodes).
    pub free_bytes: u64,
    /// Bytes ocupados = span − free (inclui overhead de chunk/tag do talc).
    pub used_bytes: u64,
    /// Maior gap contíguo (o que um alloc grande consegue served de fato).
    pub largest_free: u64,
    /// Número de gaps (fragmentos livres) no span.
    pub gaps: u64,
    /// 0 = walk completo; 1 = abortado (node fora do span / CAP) — dados
    /// parciais: leitura de lixo ou corrupção de metadados. NUNCA mintir.
    pub partial: u8,
    /// s443: HISTOGRAMA de tamanhos de gap (contagem por faixa). Totais
    /// dizem QUE fragmentou; o histograma diz DE QUE JEITO. É o dado que
    /// separa "poeira de 4KB" (milhares de gaps inúteis) de "v Few gaps
    /// grandes" (que quase nãofragmentam de verdade) — e, portanto, qual
    /// ação anti-fragmentação faz sentido. Zero alloc: só incrementos.
    pub hist: [u32; TALC_HIST_BUCKETS],
}

/// Faixas do histograma (limites superiores EXCLUSIVOS, em bytes).
/// Escolhidas pelos tamanhos reais dos consumers roteados: página de KV =
/// 4096B, chunk de arena/expert = KiB..MiB, aloc de 2MB (gate de spill).
pub const TALC_HIST_BOUNDS: [u64; TALC_HIST_BUCKETS - 1] = [
    8 * 1024,      // < 8KB  (poeira: header/tag, página de KV)
    64 * 1024,     // 8..64KB
    256 * 1024,    // 64..256KB
    1024 * 1024,   // 256KB..1MB
    16 * 1024 * 1024, // 1..16MB
];
pub const TALC_HIST_BUCKETS: usize = 6; // 5 faixas + ">= 16MB"

/// Classifica um gap no histograma (puro, testável).
#[inline]
pub fn talc_hist_bucket(size: u64) -> usize {
    let mut i = 0;
    while i < TALC_HIST_BOUNDS.len() {
        if size < TALC_HIST_BOUNDS[i] {
            return i;
        }
        i += 1;
    }
    TALC_HIST_BUCKETS - 1
}

static TALC_USAGE: Mutex<TalcUsage> = Mutex::new(TalcUsage {
    hist: [0; TALC_HIST_BUCKETS],
    free_bytes: 0,
    used_bytes: 0,
    largest_free: 0,
    gaps: 0,
    partial: 0,
});
static TALC_USAGE_SAMPLES: AtomicU64 = AtomicU64::new(0);

/// CAP do walk: um span são tem dezenas de gaps; milhares = lista corrompida
/// (node lixo encadeado em loop) — aborta com partial=1 em vez de pender.
const TALC_WALK_MAX_GAPS: u64 = 4096;

/// s439 (fuzz host): footprint MÍNIMO de um gap-node = LlistNode (2 ptr = 16B)
/// + SIZE (8B) = MIN_CHUNK_SIZE do talc 4.4.3. O walk só pode ler `size` se o
/// header inteiro couber no span: `node+24 > acme` = `next` corrompido apontando
/// pro fim — ler o size sairia do claim (página não mapeada → #PF). Também exige
/// alinhamento de palavra: `next` lixo não pode gerar `read_volatile` desalinhado
/// (UB por contrato de `*const usize`).
const TALC_GAP_MIN_FOOTPRINT: usize = 24;

/// Percorre os bins do talc somando os gap-nodes. `talc` deve estar sob
/// lock (chamador). Zero alloc, nunca panica.
fn talc_walk_bins(talc: &Talc<ErrOnOom>, span: Span) -> TalcUsage {
    // Declarado antes dos returns de "sem span": o histograma é zerado nos
    // caminhos parciais (dado parcial não inventa distribuição).
    let mut hist = [0u32; TALC_HIST_BUCKETS];
    let (base, acme) = match span.get_base_acme() {
        Some((b, a)) => (b as usize, a as usize),
        None => {
            return TalcUsage { free_bytes: 0, used_bytes: span.size() as u64, largest_free: 0, gaps: 0, partial: 1, hist }
        }
    };
    let mut free = 0u64;
    let mut largest = 0u64;
    let mut gaps = 0u64;
    unsafe {
        // Layout de Talc<O> (sem feature counters): avail_low @0, avail_high
        // @8, bins @16 (mesmo padrão do snapshot_talc_bins).
        let bins = (talc as *const _ as *const u64).add(2).read_volatile() as *const usize;
        if bins.is_null() {
            // Nunca claimado: span vazio.
            return TalcUsage { free_bytes: 0, used_bytes: span.size() as u64, largest_free: 0, gaps: 0, partial: 1, hist };
        }
        'bins: for b in 0..128usize {
            let mut node = bins.add(b).read_volatile() as usize;
            while node != 0 {
                gaps += 1;
                if gaps > TALC_WALK_MAX_GAPS {
                    return TalcUsage { free_bytes: free, used_bytes: 0, largest_free: largest, gaps, partial: 1, hist };
                }
                if node < base
                    || node % core::mem::align_of::<usize>() != 0
                    || node.saturating_add(TALC_GAP_MIN_FOOTPRINT) > acme
                {
                    return TalcUsage { free_bytes: free, used_bytes: 0, largest_free: largest, gaps, partial: 1, hist };
                }
                let size = ((node + 16) as *const usize).read_volatile();
                if size == 0 || node.saturating_add(size) > acme {
                    return TalcUsage { free_bytes: free, used_bytes: 0, largest_free: largest, gaps, partial: 1, hist };
                }
                // free NUNCA dá wrap: com `size <= span` e `gaps <= CAP`, o teto real
                // é CAP·span (~28TB no claim de 6.9GB) — o wrap de u64 exigiria
                // um span de 4.5PB, fora do budget do kernel. Mantido
                // saturating como cinto-e-suspensório: se algum dia o bound
                // acima afrouxar, o número publicado continua sendo o maior
                // possível em vez de mentir um valor pequeno. (O fuzz M4 mostra
                // que hoje o += simples passaria — este guard não é coberto
                // por teste porque é inalcançável, não por ser inútil.)
                free = free.saturating_add(size as u64);
                if size as u64 > largest {
                    largest = size as u64;
                }
                // Contador de histograma: 32 bits bastam (CAP de gaps = 4096)
                // e não pode estourar sem que o dado vire lixo — saturado.
                let b = talc_hist_bucket(size as u64);
                hist[b] = hist[b].saturating_add(1);
                node = (node as *const usize).read_volatile();
            }
        }
    }
    let span_bytes = span.size() as u64;
    TalcUsage {
        free_bytes: free,
        used_bytes: span_bytes.saturating_sub(free),
        largest_free: largest,
        gaps,
        partial: 0,
        hist,
    }
}

/// Re-amostra o uso real do TALC (chamador de baixa cadência: HUD 2 Hz,
/// hub_triage 1/min). Sem claim → devolve o cache (zeros no boot).
pub fn talc_refresh_usage() -> TalcUsage {
    let span = match *CLAIMED_HEAP.lock() {
        Some(s) => s,
        None => return *TALC_USAGE.lock(),
    };
    let usage = talc_walk_bins(&TALC_ALLOC.lock(), span);
    *TALC_USAGE.lock() = usage;
    TALC_USAGE_SAMPLES.fetch_add(1, Ordering::Relaxed);
    usage
}

/// Última amostra em cache (barata, lock-free de fato — Mutex só do snapshot).
pub fn talc_usage() -> TalcUsage {
    *TALC_USAGE.lock()
}

/// Nº de amostras desde o boot (0 = nunca medido — honestidade n/a ≠ 0).
pub fn talc_usage_samples() -> u64 {
    TALC_USAGE_SAMPLES.load(Ordering::Relaxed)
}

/// SESSION_416 diagnóstico: quantas vezes bump recusou E o TALC overflow
/// também devolveu null (o caller recebe null → deref → #PF). Se crescer
/// com heap crítico, o span TALC não tem espaço/páginas — investigar.
static TALC_OVERFLOW_NULL: AtomicU64 = AtomicU64::new(0);

pub fn talc_overflow_null_count() -> u64 {
    TALC_OVERFLOW_NULL.load(Ordering::Relaxed)
}

const TALC_RANGE_START: usize = HEAP_START;
/// s430b: range dinâmico — o claim do TALC pode cobrir o budget completo
/// (não só o HEAP_SIZE estático de 512MB). Lido de TALC_SPAN_END.
static TALC_SPAN_END: AtomicUsize = AtomicUsize::new(HEAP_START + HEAP_SIZE);
const TALC_RANGE_END: usize = HEAP_START + HEAP_SIZE; // fallback estático (const fn ptr_in_talc usa TALC_SPAN_END)

impl HybridAllocator {
    #[inline]
    fn ptr_in_talc(ptr: *mut u8) -> bool {
        let p = ptr as usize;
        p >= TALC_RANGE_START && p < TALC_SPAN_END.load(Ordering::Acquire)
    }
}

unsafe impl GlobalAlloc for HybridAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Bump PRIMEIRO (comportamento de boot intacto — TALC-first stallou o
        // boot no SMP bring-up: claim demanda páginas do span antes do IDT/
        // demand-page prontos). TALC é OVERFLOW: só entra quando o bump recusa
        // (janela esgotada) e o claim já existe — aí dealloc do TALC é real.
        let p = LazyBumpAllocator::alloc(&BUMP_ALLOC, layout);
        if !p.is_null() {
            return p;
        }
        if TALC_READY.load(Ordering::Acquire) {
            let q = TALC_ALLOC.alloc(layout);
            if !q.is_null() {
                return q;
            }
            let n = TALC_OVERFLOW_NULL.fetch_add(1, Ordering::Relaxed);
            // #630: snapshot one-shot dos bins no 1º NULL — distingue
            // fragmentação (bins não-vazios) de span nunca materializado
            // (vazio: demand-page falhou ou claim sem gaps). Zero-alloc.
            if n == 0 {
                snapshot_talc_bins(&TALC_ALLOC.lock());
                TALC_NULL_REQ_SIZE.store(layout.size(), Ordering::Relaxed);
                TALC_NULL_REQ_CHUNK.store(
                    layout.size() + 3 * core::mem::size_of::<usize>(),
                    Ordering::Relaxed,
                );
            }
            // 3 primeiras ocorrências: serial direto, zero-alloc (path de OOM).
            if n < 3 {
                let mut buf = [0u8; 96];
                let mut n2 = 0usize;
                for &b in b"ALLOC null bump+TALC size=" { if n2 < buf.len() { buf[n2] = b; n2 += 1; } }
                let mut v = layout.size();
                let mut digits = [0u8; 20];
                let mut d = 0usize;
                if v == 0 { digits[0] = b'0'; d = 1; }
                while v > 0 && d < 20 { digits[d] = b'0' + (v % 10) as u8; v /= 10; d += 1; }
                while d > 0 { d -= 1; if n2 < buf.len() { buf[n2] = digits[d]; n2 += 1; } }
                for &b in b" agente=" { if n2 < buf.len() { buf[n2] = b; n2 += 1; } }
                let agent = agent_core::oom_agent_label();
                for &b in agent.as_bytes() { if n2 < buf.len() { buf[n2] = b; n2 += 1; } }
                crate::interrupts::exception_fb_stamp(&buf[..n2]);
            }
        }
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if Self::ptr_in_talc(ptr) {
            TALC_ALLOC.dealloc(ptr, layout);
            return;
        }
        // Bump: no-op (sempre foi) — ponteiros de boot vivem e morrem.
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if Self::ptr_in_talc(ptr) && TALC_READY.load(Ordering::Acquire) {
            let q = TALC_ALLOC.realloc(ptr, layout, new_size);
            if !q.is_null() {
                return q;
            }
            // s434b (#630): o realloc do Talck chama o malloc INTERNO (não o
            // nosso GlobalAlloc::alloc) — um NULL aqui chegava ao
            // alloc_error_handler SEM counter/snapshot (os 3 OOM do log
            // 140959 com nulls=0: era realloc de chunk já-residente no TALC
            // crescendo com o bump cheio). Caminho: grow_in_place falha →
            // malloc falha (fragmentação no espaço do talc) → null direto.
            // FAIL-CLOSED CORRETO: o dado continua válido no ponteiro velho;
            // propagar null = UB no caller (realloc embutido), então
            // oom() de verdade com diag completo.
            snapshot_talc_bins(&TALC_ALLOC.lock());
            TALC_NULL_REQ_SIZE.store(new_size, Ordering::Relaxed);
            TALC_NULL_REQ_CHUNK.store(
                new_size + 3 * core::mem::size_of::<usize>(),
                Ordering::Relaxed,
            );
            TALC_OVERFLOW_NULL.fetch_add(1, Ordering::Relaxed);
            oom(Layout::from_size_align_unchecked(new_size, layout.align()))
        }
        // s434c (#630): chunk bump-residente crescendo — o default realloc
        // (alloc+copy+dealloc) do BUMP retornava NULL SEM tocar o TALC: o
        // alloc novo era do bump puro (sem overflow!) e morria com a janela
        // cheia mesmo com talc de 6911MB (os OOM com nulls=0 persistiam no
        // log 142925: era ESTE path). Cai no híbrido: TALC dá o espaço novo
        // (bump cheio), copy manual; o ponteiro velho segue no bump (sem
        // free — sempre foi assim) e a memória nova tem free real.
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let q = GlobalAlloc::alloc(self, new_layout);
        if q.is_null() {
            // OOM real do híbrido (talc também recusou) — diag completo.
            snapshot_talc_bins(&TALC_ALLOC.lock());
            TALC_NULL_REQ_SIZE.store(new_size, Ordering::Relaxed);
            TALC_NULL_REQ_CHUNK.store(
                new_size + 3 * core::mem::size_of::<usize>(),
                Ordering::Relaxed,
            );
            TALC_OVERFLOW_NULL.fetch_add(1, Ordering::Relaxed);
            oom(new_layout);
        }
        // s437: copia o MENOR tamanho — `realloc` pode ENCOLHER
        // (new_size < layout.size()); copiar o tamanho antigo transborda o
        // buffer novo (heap stray write — candidato ao Arc corrompido 0x6).
        core::ptr::copy_nonoverlapping(ptr, q, layout.size().min(new_size));
        q
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = LazyBumpAllocator::alloc_zeroed(&BUMP_ALLOC, layout);
        if !p.is_null() {
            return p;
        }
        if TALC_READY.load(Ordering::Acquire) {
            let q = TALC_ALLOC.alloc_zeroed(layout);
            if !q.is_null() {
                return q;
            }
            // s434b: mesmo gap do realloc — o alloc_zeroed do Talck cai no
            // malloc interno; NULL aqui também não passava pelo counter.
            snapshot_talc_bins(&TALC_ALLOC.lock());
            TALC_NULL_REQ_SIZE.store(layout.size(), Ordering::Relaxed);
            TALC_NULL_REQ_CHUNK.store(
                layout.size() + 3 * core::mem::size_of::<usize>(),
                Ordering::Relaxed,
            );
            TALC_OVERFLOW_NULL.fetch_add(1, Ordering::Relaxed);
        }
        core::ptr::null_mut()
    }
}

/// Instância bump usada como fallback/boot do híbrido.
static BUMP_ALLOC: LazyBumpAllocator = LazyBumpAllocator::new();

// OPCODE-0079/0093: sob Miri/Kani o alocador global do kernel (ponteiros
// crus/volatile) é incompatível — usa o alocador padrão do host nos testes.
#[cfg(not(any(miri, kani)))]
#[global_allocator]
static GLOBAL_ALLOC: HybridAllocator = HybridAllocator;

/// Returns the actual heap usage in bytes from the LazyBumpAllocator.
pub fn heap_used_bytes() -> usize {
    let offset = BUMP_ALLOC.offset.load(Ordering::Relaxed);
    if offset < 0 { 0 } else { offset as usize }
}

/// Janela endereçável do bump (~2GB) — Observe AIOS.
pub fn heap_window_bytes() -> usize {
    bump_max_offset()
}

/// s432: capacidade do span TALC claimed (MB; 0 = não pronto) — telemetria
/// honesta do overflow (HUD/hub_triage leem sem re-derivar do span).
pub fn talc_capacity_mb() -> u64 {
    if !TALC_READY.load(Ordering::Acquire) {
        return 0;
    }
    (TALC_SPAN_END.load(Ordering::Acquire)
        .saturating_sub(LARGE_HEAP_START)
        .saturating_sub(SLAB_SIZE)
        / (1024 * 1024)) as u64
}

/// Headroom real: window − used do bump + TALC LIVRE de verdade (s435).
/// O TALC claim cobre o budget em VA própria (demand-paged) — overflow do
/// bump cai lá com free real. s435: a capacidade INTEIRA contava como
/// livre (estimativa generosa); agora o headroom do TALC = free medido nos
/// gap-nodes (talc_refresh_usage a 2 Hz no HUD). Snapshot via cache — o
/// walk real nunca roda no caminho de alloc.
pub fn heap_headroom_bytes() -> usize {
    let bump = bump_max_offset().saturating_sub(heap_used_bytes());
    let talc_cap = if TALC_READY.load(Ordering::Acquire) {
        (talc_usage().free_bytes as usize).min(talc_capacity_bytes())
    } else {
        0
    };
    bump.saturating_add(talc_cap)
}

/// Capacidade do span TALC em bytes (0 = não pronto).
pub fn talc_capacity_bytes() -> usize {
    if !TALC_READY.load(Ordering::Acquire) {
        return 0;
    }
    TALC_SPAN_END.load(Ordering::Acquire)
        .saturating_sub(LARGE_HEAP_START)
        .saturating_sub(SLAB_SIZE)
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
    /// s435: uso REAL do TALC (0s = nunca medido — honestidade n/a ≠ 0).
    pub talc_used_mb: usize,
    pub talc_free_mb: usize,
    pub talc_largest_mb: usize,
    pub talc_gaps: u64,
    /// 1 = última amostra parcial (walk abortado — metadados ilegíveis).
    pub talc_partial: u8,
    /// s443: contagem de gaps por faixa de tamanho (`TALC_HIST_BOUNDS`) — o
    /// dado que distingue "poeira" de "fragmentos que importam".
    pub talc_hist: [u32; TALC_HIST_BUCKETS],
}

pub fn heap_observe() -> HeapObserve {
    let used = heap_used_bytes();
    let window = bump_max_offset();
    // s430b→s435: headroom do bump é saturado na janela ~2030MB. O TALC
    // (claim do budget completo, demand-paged) é memória real além disso —
    // s435: o headroom do TALC é o FREE medido nos gap-nodes (cache 2 Hz),
    // não o span inteiro (estimativa generosa aposentada). Gates de 64/128MB
    // continuam válidos como piso do bump; OOM/TALC real segue fail-closed.
    let headroom_bump = window.saturating_sub(used);
    let talc_free = (talc_usage().free_bytes as usize).min(talc_capacity_bytes());
    let headroom = headroom_bump.saturating_add(talc_free);
    // Warn proativo: headroom COMBINADO (bump + free TALC MEDIDO) < 256MB.
    // s437: a guarda `talc_cap == 0` (s431) morreu quando o TALC passou a
    // segurar o budget completo — com o TALC sempre claimado ela nunca mais
    // disparava, e `talc_cap` (claim) não reflete uso. s435 já mede o free
    // dos gap-nodes, então a condição honesta é o mesmo headroom que
    // `heap_headroom_bytes()` usa: bump saturado + TALC esgotado = warn.
    let mut pressure = HEAP_PRESSURE_LEVEL.load(Ordering::Acquire) as u8;
    if pressure < 1 && headroom < 256 * 1024 * 1024 {
        pressure = 1;
    }
    // s435: telemetria real do TALC — cache (2 Hz no HUD), nunca walk aqui.
    let tu = talc_usage();
    HeapObserve {
        used_mb: used / (1024 * 1024),
        window_mb: window / (1024 * 1024),
        headroom_mb: headroom / (1024 * 1024),
        pressure,
        last_refuse_need_mb: LAST_REFUSE_NEED.load(Ordering::Relaxed) / (1024 * 1024),
        refuse_count: REFUSE_COUNT.load(Ordering::Relaxed),
        seq: PRESSURE_SEQ.load(Ordering::Relaxed),
        talc_used_mb: (tu.used_bytes / (1024 * 1024)) as usize,
        talc_free_mb: (tu.free_bytes / (1024 * 1024)) as usize,
        talc_largest_mb: (tu.largest_free / (1024 * 1024)) as usize,
        talc_gaps: tu.gaps,
        talc_partial: tu.partial,
        talc_hist: tu.hist,
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

/// Headroom crítico (SESSION_415): abaixo disso, novos jobs de inferência são
/// RECUSADOS na entrada (fail-closed) — melhor recusar um claim do que OOM
/// no meio do prefill (alloc NULL → caller deref → #PF → AP hlt com stamp).
pub const HEAP_CRITICAL_HEADROOM_MB: usize = 64;

/// True se o headroom do bump está abaixo do piso crítico.
pub fn heap_headroom_critical() -> bool {
    heap_headroom_bytes() < HEAP_CRITICAL_HEADROOM_MB * 1024 * 1024
}

/// SESSION_420 (residual s419): piso PROATIVO do prefill — ACIMA do crítico
/// (64MB), dá 1 slice de margem. Os gates críticos só disparam ENTRE slices
/// (topo do poll_slice); o auto-grow acontece DENTRO do slice (KV/mask/logits
/// em apply_one_layer) e cruza o teto (~2030MB) antes do piso de 64MB ser
/// re-checado → alloc NULL no meio do slice → #PF → hlt no AP. Checar o piso
/// de 128MB no INÍCIO de cada slice pesado termina o job honesto (HITL) antes
/// do heap esgotar — recusa custa um job, OOM custa um core.
pub const HEAP_PREFILL_HEADROOM_MB: usize = 128;

/// True se o headroom do bump está abaixo do piso proativo de prefill.
pub fn heap_headroom_low() -> bool {
    heap_headroom_bytes() < HEAP_PREFILL_HEADROOM_MB * 1024 * 1024
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
/// s430b: teto VA do span TALC — antes da arena Cortex (0x4800_0000_0000),
/// deixando margem de 0x800_0000_0000 (32GB) para growth futuro sem overlap.
pub const TALC_VA_MAX: usize = 0x4780_0000_0000;
pub static CURRENT_HEAP_MB: AtomicUsize = AtomicUsize::new(512);

/// Budget máximo do heap em MB. grow_bump_auto para ao atingir este limite.
/// Definido em main.rs baseado na RAM detectada (min(75% RAM, 1536MB)).
/// s430b: valor REAL (RAM-based); o clamp da janela do bump vive em
/// BUMP_BUDGET_CLAMPED (grow_bump_auto lê o clampado, TALC claim lê o real).
pub static HEAP_BUDGET_MB: AtomicUsize = AtomicUsize::new(1536);
static BUMP_BUDGET_CLAMPED: AtomicUsize = AtomicUsize::new(1536);

/// Define o budget máximo do heap (chamado de main.rs no boot).
/// Fix A (SESSION_339): o BUMP é clampado à janela endereçável — budget em
/// MB-de-RAM não pode exceder o offset máximo antes do wrap 2^64.
/// s430b: o BUMP guarda o clamp da janela, mas HEAP_BUDGET_MB guarda o valor
/// REAL (o TALC clama o budget completo em VA própria 0x4000_0000_0000+,
/// FORA da janela do bump — demand-paged, custo zero até tocar).
pub fn set_heap_budget_mb(mb: usize) {
    let window_mb = bump_max_offset() / (1024 * 1024);
    let bump_budget = mb.min(window_mb);
    HEAP_BUDGET_MB.store(mb, Ordering::Release);
    // grow_bump_auto lê HEAP_BUDGET_MB — dá a ele o valor clampado à janela
    // (o bump não pode passar da janela), sem reescrever o budget real.
    BUMP_BUDGET_CLAMPED.store(bump_budget, Ordering::Release);
    crate::slog_nano!("HEAP", "BUDGET", "budget={}MB (bump={}MB window=~{}MB talc-claim={}MB)",
        mb, bump_budget, window_mb, mb.min(TALC_VA_MAX / (1024 * 1024)));
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

// ─── s439: rota TALC explícita p/ consumers pesados de longa vida ───────────
// Bump nunca libera: consumer longa-vida nele (ex.: KV cache da LLM, ~604MB
// em páginas de 4KB para Falcon3-1B ctx 4096 — SESSION_413: KV > modelo) é
// consumo PERMANENTE da janela ~2030MB = a classe OOM s431/s434. A rota
// explícita coloca esses chunks DIRETO no TALC, que tem dealloc real (o walk
// de bins s435 passa a medir a fragmentação REAL desses consumers).
pub static TALC_ROUTED_BYTES: AtomicU64 = AtomicU64::new(0);
pub static TALC_ROUTED_COUNT: AtomicU64 = AtomicU64::new(0);
pub static TALC_ROUTED_REFUSED: AtomicU64 = AtomicU64::new(0);

fn talc_routed_ptr_in_span(ptr: *mut u8) -> bool {
    let p = ptr as usize;
    p >= TALC_RANGE_START && p < TALC_SPAN_END.load(Ordering::Relaxed)
}

/// Alloc DIRETO no TALC (s439) — para consumers pesados de longa vida (KV
/// cache, context window da LLM). TALC pronto → chunk no TALC (dealloc real;
/// NULL honesto se cheio — caller faz fail-closed, sem oom()); TALC ainda não
/// pronto (boot cedo / host tests) → fallback no híbrido global (bump-first,
/// exatamente o que `Box::new` fazia — mesmo comportamento, mesma rota de
/// dealloc). NUNCA chamar de dentro de uma alloc em curso (o Talck lock não
/// é reentrante) nem de IRQ.
pub fn alloc_talc_routed(layout: core::alloc::Layout) -> *mut u8 {
    if TALC_READY.load(Ordering::Acquire) {
        // SAFETY: Talck GlobalAlloc — layout válido do caller; lock interno.
        let p = unsafe { TALC_ALLOC.alloc(layout) };
        if !p.is_null() {
            let n = TALC_ROUTED_COUNT.fetch_add(1, Ordering::Relaxed);
            TALC_ROUTED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            if n == 0 {
                // regra 419: evidência de wire em runtime ANTES de "done".
                crate::slog_nano!("HEAP", "TALC",
                    "1a rota TALC explícita (KV longa-vida) size={} — fora do bump",
                    layout.size());
            }
            return p;
        }
        TALC_ROUTED_REFUSED.fetch_add(1, Ordering::Relaxed);
        return core::ptr::null_mut();
    }
    unsafe { GlobalAlloc::alloc(&HybridAllocator, layout) }
}

/// Dealloc da rota TALC explícita. O par do `alloc_talc_routed`: chunk no
/// span → `TALC_ALLOC.dealloc` (free REAL — o que o bump nunca teve);
/// chunk bump-residente (fallback pré-claim) → no-op, igual ao híbrido.
/// # Safety: `ptr` DEVE ter vindo de `alloc_talc_routed` com o MESMO layout.
pub unsafe fn dealloc_talc_routed(ptr: *mut u8, layout: core::alloc::Layout) {
    if ptr.is_null() {
        return;
    }
    if talc_routed_ptr_in_span(ptr) && TALC_READY.load(Ordering::Acquire) {
        TALC_ALLOC.dealloc(ptr, layout);
        TALC_ROUTED_BYTES.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        return;
    }
    unsafe { GlobalAlloc::dealloc(&HybridAllocator, ptr, layout) }
}

/// Bytes vivos alocados pela rota TALC explícita (KV longa-vida etc.).
pub fn talc_routed_bytes() -> u64 {
    TALC_ROUTED_BYTES.load(Ordering::Relaxed)
}

/// s443: buffer POSSUÍDO na rota TALC — `String`/`Vec` não servem aqui.
///
/// O bump allocator **nunca devolve** memória (dealloc = no-op): todo
/// `String` que morre no bump é memória permanentemente perdida. Isso torna o
/// churn normal de um consumer de longa vida (uma `ContextWindow` que compacta e
/// remove mensagens a cada conversa) um **vazamento monotônico** — cada mensagem
/// descartada deixa para trás seus bytes para sempre. É por isso que rotear
/// esse consumer não é otimização: é o conserto do vazamento.
///
/// Contrato:
/// - falha de alocação = `None`/`false` (**fail-closed**, nunca `oom()`);
/// - `Drop` devolve o chunk ao TALC (free REAL);
/// - nunca chamar de dentro de outra alloc em curso (o `Talck` não é
///   reentrante) nem de IRQ — igual a `alloc_talc_routed`.
pub struct TalcBuf {
    ptr: *mut u8,
    len: usize,
    cap: usize,
}

impl TalcBuf {
    pub const fn new() -> Self {
        TalcBuf { ptr: core::ptr::null_mut(), len: 0, cap: 0 }
    }

    /// Capacidade mínima (bytes). `None` = o TALC recusou (fail-closed).
    pub fn with_capacity(n: usize) -> Option<Self> {
        if n == 0 {
            return Some(Self::new());
        }
        let layout = core::alloc::Layout::from_size_align(n, 1).ok()?;
        // SAFETY: layout válido; a rota devolve NULL se recusar.
        let p = unsafe { alloc_talc_routed(layout) };
        if p.is_null() {
            return None;
        }
        Some(TalcBuf { ptr: p, len: 0, cap: n })
    }

    /// Cópia de um `&str`. `None` = sem espaço (o caller decide o fallback —
    /// nunca finge que gravou).
    pub fn from_str(s: &str) -> Option<Self> {
        let mut b = Self::with_capacity(s.len())?;
        // SAFETY: `cap >= s.len()` por construção; o TALC devolve memória não
        // inicializada, então a escrita abaixo é a primeira.
        unsafe { core::ptr::copy_nonoverlapping(s.as_ptr(), b.ptr, s.len()) };
        b.len = s.len();
        Some(b)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Bytes vivos no TALC por este buffer (0 = vazio/sem chunk).
    pub fn bytes(&self) -> usize {
        self.cap
    }

    pub fn as_str(&self) -> &str {
        if self.len == 0 {
            return "";
        }
        // SAFETY: invariante `len <= cap` e os `len` primeiros bytes foram
        // escritos por `push_str`/`from_str` (ASCII/UTF-8 preservado byte a byte).
        unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.ptr, self.len)) }
    }

    /// Concatena. `false` = não coube (o conteúdo fica **intacto**, fail-closed).
    pub fn push_str(&mut self, s: &str) -> bool {
        if s.is_empty() {
            return true;
        }
        let need = self.len + s.len();
        if need > self.cap {
            if !self.grow(need) {
                return false;
            }
        }
        // SAFETY: garante `len + s.len() <= cap` acima.
        unsafe {
            core::ptr::copy_nonoverlapping(s.as_ptr(), self.ptr.add(self.len), s.len());
        }
        self.len = need;
        true
    }

    /// Libera a capacidade de sobra (`Vec::shrink_to_fit`): um buffer que caiu
    /// de 1MB para 2KB segura 1MB hostage no TALC. Cópia + free = coalesce real.
    pub fn shrink_to_fit(&mut self) -> bool {
        if self.cap == 0 || self.cap == self.len {
            return true;
        }
        let Ok(layout) = core::alloc::Layout::from_size_align(self.len.max(1), 1) else {
            return false;
        };
        // SAFETY: `len <= cap`; o novo chunk tem `len` bytes.
        let np = unsafe { alloc_talc_routed(layout) };
        if np.is_null() {
            return false; // sem espaço p/ mover: fica como está (honesto)
        }
        unsafe {
            core::ptr::copy_nonoverlapping(self.ptr, np, self.len);
            if !self.ptr.is_null() {
                dealloc_talc_routed(self.ptr, core::alloc::Layout::from_size_align_unchecked(self.cap, 1));
            }
        }
        self.ptr = np;
        self.cap = self.len;
        true
    }

    fn grow(&mut self, need: usize) -> bool {
        // Dobra a capacidade (como `Vec`), mas nunca menos que o necessário.
        let mut new_cap = if self.cap == 0 { 64 } else { self.cap * 2 };
        if new_cap < need {
            new_cap = need;
        }
        // NAO usa `core::alloc::realloc` de propósito (lição s434b): para um
        // chunk TALC-resident cujo realloc falha, `HybridAllocator::realloc`
        // chama `oom()` — um HALT do kernel. Aqui grow é fail-closed por
        // contrato (conteúdo intacto, `false`); então é alloc+copy+free, que
        // devolve NULL honesto em vez de derrubar o sistema. Custo: uma
        // alocação transitória no grow (barato: crescimento é raro e dobrado).
        let Some(mut nb) = TalcBuf::with_capacity(new_cap) else {
            return false;
        };
        if self.len > 0 {
            // SAFETY: `len <= cap` do buffer velho; destino tem `new_cap >= len`.
            unsafe {
                core::ptr::copy_nonoverlapping(self.ptr, nb.ptr, self.len);
            }
            nb.len = self.len;
        }
        // O drop do buffer velho devolve o chunk antigo (free real no TALC).
        drop(core::mem::replace(self, nb));
        true
    }
}

impl Default for TalcBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TalcBuf {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: par exato do `alloc_talc_routed` que produziu `ptr`.
            unsafe {
                dealloc_talc_routed(
                    self.ptr,
                    core::alloc::Layout::from_size_align_unchecked(self.cap, 1),
                )
            };
            self.ptr = core::ptr::null_mut();
        }
    }
}

// SAFETY: posse EXCLUSIVA do chunk (um único `Drop` devolve) ⇒ `Send`.
unsafe impl Send for TalcBuf {}
// Sem `Sync`: `&TalcBuf` expõe `as_str()`; escrita compartilhada exigiria
// sincronização que este tipo deliberadamente não tem (como `Vec<u8>` cru).

impl core::fmt::Debug for TalcBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TalcBuf")
            .field("len", &self.len)
            .field("cap", &self.cap)
            .finish()
    }
}

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
/// #630: alloc_pt_frame devolveu 0 dentro do map_page_direct — o demand-page
/// falhou por falta de frame de PAGE TABLE (não de dado); o caller vê
/// heap_pte_present=false → PF_DIAG_MAP_FAIL, mas a causa real era PT.
pub static PF_DIAG_PT_ALLOC_FAIL: AtomicU64 = AtomicU64::new(0);

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
        if f == 0 { PF_DIAG_PT_ALLOC_FAIL.fetch_add(1, Ordering::Relaxed); return; }
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
        if f == 0 { PF_DIAG_PT_ALLOC_FAIL.fetch_add(1, Ordering::Relaxed); return; }
        e2.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l2_virt = base + e2.addr().as_u64();
    let l2_tbl = &mut *(l2_virt.as_mut_ptr::<PageTable>());
    let e1 = &mut l2_tbl[virt.p2_index()];
    if !e1.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 { PF_DIAG_PT_ALLOC_FAIL.fetch_add(1, Ordering::Relaxed); return; }
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
    // s430b: fim DINÂMICO — o claim cobre o budget completo (demand-paged).
    let start = LARGE_HEAP_START as u64;
    let talc_end = TALC_SPAN_END.load(Ordering::Acquire) as u64;
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
        // #630: um fault no range TALC ACIMA do fim demand-pagado (cr2 >=
        // TALC_SPAN_END) = gap-node/metadata lido como endereço de memória
        // não-materializada → o handler não tem como cobrir (o span termina
        // aqui). Registra o cr2 e RECUSA — o #PF seguinte é a evidência.
        if in_talc && cr2 >= talc_end {
            TALC_PF_OUTSIDE_SPAN.store(cr2, Ordering::Relaxed);
        }
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
            // #630: diag zero-alloc no momento da morte — as 5 candidatas
            // legíveis numa linha só (sem isso, morre cego; não sabemos se
            // PMM esgotou, se o span não demand-pagou, ou se é fragmentação).
            let pmm_free = crate::memory::pmm_free_frames();
            let pmm_alloc = crate::memory::pmm_allocated_count();
            let pmm_total = crate::memory::pmm_total_frames();
            let nulls = TALC_OVERFLOW_NULL.load(Ordering::Relaxed);
            let (avail_low, avail_high) = talc_null_avail_snapshot();
            let span_end = TALC_SPAN_END.load(Ordering::Acquire);
            // s434b: diagnóstico CANÔNICO — espaço do talc agora vem do
            // snapshot dos bins (availability ≠ gap bytes, mas 0/0 = span
            // materializado em ZERO chunks ≥ req). pf_out>0 = gap-node
            // ilegível (cr2 fora do span demand-pagado na morte).
            let _ = write!(
                s,
                "[OOM-DIAG] pmm free={} alloc={} total={} | talc nulls={} bins_avail={:#x}/{:#x} req_size={} req_chunk={} span_end={:#x} pf_out={}\n",
                pmm_free, pmm_alloc, pmm_total, nulls, avail_low, avail_high,
                TALC_NULL_REQ_SIZE.load(Ordering::Relaxed),
                TALC_NULL_REQ_CHUNK.load(Ordering::Relaxed),
                span_end,
                TALC_PF_OUTSIDE_SPAN.load(Ordering::Relaxed),
            );
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
    // s434 (#630): FAIL-CLOSED de classe — o handler pode ter sido chamado
    // por DENTRO de um critical section (alloc de dentro de MutexGuard do
    // próprio kernel, ex. InferQueue poll_slice segurando locks). O hlt
    // eterno aqui congela TODOS os outros cores que pedirem o MESMO lock
    // (stall silencioso pós-OOM/TALC: 3ª sessão vendo, log 132152).
    // SPIN com watchdog de serial: a cada 10s emite 1 linha de heartbeat
    // (ordem: quem está vivo continua vivo; quem esperava o lock morre
    // ruidosamente em vez de silenciosamente).
    // Park eterno do BSP segura os locks que o OOM interrompeu (s434/s438).
    // Um warm-reset, com selo, deixa [RECOVER] para o próximo boot.
    // s439 (F1): BSP park = freeze total. BSP → reboot ORDENADO observável
    // (seal + [RECOVER] + warm_reset); AP → heartbeat park abaixo (o BSP que
    // ainda desenha a tela segue vivo).
    if crate::smp::percpu::fault_context_is_bsp() {
        crate::boot_ramlog::reboot_ordered("oom");
    }
    let mut last_beat = crate::tsc::now_us();
    loop {
        core::hint::spin_loop();
        let now = crate::tsc::now_us();
        if now.wrapping_sub(last_beat) >= 10_000_000 {
            last_beat = now;
            {
                let mut s = crate::serial::SERIAL.lock();
                if let Some(ref mut s) = *s {
                    let _ = write!(s, "[OOM-HALT] agente={} size={} tick={} — core parkado no OOM (FAIL-CLOSED s434)\n",
                        agent, layout.size(), crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed));
                }
            }
        }
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
    // s430b (foto heap 2024/2030M 99% + OOM/TALC size=83): o span do TALC era
    // FIXO em 512MB (LARGE_HEAP_SIZE = HEAP_SIZE - SLAB) — quando o bump sem
    // free satura a janela ~2030MB, o overflow cai no TALC de 512MB e estoura
    // com RAM física 70% livre. Cura estrutural: clamar o BUDGET COMPLETO.
    // Custo zero até tocar: a demanda-página (try_fault_in_heap range TALC)
    // mapeia fresh frames sob demanda — claim só escreve metadados (~2 páginas).
    // Heap em VA própria (0x4000_0000_0000..) fora da janela wrap do bump
    // (SESSION_339) — até ~8GB endereçáveis antes do overlap com a arena
    // Cortex (0x4800_0000_0000).
    let budget = HEAP_BUDGET_MB.load(Ordering::Relaxed)
        .saturating_mul(1024 * 1024)
        .max(HEAP_SIZE)
        .min(TALC_VA_MAX);
    let span = Span::from_base_size(LARGE_HEAP_START as *mut u8, budget - SLAB_SIZE);
    // s430b: TALC_SPAN_END ANTES do claim — o claim escreve size-tags no FIM do
    // span (páginas ainda não mapeadas); o demand-page (try_fault_in_heap) só
    // cobre cr2 < TALC_SPAN_END. Store depois do claim = a última página cai
    // fora do range -> #PF storm no próprio claim (evidência boot 210748:
    // cr2=0x4001affffff8 = fim do span de 6904MB, no_rng=1, "Tier 1 ready"
    // ausente do log).
    TALC_SPAN_END.store(LARGE_HEAP_START + (budget - SLAB_SIZE), Ordering::Release);
    // s434 (#630): STOP-THE-WORLD durante o claim — o talc 4.4 escreve os gap
    // nodes + size tags do span INTEIRO em memória demand-pagada; um core
    // concurrente que tente demand-pagear o MESMO endereço no meio (ou o
    // walk de PT do mapeamento) pode ver estado inconsistente → a leitura
    // subseqüente do gap node devolve lixo como tamanho de chunk (cr2 fora
    // do span demand-pagado = PF_OUTSIDE na morte). SPIN bounded (2s): se o
    // timer morrer aqui o slog de warn denuncia (não é um hlt eterno).
    static CLAIMING: AtomicBool = AtomicBool::new(false);
    if CLAIMING.swap(true, Ordering::AcqRel) {
        // Outro core já claimando (boot não deveria chegar aqui duas vezes —
        // mas AP cedo + init race): espera bounded e segue honesto.
        let t0 = crate::tsc::now_us();
        while CLAIMING.load(Ordering::Acquire) {
            core::hint::spin_loop();
            if crate::tsc::now_us().wrapping_sub(t0) > 2_000_000 {
                crate::slog_nano!("HEAP", "warn", "talc claim: SPIN timeout (outro core preso no claim?)");
                break;
            }
        }
    }
    let claimed = unsafe {
        TALC_ALLOC.lock().claim(span).map_err(|_| {
            CLAIMING.store(false, Ordering::Release);
            "talc claim failed"
        })?
    };
    CLAIMING.store(false, Ordering::Release);
    *CLAIMED_HEAP.lock() = Some(claimed);
    // SESSION_415: ativa o TALC como primário do allocator híbrido. A partir
    // daqui dealloc é REAL — o bump (sem free) só cobre allocs de boot.
    TALC_READY.store(true, Ordering::Release);
    // s435: seed da telemetria pós-claim (stop-the-world ainda ativo —
    // exclusivo). Pós-boot, o refresh é do HUD (2 Hz) sob lock do Talck.
    let _seeded = talc_refresh_usage();
    crate::slog_nano!("HEAP", "TALC", "Tier 1 ready (hybrid PRIMARY): virt={:#x} size={} MB (budget claim, demand-paged)",
        LARGE_HEAP_START,
        (budget - SLAB_SIZE) / (1024 * 1024));
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
    fn core_carve_split_math() {        let b = 1536 * 1024 * 1024;
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

    #[test]
    fn huge2m_candidate_and_step_invariants() {
        // HEAP_START 2MB-alinhado; passo 256MB = 128 PDEs exatas (sem sobra).
        assert_eq!(super::HUGE_2MB, 2 * 1024 * 1024);
        assert_eq!(super::FRAMES_PER_2MB, 512);
        assert_eq!(super::HEAP_GROW_STEP % super::HUGE_2MB, 0);
        assert!(super::huge2m_candidate(super::HEAP_START + 512 * 1024 * 1024, super::HEAP_GROW_STEP));
        assert!(!super::huge2m_candidate(super::HEAP_START + 1, super::HEAP_GROW_STEP));
        assert!(!super::huge2m_candidate(super::HEAP_START, 4096));
        assert!(!super::huge2m_candidate(super::HEAP_START, 0));
    }

    #[test]
    fn no_infer_slice_outside_tick() {
        // Fora de tick (boot/test): o gate (a) nunca barra grow de boot.
        assert!(!super::infer_slice_in_progress());
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// s439 — FUZZ HOST de talc_walk_bins contra gap-lists CORROMPIDAS
// ═══════════════════════════════════════════════════════════════════════════
// Por que host e não QEMU: o walk é o ÚNICO lugar que lê metadado do TALC
// sem confiar nele (todo resto do kernel confia). Se um `next` lixo virar um
// ptr inválido, o boot morre com #PF rotativo — exatamente a classe que a
// s438 passou 2 sessões caçando. Aqui a mesma corrupção é reproduzível em
// microssegundos e o walk tem que responder partial=1 (ou os valores
// consistentes) SEM pender e SEM ler fora do span.
//
// Layout (fonte talc-4.4.3, igual ao comentário de talc_walk_bins):
//   Talc<O>: availability_low @0, availability_high @8, bins @16 (ptr)
//   bins:    [Option<NonNull<LlistNode>>; 128] (Option<NonNull> niche = 8B)
//   node:    next @0 (0 = fim), size @16 (footprint mínimo 24B)
//
// Regra do módulo: o scratch é [span | redzone] numa caixa só. O redzone fica
// ATRÁS de `acme` com um size plausível (32) — se o walk ler o `size` de um
// node cujo cabeçalho cruza `acme`, o número muda e o teste acusa. Detecta
// leitura fora do span sem depender de página de guarda (portável no host).
#[cfg(test)]
mod talc_walk_fuzz {
    use super::*;
    use std::time::Instant;

    const MAX_BINS: usize = 128;
    const REDZONE_BYTES: usize = 64;
    /// Teto generoso: o walk tem CAP 4096 gaps (microsegundos). Só estoura se
    /// a lista corrompida conseguiu pender — que é o que o teste prova.
    const WALK_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

    /// Scratch = [span | redzone] numa alocação única, alinhado em palavra.
    struct Scratch {
        buf: Box<[u64]>,
        base: usize,
        acme: usize,
    }

    impl Scratch {
        fn new(span_bytes: usize) -> Self {
            assert!(span_bytes % 8 == 0 && span_bytes >= 4096);
            let words = (span_bytes + REDZONE_BYTES) / 8;
            let mut buf: Box<[u64]> = vec![0u64; words].into_boxed_slice();
            let base = buf.as_mut_ptr() as usize;
            // Redzone = size plausível: leitura além de acme muda o resultado.
            for w in buf[(span_bytes / 8)..].iter_mut() {
                *w = 32;
            }
            Self { buf, base, acme: base + span_bytes }
        }

        fn span(&self) -> Span {
            Span::from_base_size(self.base as *mut u8, self.acme - self.base)
        }

        fn end(&self) -> usize {
            self.base + self.buf.len() * 8
        }

        /// Escreve um `usize` word-aligned dentro da caixa (span OU redzone).
        fn put(&self, addr: usize, val: usize) {
            assert!(addr % 8 == 0, "put exige alinhamento de palavra");
            assert!(addr + 8 <= self.end(), "put fora da caixa");
            unsafe { (addr as *mut usize).write(val) }
        }

        /// Planta um gap-node: next em @0, size em @16 (footprint 24B).
        fn node(&self, addr: usize, next: usize, size: usize) {
            self.put(addr, next);
            self.put(addr + 16, size);
        }
    }

    /// Talc sintético: `new` já traz `bins = null_mut()` (fonte talc-4.4.3), então
    /// só gravamos o ponteiro de bins no offset 16 — o mesmo read_volatile que o
    /// walk faz em produção. Sem Drop no Talc: nada tenta liberar a caixa.
    fn fake_talc(bins: &[usize; MAX_BINS]) -> Talc<ErrOnOom> {
        let mut t = Talc::new(ErrOnOom);
        unsafe {
            let p = &mut t as *mut Talc<ErrOnOom> as *mut usize;
            p.add(2).write(bins.as_ptr() as usize);
        }
        t
    }

    /// Talc NUNCA claimado: `bins` fica null_mut() (ramo que reporta
    /// `used = span` como "tudo ocupado", sem nunca ter medido nada).
    fn fake_talc_unclaimed() -> Talc<ErrOnOom> {
        Talc::new(ErrOnOom)
    }

    /// Roda o walk com budget de tempo e devolve (usage, elapsed).
    fn walk(s: &Scratch, bins: &[usize; MAX_BINS]) -> (TalcUsage, std::time::Duration) {
        let t0 = Instant::now();
        let u = super::talc_walk_bins(&fake_talc(bins), s.span());
        (u, t0.elapsed())
    }

    /// Invariantes que valem para QUALQUER saída (corrompida ou não):
    /// - não pendura (teto de tempo + gaps no CAP);
    /// - `largest_free` nunca excede a soma dos free;
    /// - partial=1 ⇒ used colapsa para 0 (ou = span nos 2 ramos degenerados);
    /// - partial=0 ⇒ used fecha com span − free (dado honesto, não estimativa).
    fn assert_invariants(u: &TalcUsage, span_bytes: usize) {
        assert!(
            u.largest_free <= u.free_bytes,
            "largest {} > free {}",
            u.largest_free,
            u.free_bytes
        );
        assert!(u.gaps <= super::TALC_WALK_MAX_GAPS + 1, "CAP furado: gaps={}", u.gaps);
        assert!(u.partial <= 1, "partial precisa ser 0/1, veio {}", u.partial);
        if u.partial == 1 {
            assert!(
                u.used_bytes == 0 || u.used_bytes == span_bytes as u64,
                "partial não pode reportar uso parcial: used={} span={}",
                u.used_bytes,
                span_bytes
            );
        } else {
            assert_eq!(u.used_bytes, (span_bytes as u64).saturating_sub(u.free_bytes));
        }
    }

    // ── caso são: 2 gaps em cadeia, um deles órfão noutro bin ──────────────
    #[test]
    fn walk_sane_chain_sums_free_exactly() {
        let s = Scratch::new(64 * 1024);
        let n1 = s.base;
        let n2 = n1 + 4096;
        s.node(n1, n2, 4096);
        s.node(n2, 0, 8192);
        let n3 = s.base + 12288; // gap solto no bin 7
        s.node(n3, 0, 2048);
        let mut bins = [0usize; MAX_BINS];
        bins[3] = n1;
        bins[7] = n3;
        let (u, _) = walk(&s, &bins);
        assert_invariants(&u, s.acme - s.base);
        assert_eq!(u.partial, 0, "span são não pode ser partial");
        assert_eq!(u.gaps, 3);
        assert_eq!(u.free_bytes, 4096 + 8192 + 2048);
        assert_eq!(u.largest_free, 8192);
        assert_eq!(u.used_bytes, (s.acme - s.base) as u64 - 14336);
    }

    // ── loop: node.next = node (o CAP tem de salvar, não o kernel inteiro) ──
    #[test]
    fn walk_self_loop_aborts_at_cap_without_hanging() {
        let s = Scratch::new(64 * 1024);
        let n1 = s.base;
        s.node(n1, n1, 4096); // next = si mesmo
        let mut bins = [0usize; MAX_BINS];
        bins[0] = n1;
        let (u, dt) = walk(&s, &bins);
        assert!(dt < WALK_BUDGET, "walk pendurou no self-loop: {:?}", dt);
        assert_eq!(u.partial, 1, "loop tem que ser partial=1");
        assert_eq!(u.gaps, super::TALC_WALK_MAX_GAPS + 1);
        assert_invariants(&u, s.acme - s.base);
    }

    #[test]
    fn walk_two_node_cycle_aborts_at_cap() {
        let s = Scratch::new(64 * 1024);
        let a = s.base;
        let b = s.base + 4096;
        s.node(a, b, 4096);
        s.node(b, a, 4096); // a→b→a→…
        let mut bins = [0usize; MAX_BINS];
        bins[1] = a;
        let (u, dt) = walk(&s, &bins);
        assert!(dt < WALK_BUDGET, "walk pendurou no ciclo de 2: {:?}", dt);
        assert_eq!(u.partial, 1);
        assert_eq!(u.gaps, super::TALC_WALK_MAX_GAPS + 1);
    }

    #[test]
    fn walk_all_bins_looping_still_bounded_by_global_cap() {
        // CAP é GLOBAL (contador único de gaps): 128 bins em self-loop somam o
        // mesmo teto, não 128× o teto.
        let s = Scratch::new(1024 * 1024);
        let mut bins = [0usize; MAX_BINS];
        for (i, b) in bins.iter_mut().enumerate() {
            let addr = s.base + i * 8192;
            s.node(addr, addr, 128);
            *b = addr;
        }
        let (u, dt) = walk(&s, &bins);
        assert!(dt < WALK_BUDGET, "walk pendurou com 128 loops: {:?}", dt);
        assert_eq!(u.partial, 1);
        assert_eq!(u.gaps, super::TALC_WALK_MAX_GAPS + 1);
    }

    // ── node fora do span (abaixo de base, acima/igual a acme) ──────────────
    #[test]
    fn walk_node_outside_span_is_partial() {
        // base - 8 (abaixo do span) no PRIMEIRO bin: aborta no gap #1, sem
        // pender e sem tocar memória fora da caixa.
        let s = Scratch::new(64 * 1024);
        let mut bins = [0usize; MAX_BINS];
        bins[0] = s.base - 8;
        let (u, dt) = walk(&s, &bins);
        assert!(dt < WALK_BUDGET);
        assert_eq!(u.partial, 1);
        assert_eq!(u.gaps, 1, "o nó rejeitado ainda conta como gap visto");
        assert_eq!(u.free_bytes, 0);
        assert_invariants(&u, s.acme - s.base);

        // acme (exatamente no fim) e um lixo gigante.
        let s2 = Scratch::new(64 * 1024);
        let mut b2 = [0usize; MAX_BINS];
        b2[0] = s2.acme;
        let (u2, _) = walk(&s2, &b2);
        assert_eq!(u2.partial, 1);
        assert_eq!(u2.gaps, 1);

        let s3 = Scratch::new(64 * 1024);
        let mut b3 = [0usize; MAX_BINS];
        b3[0] = usize::MAX - 7;
        let (u3, _) = walk(&s3, &b3);
        assert_eq!(u3.partial, 1);
        assert_eq!(u3.free_bytes, 0);

        // Gap válido no bin 0 + lixo no bin 1: o válido conta, o lixo aborta.
        let s4 = Scratch::new(64 * 1024);
        s4.node(s4.base, 0, 4096);
        let mut b4 = [0usize; MAX_BINS];
        b4[0] = s4.base;
        b4[1] = usize::MAX;
        let (u4, _) = walk(&s4, &b4);
        assert_eq!(u4.partial, 1);
        assert_eq!(u4.gaps, 2, "válido + lixo rejeitado");
        assert_eq!(u4.free_bytes, 4096, "o gap válido antes do lixo conta");
        assert_invariants(&u4, s4.acme - s4.base);
    }

    // ── size gigante / size zero ────────────────────────────────────────────
    #[test]
    fn walk_giant_size_is_partial_and_never_wraps_free() {
        let s = Scratch::new(64 * 1024);
        let n1 = s.base;
        let n2 = s.base + 4096;
        s.node(n1, n2, 4096);
        s.node(n2, 0, usize::MAX); // size impossível
        let mut bins = [0usize; MAX_BINS];
        bins[0] = n1;
        let (u, _) = walk(&s, &bins);
        assert_eq!(u.partial, 1);
        assert_eq!(u.free_bytes, 4096, "gap válido antes do lixo conta");
        assert_invariants(&u, s.acme - s.base);

        let s2 = Scratch::new(64 * 1024);
        let m = s2.base;
        s2.node(m, 0, usize::MAX - 3); // satura o usize
        let mut b2 = [0usize; MAX_BINS];
        b2[0] = m;
        let (u2, _) = walk(&s2, &b2);
        assert_eq!(u2.partial, 1);
        assert_eq!(u2.free_bytes, 0);

        let s3 = Scratch::new(64 * 1024);
        let z = s3.base;
        s3.node(z, 0, 0); // size zero = gap impossível
        let mut b3 = [0usize; MAX_BINS];
        b3[0] = z;
        let (u3, _) = walk(&s3, &b3);
        assert_eq!(u3.partial, 1);
        assert_eq!(u3.free_bytes, 0);
    }

    #[test]
    fn walk_free_saturates_instead_of_wrapping() {
        // 4000 gaps INDIVIDUALMENTE válidos (cada size cabe no span, cada node
        // cabe com footprint 24B) mas com DOMÍNIOS sobrepostos: a soma passa do
        // span. `free` tem que reportar a soma cheia e `used` saturar em 0 —
        // nunca dar wrap para um número pequeno (free publicado mentindo).
        let s = Scratch::new(1024 * 1024);
        let chain = s.base;
        const N: usize = 4000;
        let each = 4096usize;
        for k in 0..N {
            let addr = chain + k * super::TALC_GAP_MIN_FOOTPRINT;
            let next = if k == N - 1 { 0 } else { addr + super::TALC_GAP_MIN_FOOTPRINT };
            s.node(addr, next, each);
        }
        let mut bins = [0usize; MAX_BINS];
        bins[5] = chain;
        let (u, dt) = walk(&s, &bins);
        assert!(dt < WALK_BUDGET);
        assert_eq!(u.gaps, N as u64, "a cadeia inteira tem que ser percorrida");
        assert_eq!(u.partial, 0, "metadado válido (só sobreposto) não é lixo");
        assert_eq!(u.free_bytes, N as u64 * each as u64, "soma cheia, sem wrap");
        assert_eq!(u.largest_free, each as u64);
        assert_eq!(u.used_bytes, 0, "soma > span: usado satura em 0");
        assert_invariants(&u, s.acme - s.base);
    }

    // ── desalinhamento: next lixo desalinhado = UB se lido ───────────────────
    #[test]
    fn walk_misaligned_node_is_partial() {
        let s = Scratch::new(64 * 1024);
        let mut bins = [0usize; MAX_BINS];
        bins[0] = s.base + 4; // dentro do span, mas desalinhado
        let (u, dt) = walk(&s, &bins);
        assert!(dt < WALK_BUDGET);
        assert_eq!(u.partial, 1, "node desalinhado tem que ser rejeitado");
        assert_eq!(u.free_bytes, 0);
    }

    // ── cabeçalho cruzando acme: PROVA de que o size não é lido fora ────────
    #[test]
    fn walk_header_crossing_acme_never_reads_past_span() {
        let s = Scratch::new(64 * 1024);
        // node a 8B do fim: `size` cai no REDZONE (preenchido com 32, valor que
        // o walk aceitaria). Lido => free=32 e partial=0 (falso "são").
        let node = s.acme - 8;
        s.put(node, 0);
        let mut bins = [0usize; MAX_BINS];
        bins[0] = node;
        let (u, _) = walk(&s, &bins);
        assert_eq!(u.partial, 1, "node sem cabeçalho inteiro no span é lixo");
        assert_eq!(u.free_bytes, 0, "leu o redzone: leitura fora do span");

        // E o caso-limite válido: node cujo footprint 24B termina exatamente
        // em acme continua aceito (não é over-reject do footprint mínimo).
        let s2 = Scratch::new(64 * 1024);
        let node2 = s2.acme - super::TALC_GAP_MIN_FOOTPRINT;
        s2.node(node2, 0, 24);
        let mut b2 = [0usize; MAX_BINS];
        b2[0] = node2;
        let (u2, _) = walk(&s2, &b2);
        assert_eq!(u2.partial, 0, "footprint mínimo exato tem que passar");
        assert_eq!(u2.free_bytes, 24);
    }

    // ── ramos degenerados: bins nulo e span vazio ───────────────────────────
    #[test]
    fn walk_null_bins_and_empty_span_are_partial() {
        let s = Scratch::new(64 * 1024);
        let span_bytes = (s.acme - s.base) as u64;
        // Nunca claimado: bins = null_mut() → o walk não mediu nada, então
        // used = span e partial=1 (nunca "0 livre, tudo ocupado" como fato).
        let u = super::talc_walk_bins(&fake_talc_unclaimed(), s.span());
        assert_eq!(u.partial, 1, "nunca claimado (bins nulo) = parcial");
        assert_eq!(u.gaps, 0);
        assert_eq!(u.free_bytes, 0);
        assert_eq!(u.used_bytes, span_bytes);
        assert_invariants(&u, span_bytes as usize);

        // span vazio: get_base_acme() = None
        let null_bins = [0usize; MAX_BINS];
        let t = super::talc_walk_bins(&fake_talc(&null_bins), Span::empty());
        assert_eq!(t.partial, 1);
        assert_eq!(t.free_bytes, 0);
        assert_eq!(t.used_bytes, 0);
    }

    // ── FUZZ dirigido: xorshift determinístico, 400 casos corrompidos ───────
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    #[test]
    fn fuzz_corrupted_gap_lists_never_hang_and_stay_consistent() {
        let mut rng = Rng(0xDEEC_E66D_0BAD_F00D);
        for case in 0..400usize {
            let span_bytes = 4096 * (1 + rng.below(16));
            let s = Scratch::new(span_bytes);
            let mut bins = [0usize; MAX_BINS];

            // Planta 1..6 "gaps" com metadado majoritariamente lixo (passo
            // 256B: 6 nós + footprint 24B cabem no span mínimo de 4096B).
            let n_nodes = 1 + rng.below(6);
            let mut addrs = [0usize; 6];
            for (i, a) in addrs.iter_mut().enumerate().take(n_nodes) {
                let addr = s.base + i * 256;
                s.put(addr, 0);
                s.put(addr + 16, 0);
                *a = addr;
            }
            for i in 0..n_nodes {
                let addr = addrs[i];
                let kind = rng.below(10);
                let next = match kind {
                    0 => 0,                                   // fim honesto
                    1 => addrs[rng.below(n_nodes)],           // ciclo honesto
                    2 => s.base - 8,                          // abaixo do span
                    3 => s.acme + 8,                          // acima do span
                    4 => usize::MAX - rng.below(64),           // lixo gigante
                    5 => addr + 1,                            // desalinhado
                    _ => addrs[rng.below(n_nodes)],
                };
                let size = match rng.below(10) {
                    0 => usize::MAX,
                    1 => 0,
                    2 => (s.acme - s.base) + rng.below(4096), // maior que o span
                    _ => 8 * (1 + rng.below(512)),
                };
                s.node(addr, next, size);
                bins[rng.below(MAX_BINS)] = addr;
            }

            let (u, dt) = walk(&s, &bins);
            assert!(dt < WALK_BUDGET, "caso {} pendurou: {:?}", case, dt);
            assert_invariants(&u, s.acme - s.base);

            // Corrupção que sobrevive (partial=0) tem que ser aritmeticamente
            // fechável: o que o walk contou bate com o que ele somou.
            if u.partial == 0 {
                assert_eq!(u.used_bytes, (s.acme - s.base) as u64 - u.free_bytes);
                assert!(u.largest_free <= u.free_bytes);
            } else {
                assert_eq!(u.used_bytes, 0, "partial não publica uso inventado");
            }
        }
    }

    // ── INTEGRAÇÃO com um Talc REAL: os guards NÃO podem rejeitar o allocator ──
    //
    // O teste sintético acima prova que os guards pegam corrupção; este prova
    // o outro lado do contrato: metadado que o talc 4.4.3 REAL escreve tem que
    // passar ileso (partial=0), senão a telemetria s435 no kernel inteiro
    // vira "nunca medida". Usa claim/alloc/dealloc de verdade — o mesmo
    // caminho que `talc_init_post_memory` + GlobalAlloc exercitam no boot.
    #[test]
    fn walk_over_real_talc_is_never_partial() {
        // 1MB word-aligned (Box<[u64]>); claim exige >= MIN_HEAP_SIZE (32B).
        let mut heap = Box::new([0u64; (1024 * 1024) / 8]);
        let span = Span::from(heap.as_mut_slice());
        let mut real = Talc::new(ErrOnOom);
        unsafe { real.claim(span).expect("claim do heap de teste") };

        // Allocs de tamanho variado (caem em bins distintos do talc) + frees
        // parciais que registram gaps reais no meio do span (fragmentação).
        let mut live: Vec<(core::ptr::NonNull<u8>, core::alloc::Layout)> = Vec::new();
        for i in 0..24usize {
            let size = 24 * (1 + i * 7); // 24B..~4KB
            let layout = core::alloc::Layout::from_size_align(size, 8).unwrap();
            match unsafe { real.malloc(layout) } {
                Ok(p) => live.push((p, layout)),
                Err(()) => panic!("malloc {i} (size={size}) falhou"),
            }
        }
        let freed: Vec<_> = live.drain(..live.len() / 2).collect();
        for (p, layout) in &freed {
            unsafe { real.free(*p, *layout) };
        }

        let u = super::talc_walk_bins(&real, span);
        assert_eq!(u.partial, 0, "metadado do talc REAL não pode ser parcial");
        assert!(u.gaps >= 1, "deveria haver gaps registrados");
        assert!(u.free_bytes > 0);
        assert!(u.largest_free > 0);
        assert_eq!(u.used_bytes, (span.size() as u64).saturating_sub(u.free_bytes));

        // Sanidade do cálculo: o free medido cabe no span e o maior gap é um
        // dos bins visitados (nunca > span).
        assert!(u.free_bytes <= span.size() as u64, "free acima do span");
        assert!(u.largest_free <= span.size() as u64);
        assert!(u.gaps <= super::TALC_WALK_MAX_GAPS);

        // Limpa só o que ainda está alocado (os `freed` já foram liberados).
        for (p, layout) in live.iter() {
            unsafe { real.free(*p, *layout) };
        }
    }

    // ── fuzz de cauda: 1 nó com next = ponteiro aleatório (a classe do s438) ─
    #[test]
    fn fuzz_single_node_random_pointer_never_pends() {
        let mut rng = Rng(0x1234_5678_9ABC_DEF1);
        for case in 0..2000usize {
            let s = Scratch::new(8192);
            let addr = s.base;
            let next = match rng.below(4) {
                0 => rng.next() as usize,                     // qualquer lixo
                1 => s.base + rng.below(64),                  // dentro do span
                2 => s.acme.wrapping_sub(rng.below(64)),       // borda de acme
                _ => 0,
            };
            let size = match rng.below(3) {
                0 => rng.next() as usize,
                1 => usize::MAX,
                _ => 24 * (1 + rng.below(8)),
            };
            s.node(addr, next, size);
            let mut bins = [0usize; MAX_BINS];
            bins[0] = addr;
            let (u, dt) = walk(&s, &bins);
            assert!(dt < WALK_BUDGET, "caso {} pendurou: {:?}", case, dt);
            assert!(u.partial <= 1);
            assert!(u.gaps <= super::TALC_WALK_MAX_GAPS + 1);
        }
    }

    // ── s443: histograma de tamanhos (o "dados reais" da fragmentação) ────────

    #[test]
    fn buckets_sao_faixas_exclusivas_no_limite_correto() {
        // Limite = exclusivo: um gap de exatamente 8192 entra na faixa de cima.
        assert_eq!(super::talc_hist_bucket(0), 0);
        assert_eq!(super::talc_hist_bucket(8191), 0);
        assert_eq!(super::talc_hist_bucket(8192), 1);
        assert_eq!(super::talc_hist_bucket(64 * 1024), 2);
        assert_eq!(super::talc_hist_bucket(256 * 1024), 3);
        assert_eq!(super::talc_hist_bucket(1024 * 1024), 4);
        assert_eq!(super::talc_hist_bucket(16 * 1024 * 1024), 5);
        assert_eq!(super::talc_hist_bucket(u64::MAX), 5);
    }

    #[test]
    fn histograma_conta_cada_gap_na_faixa_certa() {
        // Um gap por faixa, escolhido bem dentro dela (limites são exclusivos).
        let sizes: [usize; 6] = [4096, 16 * 1024, 128 * 1024, 512 * 1024, 4 << 20, 32 << 20];
        let total: usize = sizes.iter().sum::<usize>() + 64 * 6;
        let s = Scratch::new(total + 4096);
        let mut addr = s.span().get_base_acme().unwrap().0 as usize;
        let mut nodes = [0usize; 6];
        for i in 0..6 {
            nodes[i] = addr;
            s.node(addr, 0, sizes[i]);
            addr += sizes[i] + 64; // 64B de vão entre os gaps
        }
        // Fecha a CADEIA (o último aponta p/ 0). Um anel aqui seria lista corrompida:
        // o walk abortaria no CAP com partial=1 (comportamento certo, já
        // coberto pelo fuzz) e não haveria histograma para conferir.
        for i in 0..6 {
            let next = if i + 1 < 6 { nodes[i + 1] } else { 0 };
            s.node(nodes[i], next, sizes[i]);
        }
        let mut bins = [0usize; MAX_BINS];
        bins[0] = nodes[0];
        let (u, _dt) = walk(&s, &bins);
        assert_eq!(u.partial, 0, "cadeia valida nao pode ser parcial");
        assert_eq!(u.gaps, 6);
        let sum: u32 = u.hist.iter().sum();
        assert_eq!(sum as u64, u.gaps, "histograma tem que fechar com gaps");
        for b in 0..super::TALC_HIST_BUCKETS {
            assert_eq!(u.hist[b], 1, "faixa {} com {} gaps", b, u.hist[b]);
        }
    }

    #[test]
    fn histograma_zera_em_walk_parcial() {
        // Dado parcial nao inventa distribuicao: se abortou no meio, os
        // buckets ficam so com o que deu para ler (aqui: nada, o 1o node ja
        // e invalido).
        let s = Scratch::new(1 << 20);
        let mut bins = [0usize; MAX_BINS];
        bins[0] = s.span().get_base_acme().unwrap().1 as usize - 4; // fora do span
        let (u, _dt) = walk(&s, &bins);
        assert_eq!(u.partial, 1);
        assert_eq!(u.hist.iter().sum::<u32>(), 0, "walk parcial nao pode publicar buckets");
    }
}

/// s443: `TalcBuf` — buffer POSSUÍDO na rota TALC (o tipo que conserta o
/// vazamento de consumers de longa vida com churn no bump).
#[cfg(test)]
mod talc_buf_tests {
    use super::*;

    #[test]
    fn roundtrip_de_string() {
        let b = TalcBuf::from_str("ola neural").expect("host: TALC nao pronto, cai no hibrido");
        assert_eq!(b.as_str(), "ola neural");
        assert_eq!(b.len(), 10);
        assert!(b.capacity() >= 10);
        assert_eq!(b.bytes(), b.capacity());
    }

    #[test]
    fn push_cresce_preservando_o_que_ja_existia() {
        let mut b = TalcBuf::new();
        assert!(b.is_empty());
        assert!(b.push_str("abc"));
        // Estoura a capacidade inicial (64B) para exercitar o grow.
        for i in 0..40 {
            assert!(b.push_str("0123456789"), "grow falhou no chunk {}", i);
        }
        assert_eq!(b.len(), 3 + 400);
        assert!(b.as_str().starts_with("abc0123456789"));
        assert!(b.capacity() >= b.len());
    }

    #[test]
    fn push_de_string_vazio_e_noop() {
        let mut b = TalcBuf::from_str("x").unwrap();
        let cap = b.capacity();
        assert!(b.push_str(""));
        assert_eq!(b.as_str(), "x");
        assert_eq!(b.capacity(), cap);
    }

    #[test]
    fn capacidade_impossivel_falha_fechado() {
        // `Layout` de usize::MAX é inválido => None, nunca panic/oom.
        assert!(TalcBuf::with_capacity(usize::MAX).is_none());
        assert!(TalcBuf::from_str(&"z".repeat(1024)).is_some());
    }

    #[test]
    fn shrink_devolve_a_capacidade_sobrando() {
        let mut b = TalcBuf::with_capacity(64 * 1024).unwrap();
        assert!(b.push_str("curto"));
        assert_eq!(b.capacity(), 64 * 1024);
        assert!(b.shrink_to_fit());
        assert_eq!(b.capacity(), b.len());
        assert_eq!(b.as_str(), "curto");
    }

    #[test]
    fn drop_nao_panica_e_o_ponteiro_e_zerado() {
        let mut b = TalcBuf::from_str("some").unwrap();
        let before = b.bytes();
        assert!(before > 0);
        // Substitui por um vazio: o Drop do antigo roda DE VERDADE.
        b = TalcBuf::new();
        assert_eq!(b.bytes(), 0);
        drop(b);
    }

    #[test]
    fn buffers_vivos_sao_independentes() {
        let a = TalcBuf::from_str("primeiro").unwrap();
        let mut b = TalcBuf::from_str("segundo").unwrap();
        assert!(b.push_str("!!"));
        assert_eq!(a.as_str(), "primeiro", "a nao pode ver o crescimento de b");
        assert_eq!(b.as_str(), "segundo!!");
    }
}
