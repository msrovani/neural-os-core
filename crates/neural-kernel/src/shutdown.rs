//! Shutdown / reboot orderly — **HW only** (ACPI S5, QEMU 0x604, PS/2 reset).
//! Soft state (cause/arm/POWER_UI/phrases/request_*) = `k_ai::shutdown` (single truth).
//! Bin drena EventBus e executa `begin_orderly_*` (emagreçer ADR-0057+ / SESSION_376).

use core::sync::atomic::Ordering;
use k_nano::hal::Architecture;

pub use k_ai::shutdown::{
    get_cause, handle_power_phrase, hibernate_stub, label, power_confirm, power_disarm,
    read_last_shutdown_from_boot_log, request_reboot, request_shutdown, set_cause,
    write_persistent_shutdown_log, PowerArmKind, ShutdownCause, POWER_UI_STATE,
    TOPIC_SYSTEM_HIBERNATE, TOPIC_SYSTEM_REBOOT, TOPIC_SYSTEM_SHUTDOWN,
};

fn now_tick() -> u64 {
    crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64
}

fn dump_boot_log_sector() {
    // Honesty s389b: canal A = FAT BOOT.LOG. NÃO escrever LBA 2048 (ESP GPT).
    let flushed = k_nano::boot_logger::flush();
    if flushed {
        k_nano::slog_bin!("SHUTDOWN", "ok", "BOOT.LOG flush FAT ok");
    } else {
        k_nano::slog_bin!("SHUTDOWN", "warn", "BOOT.LOG flush FAT skipped/fail");
    }
    let log = crate::serial::BOOT_LOG.lock();
    let n = log.len_written();
    if n > 0 {
        k_nano::slog_bin!("SHUTDOWN", "ok", "BOOT_LOG ring ~{} bytes (FAT=fonte)", n);
    }
}

fn overlay(msg: &str) {
    crate::display::fb::console_print(msg);
}

fn halt_aps() {
    unsafe {
        crate::apic::send_ipi_halt();
    }
}

fn qemu_acpi_shutdown() {
    k_nano::slog_bin!("SHUTDOWN", "ok", "ARCH.poweroff via 0x604/0x2000");
    k_nano::hal::ARCH.poweroff();
}

fn ps2_reset() {
    k_nano::slog_bin!("SHUTDOWN", "ok", "ARCH.reboot via 0x64/FE");
    k_nano::hal::ARCH.reboot();
}

fn power_off_cascade() -> ! {
    let tried_s5 = k_nano::acpi::power_off_s5();
    if tried_s5 {
        k_nano::slog_bin!("SHUTDOWN", "warn", "S5 escrito — aguardando HW");
        k_nano::tsc::sleep_us(100_000);
    }
    k_nano::slog_bin!("SHUTDOWN", "warn", "fallback=qemu_0x604");
    qemu_acpi_shutdown();
    k_nano::tsc::sleep_us(20_000);
    ps2_reset();
    overlay(">>> halted — safe to power off");
    POWER_UI_STATE.store(3, Ordering::Release);
    loop {
        x86_64::instructions::hlt();
    }
}

/// Desligamento ordenado (CAD / confirm / EventBus).
pub fn begin_orderly_shutdown(cause: ShutdownCause) -> ! {
    POWER_UI_STATE.store(3, Ordering::Release);
    overlay(">>> Shutting down...");
    set_cause(cause);
    write_persistent_shutdown_log(cause);
    // s390b: checkpoint antes do halt (cross-boot SelfHeal).
    {
        let mut heal = k_ai::self_heal::GLOBAL_SELF_HEAL.lock();
        heal.save_checkpoint();
    }
    k_nano::slog_bin!(
        "SHUTDOWN",
        "ok",
        "orderly_shutdown cause={} tick={} pm1a={:#x}",
        label(cause),
        now_tick(),
        k_nano::acpi::pm1a_cnt_port()
    );
    dump_boot_log_sector();
    let _ = k_nano::boot_logger::try_flush_ramlog();
    k_nano::boot_ramlog::seal_for_next_boot();
    halt_aps();
    power_off_cascade()
}

/// Reinício ordenado.
pub fn begin_orderly_reboot(cause: ShutdownCause) -> ! {
    POWER_UI_STATE.store(4, Ordering::Release);
    overlay(">>> Rebooting...");
    set_cause(cause);
    write_persistent_shutdown_log(cause);
    {
        let mut heal = k_ai::self_heal::GLOBAL_SELF_HEAL.lock();
        heal.save_checkpoint();
    }
    k_nano::slog_bin!(
        "SHUTDOWN",
        "ok",
        "orderly_reboot cause={} tick={}",
        label(cause),
        now_tick()
    );
    dump_boot_log_sector();
    let _ = k_nano::boot_logger::try_flush_ramlog();
    k_nano::boot_ramlog::seal_for_next_boot();
    halt_aps();
    ps2_reset();
    loop {
        x86_64::instructions::hlt();
    }
}

static RX_SHUTDOWN: spin::Mutex<Option<event_bus::Receiver>> = spin::Mutex::new(None);
static RX_REBOOT: spin::Mutex<Option<event_bus::Receiver>> = spin::Mutex::new(None);

/// Assina os tópicos de energia UMA vez (chamar no boot, fora de IRQ).
pub fn init_power_drain() {
    *RX_SHUTDOWN.lock() = Some(crate::EVENT_BUS.subscribe(TOPIC_SYSTEM_SHUTDOWN));
    *RX_REBOOT.lock() = Some(crate::EVENT_BUS.subscribe(TOPIC_SYSTEM_REBOOT));
    k_nano::slog_bin!("SHUTDOWN", "ok", "power drain subscribed (SYSTEM_SHUTDOWN/REBOOT)");
}

/// Drena os receivers assinados — chamar do loop do scheduler (idle closure).
pub fn power_drain_tick() {
    let mut rx_s = RX_SHUTDOWN.lock();
    let mut rx_r = RX_REBOOT.lock();
    if let Some(rx) = rx_s.as_mut() {
        if rx.try_receive().is_some() {
            begin_orderly_shutdown(ShutdownCause::Triggered);
        }
    }
    if let Some(rx) = rx_r.as_mut() {
        if rx.try_receive().is_some() {
            begin_orderly_reboot(ShutdownCause::Scheduled);
        }
    }
}

/// Drena EventBus power topics (chamar do Hermes/Display tick no bin).
pub fn drain_power_requests(
    rx_shutdown: &mut event_bus::Receiver,
    rx_reboot: &mut event_bus::Receiver,
) {
    if rx_shutdown.try_receive().is_some() {
        begin_orderly_shutdown(ShutdownCause::Triggered);
    }
    if rx_reboot.try_receive().is_some() {
        begin_orderly_reboot(ShutdownCause::Scheduled);
    }
}
