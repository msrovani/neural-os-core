//! SysInfoAgent — retry de flush do BOOT.LOG + NSGDB no pendrive (HW real).
//!
//! SESSION_345 F3: UI viva **sem** MSC → deferred probe no HC bound (não
//! rebind multi-xHCI). Com MSC → ensure_persisted + remount NSGDB.
//!
//! Painel na tela: **Hub Health** (F12) — linha `infer` mostra tok/s
//! (`cortex::infer_queue::hub_infer_line`). Card 9001 removido no s261.

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
static NSGDB_REMOUNT_OK: AtomicBool = AtomicBool::new(false);

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
        // PollEvery(50) com last_poll==0 dispara no tick 1 — no mesmo ciclo
        // do 1º frame do Display. xHCI multi-porta aí congela o orb em 00:00.
        if tick < 64 {
            return AgentTickResult::Pending;
        }
        let fat_ok = k_nano::boot_logger::FAT_READY.load(Ordering::Relaxed);
        let has_msc = k_nano::globals::USB_MSC.lock().is_some();
        let ui_live = k_nano::boot_logger::ui_is_live();

        if !fat_ok {
            let n = MSC_RETRY_EPOCH.fetch_add(1, Ordering::Relaxed);
            // Sem UI: limpa skips e permite re-probe completo periodicamente.
            if n > 0 && n % 64 == 0 && !ui_live {
                k_nano::xhci::clear_msc_port_skips();
            }
            let ok = k_nano::boot_logger::ensure_persisted();
            if ok && !LOG_FAT_ANNOUNCED.swap(true, Ordering::Relaxed) {
                k_nano::slog_bin!("LOG", "ok", "BOOT.LOG gravado no FAT (SysInfo T+{})", tick);
                if k_nano::storage::remount_after_usb_msc() {
                    NSGDB_REMOUNT_OK.store(true, Ordering::Relaxed);
                    k_nano::slog_bin!(
                        "LOG",
                        "ok",
                        "NSGDB remount backend={}",
                        k_nano::storage::backend_name()
                    );
                } else {
                    k_nano::slog_bin!("LOG", "warn", "NSGDB remount skip/fail T+{}", tick);
                }
            } else if !ok && n % 16 == 0 {
                k_nano::slog_bin!(
                    "LOG",
                    "warn",
                    "BOOT.LOG persist pending T+{} msc={} ui={}",
                    tick,
                    has_msc as u8,
                    ui_live as u8
                );
            }
        } else if k_nano::storage::backend_name() != "file" && has_msc {
            if k_nano::storage::remount_after_usb_msc()
                && !NSGDB_REMOUNT_OK.swap(true, Ordering::Relaxed)
            {
                k_nano::slog_bin!(
                    "LOG",
                    "ok",
                    "NSGDB FileFlash backend={} (SysInfo T+{})",
                    k_nano::storage::backend_name(),
                    tick
                );
            }
        }
        AgentTickResult::Pending
    }
}
