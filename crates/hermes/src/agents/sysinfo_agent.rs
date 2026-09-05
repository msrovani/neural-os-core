//! SysInfoAgent — retry de flush do BOOT.LOG + NSGDB no pendrive (HW real).

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const SYSINFO_MANIFEST: AgentManifest = AgentManifest {
    name: "sysinfo",
    kind: AgentKind::System,
    schedule: ScheduleKind::PollEvery(50),
    auto_start: true,
    persist: false,
};

static LOG_FAT_ANNOUNCED: AtomicBool = AtomicBool::new(false);
static MSC_RETRY_EPOCH: AtomicU64 = AtomicU64::new(0);

pub struct SysInfoAgent;

impl SysInfoAgent {
    pub fn new() -> Self {
        SysInfoAgent
    }
}

impl Agent for SysInfoAgent {
    fn manifest(&self) -> &AgentManifest {
        &SYSINFO_MANIFEST
    }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        let fat_ok = k_nano::boot_logger::FAT_READY.load(Ordering::Relaxed);
        if !fat_ok {
            // A cada ~32 tentativas (~1600 ticks SysInfo) limpa skip de portas —
            // com rate-limit 200 ticks no ensure_persisted, evita martelar xHCI.
            let n = MSC_RETRY_EPOCH.fetch_add(1, Ordering::Relaxed);
            if n > 0 && n % 32 == 0 {
                k_nano::xhci::clear_msc_port_skips();
            }
            let ok = k_nano::boot_logger::ensure_persisted();
            if ok && !LOG_FAT_ANNOUNCED.swap(true, Ordering::Relaxed) {
                k_nano::slog_bin!("LOG", "ok", "BOOT.LOG gravado no FAT (SysInfo T+{})", tick);
                let _ = k_nano::storage::remount_after_usb_msc();
            }
        } else if k_nano::storage::backend_name() != "file"
            && k_nano::globals::USB_MSC.lock().is_some()
        {
            let _ = k_nano::storage::remount_after_usb_msc();
        }
        AgentTickResult::Pending
    }
}
