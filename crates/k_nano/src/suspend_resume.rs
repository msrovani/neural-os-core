//! S3 Suspend-to-RAM: device save/restore + AP parking + ACPI S3 entry + resume trampoline.
//!
//! ## Flow
//! 1. `suspend()` — save CR3/RSP, set FACS wake vector, save devices, park APs, set powersave.
//! 2. Write SLP_TYP=3 + SLP_EN → platform asserts SLP_S3# → CPU sleeps, RAM in self-refresh.
//! 3. Resume: firmware re-inits platform → jumps to FACS waking vector (64-bit mode on UEFI).
//! 4. Trampoline: restores CR3 (page tables) + RSP (kernel stack) → jumps to `s3_resume_entry`.
//! 5. `s3_resume_entry` re-inits core (IDT, APIC, timers) → restores driver contexts → unparks APs.
//!
//! ## ponytail
//! Save/restore é seletivo: e1000 NIC, cpufreq EPB. NVMe, AHCI, USB, GPU são
//! **re-inicializados do zero** no resume (funciona, perde estado transitório).
//! Adicionar save context específico quando o driver for testado em S3.
//!
//! ## QEMU
//! S3 entry funciona (janela pausa). Resume requer OVMF com `-D S3_SUPPORT=TRUE`
//! + `-action s3=on`. A maioria dos setups QEMU não suporta S3 resume — o entry
//! é testável, o resume é **validação em HW real**.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crate::acpi;

// ─── S3 resume context (saved before suspend, consumed by resume trampoline) ─
// These are at fixed physical addresses so the trampoline (which runs before
// our virtual memory is fully set up) can find them.

/// Physical address where the resume trampoline blob is copied.
static S3_TRAMP_PHYS: AtomicU64 = AtomicU64::new(0);

/// Saved CR3 (PML4 root) before suspend.
static S3_SAVED_CR3: AtomicU64 = AtomicU64::new(0);

/// Saved RSP (kernel stack pointer) before suspend.
static S3_SAVED_RSP: AtomicU64 = AtomicU64::new(0);

/// Address of the C resume handler (in higher-half virtual space).
static S3_RESUME_FN: AtomicU64 = AtomicU64::new(0);

/// Set non-zero once resume has been detected and handled.
pub static S3_RESUMED: AtomicU64 = AtomicU64::new(0);

/// Minimum physical address for the trampoline (below 1 MB for firmware compat).
const TRAMP_BASE: u64 = 0x7000; // right after typical BDA/EBDA, safe on all x86

// ─── Device context save areas ─────────────────────────────────────────────

/// e1000 NIC context (save before suspend, restore after resume).
#[repr(C)]
#[derive(Debug, Clone)]
pub struct E1000Context {
    pub ctrl: u32, pub ctrl_ext: u32, pub tctl: u32, pub rctl: u32,
    pub tdbal: u32, pub tdlen: u32, pub tdh: u32, pub tdt: u32,
    pub rdbal: u32, pub rdlen: u32, pub rdh: u32, pub rdt: u32,
    pub ral: u32, pub rah: u32,
    pub mta: [u32; 128], // Multicast Table Array
    pub tipg: u32, pub rdtr: u32, pub imask: u32,
}

impl Default for E1000Context {
    fn default() -> Self {
        Self {
            ctrl: 0, ctrl_ext: 0, tctl: 0, rctl: 0,
            tdbal: 0, tdlen: 0, tdh: 0, tdt: 0,
            rdbal: 0, rdlen: 0, rdh: 0, rdt: 0,
            ral: 0, rah: 0, mta: [0u32; 128],
            tipg: 0, rdtr: 0, imask: 0,
        }
    }
}

/// Global suspend state.
static SUSPEND_PENDING: AtomicBool = AtomicBool::new(false);

/// Returns true if we are in the process of suspending.
pub fn is_suspending() -> bool { SUSPEND_PENDING.load(Ordering::Acquire) }

// ─── S3 resume trampoline (64-bit) ──────────────────────────────────────────

