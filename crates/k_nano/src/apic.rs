use crate::acpi::AcpiInfo;
use crate::{println};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use core::ptr::{read_volatile, write_volatile};
use x86_64::structures::paging::{PageTable, PageTableFlags};
use x86_64::VirtAddr;

pub static USING_APIC: AtomicBool = AtomicBool::new(false);
pub static USING_X2APIC: AtomicBool = AtomicBool::new(false);
pub static LAPIC_VIRT_BASE: AtomicU64 = AtomicU64::new(0);
/// Log do IA32_PAT uma única vez (init_pat é chamado por página WC).
static PAT_LOGGED: AtomicBool = AtomicBool::new(false);

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
/// Valor fixo de arranque programado em `start_timer()`. É apenas o INIT
/// **provisório** usado para medir a taxa real do LAPIC; `init_apic` reescreve
/// o INIT para `TICK_HZ_DEFAULT` logo depois (ver `calibrate_lapic_timer`).
const LAPIC_TIMER_INIT_COUNT_VAL: u32 = 0x800000;

/// Cadência-alvo segura (Hz). ADR-0104: as rails [30,60,120] vivem em
/// `k_hal::timer_cap`; R0 só conhece o default e os limites do silício.
pub const TICK_HZ_DEFAULT: u64 = 60;
/// Menor cadência que mantém a UI viva.
pub const TICK_HZ_MIN: u64 = 30;
/// Maior cadência permitida pelo timer LAPIC.
pub const TICK_HZ_MAX: u64 = 240;
/// Piso do `INIT_COUNT`: abaixo disso o timer dispara rápido demais.
pub const INIT_FLOOR: u32 = 1000;

/// Cadência-alvo efetiva (Hz). Fonte única do timer — escrita em runtime SÓ
/// por [`set_tick_hz`] (o único ponto de mutação R0) e pela calibração inicial.
pub static TICK_TARGET_HZ: AtomicU64 = AtomicU64::new(TICK_HZ_DEFAULT);

/// INIT_COUNT **efetivamente programado** no LAPIC. Lido por
/// `rearm_lapic_timer` a cada tick, então precisa ser o valor calibrado — não
/// a constante fixa de arranque. Atualizado por `calibrate_lapic_timer`.
pub static LAPIC_TIMER_INIT_COUNT: AtomicU32 = AtomicU32::new(LAPIC_TIMER_INIT_COUNT_VAL);

/// Taxa de decremento do LAPIC (counts/s) medida na última calibração.
/// 0 = medição falhou (INIT fixo mantido). Só para diagnóstico.
pub static LAST_MEASURED_LAPIC_HZ: AtomicU64 = AtomicU64::new(0);

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
        // SESSION_310: set divide BEFORE timer mode to avoid glitch in TCG.
        self.write(LAPIC_DIVIDE_CONFIG, 0b1011);
        self.write(LAPIC_LVT_TIMER, 32 | 0x20000);
        // INIT provisório: só para poder medir a taxa real em seguida.
        let init = LAPIC_TIMER_INIT_COUNT.load(Ordering::Relaxed);
        self.write(LAPIC_INIT_COUNT, init);

        crate::slog_nano!("APIC", "info", "LAPIC timer iniciado: vetor 32, count={}, div=16.", init);
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
    // SESSION_328: o write pode ser ignorado silenciosamente (firmware/BIOS
    // deixa EXTD=0 ou o MSR não é writeable). Sem read-back, USING_X2APIC
    // latched true e TODA acesso LAPIC (SVR/LVT/INIT/EOI) ia por MSR com a
    // LAPIC real em MMIO → timer nunca armado (TIMER_TICKS=0 no metal).
    let readback = read_apic_base_raw();
    let en = (readback >> 10) & 1 != 0;
    let extd = (readback >> 11) & 1 != 0;
    let ok = en && extd;
    USING_X2APIC.store(ok, Ordering::Release);
    crate::slog_nano!(
        "APIC",
        if ok { "ok" } else { "warn" },
        "x2APIC readback base={:#x} EN={} EXTD={} USING_X2APIC={}",
        readback,
        en as u8,
        extd as u8,
        ok as u8
    );
    was_x2
}

