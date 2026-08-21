use crate::acpi::AcpiInfo;
use crate::{println};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::ptr::{read_volatile, write_volatile};
use x86_64::structures::paging::{PageTable, PageTableFlags};
use x86_64::VirtAddr;

pub static USING_APIC: AtomicBool = AtomicBool::new(false);
pub static USING_X2APIC: AtomicBool = AtomicBool::new(false);
pub static LAPIC_VIRT_BASE: AtomicU64 = AtomicU64::new(0);

const IA32_APIC_BASE_MSR: u32 = 0x1B;

/// x2APIC: MSR base = 0x800 + (LAPIC_offset >> 4)
const fn lapic_msr(reg: u64) -> u32 {
    0x800 + (reg >> 4) as u32
}

const LAPIC_SVR: u64 = 0xF0;
const LAPIC_TPR: u64 = 0x80;
const LAPIC_EOI: u64 = 0xB0;
const LAPIC_ICR_LOW: u64 = 0x300;
const LAPIC_ICR_HIGH: u64 = 0x310;
const LAPIC_LVT_TIMER: u64 = 0x320;
const LAPIC_INIT_COUNT: u64 = 0x380;
const LAPIC_CURRENT_COUNT: u64 = 0x390;
const LAPIC_DIVIDE_CONFIG: u64 = 0x3E0;
/// Valor fixo programado em `start_timer()`.
const LAPIC_TIMER_INIT_COUNT_VAL: u32 = 0x800000;

const IOAPIC_IOREGSEL: u64 = 0x00;
const IOAPIC_IOWIN: u64 = 0x10;

const PIC_MASTER_DATA: u16 = 0x21;
const PIC_SLAVE_DATA: u16 = 0xA1;

struct Lapic {
    base: u64,
}

impl Lapic {
    unsafe fn new(base: u64) -> Self {
        Lapic { base }
    }

    unsafe fn read(&self, reg: u64) -> u32 {
        if USING_X2APIC.load(Ordering::Relaxed) {
            x86_64::registers::model_specific::Msr::new(lapic_msr(reg)).read() as u32
        } else {
            read_volatile((self.base + reg) as *const u32)
        }
    }

    unsafe fn write(&self, reg: u64, value: u32) {
        if USING_X2APIC.load(Ordering::Relaxed) {
            let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(reg));
            msr.write(value as u64);
        } else {
            write_volatile((self.base + reg) as *mut u32, value);
        }
    }

    unsafe fn eoi(&self) {
        self.write(LAPIC_EOI, 0);
    }

    unsafe fn init(&self) {
        // SVR: vetor espúrio = 0xFF (255), bit 8 = APIC enable
        // Evita #DE falso quando interrupção espúria chega com vetor 0
        let svr = self.read(LAPIC_SVR);
        let svr_fixed = (svr & 0xFFFFFF00) | 0xFF | 0x100;
        self.write(LAPIC_SVR, svr_fixed);
        self.write(LAPIC_TPR, 0);

        self.write(LAPIC_DIVIDE_CONFIG, 0b1011);
        self.write(LAPIC_INIT_COUNT, 0);

        crate::slog_nano!("APIC", "info", "LAPIC inicializado. Base: 0x{:x}", self.base);
        println!("[APIC] LAPIC inicializado.");
    }

    unsafe fn start_timer(&self) {
        self.write(LAPIC_LVT_TIMER, 32 | 0x20000);
        self.write(LAPIC_DIVIDE_CONFIG, 0b1011);
        self.write(LAPIC_INIT_COUNT, LAPIC_TIMER_INIT_COUNT_VAL);

        crate::slog_nano!("APIC", "info", "LAPIC timer iniciado: vetor 32, count={}, div=1.", LAPIC_TIMER_INIT_COUNT_VAL);
    }
}

struct IoApic {
    base: u64,
}

impl IoApic {
    unsafe fn new(base: u64) -> Self {
        IoApic { base }
    }

    unsafe fn read(&self, reg: u8) -> u32 {
        let select_addr = (self.base + IOAPIC_IOREGSEL) as *mut u32;
        let window_addr = (self.base + IOAPIC_IOWIN) as *const u32;
        write_volatile(select_addr, reg as u32);
        read_volatile(window_addr)
    }

    unsafe fn write(&self, reg: u8, value: u32) {
        let select_addr = (self.base + IOAPIC_IOREGSEL) as *mut u32;
        let window_addr = (self.base + IOAPIC_IOWIN) as *mut u32;
        write_volatile(select_addr, reg as u32);
        write_volatile(window_addr, value);
    }

    unsafe fn redirect_gsi(&self, gsi: u8, vector: u8, delivery_mode: u8) {
        let redir_low = (vector as u32) | ((delivery_mode as u32) << 8);
        let redir_high = 0u32;
        let reg_index = 0x10 + gsi * 2;
        self.write(reg_index, redir_low);
        self.write(reg_index + 1, redir_high);
    }

