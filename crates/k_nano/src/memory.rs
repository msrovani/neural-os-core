use ticket_lock::TicketLock;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use x86_64::VirtAddr;

/// Número de bytes no bitmap de frames físicos.
/// SESSÃO_260 (AIOS auto-adaptar): o buffer cobre o pior caso de RAM comum
/// (512KB × 8 bits × 4096 = 16GiB). O kernel NÃO usa tudo — o `total_frames`
/// é derivado da RAM REAL detectada (`last_end` do memory map) em
/// `init_from_usable_ranges`. Aumentar o buffer se máquinas >16GB aparecerem.
pub const BITMAP_SIZE: usize = 524288; // 512KB cobre 16GiB fisicos
const BITS_PER_BYTE: usize = 8;
const FRAME_SIZE: u64 = 4096;

// Fix (SESSION_233): section .data para evitar que o bump heap estendido
// sobrescreva estas statics — HEAP_BUFFER (512MB) em .bss é seguido por
// outras statics; estender HEAP_LIMIT alem de HEAP_SIZE corrompe total_frames.
#[link_section = ".data"]
pub static GLOBAL_ALLOCATOR: TicketLock<Option<BitmapFrameAllocator>> = TicketLock::new(None);
// Fix: section .data para evitar que resize_bump_heap(2048) sobrescreva
// esta página com uma frame zerada (HEAP_BUFFER de 512MB em .bss).
#[link_section = ".data"]
pub static PHYS_MEM_OFFSET: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
/// Total RAM em MB, detectado no boot via memory map.
#[link_section = ".data"]
pub static TOTAL_RAM_MB: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(512);

/// Alocador de frames físicos baseado em bitmap.

/// Alocador de frames físicos baseado em bitmap.
/// Usa um array estático de 128 KB no .bss para rastrear cada frame de 4 KiB
/// na memória física de 0 a 4 GiB. Bit = 0 → frame livre; Bit = 1 → ocupado.
pub struct BitmapFrameAllocator {
    pub bitmap: [u8; BITMAP_SIZE],
    /// Próximo bit livre conhecido — acelera alocações consecutivas.
    pub next_free_bit: usize,
    /// Total de frames gerenciados (derivado do memory_map na init).
    pub total_frames: usize,
    /// Frames marcados como `Usable` no memory map — usado pelo hardware_context_tensor.
    pub usable_frames: usize,
    /// Contador de frames alocados e não devolvidos.
    pub allocated_count: usize,
}

impl BitmapFrameAllocator {
    pub const fn empty() -> Self {
        BitmapFrameAllocator {
            bitmap: [0xFFu8; BITMAP_SIZE],
            next_free_bit: 0,
            total_frames: 0,
            usable_frames: 0,
            allocated_count: 0,
        }
    }

