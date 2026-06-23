use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use ticket_lock::TicketLock;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use x86_64::VirtAddr;

/// Número de bytes no bitmap para cobrir 4 GB de memória física.
/// Cada frame de 4 KiB = 1 bit. 1 byte = 8 frames = 32 KiB cobertos por byte.
/// 131072 bytes × 8 bits × 4096 bytes = 4 GiB.
const BITMAP_SIZE: usize = 131072;
const BITS_PER_BYTE: usize = 8;
const FRAME_SIZE: u64 = 4096;

pub static GLOBAL_ALLOCATOR: TicketLock<Option<BitmapFrameAllocator>> = TicketLock::new(None);
pub static PHYS_MEM_OFFSET: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Alocador de frames físicos baseado em bitmap.

/// Alocador de frames físicos baseado em bitmap.
/// Usa um array estático de 128 KB no .bss para rastrear cada frame de 4 KiB
/// na memória física de 0 a 4 GiB. Bit = 0 → frame livre; Bit = 1 → ocupado.
pub struct BitmapFrameAllocator {
    pub bitmap: [u8; BITMAP_SIZE],
    /// Próximo bit livre conhecido — acelera alocações consecutivas.
    next_free_bit: usize,
    /// Total de frames gerenciados (derivado do memory_map na init).
    total_frames: usize,
    /// Frames marcados como `Usable` no memory map — usado pelo hardware_context_tensor.
    usable_frames: usize,
    /// Contador de frames alocados e não devolvidos.
    allocated_count: usize,
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

    /// Varre o mapa de memória UEFI e marca como ocupados apenas os frames
    /// que NÃO são `Usable`. Frames utilizáveis ficam com bit = 0 (livre).
    pub fn init(&mut self, memory_map: &MemoryMap) {
        self.bitmap = [0xFFu8; BITMAP_SIZE];
        let mut last_end: u64 = 0;
        let mut usable_count: usize = 0;

        for region in memory_map.iter() {
            if region.region_type == MemoryRegionType::Usable {
                let start_frame = PhysFrame::<Size4KiB>::containing_address(
                    PhysAddr::new(region.range.start_addr()),
                );
                let end_frame = PhysFrame::<Size4KiB>::containing_address(
                    PhysAddr::new(region.range.end_addr() - 1),
                );
                let start_idx = start_frame.start_address().as_u64() / FRAME_SIZE;
                let end_idx = end_frame.start_address().as_u64() / FRAME_SIZE;

                for i in start_idx..=end_idx {
                    if (i as usize) < BITMAP_SIZE * BITS_PER_BYTE {
                        self.clear_bit(i as usize);
                        usable_count += 1;
                    }
                }
                last_end = region.range.end_addr();
            }
        }

        self.total_frames = core::cmp::min(
            (last_end / FRAME_SIZE) as usize,
            BITMAP_SIZE * BITS_PER_BYTE,
        );
        self.usable_frames = usable_count;
        self.allocated_count = 0;
        self.next_free_bit = 0;
    }

    /// Marca um bit como 0 (frame livre).
    #[inline]
    fn clear_bit(&mut self, index: usize) {
        let byte_idx = index / BITS_PER_BYTE;
        let bit_idx = index % BITS_PER_BYTE;
        self.bitmap[byte_idx] &= !(1u8 << bit_idx);
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
        let mut i = 0;
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
        let max_frame = core::cmp::min(256, self.total_frames);
        for i in 0..max_frame {
            if !self.test_bit(i) {
                self.set_bit(i);
                self.allocated_count += 1;
                return Some(PhysFrame::containing_address(PhysAddr::new(i as u64 * FRAME_SIZE)));
            }
        }
        None
    }

    /// Retorna o tensor de contexto de hardware para o roteador MLP.
    /// `[taxa_ocupacao, 0.0]` — fração de frames alocados vs total utilizável.
    pub fn hardware_context_tensor(&self) -> [f32; 2] {
        let total = core::cmp::max(self.usable_frames, 1);
        [
            self.allocated_count as f32 / total as f32,
            0.0,
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
