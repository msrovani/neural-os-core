use ticket_lock::TicketLock;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use x86_64::VirtAddr;

/// Bitmap: 2MiB × 8 × 4KiB = 64GiB de endereçamento PMM (limite do array, não SKU).
/// O struct vive no static GLOBAL_ALLOCATOR (não na stack do boot).
pub const BITMAP_SIZE: usize = 2 * 1024 * 1024;
const BITS_PER_BYTE: usize = 8;
const FRAME_SIZE: u64 = 4096;

/// Frames 0..255 (<1MB) reservados: IVT/BDA/EBDA + trampoline SMP real-mode.
/// next_free_bit nasce aqui — nunca entregar lowmem ao geral.
pub const LOWMEM_RESERVE_FRAMES: usize = 256;

/// Piso único de heap (MB): nós <1.5GB usam SMALL, demais FULL.
pub const HEAP_FLOOR_SMALL_MB: usize = 128;
pub const HEAP_FLOOR_MB: usize = 256;

/// ora-1: slots do registro de spans reservados (detector de double-use).
/// Callers atuais: kernel image + heap + stack + ramlog (bin) + ramlog (init,
/// merged) ≤5 — 16 dá folga p/ callers futuros. Cheio → warn + descarta o
/// novo (bitmap continua protegido; só a cobertura do detector perde).
pub const MAX_RESERVED_SPANS: usize = 16;

