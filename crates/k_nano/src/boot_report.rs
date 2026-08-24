//! BootReport + BOOT SCORE (ADR-0092).
//! EventBus `BOOT_REPORT` + `BOOT_AI` (ADR-0100 T-001–T-004).

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
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
    pub score: String,
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
            score: String::new(),
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
static LAST_SCORE: Mutex<String> = Mutex::new(String::new());
static PHASE_SEEN: AtomicU32 = AtomicU32::new(0);

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

pub fn last_score() -> String {
    LAST_SCORE.lock().clone()
}

/// Compacto para HUD produto (sem jargão MoE/no-llm).
pub fn hud_line(mem_mb: u64, net: &str) -> String {
    alloc::format!("{}MB  {}", mem_mb, net)
}

/// Primeira emissão do banner desta fase (0..=8). Extra = step TRACE.
pub fn first_phase(n: u8) -> bool {
    if n > 8 {
        return false;
    }
    let bit = 1u32 << n;
    let prev = PHASE_SEEN.fetch_or(bit, Ordering::AcqRel);
    prev & bit == 0
}

pub fn emit_phase_banner(n: u8, name: &str, status: &str) {
    crate::slog_bin!(
        "BOOT",
        "ok",
        "=== PHASE n={} name={} status={} ===",
        n,
        name,
        status
    );
}

fn class_llm(qemu: bool) -> (&'static str, bool) {
    use crate::load_status::{get, AssetKind, LoadStatus};
    match get(AssetKind::Llm) {
        LoadStatus::Loaded => ("ok", false),
        LoadStatus::Failed => ("fail", true),
        LoadStatus::Absent if qemu => ("degraded expected", false),
        LoadStatus::Absent => ("degraded", true),
    }
}

fn class_audio(qemu: bool) -> (&'static str, bool) {
    use crate::load_status::{get, AssetKind, LoadStatus};
    match get(AssetKind::Piper) {
        LoadStatus::Loaded => ("ok", false),
        LoadStatus::Failed => ("fail", true),
        LoadStatus::Absent if qemu => ("degraded expected", false),
        LoadStatus::Absent => ("degraded", true),
    }
}

fn nic_label() -> (&'static str, bool) {
    if crate::nic_globals::E1000.lock().is_some() {
        ("e1000", true)
    } else if crate::nic_globals::RTL8139.lock().is_some() {
        ("rtl8139", true)
    } else if crate::nic_globals::VIRTIO_DEV.lock().is_some() {
        ("virtio", true)
    } else if crate::nic_globals::I225.lock().is_some() {
        ("i225", true)
    } else {
        ("none", false)
    }
}

fn storage_bus(usb: bool) -> &'static str {
    if usb {
        "usb-msc"
    } else if crate::globals::ATA_DRIVER.lock().is_some() {
        "ata"
    } else if crate::globals::AHCI_DRIVER.lock().is_some() {
        "ahci"
    } else if crate::disk_agent::nvme::NVME_DRIVER.lock().is_some() {
        "nvme"
    } else {
        "none"
    }
}

pub fn build_score_text() -> String {
    use crate::platform_probe::HypervisorKind;
    let hv = crate::platform_probe::hypervisor();
    let qemu = hv != HypervisorKind::None;
    let ram = crate::memory::TOTAL_RAM_MB.load(Ordering::Relaxed);
    let smp = crate::smp::total_cores();
    let pollable = crate::smp::ap_pollable();
    let (nic, nic_ok) = nic_label();
    let usb = crate::globals::USB_MSC.lock().is_some();
    let storage_ok = usb
        || crate::globals::ATA_DRIVER.lock().is_some()
        || crate::globals::AHCI_DRIVER.lock().is_some()
        || crate::disk_agent::nvme::NVME_DRIVER.lock().is_some();
    let bus = storage_bus(usb);
    let gpu_note = GPU_NOTE.lock().clone();
    let gpu_ok = gpu_note.as_ref().map(|(_, ok)| *ok).unwrap_or(false);
    let (llm, llm_att) = class_llm(qemu);
    let (audio, audio_att) = class_audio(qemu);

    let cpu = if smp >= 2 && !pollable {
        "warn"
    } else if smp >= 1 {
        "ok"
    } else {
        "fail"
    };
    let net = if nic_ok { "ok" } else { "fail" };
    let storage = if storage_ok || qemu {
        if storage_ok {
            "ok"
        } else {
            "degraded expected"
        }
    } else {
        "fail"
    };
    let gpu = if gpu_ok {
        "ok"
    } else {
        "await"
    };

    let mut att = alloc::string::String::new();
    if cpu == "fail" {
        att.push_str("cpu ");
    }
    if net == "fail" {
        att.push_str("net ");
    }
    if storage == "fail" {
        att.push_str("storage ");
    }
    if llm_att {
        att.push_str("llm ");
    }
    if audio_att {
        att.push_str("audio ");
    }
    if att.is_empty() {
        att.push_str("none");
    }

    alloc::format!(
        "=== BOOT SCORE qemu={} ram_mb={} smp_online={} ===\n\
phase_0_7     ok\n\
cpu           {}  online={}  pollable={}\n\
net           {}  nic={}  rx={}\n\
storage       {}  bus={}\n\
llm           {}\n\
audio_stt_tts {}\n\
gpu           {}\n\
wifi          await\n\
attention     {}\n\
===",
        qemu,
        ram,
        smp,
        cpu,
        smp,
        pollable,
        net,
        nic,
        if nic_ok { "bound" } else { "none" },
        storage,
        bus,
        llm,
        audio,
        gpu,
        att.trim()
    )
}

pub fn publish_score_serial(score: &str) {
    for line in score.lines() {
        crate::slog_bin!("BOOT", "ok", "{}", line);
    }
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
        bus: String::from(storage_bus(r.usb_msc)),
        ok: r.storage_ok,
    });

    if !r.boot_log_written {
        r.push(BootEvent::Error {
            msg: alloc::format!("BOOT.LOG nao escrito (storage={})", r.storage_ok),
        });
    }

    r.ai = *AI_NOTE.lock();
    r.score = build_score_text();
    *LAST_SCORE.lock() = r.score.clone();
    publish_score_serial(&r.score);
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

    #[test]
    fn score_template_has_required_keys() {
        let s = "=== BOOT SCORE qemu=true ram_mb=6144 smp_online=8 ===\nattention     none\n===";
        assert!(s.contains("BOOT SCORE"));
        assert!(s.contains("qemu="));
        assert!(s.contains("attention"));
    }
}
