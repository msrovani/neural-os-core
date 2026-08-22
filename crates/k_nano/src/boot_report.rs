//! BootReport — resumo do boot (EventBus `BOOT_REPORT` + `BOOT_AI`).
//! ADR-0100 T-001–T-004: Observe/Plan/Act/Escalate/Verify.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use spin::Mutex;

#[derive(Debug, Clone)]
pub enum BootEvent {
    Phase { name: String, ok: bool },
    Error { msg: String },
    Storage { bus: String, ok: bool },
    Gpu { name: String, ok: bool },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BootAiCounts {
    pub observe: u32,
    pub plan: u32,
    pub act: u32,
    pub escalate: u32,
    pub verify: u32,
}

impl BootAiCounts {
    pub fn line(&self) -> String {
        alloc::format!(
            "BOOT_AI observe={} plan={} act={} escalate={} verify={}",
            self.observe,
            self.plan,
            self.act,
            self.escalate,
            self.verify
        )
    }
}

pub fn parse_boot_ai_line(s: &str) -> Option<BootAiCounts> {
    let rest = s.find("BOOT_AI").map(|i| &s[i..])?;
    let mut c = BootAiCounts::default();
    for tok in rest.split(|ch: char| ch == ' ' || ch == ':' || ch == ',') {
        let Some((k, v)) = tok.split_once('=') else {
            continue;
        };
        let Ok(n) = v.parse::<u32>() else {
            continue;
        };
        match k {
            "observe" => c.observe = n,
            "plan" => c.plan = n,
            "act" => c.act = n,
            "escalate" => c.escalate = n,
            "verify" => c.verify = n,
            _ => {}
        }
    }
    rest.contains("observe=").then_some(c)
}

#[derive(Debug, Clone)]
pub struct BootReport {
    pub events: Vec<BootEvent>,
    pub storage_ok: bool,
    pub gpu_ok: bool,
    pub usb_msc: bool,
    pub boot_log_written: bool,
    pub last_ckpt: u8,
    pub ai: BootAiCounts,
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
            ai: BootAiCounts::default(),
        }
    }

    pub fn push(&mut self, event: BootEvent) {
        self.events.push(event);
    }
}

static BOOT_REPORT: Mutex<Option<BootReport>> = Mutex::new(None);
static GPU_NOTE: Mutex<Option<(String, bool)>> = Mutex::new(None);
static AI_NOTE: Mutex<BootAiCounts> = Mutex::new(BootAiCounts {
    observe: 0,
    plan: 0,
    act: 0,
    escalate: 0,
    verify: 0,
});

pub fn note_gpu(name: &str, ok: bool) {
    *GPU_NOTE.lock() = Some((String::from(name), ok));
}

pub fn note_ai(c: BootAiCounts) {
    let mut g = AI_NOTE.lock();
    let verify = g.verify;
    *g = c;
    g.verify = g.verify.max(verify);
}

pub fn note_ai_verify() {
    let mut g = AI_NOTE.lock();
    g.verify = g.verify.saturating_add(1);
}

pub fn store(report: BootReport) {
    *BOOT_REPORT.lock() = Some(report);
}

pub fn last() -> Option<BootReport> {
    BOOT_REPORT.lock().clone()
}

pub fn finalize_and_publish() -> BootReport {
    use crate::boot_logger::FAT_READY;
    use crate::boot_ramlog::last_ckpt;

    let mut r = BootReport::new();
    r.last_ckpt = last_ckpt();
    r.boot_log_written = FAT_READY.load(Ordering::Relaxed);
    r.usb_msc = crate::globals::USB_MSC.lock().is_some();
    r.storage_ok = crate::globals::USB_MSC.lock().is_some()
        || crate::globals::ATA_DRIVER.lock().is_some()
        || crate::globals::AHCI_DRIVER.lock().is_some()
        || crate::disk_agent::nvme::NVME_DRIVER.lock().is_some();

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

    r.ai = *AI_NOTE.lock();
    store(r.clone());
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_boot_ai_canonical() {
        let c = parse_boot_ai_line("BOOT_AI observe=4 plan=2 act=2 escalate=1 verify=1").unwrap();
        assert_eq!(c.observe, 4);
        assert_eq!(c.act, 2);
        assert_eq!(c.escalate, 1);
    }

    #[test]
    fn parse_boot_ai_prefixed() {
        assert_eq!(
            parse_boot_ai_line("klog BOOT_AI observe=0 plan=0 act=0 escalate=0 verify=0")
                .unwrap()
                .observe,
            0
        );
    }

    #[test]
    fn parse_boot_ai_rejects_garbage() {
        assert!(parse_boot_ai_line("hello").is_none());
    }
}