// Fix (SESSION_233): section .data para evitar que o bump heap estendido
// sobrescreva estas statics — HEAP_BUFFER (512MB) em .bss é seguido por
// outras statics; estender HEAP_LIMIT alem de HEAP_SIZE corrompe total_frames.
#[link_section = ".data"]
pub static GLOBAL_ALLOCATOR: TicketLock<Option<BitmapFrameAllocator>> =
    TicketLock::new(Some(BitmapFrameAllocator::empty()));
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
    /// ora-1 detector: spans reservados (índices de frame 4KiB, inclusivos).
    /// allocate_frame consulta antes de devolver — frame dentro de span
    /// registrado = reserva perdida (re-init/wipe) → warn 1x (tripwire que
    /// teria pego o #PF-storm do mesh 2-instâncias em 1 run). Sobrevive a
    /// re-init (spans EVER reservados não podem reabrir).
    pub reserved_spans: [(usize, usize); MAX_RESERVED_SPANS],
    pub reserved_count: usize,
    /// Detector já disparou nesta vida do alocador (warn 1x por boot).
    double_use_warned: bool,
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
            reserved_spans: [(0usize, 0usize); MAX_RESERVED_SPANS],
            reserved_count: 0,
            double_use_warned: false,
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
        // ora-1: o REGISTRO de spans NÃO reseta — spans ever-reservados não
        // podem reabrir (re-init que perde as reservas = exatamente o bug que
        // o detector pegaria). Só o flag do warn recomeça (1x por init).
        self.double_use_warned = false;
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
        self.next_free_bit = LOWMEM_RESERVE_FRAMES;

        // ─── ora-1: reservas de boot no init (defesa em profundidade) ──────
        // O bin (main.rs with_pmm) reserva kernel/heap/stack/ramlog DEPOIS do
        // init — mas o init em si limpava TUDO que o Limine reporta usable.
        // Re-init (ou caller futuro sem os reserves explícitos) reabriria
        // frames vivos → PMM entrega frame do kernel/heap → corrupção
        // silenciosa (mesh 2-instâncias: #PF-storm ×3 → BSP park, ~300
        // ticks). O init aplica o que conhece AQUI e registra os spans p/ o
        // detector de double-use:

        // (a) Kernel image [KERNEL_PHYS_BASE, KERNEL_PHYS_BASE + (KERNEL_END
        //     − KERNEL_VIRT_BASE)) — statics do allocator (kernel_phys_virt)
        //     + símbolo de linker KERNEL_END, lidos (não redefinidos). No
        //     path principal do boot as statics são setadas DEPOIS do init
        //     (o reserve_range explícito do bin cobre o span exato); este
        //     reserve vale p/ re-init e p/ callers que registram o kernel
        //     antes do init.
        self.reserve_kernel_image_init();

        // (b) boot_ramlog @ phys 0x1000_0000, 256KB — consts de boot_ramlog
        //     (nunca hardcode: o valor vive em boot_ramlog.rs).
        self.reserve_range(
            crate::boot_ramlog::BOOT_RAMLOG_PHYS,
            crate::boot_ramlog::BOOT_RAMLOG_CAP as u64,
        );

        // (c) Page tables — política documentada (NÃO skip silencioso):
        //     - PTs de runtime vêm DESTE PMM (allocate_frame/alloc_pt_frame →
        //       set_bit): ocupadas por construção, nunca re-entregues; o span
        //       da PT pool é registrado em init_pt_pool p/ o detector.
        //     - PTs de boot (HHDM do Limine) vivem em estruturas do
        //       bootloader FORA de MEMMAP_USABLE (limine.rs copia só o tipo
        //       0) → nunca cleared → nunca entregues.
        //     - O memmap do Limine não reporta span de page tables — não há
        //       span a cobrir; um range cego desperdiçaria RAM sem proteger
        //       nada enumerável. Se um dia o PMM enumerar PTs, reservar aqui.
        crate::slog_nano!("MEM", "info",
            "PMM init: PTs por construção (runtime=busy-marked, boot=fora USABLE) — sem span cego");

        // Armazena RAM total para hw_profiler
        let ram_mb = (last_end / (1024 * 1024)) as u64;
        if ram_mb > 0 {
            TOTAL_RAM_MB.store(ram_mb, core::sync::atomic::Ordering::Relaxed);
            // SESSÃO_260 (AIOS): loga a RAM real detectada — o dump do BOOT.LOG
            // mostra quanto o kernel viu, separado do que gerencia.
            crate::slog_nano!("MEM", "ok", "RAM detectada {} MB; frames gerenciados {} (bitmap {}B cap={}GiB)",
                ram_mb, self.total_frames, BITMAP_SIZE,
                (BITMAP_SIZE as u64) * 8 * 4096 / (1024 * 1024 * 1024));
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
        // ora-1: math pura (clip ao bitmap) + registro p/ o detector de
        // double-use. Span além do bitmap → sem bits a marcar (como antes) e
        // sem registro (nunca seria entregue). Overflow u64 → None (o código
        // antigo iterava ~4.5e15 vezes = hang latente).
        if let Some((s, e)) = frame_span(base, len, BITMAP_SIZE * BITS_PER_BYTE) {
            for i in s..=e {
                self.set_bit(i);
            }
            if !span_insert(&mut self.reserved_spans, &mut self.reserved_count, s, e) {
                crate::slog_nano!("MEM", "warn",
                    "registro de spans cheio ({}): {:#x}..{:#x} sem tripwire de double-use",
                    MAX_RESERVED_SPANS, base, end);
            }
        }
        crate::slog_nano!("MEM", "ok", "frame allocator reserva {:#x}..{:#x} ({} KB)", base, end, len / 1024);
    }

    /// ora-1 detector: frame `idx` dentro de span reservado? Dispara o warn
    /// 1x por vida do alocador. true = warn emitido nesta chamada (testável).
    /// O tripwire que teria pego o #PF-storm do mesh 2-instâncias em 1 run
    /// (PMM entregou frame do kernel/heap/PT → corrupção silenciosa).
    fn check_double_use(&mut self, idx: usize) -> bool {
        for i in 0..self.reserved_count {
            let (s, e) = self.reserved_spans[i];
            if idx >= s && idx <= e {
                if self.double_use_warned {
                    return false;
                }
                self.double_use_warned = true;
                crate::slog_nano!("MEM", "warn",
                    "DOUBLE-USE: frame {:#x} entregue dentro de span reservado {:#x}..{:#x} — reserva perdida (re-init?)",
                    (idx as u64) * FRAME_SIZE,
                    (s as u64) * FRAME_SIZE,
                    (e as u64) * FRAME_SIZE + FRAME_SIZE);
                return true;
            }
        }
        false
    }

    /// ora-1 (a): reserva a imagem do kernel no init — span físico
    /// [KERNEL_PHYS_BASE, KERNEL_PHYS_BASE + (KERNEL_END − KERNEL_VIRT_BASE)).
    /// Statics do allocator (kernel_phys_virt) + símbolo de linker KERNEL_END
    /// (limine.ld) — lidos, não redefinidos. No-op no boot principal antes do
    /// registro (statics (0,0) — o bin cobre com reserve_range explícito) e
    /// no host (sem símbolo de linker).
    #[cfg(target_os = "none")]
    fn reserve_kernel_image_init(&mut self) {
        let (kphys, kvirt) = crate::allocator::kernel_phys_virt();
        if kphys == 0 || kvirt == 0 {
            return; // ainda não registrado — bin cobre pós-init (honesto)
        }
        extern "C" {
            static KERNEL_END: u8;
        }
        let virt_end = unsafe { core::ptr::addr_of!(KERNEL_END) as u64 };
        let image_len = virt_end.saturating_sub(kvirt);
        if image_len == 0 {
            return;
        }
        self.reserve_range(kphys, image_len);
    }

    /// Host: sem símbolo de linker/bootinfo — no-op (o branch real é boot-only).
    #[cfg(not(target_os = "none"))]
    fn reserve_kernel_image_init(&mut self) {}

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
                // H13 (canvas-onda2): contabilidade de entrega — sem isto
                // allocated_count divergia do bitmap (DMA/e1000/NVMe usam isto).
                self.allocated_count += count;
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
        // ora-1 (c): registra o span da pool p/ o detector de double-use —
        // frames de PT vivos não podem reabrir sem o tripwire disparar.
        if carved > 0 {
            let s = self.pt_pool_base;
            let e = s + carved - 1;
            if !span_insert(&mut self.reserved_spans, &mut self.reserved_count, s, e) {
                crate::slog_nano!("MEM", "warn",
                    "registro de spans cheio: PT pool {:#x}..{:#x} sem tripwire",
                    s as u64 * FRAME_SIZE, (e as u64 + 1) * FRAME_SIZE);
            }
        }
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