    unsafe fn init(&self, iso_overrides: &[(u8, u32)]) {
        let max_redirect = (self.read(0x01) >> 16) & 0xFF;
        crate::slog_nano!("APIC", "info", "IOAPIC em 0x{:x}. Max redirecionamentos: {}", self.base, max_redirect);
        println!("[APIC] IOAPIC encontrado. Max redirecionamentos: {}", max_redirect);

        // Mascara TODAS as RTEs inicialmente (bit 16)
        for gsi in 0..=max_redirect as u8 {
            let reg = 0x10 + gsi * 2;
            let low = self.read(reg);
            self.write(reg, low | 0x10000); // bit 16 = MASK
        }

        let kbd_gsi = iso_overrides.iter()
            .find(|(source, _)| *source == 1)
            .map(|(_, gsi)| *gsi as u8)
            .unwrap_or(1);

        let mouse_gsi = iso_overrides.iter()
            .find(|(source, _)| *source == 12)
            .map(|(_, gsi)| *gsi as u8)
            .unwrap_or(12);

        // Timer (IRQ0) → vetor 32, desmascarado
        self.redirect_gsi(0, 32, 0);
        let reg_tmr = 0x10;
        self.write(reg_tmr, self.read(reg_tmr) & !0x10000); // unmask

        // Keyboard (IRQ1) → vetor 33, desmascarado
        self.redirect_gsi(kbd_gsi, 33, 0);
        let reg_kbd = 0x10 + kbd_gsi * 2;
        self.write(reg_kbd, self.read(reg_kbd) & !0x10000); // unmask

        let v1_low = self.read(reg_kbd);
        let v1_high = self.read(reg_kbd + 1);
        crate::slog_nano!("APIC", "info", "IOAPIC verificado: kbd GSI {} (0x{:02x}:0x{:08x})", kbd_gsi, v1_high, v1_low);
        crate::slog_nano!("APIC", "info", "Teclado (IRQ1) redirecionado para vetor 33. RTEs 0-1 ativos, demais mascarados.");
        println!("[APIC] IOAPIC configurado: keyboard->vec33, mouse->vec44.");

        // Mouse (IRQ12 / GSI override) → vetor 44, desmascarado
        self.redirect_gsi(mouse_gsi, 44, 0);
        let reg_mouse = 0x10 + mouse_gsi * 2;
        self.write(reg_mouse, self.read(reg_mouse) & !0x10000);
        crate::slog_nano!("APIC", "info", "Mouse (IRQ12) GSI {} → vetor 44.", mouse_gsi);
    }
}

    unsafe fn disable_pic() {
        core::arch::asm!("out dx, al", in("dx") PIC_MASTER_DATA, in("al") 0xFFu8, options(nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") PIC_SLAVE_DATA, in("al") 0xFFu8, options(nostack, preserves_flags));
        crate::slog_nano!("APIC", "info", "PIC 8259 desabilitado (mascara todos IRQs).");
        println!("[APIC] PIC 8259 desabilitado.");
    }

    /// ponytail: setada antes de pit_init() quando hv=WHPX (PIT ignora vector 0)
    pub static SKIP_PIT: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

    pub unsafe fn pit_init() {
        if SKIP_PIT.load(core::sync::atomic::Ordering::Relaxed) {
            crate::slog_nano!("PIT", "info", "WHPX detectado — PIT skip (LAPIC only)");
            return;
        }
        core::arch::asm!("out 0x43, al", in("al") 0x36u8, options(nostack, preserves_flags));
        core::arch::asm!("out 0x40, al", in("al") 0x00u8, options(nostack, preserves_flags));
        core::arch::asm!("out 0x40, al", in("al") 0x00u8, options(nostack, preserves_flags));
        crate::slog_nano!("PIT", "info", "Canal 0 programado: modo 3, divisor 65536 (18.2 Hz).");
    }

unsafe fn read_lapic_base_msr() -> u64 {
    let msr_value = x86_64::registers::model_specific::Msr::new(IA32_APIC_BASE_MSR).read();
    let base = msr_value & 0xFFFF_FFFF_FFFF_F000;
    crate::slog_nano!("APIC", "info", "LAPIC base via MSR: 0x{:x}", base);
    base
}

/// Mapeia uma página MMIO 4KiB como uncacheable e presente.
/// Delega a `map_page_uc` (L4→L3→L2→L1→PTE). A versão antiga
/// gravava o frame no L2 sem criar L1 — #PF em VirtIO QUEUE_NOTIFY.
pub unsafe fn map_mmio_page(phys_addr: u64, phys_mem_offset: u64) {
    map_page_uc(phys_addr & !0xFFF, phys_mem_offset);
}

/// Marca a página HHDM de `phys_addr` como UC (PCD|PWT).
///
/// Retorna `false` (sem mutar nada) se a página não estiver mapeada — o
/// chamador PRECISA tratar: um buffer DMA que ficou cacheable lê cache stale
/// silenciosamente. Para MMIO novo use `map_page_uc` (que cria o mapeamento).
#[must_use]
pub unsafe fn set_page_uc(phys_addr: u64, phys_mem_offset: u64) -> bool {
    let virt = VirtAddr::new(phys_addr + phys_mem_offset);

    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let base = VirtAddr::new(phys_mem_offset);

    let l4_virt = base + l4_frame.start_address().as_u64();
    let l4_table = &mut *(l4_virt.as_mut_ptr::<PageTable>());
    let l3_entry = &mut l4_table[usize::from(virt.p4_index())];
    if !l3_entry.flags().contains(PageTableFlags::PRESENT) { return false; }

    if l3_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut flags = l3_entry.flags();
        flags |= PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH;
        l3_entry.set_flags(flags);
        x86_64::instructions::tlb::flush(virt);
        return true;
    }

    let l3_virt = base + l3_entry.addr().as_u64();
    let l3_table = &mut *(l3_virt.as_mut_ptr::<PageTable>());
    let l2_entry = &mut l3_table[usize::from(virt.p3_index())];
    if !l2_entry.flags().contains(PageTableFlags::PRESENT) { return false; }

    if l2_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut flags = l2_entry.flags();
        flags |= PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH;
        l2_entry.set_flags(flags);
        x86_64::instructions::tlb::flush(virt);
        return true;
    }

    let l2_virt = base + l2_entry.addr().as_u64();
    let l2_table = &mut *(l2_virt.as_mut_ptr::<PageTable>());
    let l1_entry = &mut l2_table[usize::from(virt.p2_index())];
    if !l1_entry.flags().contains(PageTableFlags::PRESENT) { return false; }

    if l1_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut flags = l1_entry.flags();
        flags |= PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH;
        l1_entry.set_flags(flags);
        x86_64::instructions::tlb::flush(virt);
        return true;
    }

    let l1_virt = base + l1_entry.addr().as_u64();
    let l1_table = &mut *(l1_virt.as_mut_ptr::<PageTable>());
    let pte = &mut l1_table[usize::from(virt.p1_index())];
    if !pte.flags().contains(PageTableFlags::PRESENT) { return false; }

    let mut flags = pte.flags();
    flags |= PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH;
    pte.set_flags(flags);

    x86_64::instructions::tlb::flush(virt);
    true
}

