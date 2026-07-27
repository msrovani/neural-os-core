//! S3 Suspend-to-RAM: device save/restore + AP parking + ACPI S3 entry.
//!
//! ## Flow
//! 1. `suspend_prepare()` — save device contexts, park APs, set governors to powersave.
//! 2. `suspend_enter()` — write SLP_TYP=3 + SLP_EN → platform asserts SLP_S3# → CPU sleeps.
//! 3. Resume: CPU restarts via ACPI wake vector → boot path → `suspend_resume()` restores devices.
//!
//! ## ponytail
//! Save/restore é seletivo: APIC, NIC (e1000), PIC/PIT, cpufreq EPB.
//! NVMe, AHCI, USB xHCI context são **residuais** — adicionar quando o HW tiver
//! esses drivers e o S3 for testado em bancada. Sem save = dispositivo precisa
//! ser re-inicializado do zero no resume (funcional, perde pacotes em trânsito).
//!
//! ## QEMU
//! QEMU suspende (janela fecha, pausa) mas S3 resume não é implementado por
//! padrão (requer `-action s3=on` + OVMF S3 support). O entry functiona, o
//! resume é residual bootloader/firmware.

use core::sync::atomic::Ordering;
use crate::acpi;

// ─── Device context save areas ─────────────────────────────────────────────

/// e1000 NIC context (save before suspend, restore after resume).
#[repr(C)]
#[derive(Debug, Clone)]
pub struct E1000Context {
    pub ctrl: u32,
    pub ctrl_ext: u32,
    pub tctl: u32,
    pub rctl: u32,
    pub tdbal: u32,
    pub tdlen: u32,
    pub tdh: u32,
    pub tdt: u32,
    pub rdbal: u32,
    pub rdlen: u32,
    pub rdh: u32,
    pub rdt: u32,
    pub ral: u32,
    pub rah: u32,
    pub mta: [u32; 128], // Multicast Table Array
    pub tipg: u32,
    pub rdtr: u32,
    pub imask: u32,
}

impl Default for E1000Context {
    fn default() -> Self {
        Self {
            ctrl: 0, ctrl_ext: 0, tctl: 0, rctl: 0,
            tdbal: 0, tdlen: 0, tdh: 0, tdt: 0,
            rdbal: 0, rdlen: 0, rdh: 0, rdt: 0,
            ral: 0, rah: 0,
            mta: [0u32; 128],
            tipg: 0, rdtr: 0, imask: 0,
        }
    }
}

/// Global suspend state.
static SUSPEND_PENDING: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Returns true if we are in the process of suspending (used by drivers to
/// avoid issuing new DMA/etc during the save phase).
pub fn is_suspending() -> bool {
    SUSPEND_PENDING.load(Ordering::Acquire)
}

// ─── NIC e1000 save/restore ────────────────────────────────────────────────

/// Save e1000 MMIO registers. Caller must have access to the driver instance.
pub unsafe fn save_e1000(
    mmio_virt: u64,
    ctx: &mut E1000Context,
) {
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
    // Multicast table: 128 × 32-bit registers starting at REG_MTA
    for i in 0..128 {
        ctx.mta[i] = core::ptr::read_volatile((mmio_virt + crate::e1000::REG_MTA + (i as u64 * 4)) as *mut u32);
    }
}

/// Restore e1000 MMIO registers.
pub unsafe fn restore_e1000(
    mmio_virt: u64,
    ctx: &E1000Context,
) {
    let w = |reg, val| core::ptr::write_volatile((mmio_virt + reg) as *mut u32, val);
    w(crate::e1000::REG_CTRL, ctx.ctrl);
    w(crate::e1000::REG_CTRL_EXT, ctx.ctrl_ext);
    w(crate::e1000::REG_RAL, ctx.ral);
    w(crate::e1000::REG_RAH, ctx.rah);
    // Restore MTA
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

/// Park APs before suspend: send halt IPI to all APs so they stop before
/// the SLP_EN write. After resume, APs restart via INIT-SIPI-SIPI (boot path).
pub unsafe fn park_aps() {
    crate::slog_nano!("SUSPEND", "info", "Parking APs...");
    crate::apic::send_ipi_halt();
    // ponytail: brief spin-wait for APs to halt; real impl would poll per-AP
    for _ in 0..10_000 {
        core::hint::spin_loop();
    }
}

// ─── Main S3 entry ─────────────────────────────────────────────────────────

/// Prepare and enter S3 suspend.
///
/// 1. Switch to powersave governor (P-state min) before suspend
/// 2. Park APs
/// 3. Signal suspending flag
/// 4. Write PM1a_CNT: SLP_TYP=3 + SLP_EN
///
/// After this call, if we return, S3 did not trigger (no port/typ configured).
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

    // Signal drivers to stop issuing DMA
    SUSPEND_PENDING.store(true, Ordering::Release);

    // Switch to powersave for suspend
    crate::cpufreq::set_governor(crate::cpufreq::Governor::Powersave);
    crate::cpufreq::set_energy_perf_bias(15); // max power saving

    // Park APs
    unsafe { park_aps(); }

    // ponytail: device save hooks são chamados pelo driver via is_suspending()
    // A restore completa é residual (requer wake trampoline + boot path hook).

    // Write SLP_TYP=3 + SLP_EN to PM1a_CNT
    let typ3 = acpi::s3_slp_typa().unwrap_or(3);
    let val: u16 = ((typ3 as u16) << 10) | (1u16 << 13); // SLP_EN

    crate::slog_nano!("SUSPEND", "info", "S3 entry: port={:#x} typ={} val={:#x}", port, typ3, val);

    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") val,
            options(nostack, preserves_flags)
        );
    }

    // If execution continues here, S3 did not trigger
    crate::slog_nano!("SUSPEND", "warn", "S3 write returned — system did NOT suspend");
    SUSPEND_PENDING.store(false, Ordering::Release);
}

/// Called early in boot (Phase 0) if we detect resume from S3.
/// Checks FACS waking vector, restores driver contexts.
///
/// ponytail: stub — S3 resume path requires bootloader integration.
/// Real resume: CPU starts at wake_vector (real-mode) → firmware → bootloader
/// → kernel detects S3 flag → restore_e1000 etc. Residual until FACS trampoline.
pub fn check_resume() -> bool {
    let wake = acpi::facs_wake_vector();
    if wake != 0 {
        crate::slog_nano!("SUSPEND", "info", "S3 wake vector presente: 0x{:x}", wake);
        // ponytail: real resume path needs:
        //   1. Real-mode trampoline at wake_vector
        //   2. Boot flag to skip full init
        //   3. Restore device contexts
        //   4. Unpark APs
        // For now: log only
        false
    } else {
        false
    }
}
