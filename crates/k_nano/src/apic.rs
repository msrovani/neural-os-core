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
/// SDM Vol 3A §10.12.9: x2APIC ICR é MSR 64-bit; bits 12–17 (status/assert/level)
/// e 18–19 (shorthand) são **reservados**. INIT deassert não existe nesse modo.
const X2APIC_DELIVERY_INIT: u8 = 5;
const X2APIC_DELIVERY_SIPI: u8 = 6;

/// Bits 12–19 do ICR x2APIC (SDM §10.12.9). Não confundir com `0x1FF00`
/// (isso cobre delivery+vector e dá falso positivo).
pub const X2APIC_ICR_RESERVED_MASK: u64 = 0x0000_0000_000F_F000;

/// ICR x2APIC canônico: dest[63:32] | delivery[10:8] | vector[7:0]. Sem bits reservados.
#[inline]
pub const fn x2apic_icr_value(dest: u32, delivery: u8, vector: u8) -> u64 {
    ((dest as u64) << 32) | ((delivery as u64) << 8) | (vector as u64)
}

fn delivery_name(d: u8) -> &'static str {
    match d {
        0 => "Fixed",
        4 => "NMI",
        5 => "INIT",
        6 => "STARTUP",
        _ => "other",
    }
}

/// Evidência arquitectural: ICR bruto + campos. Não mascara bits.
pub(crate) fn slog_icr_decoded(tag: &str, x2: bool, dest: u32, icr: u64) {
    let vector = (icr & 0xFF) as u8;
    let delivery = ((icr >> 8) & 7) as u8;
    let level = ((icr >> 14) & 1) as u8;
    let trigger = ((icr >> 15) & 1) as u8;
    let shorthand = ((icr >> 18) & 3) as u8;
    let dest_field = if x2 { (icr >> 32) as u32 } else { dest };
    let reserved = icr & X2APIC_ICR_RESERVED_MASK;
    crate::slog_nano!(
        "SMP",
        "trace",
        "{} mode={} dest={:#x} dest_field={:#x} icr={:#018x} delivery={}({}) vector={:#04x} level={} trigger={} shorthand={} reserved12_19={:#x} tsc={}",
        tag,
        if x2 { "x2APIC" } else { "xAPIC" },
        dest,
        dest_field,
        icr,
        delivery,
        delivery_name(delivery),
        vector,
        level,
        trigger,
        shorthand,
        reserved,
        crate::tsc::rdtsc()
    );
}

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
    let msr_value = read_apic_base_raw();
    let base = msr_value & 0xFFFF_FFFF_FFFF_F000;
    crate::slog_nano!("APIC", "info", "LAPIC base via MSR: 0x{:x}", base);
    base
}

pub(crate) unsafe fn read_apic_base_raw() -> u64 {
    x86_64::registers::model_specific::Msr::new(IA32_APIC_BASE_MSR).read()
}

/// Snapshot observacional (não muda modo). Bits 10=EN, 11=EXTD.
pub unsafe fn smp_trace_apic_mode(bsp_id: u32) {
    let msr = read_apic_base_raw();
    crate::slog_nano!(
        "SMP",
        "trace",
        "APIC_BASE={:#x} EN={} EXTD={} USING_X2={} BSP_APIC_ID={:#x}",
        msr,
        (msr >> 10) & 1,
        (msr >> 11) & 1,
        USING_X2APIC.load(Ordering::Relaxed) as u8,
        bsp_id
    );
}