/// Restore page attributes from UC (NO_CACHE | WRITE_THROUGH) back to WB (Write-Back).
/// Clears the PCD (Page Cache Disable) and PWT (Page Write Through) bits in the PTE.
///
/// Retorna `false` se a página não estiver mapeada (nada foi alterado).
#[must_use]
pub unsafe fn set_page_wb(phys_addr: u64, phys_mem_offset: u64) -> bool {
    let virt = VirtAddr::new(phys_addr + phys_mem_offset);
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let base = VirtAddr::new(phys_mem_offset);

    let l4_virt = base + l4_frame.start_address().as_u64();
    let l4_table = &mut *(l4_virt.as_mut_ptr::<PageTable>());
    let l3_entry = &mut l4_table[usize::from(virt.p4_index())];
    if !l3_entry.flags().contains(PageTableFlags::PRESENT) { return false; }

    if l3_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut flags = l3_entry.flags();
        flags.remove(PageTableFlags::NO_CACHE);
        flags.remove(PageTableFlags::WRITE_THROUGH);
        l3_entry.set_flags(flags);
        x86_64::instructions::tlb::flush(virt);
        return true;
    }

    let l3_virt = base + l3_entry.addr().as_u64();
    let l3_table = &mut *(l3_virt.as_mut_ptr::<PageTable>());
    let l2_entry = &mut l3_table[usize::from(virt.p3_index())];
    if !l2_entry.flags().contains(PageTableFlags::PRESENT) { return false; }

    if l2_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut flags = l2_entry.flags();
        flags.remove(PageTableFlags::NO_CACHE);
        flags.remove(PageTableFlags::WRITE_THROUGH);
        l2_entry.set_flags(flags);
        x86_64::instructions::tlb::flush(virt);
        return true;
    }

    let l2_virt = base + l2_entry.addr().as_u64();
    let l2_table = &mut *(l2_virt.as_mut_ptr::<PageTable>());
    let l1_entry = &mut l2_table[usize::from(virt.p2_index())];
    if !l1_entry.flags().contains(PageTableFlags::PRESENT) { return false; }

    if l1_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut flags = l1_entry.flags();
        flags.remove(PageTableFlags::NO_CACHE);
        flags.remove(PageTableFlags::WRITE_THROUGH);
        l1_entry.set_flags(flags);
        x86_64::instructions::tlb::flush(virt);
        return true;
    }

    let l1_virt = base + l1_entry.addr().as_u64();
    let l1_table = &mut *(l1_virt.as_mut_ptr::<PageTable>());
    let pte = &mut l1_table[usize::from(virt.p1_index())];
    if !pte.flags().contains(PageTableFlags::PRESENT) { return false; }

    let mut flags = pte.flags();
    flags.remove(PageTableFlags::NO_CACHE);
    flags.remove(PageTableFlags::WRITE_THROUGH);
    pte.set_flags(flags);

    x86_64::instructions::tlb::flush(virt);
    true
}

