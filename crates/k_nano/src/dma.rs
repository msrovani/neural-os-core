//! DMA buffer allocation — páginas marcadas como UC para dispositivos PCI.
//! Previne corrupção por cache incoerente entre CPU e DMA.

use core::sync::atomic::Ordering;
use crate::memory::{GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET};

/// Buffer DMA — memória compartilhada com dispositivo PCI
pub struct DmaBuf {
    pub phys: u64,
    pub virt: *mut u8,
    pub size: usize,
}

unsafe impl Send for DmaBuf {}

impl Drop for DmaBuf {
    fn drop(&mut self) {
        if self.phys != 0 && self.size > 0 {
            let pages = (self.size + 4095) / 4096;
            restore_page_wb(self.phys, pages);
            unsafe {
                use x86_64::structures::paging::{FrameDeallocator, PhysFrame, Size4KiB};
                use x86_64::PhysAddr;
                let mut guard = GLOBAL_ALLOCATOR.lock();
                if let Some(alloc) = (*guard).as_mut() {
                    for i in 0..pages {
                        let f = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(self.phys + i as u64 * 4096));
                        alloc.deallocate_frame(f);
                    }
                }
            }
        }
    }
}

impl DmaBuf {
    pub fn as_ptr(&self) -> *const u8 { self.virt }
    pub fn as_mut_ptr(&mut self) -> *mut u8 { self.virt }
    pub unsafe fn as_slice(&self) -> &[u8] { core::slice::from_raw_parts(self.virt, self.size) }
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] { core::slice::from_raw_parts_mut(self.virt, self.size) }
}

