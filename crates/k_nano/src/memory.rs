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

/// Pool dedicada a page tables CoW/Ring 3 (Fase 1 v2.0): frames reservados do
/// alocador geral (bitmap=ocupado, nunca devolvidos ao geral). A falta de
/// frames para `clone_current()`/tabelas de página JAMAIS impede o isolamento
/// de processos — a pool tem fallback no geral (graceful), nunca o inverso.
pub const PT_POOL_FRAMES: usize = 256;      // 1 MiB
pub const PT_POOL_BITMAP_BYTES: usize = 32; // 256 bits

/// Alocador de frames físicos baseado em bitmap.
/// Usa um array estático de 128 KB no .bss para rastrear cada frame de 4 KiB
/// na memória física de 0 a 4 GiB. Bit = 0 → frame livre; Bit = 1 → ocupado.
pub struct BitmapFrameAllocator {
    pub bitmap: [u8; BITMAP_SIZE],
    /// Frames ENTREGUES por allocate_* (ownership, IDEA #526 / ora-1). Distingue
    /// de frames apenas RESERVADOS (reserve_range: kernel/heap/stack). Dealloc
    /// só é aceito se o frame foi entregue — liberar frame vivo (kernel/heap/PT)
    /// reabriria o frame para o DMA do e1000 sobrescrever memória viva (OTA
    /// hash_mismatch). Bit=1 → entregue.
    pub delivered: [u8; BITMAP_SIZE],
    /// Próximo bit livre conhecido — acelera alocações consecutivas.
    pub next_free_bit: usize,
    /// Total de frames gerenciados (derivado do memory_map na init).
    pub total_frames: usize,
    /// Frames marcados como `Usable` no memory map — usado pelo hardware_context_tensor.
    pub usable_frames: usize,
    /// Contador de frames alocados e não devolvidos.
    pub allocated_count: usize,
    /// Pool de page tables (CoW/Ring 3). Bit=1 → frame livre na pool.
    pub pt_pool: [u8; PT_POOL_BITMAP_BYTES],
    /// Índice (no bitmap geral) do 1º frame da pool; 0 = pool não iniciada.
    pub pt_pool_base: usize,
    /// Nº de frames na pool (0 = não iniciada).
    pub pt_pool_frames: usize,
}

impl BitmapFrameAllocator {
    pub const fn empty() -> Self {
        BitmapFrameAllocator {
            bitmap: [0xFFu8; BITMAP_SIZE],
            delivered: [0u8; BITMAP_SIZE],
            next_free_bit: 0,
            total_frames: 0,
            usable_frames: 0,
            allocated_count: 0,
            pt_pool: [0u8; PT_POOL_BITMAP_BYTES],
            pt_pool_base: 0,
            pt_pool_frames: 0,
        }
    }