/// Mapa uma pagina de 4KB para MMIO no endereco fisico `phys_addr`,
/// criando entradas de tabela se necessario, e marca como NO_CACHE + WRITE_THROUGH.
/// Se uma huge page (2MB/1GB) ja cobrir o endereco, modifica as flags diretamente.
pub unsafe fn map_page_uc(phys_addr: u64, phys_mem_offset: u64) {
    map_page_uc_at(phys_addr + phys_mem_offset, phys_addr, phys_mem_offset);
}

/// Mapa uma pagina de 4KB fisica em um VA ARBITRARIO (SASOS, ADR-0047-G3/0087
/// Fase 4a) com NO_CACHE + WRITE_THROUGH. Mesmo walk L4→L3→L2→L1 de
/// `map_page_uc`, mas o destino virtual é explícito — permite mapear VRAM no
/// espaço do heap (0x4020_0000_0000+) sem depender da identidade phys+pmoff.
pub unsafe fn map_page_uc_at(virt_addr: u64, phys_addr: u64, phys_mem_offset: u64) {
    use x86_64::structures::paging::PageTable;
    use x86_64::VirtAddr;
    use x86_64::PhysAddr;

    let virt = VirtAddr::new(virt_addr);
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let base = VirtAddr::new(phys_mem_offset);
    let l4_virt = base + l4_frame.start_address().as_u64();
    let l4_table = &mut *(l4_virt.as_mut_ptr::<PageTable>());

    // L4 → L3
    let l3_entry = &mut l4_table[usize::from(virt.p4_index())];
    if !l3_entry.flags().contains(PageTableFlags::PRESENT) {
        let frame = alloc_mmio_frame(base);
        l3_entry.set_addr(PhysAddr::new(frame), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    } else if l3_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut f = l3_entry.flags(); f.insert(PageTableFlags::NO_CACHE); f.insert(PageTableFlags::WRITE_THROUGH); l3_entry.set_flags(f);
        x86_64::instructions::tlb::flush(virt); return;
    }
    let l3_virt = base + l3_entry.addr().as_u64();
    let l3_table = &mut *(l3_virt.as_mut_ptr::<PageTable>());

    // L3 → L2
    let l2_entry = &mut l3_table[usize::from(virt.p3_index())];
    if !l2_entry.flags().contains(PageTableFlags::PRESENT) {
        let frame = alloc_mmio_frame(base);
        l2_entry.set_addr(PhysAddr::new(frame), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    } else if l2_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut f = l2_entry.flags(); f.insert(PageTableFlags::NO_CACHE); f.insert(PageTableFlags::WRITE_THROUGH); l2_entry.set_flags(f);
        x86_64::instructions::tlb::flush(virt); return;
    }
    let l2_virt = base + l2_entry.addr().as_u64();
    let l2_table = &mut *(l2_virt.as_mut_ptr::<PageTable>());

    // L2 → L1
    let l1_entry = &mut l2_table[usize::from(virt.p2_index())];
    if !l1_entry.flags().contains(PageTableFlags::PRESENT) {
        let frame = alloc_mmio_frame(base);
        l1_entry.set_addr(PhysAddr::new(frame), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    } else if l1_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        let mut f = l1_entry.flags(); f.insert(PageTableFlags::NO_CACHE); f.insert(PageTableFlags::WRITE_THROUGH); l1_entry.set_flags(f);
        x86_64::instructions::tlb::flush(virt); return;
    }
    let l1_virt = base + l1_entry.addr().as_u64();
    let l1_table = &mut *(l1_virt.as_mut_ptr::<PageTable>());

    // L1 → 4KB page
    let pte = &mut l1_table[usize::from(virt.p1_index())];
    pte.set_addr(PhysAddr::new(phys_addr),
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE
        | PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH);

    x86_64::instructions::tlb::flush(virt);
}