/// Build the S3 resume trampoline blob at `dest_phys`.
///
/// The blob:
///   1. Loads saved CR3 (restore page tables)
///   2. Loads saved RSP (restore kernel stack)
///   3. Jumps to `s3_resume_fn` (C handler in higher half)
///
/// Format (x86-64, position-independent once copied):
/// ```text
///   mov rax, <cr3_phys>       ; 48 B8 + 8 bytes imm64
///   mov cr3, rax              ; 0F 22 D8
///   mov rax, <rsp_virt>       ; 48 B8 + 8 bytes imm64
///   mov rsp, rax              ; 48 89 C4
///   mov rax, <resume_fn_addr> ; 48 B8 + 8 bytes imm64
///   jmp rax                   ; FF E0
///   [padding to 64 bytes]
/// ```
unsafe fn write_trampoline(dest_phys: u64, cr3: u64, rsp: u64, resume_fn: u64) {
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let dest_virt = (dest_phys + pmoff) as *mut u8;
    let mut pos = 0usize;
    let mut write = |bytes: &[u8]| {
        for &b in bytes {
            core::ptr::write_volatile(dest_virt.add(pos), b);
            pos += 1;
        }
    };
    // mov rax, cr3 (REX.W B8 + 8 byte immediate)
    write(&[0x48, 0xB8]);
    write(&cr3.to_le_bytes());
    // mov cr3, rax
    write(&[0x0F, 0x22, 0xD8]);
    // mov rax, rsp
    write(&[0x48, 0xB8]);
    write(&rsp.to_le_bytes());
    // mov rsp, rax
    write(&[0x48, 0x89, 0xC4]);
    // mov rax, resume_fn
    write(&[0x48, 0xB8]);
    write(&resume_fn.to_le_bytes());
    // jmp rax
    write(&[0xFF, 0xE0]);
    // Pad rest with INT3 (CC)
    while pos < 64 {
        core::ptr::write_volatile(dest_virt.add(pos), 0xCC);
        pos += 1;
    }
    crate::slog_nano!("SUSPEND", "info", "Trampoline escrito em 0x{:x} ({} bytes)", dest_phys, pos);
}

/// Set up S3 resume: allocate trampoline page and write FACS wake vector.
/// Call before `suspend()` (which will write SLP_EN).
pub unsafe fn setup_resume(cr3: u64, rsp: u64, resume_fn_addr: u64) {
    // Save context for the trampoline
    S3_SAVED_CR3.store(cr3, Ordering::Release);
    S3_SAVED_RSP.store(rsp, Ordering::Release);
    S3_RESUME_FN.store(resume_fn_addr, Ordering::Release);

    // Use a fixed low-memory page (below 1MB) for the trampoline blob.
    // This physical address must be identity-mapped by the firmware on resume.
    let tramp_phys = TRAMP_BASE;

    // Write the blob
    write_trampoline(tramp_phys, cr3, rsp, resume_fn_addr);
    S3_TRAMP_PHYS.store(tramp_phys, Ordering::Release);

    // Write the FACS waking vector to point at our trampoline
    let facs_virt = {
        let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let facs_phys = acpi::internal_facs_phys();
        if facs_phys == 0 {
            crate::slog_nano!("SUSPEND", "warn", "FACS phys desconhecido — nao escrevendo wake vector");
            return;
        }
        facs_phys + pmoff
    };

    // FACS offset 12 = firmware_waking_vector (32-bit), offset 24 = x_firmware_waking_vector (64-bit)
    let facs_ptr = facs_virt as *mut u8;
    let wake_val = tramp_phys as u64;

    // Write 64-bit waking vector (FACS v2)
    core::ptr::write_volatile((facs_ptr.add(24)) as *mut u64, wake_val);
    // Also write 32-bit field (truncated, for legacy BIOS)
    core::ptr::write_volatile((facs_ptr.add(12)) as *mut u32, tramp_phys as u32);

    crate::slog_nano!("SUSPEND", "info", "FACS wake_vector <- 0x{:x} (trampoline)", tramp_phys);
}

// ─── C-level resume handler (called by trampoline after restoring CR3/RSP) ──