    /// Init a partir de ranges usable `(base, length)` — path Limine (ADR-0065).
    /// `ranges` = regiões MEMMAP_USABLE (e opcionalmente reclaimable).
    pub fn init_from_usable_ranges(&mut self, ranges: &[(u64, u64)]) {
        // fill() in-place — `self.bitmap = [0xFF; BITMAP_SIZE]` materializava
        // temporário de 512KB na pilha (stack overflow em thread de teste com
        // 2 bitmaps; pressão de pilha no boot também).
        self.bitmap.fill(0xFF);
        self.delivered.fill(0);
        self.pt_pool.fill(0);
        self.pt_pool_base = 0;
        self.pt_pool_frames = 0;
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
    /// ⚠️ Reserva NÃO marca como entregue (`delivered`): dealloc de frame
    /// reservado é recusado (IDEA #526) — kernel/heap/stack são vivos.
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

    /// Marca `count` frames a partir de `start` como ENTREGUES (ownership).
    #[inline]
    fn mark_delivered(&mut self, start: usize, count: usize) {
        for i in start..start + count {
            if i < BITMAP_SIZE * BITS_PER_BYTE {
                let byte_idx = i / BITS_PER_BYTE;
                let bit_idx = i % BITS_PER_BYTE;
                self.delivered[byte_idx] |= 1u8 << bit_idx;
            }
        }
    }

    /// Frame foi ENTREGUE por allocate_*?
    #[inline]
    fn is_delivered(&self, index: usize) -> bool {
        let byte_idx = index / BITS_PER_BYTE;
        let bit_idx = index % BITS_PER_BYTE;
        (self.delivered[byte_idx] & (1u8 << bit_idx)) != 0
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
                self.mark_delivered(i, count);
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
            self.mark_delivered(idx, 1);
            self.allocated_count += 1;
            return Some(PhysFrame::containing_address(PhysAddr::new(idx as u64 * FRAME_SIZE)));
        }
        // Fallback: varre de 254 para baixo
        for i in (2..core::cmp::min(255, self.total_frames)).rev() {
            if !self.test_bit(i) {
                self.set_bit(i);
                self.mark_delivered(i, 1);
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
                self.mark_delivered(start_bit, count);
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

    // ─── Pool dedicada de page tables (CoW/Ring 3) — Fase 1 v2.0 ──────────

    /// Reserva `frames` páginas livres do alocador geral para a pool de page
    /// tables. Elas ficam Ocupadas no bitmap geral (nunca voltam ao geral) e
    /// livres na pool. Retorna quantas foram reservadas (0 = sem espaço).
    pub fn init_pt_pool(&mut self, frames: usize) -> usize {
        let mut carved = 0usize;
        let mut i = self.next_free_bit;
        while carved < frames && i < self.total_frames {
            if !self.test_bit(i) {
                self.set_bit(i);
                if carved == 0 {
                    self.pt_pool_base = i;
                }
                let byte = carved / 8;
                let bit = carved % 8;
                if byte < PT_POOL_BITMAP_BYTES {
                    self.pt_pool[byte] |= 1u8 << bit;
                }
                carved += 1;
            }
            i += 1;
        }
        self.pt_pool_frames = carved;
        crate::slog_nano!(
            "MEM",
            "info",
            "PT pool: {} frames dedicados ({} KB) para page tables CoW/Ring3",
            carved,
            carved * 4
        );
        carved
    }

    /// Índice na pool para um índice de frame do bitmap geral (None se não é
    /// frame da pool).
    #[inline]
    fn pt_pool_off(&self, frame_idx: usize) -> Option<usize> {
        if self.pt_pool_frames == 0 {
            return None;
        }
        let off = frame_idx.checked_sub(self.pt_pool_base)?;
        if off < self.pt_pool_frames {
            Some(off)
        } else {
            None
        }
    }

    /// Aloca frame da pool de page tables. Pool vazia → fallback no alocador
    /// geral (graceful: o isolamento nunca bloqueia por falta de frame).
    pub fn alloc_pt_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        for off in 0..self.pt_pool_frames {
            let byte = off / 8;
            let bit = off % 8;
            if self.pt_pool[byte] & (1u8 << bit) != 0 {
                self.pt_pool[byte] &= !(1u8 << bit);
                self.allocated_count += 1;
                let idx = self.pt_pool_base + off;
                return Some(PhysFrame::containing_address(PhysAddr::new(idx as u64 * FRAME_SIZE)));
            }
        }
        // Pool esgotada — fallback no geral (ownership `delivered` vale)
        self.allocate_frame()
    }

    /// Devolve frame à pool (se for frame da pool); senão libera no geral.
    /// Frame da pool permanece Ocupado no bitmap geral — nunca vaza para DMA.
    pub unsafe fn dealloc_pt_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        let idx = (frame.start_address().as_u64() / FRAME_SIZE) as usize;
        if let Some(off) = self.pt_pool_off(idx) {
            let byte = off / 8;
            let bit = off % 8;
            self.pt_pool[byte] |= 1u8 << bit;
            if self.allocated_count > 0 {
                self.allocated_count -= 1;
            }
            return;
        }
        self.deallocate_frame(frame);
    }

    /// Frames livres restantes na pool de page tables (telemetria).
    pub fn pt_pool_free(&self) -> usize {
        let mut n = 0;
        for off in 0..self.pt_pool_frames {
            if self.pt_pool[off / 8] & (1u8 << (off % 8)) != 0 {
                n += 1;
            }
        }
        n
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let idx = self.find_free_frame(self.next_free_bit)?;
        self.set_bit(idx);
        self.mark_delivered(idx, 1);
        self.next_free_bit = idx + 1;
        self.allocated_count += 1;
        Some(PhysFrame::containing_address(PhysAddr::new(idx as u64 * FRAME_SIZE)))
    }
}

impl FrameDeallocator<Size4KiB> for BitmapFrameAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        let idx = (frame.start_address().as_u64() / FRAME_SIZE) as usize;
        if idx >= self.total_frames {
            return;
        }
        // IDEA #526 / ora-1: só libera frame ENTREGUE pelo allocator. Double-free
        // ou dealloc de frame vivo (kernel/heap/PT reservado via reserve_range)
        // reabriria o frame para o DMA sobrescrever memória viva — recusa + log
        // (self-heal: melhor vazar do que corromper).
        if !self.is_delivered(idx) {
            crate::slog_nano!(
                "MEM",
                "error",
                "dealloc REFUSED frame {:#x} — não entregue (double-free / frame vivo reservado)",
                frame.start_address().as_u64()
            );
            return;
        }
        self.clear_bit(idx);
        let byte_idx = idx / BITS_PER_BYTE;
        let bit_idx = idx % BITS_PER_BYTE;
        self.delivered[byte_idx] &= !(1u8 << bit_idx);
        if idx < self.next_free_bit {
            self.next_free_bit = idx;
        }
        if self.allocated_count > 0 {
            self.allocated_count -= 1;
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

/// Fase 1 v2.0: reserva a pool dedicada de frames para page tables CoW/Ring3.
/// Chamar no boot APÓS `init_global_allocator` (e após `reserve_range`).
pub fn init_pt_pool(frames: usize) -> usize {
    let mut guard = GLOBAL_ALLOCATOR.lock();
    guard.as_mut().map_or(0, |a| a.init_pt_pool(frames))
}

/// Aloca frame da pool de page tables (fallback no geral se a pool esgotar).
/// Usado por `AddressSpace::clone_current()`/CoW/Ring3 — nunca bloqueia por
/// falta de frame físico.
pub fn alloc_pt_frame() -> Option<PhysFrame<Size4KiB>> {
    let mut guard = GLOBAL_ALLOCATOR.lock();
    guard.as_mut().and_then(|a| a.alloc_pt_frame())
}

/// Devolve frame de page table (à pool se for dela, senão ao geral).
pub unsafe fn dealloc_pt_frame(frame: PhysFrame<Size4KiB>) {
    let mut guard = GLOBAL_ALLOCATOR.lock();
    if let Some(a) = guard.as_mut() {
        a.dealloc_pt_frame(frame);
    }
}

/// Frames livres restantes na pool de page tables (telemetria HUD/SelfHeal).
pub fn pt_pool_available() -> usize {
    let guard = GLOBAL_ALLOCATOR.lock();
    guard.as_ref().map_or(0, |a| a.pt_pool_free())
}

/// SESSÃO_260 (AIOS): verifica se uma VA está mapeada (PRESENT) nas page
/// tables ATIVAS. Usado pelo scan do QEMU-loader para não ler hole
/// não-mapeado (CR2=pmoff+0x100000000 quando a RAM não alcança) — #PF storm.
/// Walk das 4 tabelas (PML4→PDPT→PD→PT) seguindo o CR3 atual.
pub fn is_page_present(virt: u64) -> bool {
    use x86_64::structures::paging::PageTable;
    use x86_64::VirtAddr;
    let pm = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if pm == 0 {
        return false;
    }
    let v = VirtAddr::new(virt);
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let base = VirtAddr::new(pm);
    let l4 = unsafe { &*(base + l4_frame.start_address().as_u64()).as_ptr::<PageTable>() };
    let l4e = &l4[v.p4_index()];
    if !l4e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return false;
    }
    let l3 = unsafe { &*(base + l4e.addr().as_u64()).as_ptr::<PageTable>() };
    let l3e = &l3[v.p3_index()];
    if !l3e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return false;
    }
    if l3e.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
        return true; // 1GB page
    }
    let l2 = unsafe { &*(base + l3e.addr().as_u64()).as_ptr::<PageTable>() };
    let l2e = &l2[v.p2_index()];
    if !l2e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return false;
    }
    if l2e.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
        return true; // 2MB page
    }
    let l1 = unsafe { &*(base + l2e.addr().as_u64()).as_ptr::<PageTable>() };
    l1[v.p1_index()]
        .flags()
        .contains(x86_64::structures::paging::PageTableFlags::PRESENT)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// O struct tem 1MB (2 bitmaps de 512KB) e `empty()` (const fn) o materializa
    /// na pilha do caller antes do move — thread de teste padrão estoura
    /// (STATUS_STACK_OVERFLOW). Roda o corpo numa thread com 16MB de stack.
    fn run_with_big_stack(body: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(body)
            .expect("spawn big-stack test")
            .join()
            .expect("test thread panicked");
    }

    /// 64MB usable a partir de 16MB (fora de IVT/BDA/EBDA e do kernel típico).
    fn make_allocator() -> Box<BitmapFrameAllocator> {
        let mut a = Box::new(BitmapFrameAllocator::empty());
        a.init_from_usable_ranges(&[(0x0100_0000u64, 0x0400_0000u64)]);
        a
    }

    #[test]
    fn dealloc_rejects_double_free() {
        run_with_big_stack(|| {
            let mut a = make_allocator();
            let f = a.allocate_frame().expect("frame");
            let idx = (f.start_address().as_u64() / FRAME_SIZE) as usize;
            unsafe { a.deallocate_frame(f) };
            assert!(!a.test_bit(idx), "dealloc legítimo libera o bit");
            assert!(!a.is_delivered(idx));
            assert_eq!(a.allocated_count, 0);
            // Double-free → recusado (não reabre o frame nem reentrega).
            unsafe { a.deallocate_frame(f) };
            assert!(!a.test_bit(idx), "double-free não deve marcar nada");
            assert!(!a.is_delivered(idx));
            assert_eq!(a.allocated_count, 0);
        });
    }

    #[test]
    fn dealloc_rejects_reserved_frame() {
        run_with_big_stack(|| {
            let mut a = make_allocator();
            // Reserva a 1ª página dos usable (simula kernel/heap/stack vivos).
            a.reserve_range(0x0100_0000, 0x1000);
            let f = PhysFrame::containing_address(PhysAddr::new(0x0100_0000));
            unsafe { a.deallocate_frame(f) };
            let idx = (f.start_address().as_u64() / FRAME_SIZE) as usize;
            assert!(a.test_bit(idx), "frame reservado deve continuar ocupado");
            assert!(!a.is_delivered(idx));
            assert_eq!(a.allocated_count, 0);
        });
    }

    #[test]
    fn pt_pool_isolated_from_general() {
        run_with_big_stack(|| {
            let mut a = make_allocator();
            let carved = a.init_pt_pool(64);
            assert_eq!(carved, 64);
            assert_eq!(a.pt_pool_frames, 64);
            assert_eq!(a.allocated_count, 0);
            let base = a.pt_pool_base;
            assert!(base > 0);

            // O alocador GERAL nunca entrega frame da pool (isolamento).
            for _ in 0..64 {
                let f = a.allocate_frame().expect("frame geral");
                let idx = (f.start_address().as_u64() / FRAME_SIZE) as usize;
                assert!(
                    idx < base || idx >= base + 64,
                    "geral entregou frame da pool idx={}",
                    idx
                );
            }

            // A pool entrega frames do seu range e devolve para a pool.
            let pf = a.alloc_pt_frame().expect("pool frame");
            let pidx = (pf.start_address().as_u64() / FRAME_SIZE) as usize;
            assert!(pidx >= base && pidx < base + 64);
            assert!(a.pt_pool_free() == 63);
            unsafe { a.dealloc_pt_frame(pf) };
            assert!(a.pt_pool_free() == 64);
            // Re-alocação devolve o mesmo frame (LIFO da pool)
            let pf2 = a.alloc_pt_frame().expect("pool frame 2");
            assert_eq!(pf2.start_address(), pf.start_address());
        });
    }

    #[test]
    fn pt_pool_exhaustion_falls_back_to_general() {
        run_with_big_stack(|| {
            let mut a = make_allocator();
            let carved = a.init_pt_pool(32);
            assert_eq!(carved, 32);
            let base = a.pt_pool_base;
            // Segura UM frame da pool e esgota o resto (31) + 1 fallback no geral.
            let held = a.alloc_pt_frame().expect("held pool frame");
            let held_idx = (held.start_address().as_u64() / FRAME_SIZE) as usize;
            assert!(held_idx >= base && held_idx < base + 32);
            let mut last_out = None;
            for _ in 0..32 {
                let f = a.alloc_pt_frame().expect("pt frame (fallback incluso)");
                last_out = Some((f.start_address().as_u64() / FRAME_SIZE) as usize);
            }
            let last_idx = last_out.expect("última alocação");
            assert!(
                last_idx < base || last_idx >= base + 32,
                "fallback deveria sair da pool, idx={}",
                last_idx
            );
            // Frames da pool continuam ocupados no geral (nunca vazam para DMA)
            assert!(a.test_bit(base));
            // Devolve o held → volta para a pool; próxima alocação vem da pool.
            unsafe { a.dealloc_pt_frame(held) };
            assert_eq!(a.pt_pool_free(), 1);
            let pf = a.alloc_pt_frame().expect("re-alocação da pool");
            let pidx = (pf.start_address().as_u64() / FRAME_SIZE) as usize;
            assert!(pidx >= base && pidx < base + 32, "re-alocação da pool");
        });
    }

    #[test]
    fn delivered_tracks_allocation_and_dealloc() {
        run_with_big_stack(|| {
            let mut a = make_allocator();
            let f = a.allocate_frame().expect("frame");
            let idx = (f.start_address().as_u64() / FRAME_SIZE) as usize;
            assert!(a.test_bit(idx));
            assert!(a.is_delivered(idx));
            assert_eq!(a.allocated_count, 1);
            unsafe { a.deallocate_frame(f) };
            assert_eq!(a.allocated_count, 0);
            assert!(!a.is_delivered(idx));
        });
    }
}