/// Mapa uma regiao de memoria fisica usando Huge Pages de 2MB para MMIO.
/// Muito mais rapido que map_page_uc() para grandes regioes (ex: 8GB VRAM).
/// Retorna o numero de entradas L2 configuradas.
pub unsafe fn map_region_uc_2mb(phys_start: u64, size_bytes: u64, phys_mem_offset: u64) -> usize {
    map_region_uc_2mb_at(phys_start + phys_mem_offset, phys_start, size_bytes, phys_mem_offset)
}

/// Mesmo mapeamento 2MB UC, mas com VA de destino ARBITRARIO (SASOS, ADR-0087
/// Fase 4a). Mapeia VRAM no espaço do heap (0x4020_0000_0000+) — o ponteiro
/// unificado que o `Tensor::location = MemTier::Vram` (0047-GPU §7.4) usa.
/// Requer `virt_start` e `phys_start` alinhados a 2MB.
pub unsafe fn map_region_uc_2mb_at(
    virt_start: u64,
    phys_start: u64,
    size_bytes: u64,
    phys_mem_offset: u64,
) -> usize {
    use x86_64::structures::paging::{PageTable, PageTableFlags};
    use x86_64::VirtAddr;
    use x86_64::PhysAddr;

    let mut mapped = 0;
    let mut offset = 0u64;
    while offset < size_bytes {
        let phys = phys_start + offset;
        let virt = VirtAddr::new(virt_start + offset);
        let (l4_frame, _) = x86_64::registers::control::Cr3::read();
        let base = VirtAddr::new(phys_mem_offset);
        let l4_virt = base + l4_frame.start_address().as_u64();
        let l4_table = &mut *(l4_virt.as_mut_ptr::<PageTable>());

        // SESSÃO_260 (ora-1 HIGH): o mapeamento 2MB deve ir na PDE (nível 2,
        // p2_index), NÃO na PDPTE — PDPTE+PS=1 mapeia 1GB e o loop reescrevia
        // a mesma entrada 512× (só a última 2MB por GB valia). HW real (GTX
        // 1050) congelava; QEMU (VGA dummy) nunca exercitava.
        let l3_entry = &mut l4_table[usize::from(virt.p4_index())];
        if !l3_entry.flags().contains(PageTableFlags::PRESENT) {
            let frame = alloc_mmio_frame(base);
            l3_entry.set_addr(PhysAddr::new(frame), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
        }
        let l3_virt = base + l3_entry.addr().as_u64();
        let l3_table = &mut *(l3_virt.as_mut_ptr::<PageTable>());

        // PDPTE (nível 3): aponta para o PD; SEM HUGE_PAGE.
        let pdp_entry = &mut l3_table[usize::from(virt.p3_index())];
        if !pdp_entry.flags().contains(PageTableFlags::PRESENT) {
            let frame = alloc_mmio_frame(base);
            pdp_entry.set_addr(PhysAddr::new(frame), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
        }
        let pd_virt = base + pdp_entry.addr().as_u64();
        let pd_table = &mut *(pd_virt.as_mut_ptr::<PageTable>());

        // PDE (nível 2) com HUGE_PAGE (2MB) — índice p2_index.
        let pde = &mut pd_table[usize::from(virt.p2_index())];
        let aligned_phys = phys & !((1 << 21) - 1);
        pde.set_addr(PhysAddr::new(aligned_phys),
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE
            | PageTableFlags::HUGE_PAGE | PageTableFlags::NO_CACHE
            | PageTableFlags::WRITE_THROUGH);
        x86_64::instructions::tlb::flush(virt);
        mapped += 1;
        offset += 1 << 21; // 2MB
    }
    mapped
}

fn alloc_mmio_frame(base: VirtAddr) -> u64 {
    use x86_64::structures::paging::FrameAllocator;
    let mut guard = crate::memory::GLOBAL_ALLOCATOR.lock();
    if let Some(alloc) = guard.as_mut() {
        if let Some(frame) = alloc.allocate_frame() {
            let pa = frame.start_address().as_u64();
            let v = base + pa;
            unsafe { core::ptr::write_bytes(v.as_mut_ptr::<u8>(), 0, 4096); }
            return pa;
        }
    }
    crate::slog_nano!("PAGING", "info", "No frame for MMIO mapping!");
    0
}

pub unsafe fn init_apic(info: &AcpiInfo) {
    crate::slog_nano!("APIC", "info", "Inicializando APIC...");
    println!("[APIC] Inicializando APIC...");

    let ioapic_uc = set_page_uc(0xFEC0_0000, info.phys_mem_offset);
    let lapic_uc = set_page_uc(0xFEE0_0000, info.phys_mem_offset);
    if !ioapic_uc || !lapic_uc {
        crate::slog_nano!("APIC", "warn", "set_page_uc falhou: ioapic={} lapic={} (pagina nao mapeada)", ioapic_uc, lapic_uc);
    }
    crate::slog_nano!("APIC", "info", "IOAPIC/LAPIC pages mapped uncacheable.");

    // SVR deve ser escrito IMEDIATAMENTE após mapear as páginas,
    // ANTES de habilitar x2APIC ou qualquer outra operação APIC.
    // Caso contrário, interrupções espúrias podem chegar com vetor 0
    // (valor padrão de reset) causando "Ignoring request for interrupt vector 0" no QEMU.
    let lapic_virt_base = info.lapic_base + info.phys_mem_offset;
    LAPIC_VIRT_BASE.store(lapic_virt_base, Ordering::Release);
    let svr_early = read_volatile((lapic_virt_base + LAPIC_SVR) as *const u32);
    let svr_fixed_early = (svr_early & 0xFFFFFF00) | 0xFF | 0x100;
    write_volatile((lapic_virt_base + LAPIC_SVR) as *mut u32, svr_fixed_early);
    crate::slog_nano!("APIC", "info", "SVR set early: {:#x}", svr_fixed_early);

    let _msr_base = read_lapic_base_msr();

    let mut x2apic_supported = false;
    #[cfg(target_arch = "x86_64")]
    {
        let result = core::arch::x86_64::__cpuid(0x0000_0001);
        if (result.ecx & (1 << 21)) != 0 {
            x2apic_supported = true;
        }
    }

    let lapic = Lapic::new(if x2apic_supported { 0 } else { lapic_virt_base });
    if x2apic_supported {
        // Enable x2APIC: set IA32_APIC_BASE[11] (EXTD) + [10] (EN).
        // SESSÃO_262: antes só setava bit 10 (EN) — se o firmware deixou o BSP
        // em xAPIC, o hardware continuava em xAPIC mas USING_X2APIC=true → os
        // IPIs iam para o MSR 0x830 (no-op em xAPIC) → INIT/SIPI nunca saíam
        // (0 APs acordavam no metal). Bit 11 = EXTD habilita x2APIC de verdade.
        let apic_base = x86_64::registers::model_specific::Msr::new(IA32_APIC_BASE_MSR).read();
        let was_x2 = (apic_base & (1 << 11)) != 0;
        x86_64::registers::model_specific::Msr::new(IA32_APIC_BASE_MSR)
            .write(apic_base | (1 << 10) | (1 << 11));
        USING_X2APIC.store(true, Ordering::Release);
        crate::boot_logger::log(&alloc::format!(
            "APIC: x2APIC ativado (era_x2={} base_msr={:#x})",
            was_x2, apic_base
        ));
        crate::slog_nano!("APIC", "info", "x2APIC ativado via MSR (era_x2={}).", was_x2);
    }
    lapic.init();

    disable_pic();
    pit_init();

    let ioapic_virt_base = info.ioapic_base + info.phys_mem_offset;
    let ioapic = IoApic::new(ioapic_virt_base);
    ioapic.init(&info.iso_overrides);

    lapic.start_timer();

    USING_APIC.store(true, Ordering::Release);
    // STI adiado para depois de init_smp — ver neural-kernel SESSION_139.
    crate::slog_nano!("APIC", "info", "APIC operacional. x2APIC={} (STI deferred)", x2apic_supported);
}

/// Lê registrador LAPIC (compatível xAPIC/x2APIC)
pub unsafe fn lapic_read_reg(reg: u64) -> u32 {
    if USING_X2APIC.load(Ordering::Relaxed) {
        x86_64::registers::model_specific::Msr::new(lapic_msr(reg)).read() as u32
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        read_volatile((base + reg) as *const u32)
    }
}

/// Escreve registrador LAPIC (compatível xAPIC/x2APIC)
unsafe fn lapic_write_reg(reg: u64, value: u32) {
    if USING_X2APIC.load(Ordering::Relaxed) {
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(reg));
        msr.write(value as u64);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + reg) as *mut u32, value);
    }
}