    /// Init a partir de ranges usable `(base, length)` — path Limine (ADR-0065).
    /// `ranges` = regiões MEMMAP_USABLE (e opcionalmente reclaimable).
    pub fn init_from_usable_ranges(&mut self, ranges: &[(u64, u64)]) {
        self.bitmap = [0xFFu8; BITMAP_SIZE];
        let mut last_end: u64 = 0;
        let mut usable_count: usize = 0;

        for &(base, length) in ranges.iter() {
            if length == 0 {
                continue;
            }
            let end = base.saturating_add(length);
            let start_frame =
                PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(base));
            let end_frame =
                PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(end.saturating_sub(1)));
            let start_idx = start_frame.start_address().as_u64() / FRAME_SIZE;
            let end_idx = end_frame.start_address().as_u64() / FRAME_SIZE;
            for i in start_idx..=end_idx {
                if (i as usize) < BITMAP_SIZE * BITS_PER_BYTE {
                    self.clear_bit(i as usize);
                    usable_count += 1;
                }
            }
            if end > last_end {
                last_end = end;
            }
        }

        self.total_frames = core::cmp::min(
            (last_end / FRAME_SIZE) as usize,
            BITMAP_SIZE * BITS_PER_BYTE,
        );
        if self.total_frames == 0 {
            self.total_frames = BITMAP_SIZE * BITS_PER_BYTE;
        }
        self.usable_frames = usable_count;
        self.allocated_count = 0;
        self.next_free_bit = 256;
        // Armazena RAM total para hw_profiler
        let ram_mb = (last_end / (1024 * 1024)) as u64;
        if ram_mb > 0 {
            TOTAL_RAM_MB.store(ram_mb, core::sync::atomic::Ordering::Relaxed);
            // SESSÃO_260 (AIOS): loga a RAM real detectada — o dump do BOOT.LOG
            // mostra quanto o kernel viu, separado do que gerencia.
            crate::slog_nano!("MEM", "info", "RAM detectada {} MB; frames gerenciados {} (bitmap {})",
                ram_mb, self.total_frames, BITMAP_SIZE);
        }
    }

    /// Marca um bit como 0 (frame livre).
    #[inline]
    fn clear_bit(&mut self, index: usize) {
        let byte_idx = index / BITS_PER_BYTE;
        let bit_idx = index % BITS_PER_BYTE;
        self.bitmap[byte_idx] &= !(1u8 << bit_idx);
    }

    /// Marca uma região física como OCUPADA (nunca entregue a DMA).
    /// SESSION_252/ora-1: o Limine pode reportar a RAM do kernel (image + .bss)
    /// como USABLE → o frame allocator entregava frames do kernel/heap para o
    /// e1000 (buffer RX) → DMA do NIC sobrescrevia o heap (conn.buf do OTA) →
    /// corrupção com tamanho exato. Use após init_from_usable_ranges com a
    /// região do kernel (KernelAddressRequest.physical_base → KERNEL_END).
    pub fn reserve_range(&mut self, base: u64, len: u64) {
        if len == 0 {
            return;
        }
        let end = base.saturating_add(len);
        let start_idx = (base / FRAME_SIZE) as usize;
        let end_idx = ((end.saturating_sub(1)) / FRAME_SIZE) as usize;
        for i in start_idx..=end_idx {
            if (i as usize) < BITMAP_SIZE * BITS_PER_BYTE {
                self.set_bit(i as usize);
            }
        }
        crate::slog_nano!("MEM", "info", "frame allocator reserva {:#x}..{:#x} ({} KB)", base, end, len / 1024);
    }

    /// Marca um bit como 1 (frame ocupado).
    #[inline]
    fn set_bit(&mut self, index: usize) {
        let byte_idx = index / BITS_PER_BYTE;
        let bit_idx = index % BITS_PER_BYTE;
        self.bitmap[byte_idx] |= 1u8 << bit_idx;
    }

    /// Lê o valor de um bit: 0 = livre, 1 = ocupado.
    #[inline]
    fn test_bit(&self, index: usize) -> bool {
        let byte_idx = index / BITS_PER_BYTE;
        let bit_idx = index % BITS_PER_BYTE;
        (self.bitmap[byte_idx] & (1u8 << bit_idx)) != 0
    }

    /// Busca linear por um frame livre a partir de `start_index`.
    fn find_free_frame(&self, start_index: usize) -> Option<usize> {
        let mut i = start_index;
        while i < self.total_frames {
            if !self.test_bit(i) {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    /// Aloca N frames contíguos — essencial para Huge Pages (2 MiB / 1 GiB)
    /// e para blocos de pesos compactados do FairyFuse TL/I2_S.
    #[allow(dead_code)]
    pub fn allocate_contiguous(&mut self, count: usize) -> Option<PhysFrame<Size4KiB>> {
        if count == 0 {
            return None;
        }
        let mut i = self.next_free_bit;
        while i <= self.total_frames.saturating_sub(count) {
            let mut found = true;
            for j in 0..count {
                if self.test_bit(i + j) {
                    found = false;
                    i += j + 1;
                    break;
                }
            }
            if found {
                for j in 0..count {
                    self.set_bit(i + j);
                }
                self.next_free_bit = i + count;
                return Some(PhysFrame::containing_address(PhysAddr::new(i as u64 * FRAME_SIZE)));
            }
        }
        None
    }

    /// Aloca um frame em endereço físico < 1 MiB (frames 0..255).
    /// Essencial para o trampoline real-mode do SMP.
    pub fn allocate_below_1mb(&mut self) -> Option<PhysFrame<Size4KiB>> {
        // Tenta frame 64 (0x40000 = 256 KB, longe da IVT/BDA/EBDA)
        let idx = 64;
        if idx < self.total_frames && !self.test_bit(idx) {
            self.set_bit(idx);
            self.allocated_count += 1;
            return Some(PhysFrame::containing_address(PhysAddr::new(idx as u64 * FRAME_SIZE)));
        }
        // Fallback: varre de 254 para baixo
        for i in (2..core::cmp::min(255, self.total_frames)).rev() {
            if !self.test_bit(i) {
                self.set_bit(i);
                self.allocated_count += 1;
                return Some(PhysFrame::containing_address(PhysAddr::new(i as u64 * FRAME_SIZE)));
            }
        }
        None
    }

    /// Aloca um bloco contíguo de N frames e mapeia como 2 MiB Huge Page.
    /// Se `count` for múltiplo de 512 (2 MiB / 4 KiB), mapeia como huge page.
    /// Retorna o PhysFrame do início do bloco.
    #[allow(dead_code)]
    pub fn allocate_huge_2mb(&mut self, count: usize) -> Option<PhysFrame<Size4KiB>> {
        if count == 0 || count % 512 != 0 {
            return self.allocate_contiguous(count);
        }
        // Alinha next_free_bit para boundary de 512 (2 MiB) antes de buscar,
        // evitando loop infinito quando next_free_bit % 512 != 0.
        let aligned_start = (self.next_free_bit + 511) & !511;
        let mut start_bit = aligned_start;
        loop {
            if start_bit + count > self.total_frames { break; }
            let mut ok = true;
            for j in 0..count {
                if self.test_bit(start_bit + j) { ok = false; break; }
            }
            if ok {
                for j in 0..count { self.set_bit(start_bit + j); }
                self.next_free_bit = start_bit + count;
                self.allocated_count += count;
                return Some(PhysFrame::containing_address(PhysAddr::new(start_bit as u64 * FRAME_SIZE)));
            }
            start_bit += 512;
        }
        None
    }

    /// Aloca alinhado a 1 GiB (262144 frames) — para Huge Pages 1G
    #[allow(dead_code)]
    pub fn allocate_huge_1gb(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.allocate_huge_2mb(262144)
    }

    pub fn usable_memory_bytes(&self) -> u64 {
        self.usable_frames as u64 * 4096
    }

    pub fn allocated_frame_count(&self) -> usize {
        self.allocated_count
    }

    /// Retorna o tensor de contexto de hardware para o roteador MLP.
    /// `[taxa_ocupacao, allocated_count]`.
    pub fn hardware_context_tensor(&self) -> [f32; 2] {
        let total = core::cmp::max(self.usable_frames, 1);
        [
            self.allocated_count as f32 / total as f32,
            self.allocated_count as f32,
        ]
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let idx = self.find_free_frame(self.next_free_bit)?;
        self.set_bit(idx);
        self.next_free_bit = idx + 1;
        self.allocated_count += 1;
        Some(PhysFrame::containing_address(PhysAddr::new(idx as u64 * FRAME_SIZE)))
    }
}

impl FrameDeallocator<Size4KiB> for BitmapFrameAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        let idx = (frame.start_address().as_u64() / FRAME_SIZE) as usize;
        if idx < self.total_frames {
            self.clear_bit(idx);
            if idx < self.next_free_bit {
                self.next_free_bit = idx;
            }
            if self.allocated_count > 0 {
                self.allocated_count -= 1;
            }
        }
    }
}