/// Aloca páginas de DMA coalescentes (contíguas) para burst máximo PCIe.
/// Usa allocate_contiguous() para frames contíguos → permite burst DMA sem gaps.
pub fn dma_alloc_coalesced(size: usize) -> Option<DmaBuf> {
    let pages = (size + 4095) / 4096;
    if pages == 0 { return None; }
    let pa = unsafe {
        let mut guard = GLOBAL_ALLOCATOR.lock();
        let alloc = (*guard).as_mut()?;
        let frame = alloc.allocate_contiguous(pages)?;
        let pa = frame.start_address().as_u64();
        // Mapa páginas coalescentes como UC (uncacheable) para DMA
        mark_uc_or_warn(pa, pages, "dma_alloc_coalesced");
        let va = (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8;
        core::ptr::write_bytes(va, 0, pages * 4096);
        pa
    };
    let virt = (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8;
    Some(DmaBuf { phys: pa, virt, size: pages * 4096 })
}

/// Aloca páginas de DMA uncacheable. Usa `set_page_uc` do apic para marcar cada página.
pub fn dma_alloc(size: usize) -> Option<DmaBuf> {
    let pages = (size + 4095) / 4096;
    if pages == 0 { return None; }
    let pa = unsafe {
        
        let mut guard = GLOBAL_ALLOCATOR.lock();
        let alloc = (*guard).as_mut()?;
        let frame = alloc.allocate_contiguous(pages)?;
        let pa = frame.start_address().as_u64();
        mark_uc_or_warn(pa, pages, "dma_alloc");
        let va = (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8;
        core::ptr::write_bytes(va, 0, pages * 4096);
        pa
    };
    let virt = (pa + PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as *mut u8;
    Some(DmaBuf { phys: pa, virt, size: pages * 4096 })
}

/// Marca `pages` páginas como UC e loga se alguma não estava mapeada.
///
/// Página que fica cacheable = o device escreve por DMA e a CPU lê cache stale
/// (sem erro nenhum). Não dá pra falhar a alocação aqui (drivers já dependem
/// dela), mas o log expõe o buffer suspeito.
fn mark_uc_or_warn(phys: u64, pages: usize, who: &str) {
    let pm = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let mut failed = 0usize;
    for i in 0..pages {
        let addr = phys + i as u64 * 4096;
        // Primeiro garante que a página está mapeada via HHDM (map_page_uc cria
        // o mapeamento se não existir). Depois marca como UC.
        // Sem map_page_uc, set_page_uc retorna false → cache stale silencioso.
        unsafe { crate::apic::map_page_uc(addr, pm); }
        if !unsafe { crate::apic::set_page_uc(addr, pm) } {
            failed += 1;
        }
    }
    if failed > 0 {
        crate::slog_nano!("DMA", "warn",
            "{}: {}/{} paginas nao ficaram UC phys={:#x} - risco de cache stale",
            who, failed, pages, phys);
    }
}

/// Restore page attributes from UC back to WB (should be called when freeing DMA pages)
fn restore_page_wb(phys: u64, pages: usize) {
    let pm = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let mut failed = 0usize;
    for i in 0..pages {
        let addr = phys + i as u64 * 4096;
        if !unsafe { crate::apic::set_page_wb(addr, pm) } {
            failed += 1;
        }
    }
    if failed > 0 {
        crate::slog_nano!("DMA", "warn", "restore_wb: {}/{} paginas nao mapeadas phys={:#x}", failed, pages, phys);
    }
}

/// Libera páginas DMA.
///
/// `buf` é consumido; o `Drop` impl já faz `restore_page_wb` + dealloc dos frames.
pub fn dma_free(buf: DmaBuf) {
    drop(buf);
}

// ─── PhysicalBuffer ────────────────────────────────────────────────────────
// ponytail: PhysicalBuffer<{N}> — buffer DMA com alinhamento 4K e clflushopt.
// Usado por e1000 TX/RX rings, AHCI command lists, NVMe submission/completion queues.
// Garante cache coherence entre CPU e dispositivos DMA.

/// Buffer DMA de tamanho fixo em compilação, fisicamente contíguo, mapeado UC.
///
/// Aloca N bytes via frame allocator (arredondado para cima até página de 4K)
/// e mapeia como uncacheable para DMA. Cache coherence via clflushopt + sfence.
///
/// `new()` retorna `None` se a alocação falhar.
///
/// # Example
/// ```ignore
/// let buf = PhysicalBuffer::<4096>::new().expect("DMA allocation failed");
/// let phys = buf.phys_addr();   // escrever no registrador de endereço do device
/// unsafe { buf.invalidate_cache(); }
/// ```
pub struct PhysicalBuffer<const N: usize> {
    inner: DmaBuf,
}

impl<const N: usize> PhysicalBuffer<N> {
    /// Aloca N bytes contíguos fisicamente, mapeados UC, zerados.
    pub fn new() -> Option<Self> {
        let inner = dma_alloc(N)?;
        Some(Self { inner })
    }

    /// Endereço físico do buffer — para programar registradores DMA do device.
    #[inline(always)]
    pub fn phys_addr(&self) -> u64 {
        self.inner.phys
    }

    /// Ponteiro virtual para leitura.
    #[inline(always)]
    pub fn as_ptr(&self) -> *const u8 {
        self.inner.virt
    }

    /// Ponteiro virtual para escrita.
    #[inline(always)]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.inner.virt
    }

    /// Invalida cache para range [virt, virt+N) via clflushopt + sfence.
    ///
    /// ## Safety
    /// - O buffer NÃO pode ter referências mutáveis ativas durante a chamada.
    /// - O chamador deve garantir que o dispositivo DMA não está lendo o buffer
    ///   concorrentemente (ou, se lê, que a leitura obsoleta é tolerável).
    #[inline(always)]
    pub unsafe fn invalidate_cache(&self) {
        let mut addr = self.inner.virt as usize;
        let end = addr + N;
        while addr < end {
            core::arch::asm!("clflushopt [{0}]", in(reg) addr, options(nostack, preserves_flags));
            addr += 64;
        }
        core::arch::asm!("sfence", options(nostack, preserves_flags));
    }
}

// Drop automaticamente via DmaBuf (desaloca frames + restaura WB).
