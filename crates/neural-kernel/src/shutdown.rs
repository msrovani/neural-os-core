//! Shutdown / reboot orderly — HW real (ACPI S5) first; QEMU/PS2 fallbacks.
//! Entradas: botão OFF, soft USER_INTENT, Ctrl+Alt+Del, shell.
//!
//! Onda 2 split: `k_ai::shutdown` espelha cause/arm/phrases; **este arquivo do bin**
//! permanece truth da execução HW (`begin_orderly_*`). Não `pub use` cego.

use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

pub const TOPIC_SYSTEM_SHUTDOWN: &str = "SYSTEM_SHUTDOWN";
pub const TOPIC_SYSTEM_REBOOT: &str = "SYSTEM_REBOOT";
pub const TOPIC_SYSTEM_HIBERNATE: &str = "SYSTEM_HIBERNATE";

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum ShutdownCause {
    None = 0,
    Expected = 1,
    Triggered = 2,
    Scheduled = 3,
    Unexpected = 4,
}

/// 0=none 1=shutdown 2=reboot
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum PowerArmKind {
    None = 0,
    Shutdown = 1,
    Reboot = 2,
}

static SHUTDOWN_CAUSE: AtomicU8 = AtomicU8::new(0);
static POWER_ARM_KIND: AtomicU8 = AtomicU8::new(0);
static POWER_ARM_UNTIL: AtomicU64 = AtomicU64::new(0);
/// Overlay no compositor: 0=none 1=confirm_off 2=confirm_reboot 3=shutting 4=rebooting 5=hibernate_msg
pub static POWER_UI_STATE: AtomicU8 = AtomicU8::new(0);

const ARM_WINDOW_TICKS: u64 = 90; // ~5s @ ~18Hz

pub fn set_cause(cause: ShutdownCause) {
    SHUTDOWN_CAUSE.store(cause as u8, Ordering::SeqCst);
    let label = match cause {
        ShutdownCause::Expected => "E",
        ShutdownCause::Triggered => "T",
        ShutdownCause::Scheduled => "S",
        ShutdownCause::Unexpected => "U",
        ShutdownCause::None => "N",
    };
    let tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    let msg = alloc::format!("SHUTDOWN:{} tick={}", label, tick);
    let _ = crate::agents::log_analyst_agent::write_log("shutdown", &msg);
    k_nano::slog_bin!("SHUTDOWN", "info", "Causa: {} (tick={})", label, tick);
}

pub fn get_cause() -> ShutdownCause {
    match SHUTDOWN_CAUSE.load(Ordering::SeqCst) {
        1 => ShutdownCause::Expected,
        2 => ShutdownCause::Triggered,
        3 => ShutdownCause::Scheduled,
        4 => ShutdownCause::Unexpected,
        _ => ShutdownCause::None,
    }
}

pub fn label(cause: ShutdownCause) -> &'static str {
    match cause {
        ShutdownCause::Expected => "expected",
        ShutdownCause::Triggered => "triggered",
        ShutdownCause::Scheduled => "scheduled",
        ShutdownCause::Unexpected => "unexpected",
        ShutdownCause::None => "none",
    }
}

pub fn write_persistent_shutdown_log(cause: ShutdownCause) {
    let msg = alloc::format!(
        "SHUTDOWN:{} tick={}",
        match cause {
            ShutdownCause::Expected => "E",
            ShutdownCause::Triggered => "T",
            ShutdownCause::Scheduled => "S",
            ShutdownCause::Unexpected => "U",
            ShutdownCause::None => "N",
        },
        crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed)
    );

    let ata_guard = crate::ATA_DRIVER.lock();
    let ata = match *ata_guard {
        Some(ref a) => a,
        None => return,
    };
    let parts = unsafe { crate::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code == 0x0B
            || part.type_code == 0x0C
            || part.type_code == 0x1C
            || part.type_code == 0x73
        {
            unsafe {
                crate::fat32::write_boot_log(ata, part, &msg);
            }
        }
    }
}

pub fn read_last_shutdown_from_boot_log() -> Option<ShutdownCause> {
    let log = crate::boot_log_agent::BootLogAgent::read_last_boot_log()?;
    for line in log.lines().rev() {
        if let Some(rest) = line.strip_prefix("SHUTDOWN:") {
            let code = rest.chars().next()?;
            return Some(match code {
                'E' => ShutdownCause::Expected,
                'T' => ShutdownCause::Triggered,
                'S' => ShutdownCause::Scheduled,
                'U' => ShutdownCause::Unexpected,
                _ => continue,
            });
        }
    }
    None
}

fn now_tick() -> u64 {
    crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64
}

fn disarm() {
    POWER_ARM_KIND.store(0, Ordering::Release);
    POWER_ARM_UNTIL.store(0, Ordering::Release);
}

/// 1ª chamada arma; 2ª da mesma kind dentro da janela → true (executar).
pub fn power_confirm(kind: PowerArmKind) -> bool {
    let tick = now_tick();
    let until = POWER_ARM_UNTIL.load(Ordering::Acquire);
    let cur = POWER_ARM_KIND.load(Ordering::Acquire);
    if cur == kind as u8 && tick <= until {
        disarm();
        return true;
    }
    POWER_ARM_KIND.store(kind as u8, Ordering::Release);
    POWER_ARM_UNTIL.store(tick.saturating_add(ARM_WINDOW_TICKS), Ordering::Release);
    POWER_UI_STATE.store(
        match kind {
            PowerArmKind::Shutdown => 1,
            PowerArmKind::Reboot => 2,
            PowerArmKind::None => 0,
        },
        Ordering::Release,
    );
    false
}

