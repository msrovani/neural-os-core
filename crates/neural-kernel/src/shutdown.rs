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
    // Escopo curto: soltar o guard ANTES de slogar (TicketLock não-reentrante;
    // nested slog → dispatch → BOOT_LOG.lock = self-deadlock no poweroff).
    let n = {
        let log = crate::serial::BOOT_LOG.lock();
        log.len_written()
    };
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
    k_nano::slog_bin!("SHUTDOWN", "ok", "ARCH.reboot via 0x64/FE (IBF-polled)");
    k_nano::hal::ARCH.reboot();
}

/// S5 primeiro SE pm1a != 0 (budget TSC mora no power_off_s5_try, ~100ms por
/// SLP_TYP). Se retornar, o HW ignorou — caller segue a cascata.
fn s5_poweroff_try() -> bool {
    let pm1a = k_nano::acpi::pm1a_cnt_port();
    if pm1a == 0 {
        k_nano::slog_bin!("SHUTDOWN", "warn", "S5 skip — pm1a=0 (sem S5 no FADT)");
        return false;
    }
    k_nano::slog_bin!("SHUTDOWN", "ok", "S5 try pm1a={:#x}", pm1a);
    let tried = k_nano::acpi::power_off_s5();
    // power_off_s5_try já esperou; chegar aqui = HW ignorou.
    k_nano::slog_bin!("SHUTDOWN", "warn", "S5 timeout — HW ignorou (segue cascata)");
    tried
}

/// 0x604 gated: só QEMU (hypervisor real); nunca em HW.
fn qemu_poweroff_gated() -> bool {
    let sandbox = k_nano::platform_probe::probe_done()
        && k_nano::platform_probe::hypervisor() != k_nano::platform_probe::HypervisorKind::None;
    if !sandbox {
        k_nano::slog_bin!("SHUTDOWN", "warn", "0x604 skip — HW real (QEMU-only)");
        return false;
    }
    k_nano::slog_bin!("SHUTDOWN", "ok", "0x604 try (QEMU)");
    qemu_acpi_shutdown();
    k_nano::tsc::sleep_us(20_000);
    k_nano::slog_bin!("SHUTDOWN", "warn", "0x604 timeout — segue cascata");
    true
}

/// CF9: 0x06 depois 0x0E (notebooks Intel honram CF9, não 8042).
fn cf9_reset_try() {
    k_nano::slog_bin!("SHUTDOWN", "ok", "CF9 try 0x06→0x0E");
    k_nano::hal::cf9_reset();
    k_nano::slog_bin!("SHUTDOWN", "warn", "CF9 timeout — segue 8042");
}

/// 8042 COM poll de IBF (ARCH.reboot já pola bit1 de 0x64, ~10ms, retries).
fn ps2_reset_polled() {
    ps2_reset();
    k_nano::slog_bin!("SHUTDOWN", "warn", "8042 timeout — EC ignorou (segue triple-fault)");
}

/// WBINVD — força TODAS as linhas sujas ao DRAM antes de reset/power-off.
/// (O selo do ramlog só sobrevive a um warm-reset se estiver no DRAM.)
fn flush_caches() {
    unsafe {
        core::arch::asm!("wbinvd", options(nostack, preserves_flags));
    }
}

#[allow(dead_code)] // fallback físico — S5 frio apaga a DRAM antes de captura.
fn power_off_cascade() -> ! {
    s5_poweroff_try();
    qemu_poweroff_gated();
    cf9_reset_try();
    ps2_reset_polled();
    k_nano::slog_bin!("SHUTDOWN", "warn", "triple-fault try (último recurso)");
    k_nano::hal::triple_fault();
    overlay(">>> halted — safe to power off");
    POWER_UI_STATE.store(3, Ordering::Release);
    k_nano::boot_ramlog::park_observable("power_off_cascade: tudo ignorado");
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
    // F1 anti-loop (OPCODE-0054): shutdown ordenado = estado limpo — zera o
    // contador DURÁVEL de recover-reboots (um crash futuro não herda a conta).
    k_nano::boot_ramlog::clear_recover_count();
    k_nano::boot_ramlog::seal_for_next_boot();
    // exp-1: S5 frio apaga a DRAM antes da captura. Arma o flag e faz WARM
    // RESET p/ o logwriter-efi (pré-Limine) gravar o BOOT.LOG e só então
    // desligar via UEFI ResetSystem(Shutdown).
    k_nano::boot_ramlog::set_poweroff_after_capture();
    k_nano::slog_bin!("SHUTDOWN", "ok", "warm-reset p/ captura (logwriter grava BOOT.LOG + power-off)");
    halt_aps();
    flush_caches();
    // Cascata HW real: S5 → 0x604 (QEMU-only) → reset quente p/ captura
    // (CF9 → 8042 polled → triple-fault) → park observável.
    // A captura (logwriter-efi + UEFI Shutdown) exige reset quente: S5 frio
    // apagaria a DRAM antes dela — por isso S5 vem ANTES (se funcionar, o FAT
    // já foi flushado acima) e o reset quente DEPOIS; sem ele "desligar" no
    // HW vira halt mudo e a captura nunca acontece.
    s5_poweroff_try();
    qemu_poweroff_gated();
    k_nano::slog_bin!("SHUTDOWN", "ok", "warm-reset p/ captura (CF9→8042→triple)");
    cf9_reset_try();
    ps2_reset_polled();
    k_nano::slog_bin!("SHUTDOWN", "warn", "triple-fault try (último recurso)");
    k_nano::hal::triple_fault();
    k_nano::slog_bin!("SHUTDOWN", "warn", "triple-fault retornou?! park observável");
    k_nano::boot_ramlog::park_observable("shutdown: reset ignorado");
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
    // F1 anti-loop (OPCODE-0054): reboot ordenado = estado limpo.
    k_nano::boot_ramlog::clear_recover_count();
    k_nano::boot_ramlog::seal_for_next_boot();
    halt_aps();
    flush_caches();
    // Reboot NUNCA tenta S5/0x604 (são power-off): CF9 → 8042 polled →
    // triple-fault → park observável.
    cf9_reset_try();
    ps2_reset_polled();
    k_nano::slog_bin!("SHUTDOWN", "warn", "triple-fault try (último recurso)");
    k_nano::hal::triple_fault();
    k_nano::slog_bin!("SHUTDOWN", "warn", "triple-fault retornou?! park observável");
    k_nano::boot_ramlog::park_observable("reboot: reset ignorado");
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
