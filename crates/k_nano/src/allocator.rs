//! Tier 1 — Global allocator (Hermes / JARBAS / UI).
//! Lazy Bump Allocator — auto-inicializável na primeira alloc() via CAS.
//! zero init, zero chicken-and-egg. TALC pós-boot para resize.
//! Re-exported from neural-kernel to make k_nano the canonical location.

use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write;
use core::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
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
            let current_ptr = heap_start + real_offset;
            let aligned_ptr = (current_ptr + align - 1) & !(align - 1);
            let next_offset = (aligned_ptr - heap_start) + size;

            if next_offset > HEAP_LIMIT.load(Ordering::Relaxed) {
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

#[cfg(feature = "global-alloc")]
#[global_allocator]
static HEAP_ALLOC: LazyBumpAllocator = LazyBumpAllocator::new();

#[cfg(not(feature = "global-alloc"))]
static HEAP_ALLOC: LazyBumpAllocator = LazyBumpAllocator::new();

/// Buffer de heap estático no .bss — mapeado pelo bootloader.
#[link_section = ".bss"]
pub static mut HEAP_BUFFER: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

/// TALC allocator usado APÓS o boot para resize_heap (não é o global_allocator).
static TALC_ALLOC: Talck<spin::Mutex<()>, ErrOnOom> = Talck::new(Talc::new(ErrOnOom));
static CLAIMED_HEAP: Mutex<Option<Span>> = Mutex::new(None);

pub const HEAP_START: usize = 0x_4000_0000_0000;
pub const HEAP_SIZE: usize = 512 * 1024 * 1024; // 512MB .bss
pub static CURRENT_HEAP_MB: AtomicUsize = AtomicUsize::new(512);

/// Limite dinâmico do LazyBumpAllocator — começa = HEAP_SIZE, cresce via resize_bump_heap()
pub static HEAP_LIMIT: AtomicUsize = AtomicUsize::new(256 * 1024 * 1024);

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
    if !e3.flags().contains(PageTableFlags::PRESENT) {
        let f = alloc_pt_frame(base);
        if f == 0 { return; }
        e3.set_addr(PhysAddr::new(f), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    }
    let l3_virt = base + e3.addr().as_u64();
    let l3_tbl = &mut *(l3_virt.as_mut_ptr::<PageTable>());
    let e2 = &mut l3_tbl[virt.p3_index()];
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
        map_page_direct(base, virt, phys);
        heap_pte_present(base, virt)
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