/// Liga x2APIC **neste** CPU (MSR 0x1B EN+EXTD). EXTD é por-CPU: o BSP
/// não habilita o AP. Retorna se EXTD já estava ligado antes do write.
pub unsafe fn enable_x2apic_this_cpu() -> bool {
    let apic_base = read_apic_base_raw();
    let was_x2 = (apic_base & (1 << 11)) != 0;
    // SESSION_281: TCG/WHPX nao emulam x2APIC via MSR 0x1B como writeable.
    // wrmsr EN+EXTD da #GP no QEMU (mesmo sendo "bare-metal" reporta None).
    // Consistente com init_syscall_fast_path (paging.rs) e cpufreq (gate hv).
    let hv = crate::platform_probe::detect_hypervisor();
    if !matches!(hv, crate::platform_probe::HypervisorKind::None | crate::platform_probe::HypervisorKind::Kvm) {
        crate::slog_nano!("APIC", "warn", "x2APIC gated off (hv={:?}) — fica xAPIC MMIO", hv);
        USING_X2APIC.store(false, Ordering::Release);
        return was_x2;
    }
    x86_64::registers::model_specific::Msr::new(IA32_APIC_BASE_MSR)
        .write(apic_base | (1 << 10) | (1 << 11));
    USING_X2APIC.store(true, Ordering::Release);
    was_x2
}