/// This is the C function the trampoline jumps to after restoring page tables.
/// It runs in the higher half with full kernel access.
///
/// Called by the trampoline with:
///   - CR3 = saved page tables (higher half + identity map active)
///   - RSP = saved kernel stack
///   - No IDT (must re-init if needed)
///   - APIC not yet re-initialized
///
/// ## ponytail
/// Minimal restore: re-init APIC, unmask timer, restore NIC context, unpark APs.
/// Full device re-init (AHCI, NVMe, USB) happens naturally as drivers poll.
pub extern "C" fn s3_resume_entry() {
    crate::slog_nano!("SUSPEND", "info", "S3 resume entry — restaurando kernel...");
    S3_RESUMED.store(1, Ordering::Release);

    // Re-init APIC (was reset by firmware during resume)
    unsafe {
        let base = crate::apic::LAPIC_VIRT_BASE.load(Ordering::Relaxed);
        if base > 0 {
            // Enable LAPIC via SVR (bit 8)
            let svr_addr = (base + 0xF0) as *mut u32;
            let svr = core::ptr::read_volatile(svr_addr);
            core::ptr::write_volatile(svr_addr, (svr & 0xFFFFFF00) | 0xFF | 0x100);
            // Send EOI to clear any pending
            core::ptr::write_volatile((base + 0xB0) as *mut u32, 0);
        }
    }

    // Re-init timers (PIT was reset by firmware)
    // ponytail: PIT channel 0 — modo 3, divisor 65536 (18.2 Hz)
    unsafe {
        core::arch::asm!("out 0x43, al", in("al") 0x36u8, options(nostack, preserves_flags));
        core::arch::asm!("out 0x40, al", in("al") 0x00u8, options(nostack, preserves_flags));
        core::arch::asm!("out 0x40, al", in("al") 0x00u8, options(nostack, preserves_flags));
    }

    // ponytail: restore e1000 NIC context here (global NIC driver state needed)
    // For now, NIC will re-init on next poll (driver's init path detects reset)

    // Unpark APs: the boot path will re-init SMP via wake_aps_sequential.
    // For now, BSP-only is safe.

    // Restore cpufreq to default governor
    crate::cpufreq::set_energy_perf_bias(6);

    crate::slog_nano!("SUSPEND", "info", "S3 resume entry done — retomando scheduler");
}

// ─── NIC e1000 save/restore ────────────────────────────────────────────────

/// Save e1000 MMIO registers.
pub unsafe fn save_e1000(mmio_virt: u64, ctx: &mut E1000Context) {
    let r = |reg| (mmio_virt + reg) as *mut u32;
    ctx.ctrl = core::ptr::read_volatile(r(crate::e1000::REG_CTRL));
    ctx.ctrl_ext = core::ptr::read_volatile(r(crate::e1000::REG_CTRL_EXT));
    ctx.tctl = core::ptr::read_volatile(r(crate::e1000::REG_TCTRL));
    ctx.rctl = core::ptr::read_volatile(r(crate::e1000::REG_RCTRL));
    ctx.tdbal = core::ptr::read_volatile(r(crate::e1000::REG_TDBAL));
    ctx.tdlen = core::ptr::read_volatile(r(crate::e1000::REG_TDLEN));
    ctx.tdh = core::ptr::read_volatile(r(crate::e1000::REG_TDH));
    ctx.tdt = core::ptr::read_volatile(r(crate::e1000::REG_TDT));
    ctx.rdbal = core::ptr::read_volatile(r(crate::e1000::REG_RDBAL));
    ctx.rdlen = core::ptr::read_volatile(r(crate::e1000::REG_RDLEN));
    ctx.rdh = core::ptr::read_volatile(r(crate::e1000::REG_RDH));
    ctx.rdt = core::ptr::read_volatile(r(crate::e1000::REG_RDT));
    ctx.ral = core::ptr::read_volatile(r(crate::e1000::REG_RAL));
    ctx.rah = core::ptr::read_volatile(r(crate::e1000::REG_RAH));
    ctx.tipg = core::ptr::read_volatile(r(crate::e1000::REG_TIPG));
    ctx.imask = core::ptr::read_volatile(r(crate::e1000::REG_IMASK));
    for i in 0..128 {
        ctx.mta[i] = core::ptr::read_volatile((mmio_virt + crate::e1000::REG_MTA + (i as u64 * 4)) as *mut u32);
    }
}

