//! BootLogAgent — truth em k_ai (SESSION_376). Bin só registra reader FAT/USB via bridge.
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use core::sync::atomic::{AtomicPtr, Ordering};
use event_bus::Receiver;

const MANIFEST: AgentManifest = AgentManifest {
    name: "boot_log",
    kind: AgentKind::Skill,
    // PollEvery(32): drena BOOT_PHASE; FAT analyze uma vez (não Continuous).
    schedule: ScheduleKind::PollEvery(32),
    auto_start: true,
    persist: true,
};

const MAX_ROOT_CLUSTERS: u32 = 8;
const MAX_BOOT_LOG_BYTES: usize = 64 * 1024;

/// Bridge: bin instala a implementação orçada (sandbox/USB-MSC). Sem bridge = fallback ATA local.
type ReadBootLogFn = fn() -> Option<alloc::string::String>;
static READ_BOOT_LOG_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

type PushPeriodicFn = fn(u64);
static PUSH_PERIODIC_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

pub fn register_read_boot_log(f: ReadBootLogFn) {
    READ_BOOT_LOG_FN.store(f as *mut (), Ordering::Release);
}

/// Bin: `log_agent::maybe_push_periodic` — push BOOT.LOG periódico sem acoplar VFS no k_ai.
pub fn register_push_periodic(f: PushPeriodicFn) {
    PUSH_PERIODIC_FN.store(f as *mut (), Ordering::Release);
}

pub struct BootLogAgent {
    boot_phase_rx: Receiver,
    analyzed: bool,
}

impl BootLogAgent {
    pub fn new() -> Self {
        BootLogAgent {
            boot_phase_rx: k_nano::EVENT_BUS.subscribe("BOOT_PHASE"),
            analyzed: false,
        }
    }

    /// Le o ultimo log de boot e retorna como string para o Cortex
    /// Suporta FAT32 (B<TICK>.LOG) e LogFsAgent (memoria)
    pub fn read_last_boot_log() -> Option<alloc::string::String> {
        let ptr = READ_BOOT_LOG_FN.load(Ordering::Acquire);
        if !ptr.is_null() {
            let f: ReadBootLogFn = unsafe { core::mem::transmute(ptr) };
            return f();
        }
        Self::read_last_boot_log_local()
    }