pub fn power_disarm() {
    disarm();
    POWER_UI_STATE.store(0, Ordering::Release);
}

fn dump_boot_log_sector() {
    let log = crate::serial::BOOT_LOG.lock();
    let dump = log.dump();
    if dump.is_empty() {
        return;
    }
    k_nano::slog_bin!("SHUTDOWN", "info", "BOOT_LOG dump {} bytes", dump.len());
    let ata = crate::ATA_DRIVER.lock();
    if let Some(ref ata) = *ata {
        if dump.len() <= 512 {
            let mut sector = [0u8; 512];
            sector[..dump.len()].copy_from_slice(dump);
            if unsafe { ata.write_sectors(crate::LOG_SECTOR, &sector, 1) } {
                k_nano::slog_bin!(
                    "SHUTDOWN",
                    "info",
                    "Log escrito LBA {}",
                    crate::LOG_SECTOR
                );
            }
        }
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
    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") 0x604u16,
            in("ax") 0x2000u16,
            options(nostack, preserves_flags)
        );
    }
}

fn ps2_reset() {
    k_nano::slog_bin!("SHUTDOWN", "info", "fallback=ps2_reset (0x64/FE) — nao e power-off");
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") 0x64u16,
            in("al") 0xFEu8,
            options(nostack, preserves_flags)
        );
    }
}

fn power_off_cascade() -> ! {
    let tried_s5 = k_nano::acpi::power_off_s5();
    if tried_s5 {
        k_nano::slog_bin!("SHUTDOWN", "info", "S5 escrito — aguardando HW");
        for _ in 0..5_000_000 {
            core::hint::spin_loop();
        }
    }
    k_nano::slog_bin!("SHUTDOWN", "info", "fallback=qemu_0x604");
    qemu_acpi_shutdown();
    for _ in 0..1_000_000 {
        core::hint::spin_loop();
    }
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
    let tick = now_tick();
    k_nano::slog_bin!(
        "SHUTDOWN",
        "info",
        "orderly_shutdown cause={} tick={} pm1a={:#x}",
        label(cause),
        tick,
        k_nano::acpi::pm1a_cnt_port()
    );
    dump_boot_log_sector();
    halt_aps();
    power_off_cascade()
}

/// Reinício ordenado.
pub fn begin_orderly_reboot(cause: ShutdownCause) -> ! {
    POWER_UI_STATE.store(4, Ordering::Release);
    overlay(">>> Rebooting...");
    set_cause(cause);
    write_persistent_shutdown_log(cause);
    k_nano::slog_bin!(
        "SHUTDOWN",
        "info",
        "orderly_reboot cause={} tick={}",
        label(cause),
        now_tick()
    );
    dump_boot_log_sector();
    halt_aps();
    ps2_reset();
    loop {
        x86_64::instructions::hlt();
    }
}

/// Hibernate: noop honesto (sem S4).
pub fn hibernate_stub() -> &'static str {
    POWER_UI_STATE.store(5, Ordering::Release);
    const MSG: &str = "Hibernacao ainda nao disponivel neste build (sem S4/imagem)";
    k_nano::slog_bin!("SHUTDOWN", "info", "{}", MSG);
    overlay(">>> Hibernate: not available");
    MSG
}

/// Publica pedido no EventBus (UI/soft); bin drena e executa.
pub fn request_shutdown() {
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_SYSTEM_SHUTDOWN),
        payload: b"ui".to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

pub fn request_reboot() {
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_SYSTEM_REBOOT),
        payload: b"ui".to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

/// Interpreta texto PT/EN → ação. Retorna mensagem para Chat/TTS se arm/hibernate.
pub fn handle_power_phrase(text: &str) -> Option<alloc::string::String> {
    let lower = text.to_lowercase();
    let t = lower
        .trim_start_matches("jarbas,")
        .trim_start_matches("jarvis,")
        .trim_start_matches("jarbas ")
        .trim_start_matches("jarvis ")
        .trim();

    let is_hib = t.contains("hibern") || t.contains("suspenda") || t.contains("suspender");
    let is_reb = t.contains("reinic")
        || t.contains("reboot")
        || t.contains("reinicial");
    let is_off = t.contains("deslig")
        || t.contains("shutdown")
        || t.contains("power off")
        || t.contains("apague o computador")
        || (t.contains("apague") && t.contains("computador"));

    if is_hib {
        let msg = hibernate_stub();
        return Some(alloc::string::String::from(msg));
    }
    if is_reb {
        if power_confirm(PowerArmKind::Reboot) {
            request_reboot();
            return Some(alloc::string::String::from("Reiniciando..."));
        }
        return Some(alloc::string::String::from(
            "Confirme: diga reiniciar de novo",
        ));
    }
    if is_off {
        if power_confirm(PowerArmKind::Shutdown) {
            request_shutdown();
            return Some(alloc::string::String::from("Desligando..."));
        }
        return Some(alloc::string::String::from(
            "Confirme: diga desligar de novo",
        ));
    }
    None
}

/// Drena EventBus power topics (chamar do Hermes/Display tick no bin).
pub fn drain_power_requests(rx_shutdown: &mut event_bus::Receiver, rx_reboot: &mut event_bus::Receiver) {
    if rx_shutdown.try_receive().is_some() {
        begin_orderly_shutdown(ShutdownCause::Triggered);
    }
    if rx_reboot.try_receive().is_some() {
        begin_orderly_reboot(ShutdownCause::Scheduled);
    }
}