/// SESSION_328 — diagnóstico do timer LAPIC no metal (TIMER_TICKS=0). Publica
/// no canal de boot (slog visível `ok`/`warn` + ramlog/BOOT.LOG + FB stamp se o
/// DisplayAgent já registrou o bridge). Serial é invisível no metal; FB/BOOT.LOG
/// não. Lê o MSR base, o modo verificado e os read-backs SVR/LVT/INIT/CUR (CUR
/// duas vezes para mostrar o contador a andar).
pub unsafe fn lapic_timer_diag() {
    let msr = read_apic_base_raw();
    let en = (msr >> 10) & 1;
    let extd = (msr >> 11) & 1;
    let x2 = USING_X2APIC.load(Ordering::Acquire) as u8;
    let svr = lapic_read_reg(LAPIC_SVR);
    let lvt = lapic_read_reg(LAPIC_LVT_TIMER);
    let init = lapic_read_reg(LAPIC_INIT_COUNT);
    let cur1 = lapic_read_reg(LAPIC_CURRENT_COUNT);
    let cur2 = lapic_read_reg(LAPIC_CURRENT_COUNT);
    let measured = LAST_MEASURED_LAPIC_HZ.load(Ordering::Relaxed);
    let chosen = LAPIC_TIMER_INIT_COUNT.load(Ordering::Relaxed);
    let line = alloc::format!(
        "APICDIAG base={:#x} EN={} EXTD={} x2={} SVR={:#x} LVT={:#x} INIT={:#x} CUR={:#x}/{:x} MEAS={} TARGET={} CHOSEN={:#x}",
        msr, en, extd, x2, svr, lvt, init, cur1, cur2, measured, TICK_TARGET_HZ.load(Ordering::Relaxed), chosen
    );
    crate::slog_nano!("APIC", if x2 == 1 { "ok" } else { "warn" }, "{}", line);
    // Persistência boot (BOOT.LOG/ramlog) — sem eco duplicado no serial.
    crate::boot_logger::log_quiet(&line);
    // Canal FB existente (bridge do DisplayAgent; no-op até registrar).
    let b = line.as_bytes();
    let n = b.len().min(200);
    let mut buf = [0u8; 200];
    buf[..n].copy_from_slice(&b[..n]);
    crate::interrupts::exception_fb_stamp(&buf[..n]);
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

/// IA32_PAT (MSR 0x277) — Programmable Attribute Table.
///
/// O default de power-on (`0x0007_0406_0007_0406`) tem as 4 primeiras entradas
/// WB/WT/UC-/UC e repete o mesmo nas 4 seguintes — **não há entrada WC
/// utilizável por página 4KB** (a entrada 4 é a única alcançável pelo bit PAT
/// de um PTE). Para `map_page_wc` (PTE bit 7 = PAT, PCD=PWT=0) a entrada 4
/// precisa ser `0x01` (WC). Read-modify-write preserva as outras 7 entradas.
///
/// PAT é **por logical processor** (é MSR, não page-table): cada CPU que
/// acesse a página WC precisa do seu próprio WRMSR. Esta função é idempotente
/// e roda no boot BSP (via `init_apic`/`fb_remap_wc`). Os APs NÃO passam por
/// este caminho hoje — se um AP for escrever no framebuffer, o `ap_entry`
/// precisa chamar `init_pat()` também (fora do escopo desta lane).
///
/// Retorna `true` se a entrada 4 ficou WC (verificado por read-back).
pub fn init_pat() -> bool {
    use x86_64::registers::model_specific::Msr;
    const IA32_PAT: u32 = 0x277;
    let mut msr = Msr::new(IA32_PAT);
    let cur = unsafe { msr.read() };
    // entry 4 = bits 32..39 = WC (0x01); preserva 0..3 e 5..7.
    let new = (cur & !(0xFFu64 << 32)) | (0x01u64 << 32);
    unsafe { msr.write(new); }
    let back = unsafe { msr.read() };
    let ok = (back >> 32) & 0xFF == 0x01;
    if !PAT_LOGGED.swap(true, Ordering::Relaxed) {
        if ok {
            crate::slog_nano!("PAGING", "ok", "PAT entry4=WC (0x{:016x} -> 0x{:016x})", cur, back);
        } else {
            crate::slog_nano!("PAGING", "warn", "PAT entry4=WC falhou (rd=0x{:016x}) — FB fica UC", back);
        }
    }
    ok
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

/// Marca uma entrada de huge page (PDE 2MB ou PDPTE 1GB) como
/// Write-Combining. Em huge pages o bit PAT é o **bit 12** (o bit 7 é PS).
unsafe fn set_huge_entry_wc(
    entry: &mut x86_64::structures::paging::page_table::PageTableEntry,
) {
    let mut raw = entry.flags().bits();
    raw &= !(PageTableFlags::NO_CACHE.bits() | PageTableFlags::WRITE_THROUGH.bits());
    raw |= 1u64 << 12; // PAT em huge page
    entry.set_flags(PageTableFlags::from_bits_retain(raw));
}

/// Mapa uma página 4KB como **Write-Combining** (WC), criando L4→L1.
///
/// WC = PAT entry 4 (PTE bit 7 = PAT, PCD=PWT=0). Exige `init_pat()`;
/// esta função chama `init_pat()` (idempotente) por invocação. Retorna
/// `true` se a página ficou presente; `false` se faltou frame para page
/// table — o chamador deve cair para `map_page_uc` (UC continua correto,
/// só mais lento no present).
///
/// ⚠️ Ganho de WC/NT é **metal-only**: QEMU/RAM é WB comum e o hint
/// non-temporal é ignorado — não dá para medir em QEMU.
pub unsafe fn map_page_wc(phys_addr: u64, phys_mem_offset: u64) -> bool {
    map_page_wc_at(phys_addr + phys_mem_offset, phys_addr, phys_mem_offset)
}

/// `map_page_wc` com VA de destino explícito (mesmo walk de `map_page_uc_at`).
pub unsafe fn map_page_wc_at(virt_addr: u64, phys_addr: u64, phys_mem_offset: u64) -> bool {
    use x86_64::structures::paging::PageTable;
    use x86_64::VirtAddr;
    use x86_64::PhysAddr;

    init_pat();

    let virt = VirtAddr::new(virt_addr);
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let base = VirtAddr::new(phys_mem_offset);
    let l4_virt = base + l4_frame.start_address().as_u64();
    let l4_table = &mut *(l4_virt.as_mut_ptr::<PageTable>());

    // L4 → L3
    let l3_entry = &mut l4_table[usize::from(virt.p4_index())];
    if !l3_entry.flags().contains(PageTableFlags::PRESENT) {
        let frame = alloc_mmio_frame(base);
        if frame == 0 { return false; }
        l3_entry.set_addr(PhysAddr::new(frame), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    } else if l3_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        set_huge_entry_wc(l3_entry); // 1GB: PAT bit 12
        x86_64::instructions::tlb::flush(virt);
        return true;
    }
    let l3_virt = base + l3_entry.addr().as_u64();
    let l3_table = &mut *(l3_virt.as_mut_ptr::<PageTable>());

    // L3 → L2
    let l2_entry = &mut l3_table[usize::from(virt.p3_index())];
    if !l2_entry.flags().contains(PageTableFlags::PRESENT) {
        let frame = alloc_mmio_frame(base);
        if frame == 0 { return false; }
        l2_entry.set_addr(PhysAddr::new(frame), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    } else if l2_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        set_huge_entry_wc(l2_entry); // 1GB: PAT bit 12
        x86_64::instructions::tlb::flush(virt);
        return true;
    }
    let l2_virt = base + l2_entry.addr().as_u64();
    let l2_table = &mut *(l2_virt.as_mut_ptr::<PageTable>());

    // L2 → L1
    let l1_entry = &mut l2_table[usize::from(virt.p2_index())];
    if !l1_entry.flags().contains(PageTableFlags::PRESENT) {
        let frame = alloc_mmio_frame(base);
        if frame == 0 { return false; }
        l1_entry.set_addr(PhysAddr::new(frame), PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    } else if l1_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        set_huge_entry_wc(l1_entry); // 2MB: PAT bit 12
        x86_64::instructions::tlb::flush(virt);
        return true;
    }
    let l1_virt = base + l1_entry.addr().as_u64();
    let l1_table = &mut *(l1_virt.as_mut_ptr::<PageTable>());

    // L1 → folha 4KB: bit 7 = PAT (na folha, NÃO é huge), PCD/PWT = 0.
    let pte = &mut l1_table[usize::from(virt.p1_index())];
    pte.set_addr(PhysAddr::new(phys_addr),
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
    let mut raw = pte.flags().bits();
    raw &= !(PageTableFlags::NO_CACHE.bits() | PageTableFlags::WRITE_THROUGH.bits());
    raw |= 1u64 << 7; // PAT em PTE de 4KB
    pte.set_flags(PageTableFlags::from_bits_retain(raw));

    x86_64::instructions::tlb::flush(virt);
    true
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

    // PAT entry 4 = WC (pré-requisito do FB Write-Combining). BSP boot path.
    init_pat();

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

    // Candidato a x2APIC (firmware já EXTD ou CPUID bit21), gated por hypervisor.
    // NÃO liga USING_X2APIC — só dispara a tentativa de enable (read-back abaixo).
    let mut x2apic_candidate = firmware_x2 && hv_allows_x2;
    #[cfg(target_arch = "x86_64")]
    {
        let result = core::arch::x86_64::__cpuid(0x0000_0001);
        if (result.ecx & (1 << 21)) != 0 {
            x2apic_candidate = true;
        }
    }
    x2apic_candidate &= hv_allows_x2; // SESSION_281: gate hypervisor tambem no CPUID.

    // SESSION_328: USING_X2APIC é derivado do READ-BACK verificado em
    // enable_x2apic_this_cpu — nunca do flag pré-latchado. Se o write não
    // confirmar EN+EXTD, opera a LAPIC por MMIO para TODO o init (SVR early +
    // Lapic com base virtual), senão todo acesso MSR vai a lugar nenhum e o
    // timer nunca arma (TIMER_TICKS=0 no metal).
    let lapic = if x2apic_candidate {
        let was_x2 = enable_x2apic_this_cpu();
        crate::boot_logger::log(&alloc::format!(
            "APIC: x2APIC tentativa (era_x2={} base_msr={:#x})",
            was_x2, apic_base_now
        ));
        if USING_X2APIC.load(Ordering::Acquire) {
            Lapic::new(0)
        } else {
            let svr_early = read_volatile((lapic_virt_base + LAPIC_SVR) as *const u32);
            let svr_fixed_early = (svr_early & 0xFFFFFF00) | 0xFF | 0x100;
            write_volatile((lapic_virt_base + LAPIC_SVR) as *mut u32, svr_fixed_early);
            crate::slog_nano!("APIC", "warn", "x2APIC nao confirmado — SVR MMIO fallback {:#x}", svr_fixed_early);
            Lapic::new(lapic_virt_base)
        }
    } else {
        // MMIO 0xFEE00000 é #GP se o firmware já deixou EXTD=1 (240H comum);
        // hv gated acima garante que este path só roda em xAPIC.
        USING_X2APIC.store(false, Ordering::Release);
        let svr_early = read_volatile((lapic_virt_base + LAPIC_SVR) as *const u32);
        let svr_fixed_early = (svr_early & 0xFFFFFF00) | 0xFF | 0x100;
        write_volatile((lapic_virt_base + LAPIC_SVR) as *mut u32, svr_fixed_early);
        crate::slog_nano!("APIC", "info", "SVR set early: {:#x}", svr_fixed_early);
        Lapic::new(lapic_virt_base)
    };
    lapic.init();

    disable_pic();
    pit_init();

    let ioapic_virt_base = info.ioapic_base + info.phys_mem_offset;
    let ioapic = IoApic::new(ioapic_virt_base);
    ioapic.init(&info.iso_overrides);

    lapic.start_timer();
    // SESSION_330/ADR-0104: mede a taxa real do contador LAPIC e reescreve
    // INIT_COUNT para a cadência-alvo de `TICK_TARGET_HZ`. Fallback: mantém o
    // INIT fixo (sem regressão). `calibrate_lapic_timer` já publica TIMER_HZ.
    let (meas_hz, _init) = calibrate_lapic_timer();
    if meas_hz > 0 {
        crate::interrupts::TIMER_HZ.store(tick_hz(), Ordering::Relaxed);
    }
    // SESSION_310: PIT channel 0 → IOAPIC GSI 0 → vec32 (backup timer)
    crate::slog_nano!("APIC", "info", "LAPIC timer started + PIT→IOAPIC GSI0→vec32 (SESSION_310)");
    lapic_timer_diag();

    USING_APIC.store(true, Ordering::Release);
    // STI adiado para depois de init_smp — ver neural-kernel SESSION_139.
    crate::slog_nano!(
        "APIC",
        if USING_X2APIC.load(Ordering::Acquire) { "ok" } else { "warn" },
        "APIC operacional. x2APIC={} (STI deferred)",
        USING_X2APIC.load(Ordering::Acquire) as u8
    );
}

/// Lê registrador LAPIC (compatível xAPIC/x2APIC)
/// SESSION_310: rearm LAPIC timer — QEMU TCG periodic auto-reload may fail.
/// Simply rewrite INIT_COUNT to reload the counter.
pub static REARM_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
pub unsafe fn rearm_lapic_timer() {
    REARM_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    // In periodic mode, writing INIT_COUNT reloads the counter. Usa o valor
    // calibrado — a constante fixa de arranque destrói a cadência de 60 Hz.
    lapic_write_reg(
        LAPIC_INIT_COUNT,
        LAPIC_TIMER_INIT_COUNT.load(Ordering::Relaxed),
    );
}

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

/// Mede a **taxa de decremento do LAPIC em counts/s** (já com o divide
/// aplicado) lendo `CURRENT_COUNT` sobre um intervalo de TSC.
///
/// `counts_per_sec = Δcount * tsc_hz / Δtsc`. NÃO divide pelo INIT: o retorno é
/// a taxa do contador do barramento, não a frequência de interrupção. Para
/// achar o INIT que produz `freq` Hz basta `counts_per_sec / freq` (ver
/// `calibrate_lapic_timer`). Usa o INIT corrente só para dimensionar a amostra;
/// timeout de ~10 ms evita hang se o timer estiver parado. Retorna 0 se falhar.
pub fn estimate_timer_hz(tsc_hz: u64) -> u64 {
    let initial = LAPIC_TIMER_INIT_COUNT.load(Ordering::Relaxed) as u64;
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
            // counts/s = decrement * tsc_hz / elapsed_tsc
            let hz = (decrement as u128)
                .saturating_mul(tsc_hz as u128)
                .checked_div(elapsed_tsc as u128)
                .unwrap_or(0) as u64;
            return hz.min(1_000_000_000).max(1);
        }
    }
    0 // falha
}

/// PROVA (SESSION_330): mede a taxa real do LAPIC e reprograma `INIT_COUNT`
/// para a cadência-alvo corrente (`TICK_TARGET_HZ`, default 60 Hz). Deve ser
/// chamada logo após `start_timer()`, com o LVT em modo periódico. Se a medição
/// falhar (0), mantém o INIT corrente (fallback fixo) e retorna 0 — QEMU/TCG
/// que não exponham `CURRENT_COUNT` continuam funcionando sem regressão.
/// Retorna `(measured_counts_s, init)`. Fonte única de TIMER_HZ.
pub unsafe fn calibrate_lapic_timer() -> (u64, u32) {
    let tsc_hz = crate::interrupts::estimate_tsc_hz();
    let measured = estimate_timer_hz(tsc_hz);
    LAST_MEASURED_LAPIC_HZ.store(measured, Ordering::Relaxed);
    if measured == 0 {
        let init = LAPIC_TIMER_INIT_COUNT.load(Ordering::Relaxed);
        crate::slog_nano!(
            "APIC",
            "warn",
            "LAPIC timer calibracao falhou (CURRENT_COUNT=0) — INIT fixo {:#x}",
            init
        );
        return (0, init);
    }
    let target = TICK_TARGET_HZ.load(Ordering::Relaxed).clamp(TICK_HZ_MIN, TICK_HZ_MAX);
    // counts_per_sec / alvo = counts por periodo (1 interrupcao a cada INIT).
    let init = (measured / target).clamp(INIT_FLOOR as u64, u32::MAX as u64) as u32;
    LAPIC_TIMER_INIT_COUNT.store(init, Ordering::Relaxed);
    lapic_write_reg(LAPIC_INIT_COUNT, init);
    TICK_TARGET_HZ.store(target, Ordering::Relaxed);
    crate::interrupts::TIMER_HZ.store(target, Ordering::Relaxed);
    crate::slog_nano!(
        "APIC",
        "ok",
        "LAPIC timer calibrado: meas={} counts/s tsc={} Hz target={} Hz INIT={:#x}",
        measured,
        tsc_hz,
        target,
        init
    );
    (measured, init)
}

/// Cadência-alvo corrente do timer (Hz). Fonte única do scheduler/compositor.
#[inline]
pub fn tick_hz() -> u64 {
    TICK_TARGET_HZ.load(Ordering::Relaxed)
}

/// ADR-0104 — **o único ponto de mutação da cadência do timer** (R0 programa;
/// R1 sanciona a banda). Reprograma `INIT_COUNT` do LAPIC a partir da taxa
/// medida. Retorna a cadência efetiva, ou 0 se a taxa do LAPIC nunca foi medida
/// (não arma o timer — sem regressão, fica o default).
pub unsafe fn set_tick_hz(hz: u64) -> u64 {
    let hz = hz.clamp(TICK_HZ_MIN, TICK_HZ_MAX);
    let counts_hz = LAST_MEASURED_LAPIC_HZ.load(Ordering::Relaxed);
    if counts_hz == 0 {
        return 0;
    }
    let init = (counts_hz / hz).clamp(INIT_FLOOR as u64, u32::MAX as u64) as u32;
    LAPIC_TIMER_INIT_COUNT.store(init, Ordering::Relaxed);
    lapic_write_reg(LAPIC_INIT_COUNT, init);
    TICK_TARGET_HZ.store(hz, Ordering::Relaxed);
    crate::interrupts::TIMER_HZ.store(hz, Ordering::Relaxed);
    hz
}

/// Snapshot observacional da medição do timer (R0). Consumido por
/// `k_hal::timer_cap` para sancionar a banda e decidir `trusted`.
#[derive(Debug, Clone, Copy)]
pub struct TimerMeasured {
    /// Taxa do contador LAPIC (counts/s) — 0 = não medido.
    pub lapic_counts_hz: u64,
    /// TSC declarada pelo firmware (CPUID). 2 GHz = fallback não confiável.
    pub tsc_hz: u64,
    /// INIT_COUNT programado.
    pub init_count: u32,
    /// Cadência-alvo atual (Hz).
    pub tick_hz: u64,
    /// Jitter medido (ppm; 0 = desconhecido).
    pub jitter_ppm: u32,
    /// TIMER_TICKS avançou entre duas amostras.
    pub alive: bool,
    /// Medição confiável (counts≠0, TSC real, jitter≤5%, alive).
    pub trusted: bool,
}

/// Lê o snapshot atual do timer. Não bloqueia além da amostra de `alive`
/// (~2 ms no pior caso, sem timer).
pub fn timer_measured() -> TimerMeasured {
    let lapic_counts_hz = LAST_MEASURED_LAPIC_HZ.load(Ordering::Relaxed);
    let tsc_hz = crate::interrupts::estimate_tsc_hz();
    let init_count = LAPIC_TIMER_INIT_COUNT.load(Ordering::Relaxed);
    let tick_hz = TICK_TARGET_HZ.load(Ordering::Relaxed);
    let jitter_ppm = crate::interrupts::timer_jitter_ppm();
    let alive = crate::interrupts::timer_alive();
    let trusted = lapic_counts_hz != 0
        && tsc_hz != 2_000_000_000
        && jitter_ppm <= 50_000
        && alive;
    TimerMeasured {
        lapic_counts_hz,
        tsc_hz,
        init_count,
        tick_hz,
        jitter_ppm,
        alive,
        trusted,
    }
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
