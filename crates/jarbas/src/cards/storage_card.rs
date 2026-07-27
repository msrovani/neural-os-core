//! ADR-0040 #419 — Storage Manager Card UI.
//! Disk usage gauge, disk info, and format button via the existing
//! `UiDeclaration` / `Widget` card system.

use crate::display::card::{UiDeclaration, Widget};
use alloc::format;
use alloc::string::String;

/// Build a storage info card using actual ATA driver state.
/// Returns a `UiDeclaration` ready for `render_card()` / `desktop.spawn_card()`.
pub fn storage_card() -> UiDeclaration {
    let mut card = UiDeclaration::new(419, "Storage", 60, 92, 300, 170);
    card.closable = true;

    // Snapshot disk state under the lock (brief — no I/O inside the lock).
    let (present, io_base, slave, sectors) = {
        let guard = k_nano::ATA_DRIVER.lock();
        match guard.as_ref() {
            Some(ata) => {
                let total = unsafe { ata.total_sectors().unwrap_or(0) };
                (true, ata.io_base, ata.slave, total)
            }
            None => (false, 0, false, 0),
        }
    };

    if !present {
        card.body.push(Widget::Text(String::from("No disk detected")));
        return card;
    }

    // Disk usage gauge — stub 45 % until real FS usage tracking is wired.
    // ponytail: hardcoded 45%; replace with real `used_sectors / total_sectors` when
    // the FAT32/exFAT driver exposes allocation metadata.
    card.body.push(Widget::Gauge {
        label: String::from("Usage"),
        value: 45,
        max: 100,
        unit: String::from("%"),
    });

    // Disk identity
    card.body.push(Widget::KeyValue(
        String::from("Port"),
        format!("{:#x} {}", io_base, if slave { "slave" } else { "master" }),
    ));

    // Total sectors (human-readable)
    let gb = (sectors as f64 * 512.0) / (1024.0 * 1024.0 * 1024.0);
    // ponytail: soft-float; use integer arithmetic if FPU becomes unavailable at runtime.
    card.body.push(Widget::KeyValue(
        String::from("Capacity"),
        format!("{} sectors ({:.1} GB)", sectors, gb),
    ));

    // ponytail: Format button — no-op until a format skill / confirmation dialog
    // is wired. The compositor must route `CARD_ACTION` back to a handler.
    card.body.push(Widget::Button(String::from("Format")));

    card
}
