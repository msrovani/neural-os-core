use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::serial_println;

const MANIFEST: AgentManifest = AgentManifest {
    name: "boot_log",
    kind: AgentKind::Skill,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct BootLogAgent;

impl BootLogAgent {
    pub fn new() -> Self { BootLogAgent }

    /// Le o ultimo log de boot e retorna como string para o Cortex
    /// Suporta FAT12 (BOOT.LOG) e FAT32 (B<TICK>.LOG)
    pub fn read_last_boot_log() -> Option<alloc::string::String> {
        let ata_guard = crate::ATA_DRIVER.lock();
        let ata = (*ata_guard).as_ref()?;
        let parts = unsafe { crate::fat::read_mbr(ata) };
        for part in &parts {
            match part.type_code {
                0x01 => {
                    // FAT12: ler BOOT.LOG fixo
                    if let Some(fat12) = unsafe { crate::fat::Fat12Writer::new(ata, part) } {
                        let root_lba = fat12.root_lba();
                        let mut root = [0u8; 512];
                        unsafe { ata.read_sectors(root_lba, &mut root, 1); }
                        for i in 0..16 {
                            if &root[i*32..i*32+11] == b"BOOT    LOG" {
                                let size = u32::from_le_bytes(root[i*32+28..i*32+32].try_into().ok()?);
                                if size == 0 { return None; }
                                let cluster = u16::from_le_bytes([root[i*32+26], root[i*32+27]]) as u32;
                                let data_lba = fat12.data_lba();
                                let lba = data_lba + (cluster - 2);
                                let mut buf = alloc::vec![0u8; size as usize];
                                let sectors = ((size as usize + 511) / 512) as u8;
                                unsafe { ata.read_sectors(lba, &mut buf, sectors); }
                                return core::str::from_utf8(&buf[..size as usize]).ok().map(|s| alloc::string::String::from(s));
                            }
                        }
                    }
                }
                0x0B | 0x0C => {
                    // FAT32: procurar B<TICK>.LOG mais recente
                    if let Some(fat32) = unsafe { crate::fat::Fat32Reader::new(ata, part) } {
                        let mut best_name = alloc::string::String::new();
                        let mut best_tick = 0u64;
                        let mut cluster = fat32.get_root_cluster();
                        while cluster < 0x0FFF_FFF8 && cluster >= 2 {
                            let lba = fat32.cluster_lba(cluster);
                            let cs = fat32.sectors_per_cluster as usize * fat32.bytes_per_sector as usize;
                            let mut buf = alloc::vec![0u8; cs];
                            for i in 0..fat32.sectors_per_cluster as u32 {
                                unsafe { ata.read_sectors(lba + i, &mut buf[i as usize * 512..(i+1) as usize * 512], 1); }
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
                            if let Some(data) = unsafe { fat32.read_file(best_name.trim_end()) } {
                                return core::str::from_utf8(&data).ok().map(|s| alloc::string::String::from(s));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        None
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
        if log.contains("GPU") && log.contains("nao respondeu") {
            issues.push(("GPU_HUNG", alloc::string::String::from("GPU nao respondeu FORCE_WAKEUP")));
        }
        if log.contains("DISPLAY") && log.contains("framebuffer") {
            issues.push(("FB_OK", alloc::string::String::from("Framebuffer detectado OK")));
        }
        // Contar erros
        let error_count = log.lines().filter(|l| l.contains("error") || l.contains("Error") || l.contains("FAIL")).count();
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
    fn manifest(&self) -> &AgentManifest { &MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        if let Some(log) = Self::read_last_boot_log() {
            let diagnostics = Self::analyze_log(&log);
            for (kind, msg) in &diagnostics {
                serial_println!("[BOOT-LOG-AGENT] {}: {}", kind, msg);
                
                // Se detectou panic, acionar Self-Heal
                if *kind == "PANIC" || *kind == "GPU_HUNG" {
                    let ctx = crate::self_heal::ErrorContext {
                        kind,
                        message: msg.clone(),
                        file: alloc::string::String::from("boot_log"),
                        line: 0,
                        ring: 0,
                        daemon: alloc::string::String::from("boot_log_agent"),
                        tick: _tick,
                    };
                    let mut heal = crate::SELF_HEAL.lock();
                    heal.analyze(&ctx, true);
                    drop(heal);
                }
            }
        }
        AgentTickResult::Pending
    }
}