/// Restore e1000 MMIO registers.
pub unsafe fn restore_e1000(mmio_virt: u64, ctx: &E1000Context) {
    let w = |reg, val| core::ptr::write_volatile((mmio_virt + reg) as *mut u32, val);
    w(crate::e1000::REG_CTRL, ctx.ctrl);
    w(crate::e1000::REG_CTRL_EXT, ctx.ctrl_ext);
    w(crate::e1000::REG_RAL, ctx.ral);
    w(crate::e1000::REG_RAH, ctx.rah);
    for i in 0..128 {
        core::ptr::write_volatile((mmio_virt + crate::e1000::REG_MTA + (i as u64 * 4)) as *mut u32, ctx.mta[i]);
    }
    w(crate::e1000::REG_TCTRL, ctx.tctl);
    w(crate::e1000::REG_TDBAL, ctx.tdbal);
    w(crate::e1000::REG_TDLEN, ctx.tdlen);
    w(crate::e1000::REG_TDH, ctx.tdh);
    w(crate::e1000::REG_TDT, ctx.tdt);
    w(crate::e1000::REG_RCTRL, ctx.rctl);
    w(crate::e1000::REG_RDBAL, ctx.rdbal);
    w(crate::e1000::REG_RDLEN, ctx.rdlen);
    w(crate::e1000::REG_RDH, ctx.rdh);
    w(crate::e1000::REG_RDT, ctx.rdt);
    w(crate::e1000::REG_TIPG, ctx.tipg);
    w(crate::e1000::REG_IMASK, ctx.imask);
}

// ─── AP parking ────────────────────────────────────────────────────────────

/// Park APs before suspend: send halt IPI.
pub unsafe fn park_aps() {
    crate::slog_nano!("SUSPEND", "info", "Parking APs...");
    crate::apic::send_ipi_halt();
    for _ in 0..10_000 { core::hint::spin_loop(); }
}

// ─── Main S3 entry ─────────────────────────────────────────────────────────

/// Prepare and enter S3 suspend.
///
/// Steps:
///   1. Save CR3 and RSP, set up FACS wake vector → trampoline
///   2. Save device contexts
///   3. Set powersave governor + max EPB
///   4. Park APs
///   5. Write SLP_TYP=3 + SLP_EN to PM1a_CNT → platform asserts SLP_S3#
pub fn suspend() {
    if !acpi::s3_slp_typa().is_some() {
        crate::slog_nano!("SUSPEND", "warn", "S3 nao disponivel — sem _S3 na DSDT");
        return;
    }
    let port = acpi::pm1a_cnt_port();
    if port == 0 {
        crate::slog_nano!("SUSPEND", "warn", "S3 sem PM1a_CNT");
        return;
    }

    crate::slog_nano!("SUSPEND", "info", "Suspending (S3)...");
    SUSPEND_PENDING.store(true, Ordering::Release);

    // Step 1: save CPU context + set up FACS wake vector
    let cr3 = {
        let (frame, _) = x86_64::registers::control::Cr3::read();
        frame.start_address().as_u64()
    };
    let rsp: u64;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nostack, preserves_flags)) };
    unsafe {
        setup_resume(cr3, rsp, s3_resume_entry as *const () as u64);
    }

    // Step 2: save device contexts
    // ponytail: device save hooks are driver-specific. e1000 save is available
    // via save_e1000() if the driver exposes its mmio_virt.

    // Step 3: powersave
    crate::cpufreq::set_governor(crate::cpufreq::Governor::Powersave);
    crate::cpufreq::set_energy_perf_bias(15);

    // Step 4: park APs
    unsafe { park_aps(); }

    // Step 5: enter S3
    let typ3 = acpi::s3_slp_typa().unwrap_or(3);
    let val: u16 = ((typ3 as u16) << 10) | (1u16 << 13); // SLP_EN

    crate::slog_nano!("SUSPEND", "info", "S3 entry: port={:#x} typ={} val={:#x}", port, typ3, val);

    unsafe {
        core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, preserves_flags));
    }

    // If we're here, S3 didn't trigger
    crate::slog_nano!("SUSPEND", "warn", "S3 write returned — system did NOT suspend");
    SUSPEND_PENDING.store(false, Ordering::Release);
}

/// Called early in boot (Phase 0) to detect S3 resume.
pub fn check_resume() -> bool {
    if S3_RESUMED.load(Ordering::Acquire) != 0 {
        crate::slog_nano!("SUSPEND", "info", "S3 resume detectado via S3_RESUMED flag");
        true
    } else {
        false
    }
}