/// Math pura de reserva (ora-1, testável no host): span físico `[base,
/// base+len)` → range de índices de frame 4KiB (inclusivo), clipado à região
/// gerenciada (`max_frames`). None = vazio/overflow/totalmente fora do
/// bitmap — nunca panic (len=0, base alto, overflow u64).
fn frame_span(base: u64, len: u64, max_frames: usize) -> Option<(usize, usize)> {
    if len == 0 || max_frames == 0 {
        return None;
    }
    let end = base.checked_add(len)?;
    let start_idx = (base / FRAME_SIZE) as usize;
    if start_idx >= max_frames {
        return None;
    }
    let end_idx = ((end - 1) / FRAME_SIZE) as usize;
    Some((start_idx, core::cmp::min(end_idx, max_frames - 1)))
}

/// Math pura (testável): insere span `[start, end]` (índices de frame,
/// inclusivos) numa lista fixa com merge de overlap/adjacência. false =
/// lista cheia (span descartado — o bitmap continua protegido; só a
/// cobertura do detector perde). Precondição: start <= end (frame_span
/// garante). Sem panic em entrada vazia.
fn span_insert(spans: &mut [(usize, usize)], count: &mut usize, start: usize, end: usize) -> bool {
    let mut start = start;
    let mut end = end;
    let mut i = 0;
    while i < *count {
        let (s, e) = spans[i];
        // Overlap ou adjacência: novo encosta no existente.
        if start <= e.saturating_add(1) && end.saturating_add(1) >= s {
            start = core::cmp::min(s, start);
            end = core::cmp::max(e, end);
            *count -= 1;
            spans[i] = spans[*count]; // swap com o último; slot re-checado
        } else {
            i += 1;
        }
    }
    if *count < spans.len() {
        spans[*count] = (start, end);
        *count += 1;
        true
    } else {
        false
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let idx = self.find_free_frame(self.next_free_bit)?;
        self.set_bit(idx);
        self.mark_delivered(idx, 1);
        self.next_free_bit = idx + 1;
        self.allocated_count += 1;
        // ora-1 tripwire: frame devolvido dentro de span reservado = reserva
        // perdida (re-init/wipe/caller sem reserve) → warn 1x por boot. O
        // #PF-storm do mesh 2-instâncias (PMM entregou frame do kernel/heap/
        // PT → corrupção → #PF ×3 → BSP park) teria sido pego AQUI em 1 run.
        self.check_double_use(idx);
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

/// Heap = min(75% RAM, RAM − keep). Em nós apertados (<1.5G) o keep sobe:
/// FB + PMM + FRAG working-set — senão o piso 512MB + TALC OOM em aloc minúscula.
/// POLÍTICA (librarian): talc 4.4.3 e x86_64 0.14 PINADOS — bump 4.x/0.15 é
/// breaking, fora deste lane. Memtype guard anti-aliasing = follow-up, não aqui.
pub fn heap_budget_mb(ram_mb: u64) -> usize {
    if ram_mb == 0 {
        return 512;
    }
    let pct75 = ram_mb.saturating_mul(3) / 4;
    let kernel_keep = if ram_mb < 1536 {
        // Lab mesh 1G / metal frugal: reserva ~⅓ pra kernel+FB+mesh, piso 384MB.
        (ram_mb / 3).max(384)
    } else {
        (ram_mb / 8).max(128)
    };
    let floor = if ram_mb < 1536 { HEAP_FLOOR_SMALL_MB } else { HEAP_FLOOR_MB };
    pct75
        .min(ram_mb.saturating_sub(kernel_keep))
        .max(floor as u64) as usize
}

/// Piso inicial do bump no boot — AIOS mede RAM, não hardcode 512 em 1G.
/// Invariante: piso >= HEAP_FLOOR_MB (nunca abaixo do floor do budget).
pub fn heap_piso_mb(ram_mb: u64) -> usize {
    if ram_mb == 0 {
        return 512;
    }
    if ram_mb < 1536 {
        HEAP_FLOOR_MB
    } else {
        512
    }
}

/// Budget máximo p/ reassembly FRAG neste nó (Observe→Plan: RAM baixa = DEGRADED).
/// Acima disso o RX dropa o payload (slog warn) em vez de OOM/halt.
pub fn frag_reassembly_budget_bytes(ram_mb: u64) -> usize {
    if ram_mb == 0 {
        return 64 * 1000;
    }
    if ram_mb < 1280 {
        // ~1G: só pacotes ≤ MTU — matmul 64×64 (~17KB) é recusado honestamente.
        1200
    } else if ram_mb < 2048 {
        // ~1.5–2G: cabe matmul FRAG (~17.5KB) com margem.
        24 * 1024
    } else {
        64 * 1000
    }
}

#[inline]
pub fn can_afford_frag(bytes: usize) -> bool {
    let ram = TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed);
    bytes <= frag_reassembly_budget_bytes(ram)
}

/// True quando o nó deve evitar carga pesada de mesh (FRAG matmul, self-test grande).
#[inline]
pub fn mesh_frag_pressure() -> bool {
    let ram = TOTAL_RAM_MB.load(core::sync::atomic::Ordering::Relaxed);
    ram > 0 && ram < 1536
}

/// Belt único: nó frugal **nunca** inicia TX/RX de FRAG pesado (s366).
/// Alias semântico de `mesh_frag_pressure` p/ callers de matmul/self-test.
#[inline]
pub fn refuse_heavy_frag() -> bool {
    mesh_frag_pressure()
}

pub fn with_pmm<F, R>(f: F) -> R
where
    F: FnOnce(&mut BitmapFrameAllocator) -> R,
{
    let mut g = GLOBAL_ALLOCATOR.lock();
    let fa = g.as_mut().expect("PMM static");
    f(fa)
}

/// Boot antigo movia um PMM da stack para o static — com bitmap 64GiB isso
/// estoura a stack Limine. O PMM já nasce no static (`Some(empty())`).
pub fn init_global_allocator() {
    // PMM já está no static (`Some(empty())`) e foi preenchido por with_pmm.
}

#[allow(dead_code)]
pub fn alloc_physical_frame() -> Option<PhysFrame<Size4KiB>> {
    let mut guard = GLOBAL_ALLOCATOR.lock();
    guard.as_mut().and_then(|a| a.allocate_frame())
}

// ─── Diagnóstico PMM (idea #630, s434) — telemetria zero-alloc do estado do
// frame allocator no momento de um OOM (lida de dentro do handler de morte).

/// Frames livres estimados (watermark pessimista: total − entregues).
pub fn pmm_free_frames() -> usize {
    with_pmm(|a| {
        a.total_frames.saturating_sub(a.allocated_count)
    })
}

/// Frames entregues desde o boot (allocated_count do bitmap).
pub fn pmm_allocated_count() -> usize {
    with_pmm(|a| a.allocated_count)
}

/// Frames totais gerenciados (usable ranges).
pub fn pmm_total_frames() -> usize {
    with_pmm(|a| a.total_frames)
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

/// Pool DMA do HDA (freeze s322): base física de 64KB contíguos reservados do
/// PMM (frames garantidamente não-vivos — ora-1 exclui kernel/heap/stack).
/// Os buffers antigos em phys FIXOS baixos (0x102000-0x108000) sobrepunham a
/// imagem do kernel em metal real: o DMA da saudação escrevia áudio sobre
/// .text/.data — e o BDL apontava para o PRÓPRIO buffer de amostras (o
/// controlador lia amostras como ponteiro de DMA!). QEMU mascarava (HDA
/// meio-morto, DMA inerte). 0 = pool não reservada → HDA usa fallback formant.
pub static HDA_DMA_BASE: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Reserva 64KB contíguos (<4GB) do PMM p/ o pool DMA do HDA. Idempotente.
/// Chamar uma vez no boot (pós-PMM init). Falha = HDA fica sem DMA (honesto).
pub fn reserve_hda_dma_pool() {
    if HDA_DMA_BASE.load(core::sync::atomic::Ordering::Relaxed) != 0 {
        return;
    }
    const FRAMES: usize = 16; // 64KB
    for _attempt in 0..3 {
        let mut frames = [0u64; FRAMES];
        let mut ok = true;
        for f in frames.iter_mut() {
            match alloc_pt_frame() {
                Some(fr) => *f = fr.start_address().as_u64(),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            for i in 1..FRAMES {
                if frames[i] != frames[i - 1] + 0x1000 {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            // Frames já alocados = não-vivos por definição; base contígua.
            HDA_DMA_BASE.store(frames[0], core::sync::atomic::Ordering::Relaxed);
            return;
        }
        // Não contíguo: frames alocados ficam retidos (sem free no bitmap);
        // cap 3 tentativas — no boot cedo o PMM quase sempre é contíguo.
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
        // L2: HHDM ainda não setado — warn (não silêncio) p/ diagnóstico de boot.
        crate::slog_nano!("MEM", "warn", "is_page_present: pm==0 (HHDM nao setado)");
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

/// Flags da folha (ou huge) que mapeia `virt`. None se não PRESENT.
/// Bit 63 = NX no PTE Intel.
pub fn page_leaf_flags(virt: u64) -> Option<u64> {
    use x86_64::structures::paging::PageTable;
    use x86_64::VirtAddr;
    let pm = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if pm == 0 {
        return None;
    }
    let v = VirtAddr::new(virt);
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let base = VirtAddr::new(pm);
    let l4 = unsafe { &*(base + l4_frame.start_address().as_u64()).as_ptr::<PageTable>() };
    let l4e = &l4[v.p4_index()];
    if !l4e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return None;
    }
    let l3 = unsafe { &*(base + l4e.addr().as_u64()).as_ptr::<PageTable>() };
    let l3e = &l3[v.p3_index()];
    if !l3e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return None;
    }
    if l3e.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
        return Some(l3e.flags().bits());
    }
    let l2 = unsafe { &*(base + l3e.addr().as_u64()).as_ptr::<PageTable>() };
    let l2e = &l2[v.p2_index()];
    if !l2e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return None;
    }
    if l2e.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
        return Some(l2e.flags().bits());
    }
    let l1 = unsafe { &*(base + l2e.addr().as_u64()).as_ptr::<PageTable>() };
    let l1e = &l1[v.p1_index()];
    if !l1e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return None;
    }
    Some(l1e.flags().bits())
}

/// Phys frame da folha (ou huge) que mapeia `virt`. None se não PRESENT.
/// Diagnóstico #PF-storm / virtio-blk: verifica se a VA HHDM do driver aponta
/// para o frame físico que o device DMA lê. Alias HHDM → frame errado = device
/// vê zeros → "virtio-blk missing headers" / nós BTree zerados (Bug #2).
pub fn page_leaf_phys(virt: u64) -> Option<u64> {
    use x86_64::structures::paging::PageTable;
    use x86_64::VirtAddr;
    let pm = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if pm == 0 {
        return None;
    }
    let v = VirtAddr::new(virt);
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let base = VirtAddr::new(pm);
    let l4 = unsafe { &*(base + l4_frame.start_address().as_u64()).as_ptr::<PageTable>() };
    let l4e = &l4[v.p4_index()];
    if !l4e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return None;
    }
    let l3 = unsafe { &*(base + l4e.addr().as_u64()).as_ptr::<PageTable>() };
    let l3e = &l3[v.p3_index()];
    if !l3e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return None;
    }
    if l3e.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
        return Some(l3e.addr().as_u64() + (virt & 0x3FFF_FFFF));
    }
    let l2 = unsafe { &*(base + l3e.addr().as_u64()).as_ptr::<PageTable>() };
    let l2e = &l2[v.p2_index()];
    if !l2e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return None;
    }
    if l2e.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
        return Some(l2e.addr().as_u64() + (virt & 0x1F_FFFF));
    }
    let l1 = unsafe { &*(base + l2e.addr().as_u64()).as_ptr::<PageTable>() };
    let l1e = &l1[v.p1_index()];
    if !l1e.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return None;
    }
    Some(l1e.addr().as_u64() + (virt & 0xFFF))
}

