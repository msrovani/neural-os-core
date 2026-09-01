//! Tier 1 — Global allocator (Hermes / JARBAS / UI).
//! Lazy Bump Allocator — auto-inicializável na primeira alloc() via CAS.
//! zero init, zero chicken-and-egg. TALC pós-boot para resize.
//! Re-exported from neural-kernel to make k_nano the canonical location.

use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write;
use core::sync::atomic::{AtomicIsize, AtomicU64, AtomicUsize, Ordering};
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

        let mut current_offset = self.offset.load(Ordering::Relaxed);
        loop {
            let real_offset = if current_offset < 0 { 0 } else { current_offset as usize };
            let current_ptr = match heap_start.checked_add(real_offset) {
                Some(p) => p,
                None => return core::ptr::null_mut(),
            };
            let aligned_ptr = (current_ptr + align - 1) & !(align - 1);
            let next_offset = aligned_ptr.wrapping_sub(heap_start).saturating_add(size);

            if next_offset > HEAP_LIMIT.load(Ordering::Relaxed) {
                // AIOS na veia (premissa 2): heap se auto-adapta à necessidade.
                // Em vez de retornar null (→ OOM → hlt), mapeia mais frames e
                // retry. Fallback real de OOM só se o frame allocator esgotar.
                if grow_bump_auto(next_offset) {
                    current_offset = self.offset.load(Ordering::Relaxed);
                    continue;
                }
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

    let want_raw = need.saturating_add(HEAP_GROW_STEP - 1) / HEAP_GROW_STEP * HEAP_GROW_STEP;
    let want = want_raw.min(budget_bytes);
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
pub fn set_heap_budget_mb(mb: usize) {
    HEAP_BUDGET_MB.store(mb, Ordering::Release);
    crate::slog_nano!("HEAP", "BUDGET", "budget={}MB", mb);
}

/// Limite do LazyBumpAllocator — o array HEAP_BUFFER tem 512MB (todo seguro).
/// Nunca estender alem de HEAP_SIZE: alem do array .bss ha outras statics
/// (GLOBAL_ALLOCATOR, etc.) — corrompe total_frames (SESSION_233).
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
            if let Some(old) = *CLAIMED_HEAP.lock() {
                let req = Span::from_base_size(LARGE_HEAP_START as *mut u8, new_size - SLAB_SIZE);
                let extended = guard.extend(old, req);
                *CLAIMED_HEAP.lock() = Some(extended);
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
    let start = HEAP_START as u64;
    let talc_end = start + (CURRENT_HEAP_MB.load(Ordering::Relaxed) as u64) * 1024 * 1024;
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

#[cfg(feature = "global-alloc")]
#[alloc_error_handler]
fn oom(layout: core::alloc::Layout) -> ! {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b'O', options(nostack, preserves_flags));
    }
    {
        let mut w = crate::vga_buffer::WRITER.lock();
        if let Some(ref mut w) = *w {
            let _ = write!(w, "[OOM/TALC] size={} align={}", layout.size(), layout.align());
        }
    }
    {
        let mut s = crate::serial::SERIAL.lock();
        if let Some(ref mut s) = *s {
            let _ = write!(s, "[OOM/TALC] sem memoria Tier 1. size={} align={} Verifique HEAP_SIZE.\n",
                layout.size(), layout.align());
        }
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
    let current = CURRENT_HEAP_MB.load(Ordering::Relaxed);
    if target_mb <= current { return; }
    let diff_mb = target_mb - current;
    let diff_pages = diff_mb * 256;
    let heap_start = unsafe { HEAP_BUFFER.as_mut_ptr() as usize };
    let base = VirtAddr::new(crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed));
    if base.as_u64() == 0 { return; }

    let mut allocated = 0usize;
    for _i in 0..diff_pages {
        let phys = {
            let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
            match g.as_mut().and_then(|a| a.allocate_frame()) {
                Some(f) => f.start_address().as_u64(),
                None => break,
            }
        };
        let virt = VirtAddr::new((heap_start + HEAP_SIZE + allocated * 4096) as u64);
        unsafe {
            map_page_direct(base, virt, phys);
        }
        allocated += 1;
    }
    if allocated > 0 {
        let new_limit = HEAP_SIZE + allocated * 4096;
        HEAP_LIMIT.store(new_limit, Ordering::Release);
        let new_mb = (new_limit + 1024*1024 - 1) / (1024*1024);
        CURRENT_HEAP_MB.store(new_mb, Ordering::SeqCst);
        crate::slog_nano!("HEAP", "BUMP", "extendido para {} MB ({} páginas)", new_mb, allocated);
    }
}
