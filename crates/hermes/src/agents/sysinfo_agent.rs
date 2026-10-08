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

/// Fase 2 (Gate 0 §2): retry tardio de modelos quando o MSC chega depois do
/// skip de FAT do boot. Setado pelo bin no ramo `!has_fat_block`
/// (`note_model_late_needed`); consumido once-only em `tick` após BOOT.LOG
/// persistido + remount NSGDB. USB-only: nunca limpa SKIP_ATA/AHCI/NVME.
static MODEL_LATE_NEEDED: AtomicBool = AtomicBool::new(false);
static MODEL_LATE_DONE: AtomicBool = AtomicBool::new(false);

/// Bin chama no skip de modelos (`!has_fat_block` em `main.rs`).
pub fn note_model_late_needed() {
    MODEL_LATE_NEEDED.store(true, Ordering::Relaxed);
}

/// Retry tardio do LLM via USB-MSC (FAT32 dados). Fail-closed:
/// once-only (`MODEL_LATE_DONE`), `try_lock` fail-silent, skip se modelo já
/// carregado, refuse honesto em heap/fit antes de copiar. Nenhum write no FAT,
/// nenhum clear de SKIP_* — PIO-on-wrong-disk não volta.
pub fn late_model_retry() {
    // Once-only: sem retry por tick, sem spawn de thread, sem lock().
    if MODEL_LATE_DONE.swap(true, Ordering::AcqRel) {
        return;
    }
    // USB-only: try_lock fail-silent (contenção = tenta nunca; próximo tick não
    // re-tenta — once-only acima).
    let mut guard = match k_nano::globals::USB_MSC.try_lock() {
        Some(g) => g,
        None => return,
    };
    let msc = match guard.as_mut() {
        Some(m) => m,
        None => return,
    };
    // Skip se o boot (QEMU-loader ou FAT cedo) já carregou.
    if cortex::model::loaded_model_header().is_some() {
        return;
    }
    if let Some(t) = cortex::trinity::TRINITY.try_lock() {
        if !t.experts().is_empty() {
            return;
        }
    }
    // Heap fail-closed antes de qualquer cópia (nunca grow-then-crash).
    if k_nano::allocator::heap_headroom_critical() {
        k_nano::slog_bin!("LLM", "warn", "late retry refuse: heap critical (tardio, sem grow)");
        return;
    }
    let ram_mb = k_nano::memory::TOTAL_RAM_MB.load(Ordering::Relaxed);
    let plan = cortex::model_fit::llm_boot_plan(ram_mb);
    let pio_cap = (plan.max_resident_mb as usize).saturating_mul(1024 * 1024);
    // Size primeiro (walk barato); bytes só do candidato que passa no fit.
    for name in cortex::model_fit::falcon3_boot_names() {
        let sz = match unsafe { k_nano::fat32::lookup_file_on_dev(&mut *msc, name) } {
            Some(s) => s,
            None => continue,
        };
        let file_mb = (sz / (1024 * 1024)) as u64;
        if sz > pio_cap
            || !cortex::model_fit::pack_resident_ok(ram_mb, 0, file_mb)
            || k_nano::allocator::heap_headroom_low()
            || k_nano::allocator::heap_headroom_bytes() < sz.saturating_add(128 * 1024 * 1024)
        {
            k_nano::slog_bin!("LLM", "warn", "late retry refuse {} ({}MB, heap/fit)", name, file_mb);
            continue;
        }
        let parts = unsafe { k_nano::fat32::partitions_on_dev(&mut *msc) };
        let mut data_opt = None;
        for part in &parts {
            if let Some(d) = unsafe { k_nano::fat32::read_root_file_dev(&mut *msc, part, name) } {
                data_opt = Some(d);
                break;
            }
        }
        let Some(data) = data_opt else { continue };
        let v6 = cortex::model::load_model_v6(&data).and_then(|v| match v {
            cortex::model::ModelView::Llm(m) => Some(m),
            _ => None,
        });
        if let Some(m) = v6.or_else(|| cortex::cortex::load_model(&data)) {
            cortex::cortex::set_model(alloc::boxed::Box::new(m));
            if cortex::model::loaded_model_header().is_none() {
                cortex::model::note_header_from_bytes(&data);
            }
            k_nano::slog_bin!("LLM", "ok", "LLM LOADED (FAT tardio) file={} size={}KB via USB-MSC", name, data.len() / 1024);
            k_nano::boot_logger::log("BOOT: FAT BitNet model loaded (USB tardio)");
            k_nano::display::fb::boot_ckpt_noflush(193, "LLM LOADED (FAT tardio)");
            return;
        }
        k_nano::slog_bin!("LLM", "warn", "late retry {} presente mas load FAILED", name);
    }
    k_nano::slog_bin!("LLM", "warn", "late retry: nenhum candidato FAT no USB-MSC");
}

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
        // try_lock: SysInfo nunca bloqueia atrás de enumeração xHCI.
        let has_msc = k_nano::globals::USB_MSC.try_lock().map(|g| g.is_some()).unwrap_or(false);
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
                // Fase 2: retry tardio de modelos (USB-only, once-only, sem
                // spawn/lock; late_model_retry consome MODEL_LATE_DONE 1x).
                if has_msc && MODEL_LATE_NEEDED.load(Ordering::Relaxed) {
                    late_model_retry();
                }
            } else if !ok && n % 16 == 0 {
                // s425: razão da falha na linha (mesma string da UI — single source).
                k_nano::slog_bin!(
                    "LOG",
                    "warn",
                    "BOOT.LOG persist pending T+{} msc={} ui={} hub={}",
                    tick,
                    has_msc as u8,
                    ui_live as u8,
                    k_nano::boot_logger::hub_log_line().as_str()
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

#[cfg(test)]
mod tests {
    // Flag-only: sem DONE, sem HW, sem lock — seguro em suite paralela.
    #[test]
    fn late_needed_flag_sticks() {
        super::note_model_late_needed();
        assert!(super::MODEL_LATE_NEEDED.load(super::Ordering::Relaxed));
    }
}