// ─── QEMU-loader READ-ONLY (ora-2 item 2A, ADAPTAR) ────────────────────────
// A região [0x100000000..0x180000000) recebe os .bitnet via `-device loader`
// (RAM física, identidade via HHDM). O HHDM do Limine mapeia tudo RW; este
// passo rebaixa para RO (PRESENT mantido, WRITABLE limpo) em granularidade
// 2MB — pesos de expert/modelo nunca sofrem store acidental do kernel.
// REGRAS (demand-page + hybrid allocator intactos):
// - NUNCA cria mapeamento: só rebaixa PDE/PTE já PRESENT (hole = skip).
//   `is_page_present` do scan (main.rs:3002-3059) segue byte-igual.
// - NUNCA reparte huge 1GB (split exigiria 512 frames): skip honesto.
// - NUNCA toca heap/TALC/bump: só endereços dentro do range loader.

/// Base física (inclusiva) da região QEMU-loader.
pub const LOADER_REGION_START: u64 = 0x1_0000_0000;
/// Fim físico (exclusivo) da região QEMU-loader.
pub const LOADER_REGION_END: u64 = 0x1_8000_0000;
/// Passo de cobertura RO (PDE 2MB).
pub const LOADER_REGION_STEP_2MB: u64 = 0x20_0000;