/// Espera ICR idle (bit 12). Timeout evita hang eterno em HW real.
/// Em x2APIC o bit 12 é reserved/0 — retorna imediato.
pub(crate) unsafe fn icr_wait_idle() {
    if USING_X2APIC.load(Ordering::Relaxed) {
        return;
    }
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    for _ in 0..2_000_000u32 {
        if (read_volatile((base + LAPIC_ICR_LOW) as *const u32) & (1 << 12)) == 0 {
            return;
        }
        core::hint::spin_loop();
    }
    crate::slog_nano!("SMP", "warn", "ICR delivery timeout — continue BSP");
}

pub unsafe fn apic_eoi() {
    if USING_X2APIC.load(Ordering::Relaxed) {
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_EOI));
        msr.write(0);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_EOI) as *mut u32, 0);
    }
}

pub unsafe fn send_init_ipi() {
    icr_wait_idle();

    if USING_X2APIC.load(Ordering::Relaxed) {
        // x2APIC: ICR 64-bit, delivery=INIT(5), shorthand=all_excl_self(0x180000)
        let icr_val: u64 = (5u64 << 8) | (3u64 << 18);
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
        msr.write(icr_val);
        crate::slog_nano!("SMP", "info", "INIT IPI (x2APIC, ICR={:#x})", icr_val);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
        let icr_val = (5u32 << 8) | (1 << 14) | (1 << 15) | (3 << 18);
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
        crate::slog_nano!("SMP", "info", "INIT IPI (xAPIC, ICR=0x{:08x})", icr_val);
    }
}