unsafe fn x2apic_icr_write(val: u64) {
    let reserved = val & X2APIC_ICR_RESERVED_MASK;
    if reserved != 0 {
        crate::slog_nano!(
            "SMP",
            "error",
            "FATAL ICR x2 reserved12_19={:#x} icr={:#018x} — nao mascara, nao WRMSR",
            reserved,
            val
        );
        loop {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
    // SESSION_281: x2APIC ICR via MSR 0x830 nao emulado em TCG/WHPX -> #GP.
    let hv = crate::platform_probe::detect_hypervisor();
    if !matches!(hv, crate::platform_probe::HypervisorKind::None | crate::platform_probe::HypervisorKind::Kvm) {
        crate::slog_nano!("SMP", "warn", "x2APIC ICR gated off (hv={:?}) — fallback MMIO", hv);
        return;
    }
    let mut msr = x86_64::registers::model_specific::Msr::new(lapic_msr(LAPIC_ICR_LOW));
    msr.write(val);
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
        // 1GB huge page: add NO_CACHE to the entire 1GB region
        let mut f = l3_entry.flags(); f.insert(PageTableFlags::NO_CACHE); f.insert(PageTableFlags::WRITE_THROUGH); l3_entry.set_flags(f);
        x86_64::instructions::tlb::flush(virt); return;
    }
    // Also: if L3 is present but NOT huge page, check if L2 will be huge
    // This is the normal 4KB page walk path — continue to L2
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

    let apic_base_now = read_apic_base_raw();
    let firmware_x2 = (apic_base_now & (1 << 11)) != 0;
    // SESSION_281: sob hypervisor (TCG/WHPX) o MSR x2APIC não é emulado como
    // writeable — se o firmware reporta EXTD=1 mas o QEMU não suporta, wrmsr daria
    // #GP. Sempre checar hypervisor mesmo quando firmware_x2 (detect via CPUID direto).
    let hv = crate::platform_probe::detect_hypervisor();
    let hv_allows_x2 = matches!(hv, crate::platform_probe::HypervisorKind::None | crate::platform_probe::HypervisorKind::Kvm);
    // MMIO 0xFEE00000 é #GP se o firmware já deixou EXTD=1 (240H comum).
    if firmware_x2 && hv_allows_x2 {
        USING_X2APIC.store(true, Ordering::Release);
        crate::slog_nano!("APIC", "info", "firmware já em x2APIC — skip SVR MMIO");
    } else if firmware_x2 && !hv_allows_x2 {
        // fica xAPIC MMIO apesar do firmware ter EXTD — evita #GP no wrmsr.
        USING_X2APIC.store(false, Ordering::Release);
        crate::slog_nano!("APIC", "warn", "firmware x2APIC mas hv={:?} — forca xAPIC MMIO (sem #GP)", hv);
    } else {
        let svr_early = read_volatile((lapic_virt_base + LAPIC_SVR) as *const u32);
        let svr_fixed_early = (svr_early & 0xFFFFFF00) | 0xFF | 0x100;
        write_volatile((lapic_virt_base + LAPIC_SVR) as *mut u32, svr_fixed_early);
        crate::slog_nano!("APIC", "info", "SVR set early: {:#x}", svr_fixed_early);
    }

    let mut x2apic_supported = firmware_x2 && hv_allows_x2;
    #[cfg(target_arch = "x86_64")]
    {
        let result = core::arch::x86_64::__cpuid(0x0000_0001);
        if (result.ecx & (1 << 21)) != 0 {
            x2apic_supported = true;
        }
    }
    x2apic_supported &= hv_allows_x2; // SESSION_281: gate hypervisor tambem no CPUID.

    let lapic = Lapic::new(if x2apic_supported { 0 } else { lapic_virt_base });
    if x2apic_supported {
        // Enable x2APIC neste CPU: EN (10) + EXTD (11). ICR daqui pra frente
        // usa x2apic_icr_value — bits 14/15 no MSR 0x830 = #GP no Kaby Lake.
        let was_x2 = enable_x2apic_this_cpu();
        crate::boot_logger::log(&alloc::format!(
            "APIC: x2APIC ativado (era_x2={} base_msr={:#x})",
            was_x2, apic_base_now
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
pub unsafe fn lapic_write_reg(reg: u64, value: u32) {
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
    crate::slog_nano!("SMP", "trace", "WAIT_IDLE enter (xAPIC bit12)");
    for n in 0..2_000_000u32 {
        if (read_volatile((base + LAPIC_ICR_LOW) as *const u32) & (1 << 12)) == 0 {
            if n > 0 {
                crate::slog_nano!("SMP", "trace", "WAIT_IDLE done spins={}", n);
            }
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
        crate::slog_nano!("SMP", "warn", "INIT broadcast ignorado em x2APIC (use dirigido)");
        return;
    }
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
    let icr_val = (5u32 << 8) | (1 << 14) | (1 << 15) | (3 << 18);
    write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    crate::slog_nano!("SMP", "info", "INIT IPI (xAPIC, ICR=0x{:08x})", icr_val);
}

pub unsafe fn send_init_deassert_ipi() {
    if USING_X2APIC.load(Ordering::Relaxed) {
        return;
    }
    icr_wait_idle();
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    let icr_val = (5u32 << 8) | (3u32 << 18) | (1u32 << 15);
    write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
    write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
}

pub unsafe fn send_sipi(trampoline_vector: u8) {
    icr_wait_idle();
    if USING_X2APIC.load(Ordering::Relaxed) {
        crate::slog_nano!("SMP", "warn", "SIPI broadcast ignorado em x2APIC (use dirigido)");
        return;
    }
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    let icr_val = (6u32 << 8) | (3 << 18) | trampoline_vector as u32;
    write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, 0);
    write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
}

/// ADR-0057 WS-A: INIT IPI direcionado a UM LAPIC ID (sem shorthand).
pub unsafe fn send_init_ipi_to(dest_apic: u32) {
    icr_wait_idle();
    if USING_X2APIC.load(Ordering::Relaxed) {
        let v = x2apic_icr_value(dest_apic, X2APIC_DELIVERY_INIT, 0);
        slog_icr_decoded("INIT_ASSERT", true, dest_apic, v);
        x2apic_icr_write(v);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        let icr_val = (5u32 << 8) | (1 << 14) | (1 << 15);
        slog_icr_decoded(
            "INIT_ASSERT",
            false,
            dest_apic,
            (dest_apic as u64) << 32 | icr_val as u64,
        );
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, dest_apic << 24);
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

/// INIT deassert só em xAPIC (Kaby Lake). x2APIC: no-op (SDM).
pub unsafe fn send_init_deassert_ipi_to(dest_apic: u32) {
    if USING_X2APIC.load(Ordering::Relaxed) {
        return;
    }
    icr_wait_idle();
    let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
    write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, dest_apic << 24);
    let icr_val = (5u32 << 8) | (1u32 << 15);
    slog_icr_decoded(
        "INIT_DEASSERT",
        false,
        dest_apic,
        (dest_apic as u64) << 32 | icr_val as u64,
    );
    write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
}

/// ADR-0057 WS-A: SIPI direcionado a UM LAPIC ID (sem shorthand).
pub unsafe fn send_sipi_to(dest_apic: u32, trampoline_vector: u8) {
    icr_wait_idle();
    if USING_X2APIC.load(Ordering::Relaxed) {
        let v = x2apic_icr_value(dest_apic, X2APIC_DELIVERY_SIPI, trampoline_vector);
        slog_icr_decoded("SIPI", true, dest_apic, v);
        x2apic_icr_write(v);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        let icr_val = (6u32 << 8) | trampoline_vector as u32;
        slog_icr_decoded(
            "SIPI",
            false,
            dest_apic,
            (dest_apic as u64) << 32 | icr_val as u64,
        );
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, dest_apic << 24);
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

pub unsafe fn wait_for_ipi_delivery() {
    icr_wait_idle();
}

pub fn lapic_id() -> u32 {
    if USING_X2APIC.load(Ordering::Relaxed) {
        unsafe {
            let msr = x86_64::registers::model_specific::Msr::new(0x802);
            // SDM: em x2APIC o ID é o valor inteiro de 32 bits (não bits 31:24).
            msr.read() as u32
        }
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        if base == 0 {
            return 0;
        }
        unsafe {
            let id_reg = read_volatile((base + 0x20) as *const u32);
            id_reg >> 24
        }
    }
}

/// Envia IPI de reschedule para UMA AP específica (directed, LAPIC ID).
pub unsafe fn send_ipi_reschedule_to(dest_apic: u32) {
    icr_wait_idle();
    if USING_X2APIC.load(Ordering::Relaxed) {
        let icr_val = x2apic_icr_value(dest_apic, 0, 0x80);
        x2apic_icr_write(icr_val);
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, dest_apic << 24);
        let icr_val = 0x80u32;
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, icr_val);
    }
}

/// Envia IPI de reschedule para todas as APs (dirigido — shorthand ilegal em x2APIC).
pub unsafe fn send_ipi_reschedule() {
    ipi_all_aps_fixed(0x80);
}

/// Envia IPI de halt para todas as APs
pub unsafe fn send_ipi_halt() {
    ipi_all_aps_fixed(0x81);
}

unsafe fn ipi_all_aps_fixed(vector: u8) {
    let bsp = lapic_id();
    let ids = crate::acpi::BOOT_APIC_IDS.lock();
    for &id in ids.iter() {
        if id == bsp {
            continue;
        }
        send_ipi_vector_to(id, vector);
    }
}

unsafe fn send_ipi_vector_to(dest_apic: u32, vector: u8) {
    icr_wait_idle();
    if USING_X2APIC.load(Ordering::Relaxed) {
        x2apic_icr_write(x2apic_icr_value(dest_apic, 0, vector));
    } else {
        let base = LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        write_volatile((base + LAPIC_ICR_HIGH) as *mut u32, dest_apic << 24);
        write_volatile((base + LAPIC_ICR_LOW) as *mut u32, vector as u32);
    }
}

/// Envia IPI de call function para todas as APs
pub unsafe fn send_ipi_call_function() {
    ipi_all_aps_fixed(0x82);
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

#[cfg(test)]
mod x2apic_icr_tests {
    use super::*;

    #[test]
    fn icr_init_has_no_reserved_bits() {
        let v = x2apic_icr_value(4, X2APIC_DELIVERY_INIT, 0);
        assert_eq!(v >> 32, 4);
        assert_eq!((v >> 8) & 7, 5);
        assert_eq!(v & 0xFF, 0);
        // bits 12–19 must be 0 (SDM reserved + no shorthand)
        assert_eq!(v & X2APIC_ICR_RESERVED_MASK, 0);
        assert_ne!(v & 0x1FF00, 0, "0x1FF00 nao e mascara reserved (entrega INIT=0x500)");
    }

    #[test]
    fn icr_sipi_vector_in_low_byte() {
        let v = x2apic_icr_value(0x11, X2APIC_DELIVERY_SIPI, 0x08);
        assert_eq!(v & 0xFF, 0x08);
        assert_eq!((v >> 8) & 7, 6);
        assert_eq!(v & X2APIC_ICR_RESERVED_MASK, 0);
    }
}