/// True se `phys` está na região QEMU-loader (puro — testável no host).
#[inline]
pub fn loader_range_contains(phys: u64) -> bool {
    phys >= LOADER_REGION_START && phys < LOADER_REGION_END
}

/// VA HHDM de `phys` se (e só se) está no range loader e o HHDM existe.
/// None = fora do range ou boot ainda sem PHYS_MEM_OFFSET (honesto).
pub fn loader_hhdm_va(phys: u64) -> Option<u64> {
    if !loader_range_contains(phys) {
        return None;
    }
    let pm = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Acquire);
    if pm == 0 {
        return None;
    }
    phys.checked_add(pm)
}

/// Rebaixa para RO a janela 2MB que contém `phys` (idempotente).
/// - PDE 2MB PRESENT: limpa WRITABLE, flush, true.
/// - PDE-tabela PRESENT: limpa WRITABLE das 512 folhas PRESENT, flush, true.
/// - Hole (qualquer nível ausente) ou huge 1GB: false (skip, sem fabricar RAM).
pub fn ensure_loader_page_ro(phys: u64) -> bool {
    use x86_64::structures::paging::{PageTable, PageTableFlags};
    use x86_64::VirtAddr;
    if !loader_range_contains(phys) {
        return false;
    }
    let pm = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Acquire);
    if pm == 0 {
        return false;
    }
    // Alinha o chunk em 2MB (a PDE cobre exatamente o chunk se ele está no range).
    let base_phys = phys & !(LOADER_REGION_STEP_2MB - 1);
    if base_phys + LOADER_REGION_STEP_2MB > LOADER_REGION_END {
        return false;
    }
    let chunk_va = match base_phys.checked_add(pm) {
        Some(v) => VirtAddr::new(v),
        None => return false,
    };
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let hhdm = VirtAddr::new(pm);
    let l4 = unsafe { &mut *(hhdm + l4_frame.start_address().as_u64()).as_mut_ptr::<PageTable>() };
    let l4e = &l4[chunk_va.p4_index()];
    if !l4e.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }
    let l3 = unsafe { &mut *(hhdm + l4e.addr().as_u64()).as_mut_ptr::<PageTable>() };
    let l3e = &l3[chunk_va.p3_index()];
    if !l3e.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }
    if l3e.flags().contains(PageTableFlags::HUGE_PAGE) {
        return false; // 1GB huge: split = 512 frames — skip honesto.
    }
    let l2 = unsafe { &mut *(hhdm + l3e.addr().as_u64()).as_mut_ptr::<PageTable>() };
    let pde = &mut l2[chunk_va.p2_index()];
    if !pde.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }
    if pde.flags().contains(PageTableFlags::HUGE_PAGE) {
        if pde.flags().contains(PageTableFlags::WRITABLE) {
            let mut f = pde.flags();
            f.remove(PageTableFlags::WRITABLE);
            pde.set_flags(f);
            x86_64::instructions::tlb::flush(chunk_va);
        }
        return true;
    }
    // PDE-tabela: rebaixa folha a folha (só PRESENT; ausente = hole parcial).
    // A tabela cobre exatamente este chunk 2MB → índices 0..512.
    let l1 = unsafe { &mut *(hhdm + pde.addr().as_u64()).as_mut_ptr::<PageTable>() };
    let mut any = false;
    for idx in 0..512usize {
        let leaf = &mut l1[idx];
        if leaf.flags().contains(PageTableFlags::PRESENT) {
            any = true;
            if leaf.flags().contains(PageTableFlags::WRITABLE) {
                let mut f = leaf.flags();
                f.remove(PageTableFlags::WRITABLE);
                leaf.set_flags(f);
                x86_64::instructions::tlb::flush(VirtAddr::new(
                    chunk_va.as_u64() + (idx as u64) * 0x1000,
                ));
            }
        }
    }
    any
}