pub unsafe fn send_init_deassert_ipi() {
    icr_wait_idle();
    // SESSION_275: deassert = level=1 (bit15) + assert=0 (bit14).
    // Bug antigo: ambos os bits zerados → edge+assert (re-assert em vez de deassert).
    if USING_X2APIC.load(Ordering::Relaxed) {
        let icr_val: u64 = (5u64 << 8) | (3u64 << 18) | (1u64 << 15); // level=1, assert=0
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
        msr.write(icr_val);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        let icr_val = (5u32 << 8) | (3u32 << 18) | (1u32 << 15); // level=1, assert=0
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

pub unsafe fn send_sipi(trampoline_vector: u8) {
    icr_wait_idle();

    if USING_X2APIC.load(Ordering::Relaxed) {
        let icr_val: u64 = (6u64 << 8) | (3u64 << 18) | trampoline_vector as u64;
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
        msr.write(icr_val);
        crate::slog_nano!("SMP", "info", "SIPI (x2APIC, ICR={:#x}, vetor={:#04x})", icr_val, trampoline_vector);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        let icr_val = (6u32 << 8) | (3 << 18) | trampoline_vector as u32;
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
        crate::slog_nano!("SMP", "info", "SIPI (xAPIC, ICR=0x{:08x}, vetor={:#04x})", icr_val, trampoline_vector);
    }
}

/// ADR-0057 WS-A: INIT IPI direcionado a UM LAPIC ID (sem shorthand).
/// Necessário para o wake sequencial (broadcast acorda todos ao mesmo tempo →
/// corrompem a stack compartilhada na transição de modo).
pub unsafe fn send_init_ipi_to(dest_apic: u8) {
    icr_wait_idle();
    if USING_X2APIC.load(Ordering::Relaxed) {
        let icr_val: u64 = ((dest_apic as u64) << 32) | (5u64 << 8) | (1 << 14) | (1 << 15);
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
        msr.write(icr_val);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, (dest_apic as u32) << 24);
        let icr_val = (5u32 << 8) | (1 << 14) | (1 << 15);
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

/// ADR-0057 WS-A: INIT deassert dirigido a UM LAPIC ID.
/// Sequência canônica do Linux (arch/x86/kernel/apic/ipi.c): INIT assert →
/// ~10ms → INIT deassert → ~10ms → SIPI → 200µs → SIPI. Sem o deassert,
/// alguns firmwares/CPUs reais (Kaby Lake) mantêm o AP em wait-for-SIPI
/// travado. QEMU tolera a ausência; HW real não.
pub unsafe fn send_init_deassert_ipi_to(dest_apic: u8) {
    icr_wait_idle();
    // ADR-0057 + SESSION_275: INIT deassert REQUIRES level=1 (bit15) + assert=0 (bit14).
    // Bug antigo: bit14=1 re-asserted INIT em vez de deassert → AP nunca sai de
    // wait-for-SIPI → counter=0 em todas as tentativas. Corrigido para
    // (5 << 8) | (1 << 15) = level-triggered + deassert.
    if USING_X2APIC.load(Ordering::Relaxed) {
        let icr_val: u64 = ((dest_apic as u64) << 32) | (5u64 << 8) | (1u64 << 15);
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
        msr.write(icr_val);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, (dest_apic as u32) << 24);
        let icr_val = (5u32 << 8) | (1u32 << 15); // level=1 + assert=0 = deassert
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

/// ADR-0057 WS-A: SIPI direcionado a UM LAPIC ID (sem shorthand).
pub unsafe fn send_sipi_to(dest_apic: u8, trampoline_vector: u8) {
    icr_wait_idle();
    if USING_X2APIC.load(Ordering::Relaxed) {
        let icr_val: u64 = ((dest_apic as u64) << 32) | (6u64 << 8) | trampoline_vector as u64;
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
        msr.write(icr_val);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, (dest_apic as u32) << 24);
        let icr_val = (6u32 << 8) | trampoline_vector as u32;
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

pub unsafe fn wait_for_ipi_delivery() {
    icr_wait_idle();
}

pub fn lapic_id() -> u8 {
    if USING_X2APIC.load(Ordering::Relaxed) {
        unsafe {
            let msr = x86_64::registers::model_specific::Msr::new(0x802); // LAPIC_ID MSR
            (msr.read() >> 24) as u8
        }
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        if base == 0 { return 0; }
        unsafe {
            let id_reg = read_volatile((base + 0x20) as *const u32);
            (id_reg >> 24) as u8
        }
    }
}

/// Envia IPI de reschedule para UMA AP específica (directed, LAPIC ID).
pub unsafe fn send_ipi_reschedule_to(dest_apic: u8) {
    icr_wait_idle();
    if USING_X2APIC.load(Ordering::Relaxed) {
        // x2APIC: ICR[63:32] = dest LAPIC ID, delivery=Fixed(0), vector=0x80
        let icr_val: u64 = ((dest_apic as u64) << 32) | 0x80u64;
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
        msr.write(icr_val);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, (dest_apic as u32) << 24);
        let icr_val = 0x80u32;
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

/// Envia IPI de reschedule para todas as APs
pub unsafe fn send_ipi_reschedule() {
    icr_wait_idle();

    if USING_X2APIC.load(Ordering::Relaxed) {
        // x2APIC: ICR 64-bit, delivery=Fixed(0), shorthand=all_excl_self(0x180000), vector=0x80
        let icr_val: u64 = (3u64 << 18) | 0x80u64;
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
        msr.write(icr_val);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
        let icr_val = (0 << 8) | (1 << 14) | (1 << 15) | (3 << 18) | 0x80u32;
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

/// Envia IPI de halt para todas as APs
pub unsafe fn send_ipi_halt() {
    icr_wait_idle();

    if USING_X2APIC.load(Ordering::Relaxed) {
        // x2APIC: ICR 64-bit, delivery=Fixed(0), shorthand=all_excl_self(0x180000), vector=0x81
        let icr_val: u64 = (3u64 << 18) | 0x81u64;
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
        msr.write(icr_val);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
        let icr_val = (0 << 8) | (1 << 14) | (1 << 15) | (3 << 18) | 0x81u32;
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

/// Envia IPI de call function para todas as APs
pub unsafe fn send_ipi_call_function() {
    icr_wait_idle();

    if USING_X2APIC.load(Ordering::Relaxed) {
        // x2APIC: ICR 64-bit, delivery=Fixed(0), shorthand=all_excl_self(0x180000), vector=0x82
        let icr_val: u64 = (3u64 << 18) | 0x82u64;
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
        msr.write(icr_val);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
        let icr_val = (0 << 8) | (1 << 14) | (1 << 15) | (3 << 18) | 0x82u32;
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

/// Send End of Interrupt (EOI) to the Local APIC
/// Used by interrupt handlers to signal completion
pub unsafe fn end_of_interrupt() {
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    if USING_X2APIC.load(Ordering::Relaxed) {
        let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_EOI));
        msr.write(0);
    } else {
        write_volatile((base + LAPIC_EOI) as *mut u32, 0);
    }
}

/// Estima TIMER_HZ lendo o registrador LAPIC_CURRENT_COUNT.
/// O timer decrementa a cada ciclo do barramento APIC.
/// timer_freq = decremento * tsc_hz / (elapsed_tsc * initial_count)
/// Nao depende de TIMER_TICKS (interrupção) nem de busy-wait longo.
pub fn estimate_timer_hz(tsc_hz: u64) -> u64 {
    let initial = LAPIC_TIMER_INIT_COUNT_VAL as u64;
    if initial == 0 { return 0; }
    let target_decrement = initial / 8; // espera ~12.5% do periodo
    unsafe {
        let count_start = lapic_read_reg(LAPIC_CURRENT_COUNT) as u64;
        // RDTSC antes do loop
        let tsc_start = core::arch::x86_64::_rdtsc();
        loop {
            let count_now = lapic_read_reg(LAPIC_CURRENT_COUNT) as u64;
            if count_start.wrapping_sub(count_now) >= target_decrement { break; }
            core::hint::spin_loop();
            // timeout de seguranca: ~10ms em TSC (evita hang se timer parado)
            let tsc_now = core::arch::x86_64::_rdtsc();
            if tsc_now.wrapping_sub(tsc_start) > tsc_hz / 100 { break; }
        }
        let tsc_end = core::arch::x86_64::_rdtsc();
        let count_now = lapic_read_reg(LAPIC_CURRENT_COUNT) as u64;
        let decrement = count_start.wrapping_sub(count_now);
        let elapsed_tsc = tsc_end.wrapping_sub(tsc_start);
        if decrement > 0 && elapsed_tsc > 0 {
            // timer_freq = decrement * tsc_hz / (elapsed_tsc * initial)
            let hz = (decrement as u128)
                .saturating_mul(tsc_hz as u128)
                .checked_div((elapsed_tsc as u128).saturating_mul(initial as u128))
                .unwrap_or(0) as u64;
            return hz.min(1_000_000).max(1);
        }
    }
    0 // falha
}