    fn read_last_boot_log_local() -> Option<alloc::string::String> {
        // Boot USB unificado: MSC no mesmo stick — walk FAT via BOT trava init_phase.
        if k_nano::globals::USB_MSC.lock().is_some() {
            k_nano::slog_kai!("BOOTLOG", "warn", "skip FAT walk (USB-MSC boot)");
        } else {
        // Tenta ler do disco ATA primeiro
        let ata_guard = k_nano::ATA_DRIVER.lock();
        let ata = (*ata_guard).as_ref()?;
        let parts = unsafe { k_nano::fat32::read_mbr(ata) };
        for part in &parts {
            match part.type_code {
                0x0B | 0x0C | 0x1C | 0x73 => {
                    // FAT32 (ou mascarado): ler B<TICK>.LOG mais recente
                    if let Some(fat32) = unsafe { k_nano::fat32::Fat32Reader::new(ata, part) } {
                        let mut best_name = alloc::string::String::new();
                        let mut best_tick = 0u64;
                        let mut cluster = fat32.get_root_cluster();
                        let mut walked = 0u32;
                        while cluster < 0x0FFF_FFF8 && cluster >= 2 && walked < MAX_ROOT_CLUSTERS {
                            walked += 1;
                            let lba = fat32.cluster_lba(cluster);
                            let cs = fat32.sectors_per_cluster as usize * fat32.bytes_per_sector as usize;
                            let mut buf = alloc::vec![0u8; cs];
                            for i in 0..fat32.sectors_per_cluster as u32 {
                                unsafe { ata.read_sectors(lba.wrapping_add(i), &mut buf[i as usize * 512..(i+1) as usize * 512], 1); }
                            }
                            for entry_off in (0..buf.len()).step_by(32) {
                                let first = buf[entry_off];
                                if first == 0 || first == 0xE5 { continue; }
                                if buf[entry_off + 11] & 0x08 != 0 { continue; }
                                let name_bytes = &buf[entry_off..entry_off+11];
                                let name_str = core::str::from_utf8(name_bytes).unwrap_or("");
                                if name_str.starts_with("B") && name_str.len() >= 8 {
                                    let tick_str = &name_str[1..8];
                                    if let Ok(tick) = u64::from_str_radix(tick_str, 16) {
                                        if tick > best_tick {
                                            best_tick = tick;
                                            best_name = alloc::string::String::from(name_str);
                                        }
                                    }
                                }
                            }
                            cluster = unsafe { fat32.read_fat_entry(cluster) }
                        }
                        if !best_name.is_empty() {
                            if let Some(data) = unsafe {
                                fat32.read_file_range(&best_name, 0, MAX_BOOT_LOG_BYTES)
                            } {
                                return core::str::from_utf8(&data).ok().map(|s| alloc::string::String::from(s));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        }
        // Fallback: VFS /logs via k_nano (sem acoplar hermes Ring 2)
        if let Ok(entries) = k_nano::fs::list_vfs("/logs") {
            for file in entries {
                if file.starts_with("boot_") && file.ends_with(".log") {
                    let path = alloc::format!("/logs/{}", file);
                    if let Ok(data) = k_nano::fs::read_vfs(&path) {
                        return core::str::from_utf8(&data).ok().map(|s| alloc::string::String::from(s));
                    }
                }
            }
        }
        None
    }

    /// Escreve log de boot para persistencia
    pub fn write_boot_log(tick: u64, content: &str) -> Result<(), &'static str> {
        let path = alloc::format!("/logs/boot_{:07X}.log", tick);
        k_nano::fs::write_vfs(&path, content.as_bytes())
            .or_else(|_| {
                k_nano::slog_kai!("BOOTLOG", "warn", "VFS write falhou para {}", path);
                Err("boot log persist failed")
            })
    }

    /// Analisa o log e retorna diagnostics para o Cortex
    pub fn analyze_log(log: &str) -> alloc::vec::Vec<(&'static str, alloc::string::String)> {
        let mut issues = alloc::vec::Vec::new();
        
        if log.contains("PANIC") || log.contains("panic") {
            issues.push(("PANIC", alloc::string::String::from("Kernel panic detectado no boot")));
        }
        if log.contains("FAIL") || log.contains("fail") {
            issues.push(("FAILURE", alloc::string::String::from("Falha detectada no boot")));
        }
        if log.contains("SHUTDOWN:U") {
            issues.push(("UNEXPECTED_SHUTDOWN", alloc::string::String::from("Desligamento inesperado detectado no ultimo boot")));
        }
        if log.contains("GPU") && log.contains("nao respondeu") {
            issues.push(("GPU_HUNG", alloc::string::String::from("GPU nao respondeu FORCE_WAKEUP")));
        }
        if log.contains("DISPLAY") && log.contains("framebuffer") {
            issues.push(("FB_OK", alloc::string::String::from("Framebuffer detectado OK")));
        }
        // Contar erros
        let error_count = log.lines().filter(|l| l.contains("error") || l.contains("Error") || l.contains("FAIL") || l.contains("SHUTDOWN:U")).count();
        if error_count > 0 {
            issues.push(("ERROR_COUNT", alloc::format!("{} erros no log", error_count)));
        }
        if issues.is_empty() {
            issues.push(("OK", alloc::string::String::from("Nenhum problema detectado no log")));
        }
        issues
    }
}

impl Agent for BootLogAgent {
    fn manifest(&self) -> &AgentManifest {
        &MANIFEST
    }

    fn tick(&mut self, tick: u64, _count: u64) -> AgentTickResult {
        while let Some(ev) = self.boot_phase_rx.try_receive() {
            let msg = core::str::from_utf8(&ev.payload).unwrap_or("?");
            k_nano::slog_kai!("BOOT", "ok", "fase={}", msg);
        }
        let push = PUSH_PERIODIC_FN.load(Ordering::Acquire);
        if !push.is_null() {
            let f: PushPeriodicFn = unsafe { core::mem::transmute(push) };
            f(tick);
        }
        // Uma análise FAT — re-walk Continuous/Poll engasgava mesh 1G.
        if !self.analyzed {
            self.analyzed = true;
            if let Some(log) = Self::read_last_boot_log() {
                let diagnostics = Self::analyze_log(&log);
                for (kind, msg) in &diagnostics {
                    k_nano::slog_kai!("BOOT", "ok", "{}: {}", kind, msg);
                    if *kind == "PANIC" || *kind == "GPU_HUNG" {
                        let msg_out = alloc::format!("BOOT_{}: {}", kind, msg);
                        let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
                            id: 0,
                            topic: alloc::string::String::from("HEALTH_ISSUE"),
                            payload: msg_out.into_bytes(),
                            token: event_bus::CapabilityToken::Legacy(1),
                        });
                        let ctx = crate::self_heal::ErrorContext {
                            kind,
                            message: msg.clone(),
                            file: alloc::string::String::from("boot_log"),
                            line: 0,
                            ring: 0,
                            daemon: alloc::string::String::from("boot_log_agent"),
                            tick,
                        };
                        let mut heal = crate::self_heal::GLOBAL_SELF_HEAL.lock();
                        // Boot = observe/plan only — Act fica no SelfHealAgent runtime (s390b H5).
                        let _ = heal.analyze(&ctx, false);
                        drop(heal);
                        k_nano::slog_kai!("BOOTLOG", "ok", "Health issue publicado: {}", kind);
                    }
                }
            }
        }
        AgentTickResult::Pending
    }
}



