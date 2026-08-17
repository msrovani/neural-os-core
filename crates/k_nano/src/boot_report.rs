//! BootReport — resumo do boot para camadas superiores (cortex/hermes/jarbas).
//! Publicado no EventBus como tópico `BOOT_REPORT` após a inicialização.
//! Agentes podem consultar o último relatório via `BootReport::last()`.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use spin::Mutex;

/// Eventos de boot registrados sequencialmente.
#[derive(Debug, Clone)]
pub enum BootEvent {
    Phase { name: String, ok: bool },
    Error { msg: String },
    Storage { bus: String, ok: bool },
    Gpu { name: String, ok: bool },
}

#[derive(Debug, Clone)]
pub struct BootReport {
    pub events: Vec<BootEvent>,
    pub storage_ok: bool,   // algum storage respondeu?
    pub gpu_ok: bool,       // GPU detectada e backend ok?
    pub usb_msc: bool,      // USB-MSC disponivel?
    pub boot_log_written: bool, // BOOT.LOG foi escrito?
    pub last_ckpt: u8,      // ultimo checkpoint
}

impl BootReport {
    pub fn new() -> Self {
        BootReport {
            events: Vec::new(),
            storage_ok: false,
            gpu_ok: false,
            usb_msc: false,
            boot_log_written: false,
            last_ckpt: 0,
        }
    }

    pub fn push(&mut self, event: BootEvent) {
        self.events.push(event);
    }
}

static BOOT_REPORT: Mutex<Option<BootReport>> = Mutex::new(None);

/// Postura GPU real, anotada pelo BE (k_hal) no fim do bring-up — R0 não vê
/// k_hal (ordem de anéis), então o valor chega por nota (SESSION_274).
static GPU_NOTE: Mutex<Option<(String, bool)>> = Mutex::new(None);

/// k_hal chama no fim de `init_backend_with_plan` (qualquer terminal).
pub fn note_gpu(name: &str, ok: bool) {
    *GPU_NOTE.lock() = Some((String::from(name), ok));
}

pub fn store(report: BootReport) {
    *BOOT_REPORT.lock() = Some(report);
}

pub fn last() -> Option<BootReport> {
    BOOT_REPORT.lock().clone()
}

/// Constrói relatório final e publica no EventBus.
pub fn finalize_and_publish() -> BootReport {
    use crate::boot_logger::FAT_READY;
    use crate::boot_ramlog::last_ckpt;

    let mut r = BootReport::new();
    r.last_ckpt = last_ckpt();
    r.boot_log_written = FAT_READY.load(Ordering::Relaxed);
    r.usb_msc = crate::globals::USB_MSC.lock().is_some();

    // Storage: verifica qual bus respondeu
    r.storage_ok = crate::globals::USB_MSC.lock().is_some()
        || crate::globals::ATA_DRIVER.lock().is_some()
        || crate::globals::AHCI_DRIVER.lock().is_some()
        || crate::disk_agent::nvme::NVME_DRIVER.lock().is_some();

    // GPU: valor REAL anotado pelo k_hal (note_gpu). Sem nota = false —
    // nunca claim Ready sem evidência (era `true` placeholder até SESSION_274).
    let gpu_note = GPU_NOTE.lock().clone();
    r.gpu_ok = gpu_note.as_ref().map(|(_, ok)| *ok).unwrap_or(false);
    if let Some((name, ok)) = gpu_note {
        r.push(BootEvent::Gpu { name, ok });
    }

    r.push(BootEvent::Storage {
        bus: String::from("USB-MSC"),
        ok: r.usb_msc,
    });

    if !r.boot_log_written {
        r.push(BootEvent::Error {
            msg: alloc::format!("BOOT.LOG nao escrito (storage={})", r.storage_ok),
        });
    }

    store(r.clone());
    r
}
