//! Power / shutdown — causa + arm + soft phrases; execução full no bin (orderly).

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
pub static POWER_UI_STATE: AtomicU8 = AtomicU8::new(0);

const ARM_WINDOW_TICKS: u64 = 90;

pub fn set_cause(cause: ShutdownCause) {
    SHUTDOWN_CAUSE.store(cause as u8, Ordering::SeqCst);
    let label = match cause {
        ShutdownCause::Expected => "E",
        ShutdownCause::Triggered => "T",
        ShutdownCause::Scheduled => "S",
        ShutdownCause::Unexpected => "U",
        ShutdownCause::None => "N",
    };
    let tick = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
    let _msg = alloc::format!("SHUTDOWN:{} tick={}", label, tick);
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: alloc::string::String::from("SHUTDOWN"),
        payload: _msg.as_bytes().to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    k_nano::slog_kai!("SHUTDOWN", "info", "Causa: {} (tick={})", label, tick);
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
        k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed)
    );

    let ata_guard = k_nano::ATA_DRIVER.lock();
    let ata = match *ata_guard {
        Some(ref a) => a,
        None => return,
    };
    let parts = unsafe { k_nano::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code == 0x0B
            || part.type_code == 0x0C
            || part.type_code == 0x1C
            || part.type_code == 0x73
        {
            unsafe {
                k_nano::fat32::write_boot_log(ata, part, &msg);
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
    k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64
}

fn disarm() {
    POWER_ARM_KIND.store(0, Ordering::Release);
    POWER_ARM_UNTIL.store(0, Ordering::Release);
}

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

pub fn hibernate_stub() -> &'static str {
    POWER_UI_STATE.store(5, Ordering::Release);
    const MSG: &str = "Hibernacao ainda nao disponivel neste build (sem S4/imagem)";
    k_nano::slog_kai!("SHUTDOWN", "info", "{}", MSG);
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_SYSTEM_HIBERNATE),
        payload: MSG.as_bytes().to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    MSG
}

pub fn request_shutdown() {
    set_cause(ShutdownCause::Triggered);
    write_persistent_shutdown_log(ShutdownCause::Triggered);
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_SYSTEM_SHUTDOWN),
        payload: b"request".to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

pub fn request_reboot() {
    set_cause(ShutdownCause::Scheduled);
    write_persistent_shutdown_log(ShutdownCause::Scheduled);
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: alloc::string::String::from(TOPIC_SYSTEM_REBOOT),
        payload: b"request".to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

/// Soft phrases PT/EN. None = não é power intent.
pub fn handle_power_phrase(text: &str) -> Option<alloc::string::String> {
    let lower = text.to_lowercase();
    let t = lower
        .trim_start_matches("jarbas,")
        .trim_start_matches("jarvis,")
        .trim_start_matches("jarbas ")
        .trim_start_matches("jarvis ")
        .trim();

    let is_hib = t.contains("hibern") || t.contains("suspenda") || t.contains("suspender");
    let is_reb = t.contains("reinic") || t.contains("reboot") || t.contains("reinicial");
    let is_off = t.contains("deslig")
        || t.contains("shutdown")
        || t.contains("power off")
        || t.contains("apague o computador")
        || (t.contains("apague") && t.contains("computador"));

    if is_hib {
        return Some(alloc::string::String::from(hibernate_stub()));
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