/// Pass de boot: rebaixa TODA a região loader para RO (idempotente).
/// Retorna nº de janelas 2MB rebaixadas. Skips (hole/1GB) só no slog.
/// Chamar no boot APÓS o HHDM estar ativo e ANTES dos scans de expert/LLM
/// (main.rs, 1 linha — fora do escopo deste diff). Custo: ≤1024 iterações.
pub fn map_loader_region_ro() -> u64 {
    let pm = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Acquire);
    if pm == 0 {
        crate::slog_nano!("MEM", "warn", "loader RO skip: HHDM nao setado");
        return 0;
    }
    let mut ro = 0u64;
    let mut skip = 0u64;
    let mut addr = LOADER_REGION_START;
    while addr < LOADER_REGION_END {
        if ensure_loader_page_ro(addr) {
            ro += 1;
        } else {
            skip += 1;
        }
        addr += LOADER_REGION_STEP_2MB;
    }
    crate::slog_nano!("MEM", "ok", "loader RO [0x100000000..0x180000000): {}x2MB ro skip={} (hole/1G)", ro, skip);
    ro
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

    /// O struct tem 4MB (2 bitmaps de 2MB / 64GiB) e `empty()` materializa
    /// na pilha do caller — thread de teste padrão estoura. 32MB de stack.
    fn run_with_big_stack(body: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
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
    fn reserved_heap_range_never_handed_out() {
        run_with_big_stack(|| {
            let mut a = make_allocator();
            // Simula .kheap @ 16MB, 1MB — o PMM NÃO pode entregar estes frames.
            a.reserve_range(0x0100_0000, 1024 * 1024);
            for _ in 0..64 {
                let f = a.allocate_frame().expect("frame");
                let pa = f.start_address().as_u64();
                assert!(
                    !(0x0100_0000..0x0110_0000).contains(&pa),
                    "PMM entregou frame do heap reservado {:#x}",
                    pa
                );
            }
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

    /// ora-2 2A: limites do range loader (puro, sem HW).
    #[test]
    fn loader_range_bounds() {
        assert!(!loader_range_contains(0));
        assert!(!loader_range_contains(LOADER_REGION_START - 1));
        assert!(loader_range_contains(LOADER_REGION_START));
        assert!(loader_range_contains(LOADER_REGION_START + 0x100000));
        assert!(loader_range_contains(LOADER_REGION_END - 1));
        assert!(!loader_range_contains(LOADER_REGION_END));
        assert!(!loader_range_contains(u64::MAX));
        // loader_hhdm_va fora do range = None sem tocar no HHDM.
        assert!(loader_hhdm_va(0).is_none());
        assert!(loader_hhdm_va(LOADER_REGION_END).is_none());
        // ensure fora do range = false sem tocar nas page tables.
        assert!(!ensure_loader_page_ro(0));
        assert!(!ensure_loader_page_ro(LOADER_REGION_END));
    }

    // ─── ora-1: reservas de boot + detector de double-use ──────────────────

    /// Math pura de reserva: clip à região gerenciada, rejeita vazio/overflow,
    /// sem panic.
    #[test]
    fn frame_span_math_clips_and_rejects_empty() {
        let max = BITMAP_SIZE * BITS_PER_BYTE; // 16M frames (64GiB)
        // Vazio/zero → None, sem panic.
        assert_eq!(frame_span(0x1000, 0, max), None);
        assert_eq!(frame_span(0x1000, 0x1000, 0), None);
        // Overflow u64 → None, sem panic.
        assert_eq!(frame_span(u64::MAX - 0x800, 0x1000, max), None);
        // Span normal: [base, base+len) → índices inclusivos.
        assert_eq!(frame_span(0x1000, 0x1000, max), Some((1, 1)));
        assert_eq!(frame_span(0x1000, 0x2001, max), Some((1, 3))); // 2 frames + 1 byte
        // Clip: span além do bitmap → end clipado ao último frame gerenciado.
        let last = max - 1;
        assert_eq!(
            frame_span(last as u64 * FRAME_SIZE, 0x10000, max),
            Some((last, last))
        );
        // Totalmente fora do bitmap → None.
        assert_eq!(frame_span(max as u64 * FRAME_SIZE, 0x1000, max), None);
    }

    /// Math pura do registro: merge de overlap/adjacência, cheio → false.
    #[test]
    fn span_insert_merges_overlap_and_adjacency() {
        let mut spans = [(0usize, 0usize); MAX_RESERVED_SPANS];
        let mut count = 0usize;
        // Overlap: [10,12] + [11,14] → [10,14].
        assert!(span_insert(&mut spans, &mut count, 10, 12));
        assert!(span_insert(&mut spans, &mut count, 11, 14));
        assert_eq!(count, 1);
        assert_eq!(spans[0], (10, 14));
        // Adjacência: [15,16] encosta em [10,14] → [10,16].
        assert!(span_insert(&mut spans, &mut count, 15, 16));
        assert_eq!(count, 1);
        assert_eq!(spans[0], (10, 16));
        // Disjunto: novo slot.
        assert!(span_insert(&mut spans, &mut count, 20, 21));
        assert_eq!(count, 2);
        // Merge que fecha lacuna: [17,19] encosta nos dois → [10,21].
        assert!(span_insert(&mut spans, &mut count, 17, 19));
        assert_eq!(count, 1);
        assert_eq!(spans[0], (10, 21));
        // Cheio: descarta o novo (false), sem panic. (count=1 pós-merge →
        // cabem MAX-1 spans disjuntos.)
        for i in 0..MAX_RESERVED_SPANS - 1 {
            assert!(span_insert(&mut spans, &mut count, 100 + i * 10, 101 + i * 10));
        }
        assert_eq!(count, MAX_RESERVED_SPANS);
        assert!(!span_insert(&mut spans, &mut count, 9999, 10000));
    }

    /// Detector: dispara dentro de span reservado (1x), silencioso fora.
    #[test]
    fn double_use_detector_fires_inside_reserved_span_only() {
        run_with_big_stack(|| {
            let mut a = make_allocator();
            // Reserva 16 frames @ 0x0100_0000 (span registrado).
            a.reserve_range(0x0100_0000, 0x10000);
            let idx = (0x0100_0000 / FRAME_SIZE) as usize;
            // Simula reserva perdida (re-init/wipe): bit limpo à mão.
            a.clear_bit(idx);
            // 1ª entrega dentro do span → detector dispara (warn 1x).
            assert!(a.check_double_use(idx), "detector dispara no 1º double-use");
            // Once: 2ª ocorrência silenciosa.
            assert!(!a.check_double_use(idx), "warn é 1x por boot");
            // Fora de span reservado → silencioso (sem falso positivo).
            let outside = (0x0300_0000 / FRAME_SIZE) as usize;
            assert!(!a.check_double_use(outside));
        });
    }

    /// Caminho real: allocate_frame devolvendo frame de reserva perdida arma
    /// o tripwire (o que teria pego o #PF-storm do mesh em 1 run).
    #[test]
    fn allocate_frame_trips_detector_on_lost_reservation() {
        run_with_big_stack(|| {
            let mut a = make_allocator();
            a.reserve_range(0x0100_0000, 0x10000);
            let idx = (0x0100_0000 / FRAME_SIZE) as usize;
            a.clear_bit(idx); // reserva perdida
            let f = a.allocate_frame().expect("frame do span perdido");
            assert_eq!(f.start_address().as_u64(), 0x0100_0000);
            assert!(a.double_use_warned, "tripwire armado pela entrega");
        });
    }

    /// Init reserva o ramlog (consts reais de boot_ramlog — nunca hardcode):
    /// frames ocupados + span registrado + nenhuma alocação cai nele.
    /// (O branch do kernel image é boot-only — statics/símbolo de linker
    /// ausentes no host; o no-op roda no init acima sem panic.)
    #[test]
    fn init_reserves_ramlog_region() {
        run_with_big_stack(|| {
            // Usable 16MB..272MB cobre o ramlog @ 256MB.
            let mut a = Box::new(BitmapFrameAllocator::empty());
            a.init_from_usable_ranges(&[(0x0100_0000u64, 0x1000_0000u64)]);
            let rl_phys = crate::boot_ramlog::BOOT_RAMLOG_PHYS;
            let rl_cap = crate::boot_ramlog::BOOT_RAMLOG_CAP as u64;
            let rl_start = (rl_phys / FRAME_SIZE) as usize;
            let rl_end = ((rl_phys + rl_cap - 1) / FRAME_SIZE) as usize;
            for i in rl_start..=rl_end {
                assert!(a.test_bit(i), "frame do ramlog idx={} deve ficar ocupado", i);
            }
            // Span registrado p/ o detector.
            let hit = a.reserved_spans[..a.reserved_count]
                .iter()
                .any(|&(s, e)| rl_start >= s && rl_end <= e);
            assert!(hit, "span do ramlog deve estar no registro do detector");
            // Nenhuma alocação cai no ramlog.
            for _ in 0..64 {
                let f = a.allocate_frame().expect("frame");
                let pa = f.start_address().as_u64();
                assert!(
                    !(rl_phys..rl_phys + rl_cap).contains(&pa),
                    "PMM entregou frame do ramlog {:#x}",
                    pa
                );
            }
            assert!(!a.double_use_warned, "sem falso positivo fora de span");
        });
    }
}
