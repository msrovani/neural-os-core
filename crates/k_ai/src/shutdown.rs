use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum ShutdownCause {
    None = 0,
    Expected = 1,
    Triggered = 2,
    Scheduled = 3,
    Unexpected = 4,
}

static SHUTDOWN_CAUSE: AtomicU8 = AtomicU8::new(0);

pub fn set_cause(cause: ShutdownCause) {
    SHUTDOWN_CAUSE.store(cause as u8, Ordering::SeqCst);
    let label = match cause {ShutdownCause::Expected => "E", ShutdownCause::Triggered => "T", ShutdownCause::Scheduled => "S", ShutdownCause::Unexpected => "U", ShutdownCause::None => "N"};
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

/// Tenta escrever shutdown cause no boot log persistente (FAT32)
pub fn write_persistent_shutdown_log(cause: ShutdownCause) {
    let msg = alloc::format!("SHUTDOWN:{} tick={}",
        match cause {
            ShutdownCause::Expected => "E",
            ShutdownCause::Triggered => "T",
            ShutdownCause::Scheduled => "S",
            ShutdownCause::Unexpected => "U",
            ShutdownCause::None => "N",
        },
        k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed));

    let ata_guard = k_nano::ATA_DRIVER.lock();
    let ata = match *ata_guard { Some(ref a) => a, None => return };
    let parts = unsafe { k_nano::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
            unsafe { k_nano::fat32::write_boot_log(ata, part, &msg); }
        }
    }
}

/// Le o ultimo shutdown cause do boot log persistente
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


