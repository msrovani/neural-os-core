//! Tier 1 — Global allocator (Hermes / JARBAS / UI).
//! Lazy Bump Allocator — auto-inicializável na primeira alloc() via CAS.
//! zero init, zero chicken-and-egg. TALC pós-boot para resize.
//! Re-exported from neural-kernel to make k_nano the canonical location.

use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write;
use core::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use spin::Mutex;
use talc::{Span, Talc, Talck, ErrOnOom};
use x86_64::structures::paging::page_table::PageTableEntry;
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
            let current_ptr = heap_start + real_offset;
            let aligned_ptr = (current_ptr + align - 1) & !(align - 1);
            let next_offset = (aligned_ptr - heap_start) + size;

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
    let heap_start = unsafe { HEAP_BUFFER.as_mut_ptr() as usize };
    let base = VirtAddr::new(crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed));
    if base.as_u64() == 0 {
        return false;
    }

    let want = need.saturating_add(HEAP_GROW_STEP - 1) / HEAP_GROW_STEP * HEAP_GROW_STEP;
    let extra = want.saturating_sub(current_limit);
    // SESSION_258/fix: cobrir a página que contém o último byte pedido
    // (heap_start + want - 1) — com heap_start desalinhado, o fim da região
    // caía no meio de uma página desmapeada → #PF (insert_rec/ART no boot).
    let base_off = (heap_start + current_limit) % 4096;
    let diff_pages = (extra + base_off).div_ceil(4096);

    #[cfg(feature = "heap-trace")]
    {
        crate::slog_nano!("HEAP", "PT", "grow: heap_start={:#x} (aligned={:#x}) cr3base={:#x} current={}MB need={} want={}MB diff_pages={}",
            heap_start, heap_start & !0xFFF, base.as_u64(), current_limit / (1024*1024), need, want / (1024*1024), diff_pages);
    }

    let mut allocated = 0usize;
    #[cfg_attr(not(feature = "heap-trace"), allow(unused_variables))]
    for i in 0..diff_pages {
        #[cfg(feature = "heap-trace")]
        if i == 0 || i == 511 || i == 512 || i == diff_pages - 1 {
            let v = VirtAddr::new((heap_start + current_limit + i * 4096) as u64);
            unsafe { dump_pt_walk(base, v, "grow", i) };
        }
        // Lock só para allocate_frame — solta ANTES de map_page_direct
        // (map_page_direct → alloc_pt_frame re-locka; TicketLock não é reentrante).
        let phys = {
            let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
            match g.as_mut().and_then(|a| a.allocate_frame()) {
                Some(f) => f.start_address().as_u64(),
                None => break,
            }
        };
        let virt = VirtAddr::new((heap_start + current_limit + allocated * 4096) as u64);
        // SESSION_252 diagnóstico (ora-1) — OFF default (feature heap-trace):
        // loga o 1º frame físico alocado ao heap para comparar com os RX buffers
        // do e1000. Causa raiz do OTA hash_mismatch era o sha256 (aa66c8b), não
        // overlap de frames — o log só é útil em sessões de investigação.
        #[cfg(feature = "heap-trace")]
        if allocated < 4 || allocated % 512 == 0 {
            crate::slog_nano!("HEAP", "BUMP", "frame[{}] phys={:#x} virt={:#x}", allocated, phys, virt.as_u64());
        }
        // Auditoria #1: mapa apenas se map_page_direct confirma (bool) E a
        // página está PRESENT (heap_pte_present — verificação real de CR3).
        if unsafe { map_page_direct(base, virt, phys) && heap_pte_present(base, virt) } {
            allocated += 1;
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

/// Debug heap-trace: caminha a page table ATUAL (CR3) para um endereço e
/// imprime as entradas e3/e2/e1/pte com flags. Só compila com a feature
/// heap-trace (OFF default). Ajuda a diagnosticar mapeamentos "verificados"
/// por heap_pte_present que depois #PFam (classe map_page_direct/HUGE_PAGE).
#[cfg(feature = "heap-trace")]
unsafe fn dump_pt_walk(base: VirtAddr, virt: VirtAddr, tag: &str, i: usize) {
    use x86_64::structures::paging::PageTableFlags;
    let (l4f, _) = x86_64::registers::control::Cr3::read();
    let (p4, p3, p2, p1) = (virt.p4_index(), virt.p3_index(), virt.p2_index(), virt.p1_index());
    crate::slog_nano!("HEAP", "PT", "{} i={} cr3={:#x} virt={:#x} p4={:?} p3={:?} p2={:?} p1={:?}",
        tag, i, l4f.start_address().as_u64(), virt.as_u64(), p4, p3, p2, p1);
    let l4 = &*((base + l4f.start_address().as_u64()).as_ptr::<PageTable>());
    let e3 = &l4[p4];
    let e3f = e3.flags();
    crate::slog_nano!("HEAP", "PT", "{} e3(p4[{:?}]) addr={:#x} flags={:#x} huge={} present={}",
        tag, p4, e3.addr().as_u64(), e3f.bits(), e3f.contains(PageTableFlags::HUGE_PAGE), e3f.contains(PageTableFlags::PRESENT));
    if !e3f.contains(PageTableFlags::PRESENT) || e3f.contains(PageTableFlags::HUGE_PAGE) { return; }
    let l3 = &*((base + e3.addr().as_u64()).as_ptr::<PageTable>());
    let e2 = &l3[p3];
    let e2f = e2.flags();
    crate::slog_nano!("HEAP", "PT", "{} e2(p3[{:?}]) addr={:#x} flags={:#x} huge={} present={}",
        tag, p3, e2.addr().as_u64(), e2f.bits(), e2f.contains(PageTableFlags::HUGE_PAGE), e2f.contains(PageTableFlags::PRESENT));
    if !e2f.contains(PageTableFlags::PRESENT) || e2f.contains(PageTableFlags::HUGE_PAGE) { return; }
    let l2 = &*((base + e2.addr().as_u64()).as_ptr::<PageTable>());
    let e1 = &l2[p2];
    let e1f = e1.flags();
    crate::slog_nano!("HEAP", "PT", "{} e1(p2[{:?}]) addr={:#x} flags={:#x} huge={} present={}",
        tag, p2, e1.addr().as_u64(), e1f.bits(), e1f.contains(PageTableFlags::HUGE_PAGE), e1f.contains(PageTableFlags::PRESENT));
    if !e1f.contains(PageTableFlags::PRESENT) || e1f.contains(PageTableFlags::HUGE_PAGE) { return; }
    let l1 = &*((base + e1.addr().as_u64()).as_ptr::<PageTable>());
    let pte = &l1[p1];
    let ptef = pte.flags();
    crate::slog_nano!("HEAP", "PT", "{} pte(p1[{:?}]) addr={:#x} flags={:#x} present={} writable={}",
        tag, p1, pte.addr().as_u64(), ptef.bits(), ptef.contains(PageTableFlags::PRESENT), ptef.contains(PageTableFlags::WRITABLE));
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

/// Buffer de heap estático — seção própria `.heap` (NÃO `.bss.heap`: o padrão
/// `*(.bss .bss.*)` do linker engole `.bss.heap` e o HEAP_BUFFER voltava para
/// o `.bss` comum, desalinhado — SESSION_258). Colocada no FIM da imagem
/// (limine.ld) e ALINHADA a MAXPAGESIZE: heap_start alinhado garante que a
/// extensão (grow) comece e termine em fronteiras de página.
/// Extensão alem dele (resize_bump_heap) só toca espaço
/// livre — NUNCA corrompe outras statics .bss (GLOBAL_ALLOCATOR, etc).
/// SESSION_233: sem isso, extender HEAP_LIMIT alem de HEAP_SIZE sobrescrevia
/// statics adjacentes e zerava total_frames (falsa exaustao de frames).
#[link_section = ".heap"]
pub static mut HEAP_BUFFER: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

/// TALC allocator usado APÓS o boot para resize_heap (não é o global_allocator).
static TALC_ALLOC: Talck<spin::Mutex<()>, ErrOnOom> = Talck::new(Talc::new(ErrOnOom));
static CLAIMED_HEAP: Mutex<Option<Span>> = Mutex::new(None);

pub const HEAP_START: usize = 0x_4000_0000_0000;
pub const HEAP_SIZE: usize = 512 * 1024 * 1024; // 512MB .bss
pub static CURRENT_HEAP_MB: AtomicUsize = AtomicUsize::new(512);

/// Limite do LazyBumpAllocator — o array HEAP_BUFFER tem 512MB (todo seguro).
/// Nunca estender alem de HEAP_SIZE: alem do array .bss ha outras statics
/// (GLOBAL_ALLOCATOR, etc.) — corrompe total_frames (SESSION_233).
pub static HEAP_LIMIT: AtomicUsize = AtomicUsize::new(HEAP_SIZE);

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
        // Auditoria #1: só conta página realmente mapeada (rollback se falhar).
        if unsafe { map_page_direct(base, virt, phys) } {
            allocated += 1;
        }
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

/// Mapeia a página de 4KiB `virt` para o frame físico `phys` na page table
/// ATUAL (CR3). **Contrato verificável (auditoria #1):** retorna `true` SOMENTE
/// se, após a chamada, a página está mapeada (present) para `phys`.
///
/// HUGE_PAGE em qualquer nível (SESSION_250 §2.3/§4): NUNCA descer para um walk
/// se a entrada já é 1GB/2MB page (lê P2 garbage → páginas não-mapeadas → #PF).
/// Em vez de early-return mudo, VALIDA a cobertura: se a página gigante cobre
/// `phys` e é writable → ok (true, sem dividir). Se não cobre → falha (false)
/// — o chamador NÃO avança o limite (rollback), em vez de mapear VA errada.
/// PTE já presente com outro `phys` = conflito → false (nunca alias silencioso:
/// foi exatamente isso que corrompeu o boot no SESSION_258 pré-alinhamento).
unsafe fn map_page_direct(base: VirtAddr, virt: VirtAddr, phys: u64) -> bool {
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4_virt = base + l4_frame.start_address().as_u64();
    let l4_tbl = &mut *(l4_virt.as_mut_ptr::<PageTable>());
    let e3 = &mut l4_tbl[virt.p4_index()];
    if e3.flags().contains(PageTableFlags::HUGE_PAGE) {
        return huge_page_covers(e3, phys, 1024 * 1024 * 1024);
    }
    if !e3.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 { return false; }
        e3.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l3_virt = base + e3.addr().as_u64();
    let l3_tbl = &mut *(l3_virt.as_mut_ptr::<PageTable>());
    let e2 = &mut l3_tbl[virt.p3_index()];
    if e2.flags().contains(PageTableFlags::HUGE_PAGE) {
        return huge_page_covers(e2, phys, 1024 * 1024 * 1024);
    }
    if !e2.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 { return false; }
        e2.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l2_virt = base + e2.addr().as_u64();
    let l2_tbl = &mut *(l2_virt.as_mut_ptr::<PageTable>());
    let e1 = &mut l2_tbl[virt.p2_index()];
    if e1.flags().contains(PageTableFlags::PRESENT) && e1.flags().contains(PageTableFlags::HUGE_PAGE) {
        return huge_page_covers(e1, phys, 2 * 1024 * 1024);
    }
    if !e1.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 { return false; }
        e1.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l1_virt = base + e1.addr().as_u64();
    let l1_tbl = &mut *(l1_virt.as_mut_ptr::<PageTable>());
    let pte = &mut l1_tbl[virt.p1_index()];
    if pte.flags().contains(PageTableFlags::PRESENT) {
        if pte.addr().as_u64() != phys {
            // Conflito: página já mapeada para outro frame — NUNCA sobrescrever
            // nem fingir sucesso (alias silencioso). Chamador faz rollback.
            return false;
        }
        if !pte.flags().contains(PageTableFlags::WRITABLE) {
            let mut flags = pte.flags();
            flags.insert(PageTableFlags::WRITABLE);
            pte.set_flags(flags);
            x86_64::instructions::tlb::flush(virt);
        }
        return true;
    }
    pte.set_addr(PhysAddr::new(phys), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    x86_64::instructions::tlb::flush(virt);
    true
}

/// true se a entrada é uma página gigante present+writable cujo range físico
/// [addr, addr+size) contém `phys` — ou seja, `virt` resolve para `phys`.
fn huge_page_covers(entry: &PageTableEntry, phys: u64, size: u64) -> bool {
    let f = entry.flags();
    if !f.contains(PageTableFlags::PRESENT) || !f.contains(PageTableFlags::WRITABLE) {
        return false;
    }
    let base_pa = entry.addr().as_u64();
    phys >= base_pa && phys < base_pa + size
}

/// #PF cure: buraco no heap Tier-1 (BitNet `to_vec` mid-load). Retorna true se page presente apos.
pub fn try_fault_in_heap(cr2: u64) -> bool {
    let start = HEAP_START as u64;
    let end = start + (CURRENT_HEAP_MB.load(Ordering::Relaxed) as u64) * 1024 * 1024;
    if cr2 < start || cr2 >= end {
        return false;
    }
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if pmoff == 0 {
        return false;
    }
    let virt = VirtAddr::new(cr2 & !0xFFF);
    let base = VirtAddr::new(pmoff);
    // Ja presente? So TLB stale apos CR3 switch — flush e retry.
    if unsafe { heap_pte_present(base, virt) } {
        x86_64::instructions::tlb::flush(virt);
        return true;
    }
    let phys = match crate::memory::alloc_physical_frame() {
        Some(f) => f.start_address().as_u64(),
        None => return false,
    };
    unsafe {
        map_page_direct(base, virt, phys) && heap_pte_present(base, virt)
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
    use x86_64::structures::paging::FrameAllocator;
    let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
    if let Some(alloc) = g.as_mut() {
        if let Some(frame) = alloc.allocate_frame() {
            let pa = frame.start_address().as_u64();
            core::ptr::write_bytes((base + pa).as_mut_ptr::<u8>(), 0, 4096);
            return pa;
        }
    }
    0
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
        // Auditoria #1: só conta página realmente mapeada (rollback se falhar).
        if unsafe { map_page_direct(base, virt, phys) } {
            allocated += 1;
        }
    }
    if allocated > 0 {
        let new_limit = HEAP_SIZE + allocated * 4096;
        HEAP_LIMIT.store(new_limit, Ordering::Release);
        let new_mb = (new_limit + 1024*1024 - 1) / (1024*1024);
        CURRENT_HEAP_MB.store(new_mb, Ordering::SeqCst);
        crate::slog_nano!("HEAP", "BUMP", "extendido para {} MB ({} páginas)", new_mb, allocated);
    }
}