pub fn init_global_allocator(alloc: BitmapFrameAllocator) {
    *GLOBAL_ALLOCATOR.lock() = Some(alloc);
}

#[allow(dead_code)]
pub fn alloc_physical_frame() -> Option<PhysFrame<Size4KiB>> {
    let mut guard = GLOBAL_ALLOCATOR.lock();
    guard.as_mut().and_then(|a| a.allocate_frame())
}

#[allow(dead_code)]
pub unsafe fn dealloc_physical_frame(frame: PhysFrame<Size4KiB>) {
    let mut guard = GLOBAL_ALLOCATOR.lock();
    if let Some(ref mut a) = *guard {
        a.deallocate_frame(frame);
    }
}

pub fn global_hardware_context() -> [f32; 2] {
    let guard = GLOBAL_ALLOCATOR.lock();
    guard.as_ref().map_or([0.0, 0.0], |a| a.hardware_context_tensor())
}

pub unsafe fn init_memory(physical_memory_offset: u64) -> OffsetPageTable<'static> {
    PHYS_MEM_OFFSET.store(physical_memory_offset, core::sync::atomic::Ordering::Release);
    let (level_4_frame, _) = x86_64::registers::control::Cr3::read();
    let phys = level_4_frame.start_address();
    let virt = VirtAddr::new(physical_memory_offset) + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
    let page_table = unsafe { &mut *page_table_ptr };
    unsafe { OffsetPageTable::new(page_table, VirtAddr::new(physical_memory_offset)) }
}
