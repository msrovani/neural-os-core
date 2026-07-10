//! #308 Self-Update Agent — A/B dual-slot update via FAT32.
//! Boot slot A (KERNEL~1) e slot B (KERNEL~2). BOOTCFG.JSON aponta qual usar.

use alloc::string::String;
use alloc::vec::Vec;
use crate::ATA_DRIVER;
use crate::kjson;

const SLOT_A: &str = "KERNEL~1";
const SLOT_B: &str = "KERNEL~2";
const BOOT_CFG: &str = "BOOTCFG~1";

pub enum UpdateChannel { Stable, Nightly, Security }

pub struct SelfUpdate;

impl SelfUpdate {
    /// Detecta qual slot esta ativo lendo BOOTCFG~1 da FAT32
    pub fn active_slot() -> u8 {
        let ata = ATA_DRIVER.lock();
        let ata = match ata.as_ref() { Some(a) => a, None => return 1 };
        let parts = unsafe { crate::fat32::read_mbr(ata) };
        for part in &parts {
            if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C {
                let fs = unsafe { crate::fat32::Fat32Reader::new(ata, part) };
                if let Some(fs) = fs {
                    if let Some(cfg) = unsafe { fs.read_file(BOOT_CFG) } {
                        let text = core::str::from_utf8(&cfg).unwrap_or("");
                        if text.contains("slot_b") { return 2; }
                    }
                }
                break;
            }
        }
        1
    }

    /// Ativa o outro slot para o proximo boot
    pub fn switch_slot() -> bool {
        let current = Self::active_slot();
        let next = if current == 1 { 2 } else { 1 };
        let next_name = if next == 1 { SLOT_A } else { SLOT_B };
        let cfg_text = alloc::format!("{{\"boot_slot\":\"{}\",\"kernel\":\"{}\"}}", next, next_name);
        Self::write_bootcfg(&cfg_text)
    }

    /// Rollback: volta para o slot anterior
    pub fn rollback() -> bool {
        let current = Self::active_slot();
        let fallback = if current == 1 { 2 } else { 1 };
        let fb_name = if fallback == 1 { SLOT_A } else { SLOT_B };
        let cfg_text = alloc::format!("{{\"boot_slot\":\"{}\",\"kernel\":\"{}.bak\"}}", fallback, fb_name);
        Self::write_bootcfg(&cfg_text)
    }

    /// Nova atualizacao recebida do canal — salva no slot inativo
    pub fn apply_update(data: &[u8]) -> bool {
        let slot = if Self::active_slot() == 1 { SLOT_B } else { SLOT_A };
        Self::write_kernel(slot, data)
    }

    fn write_bootcfg(text: &str) -> bool {
        let ata = ATA_DRIVER.lock();
        let ata = match ata.as_ref() { Some(a) => a, None => return false };
        let parts = unsafe { crate::fat32::read_mbr(ata) };
        for part in &parts {
            if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C {
                if let Some(w) = unsafe { crate::fat32::Fat32Writer::new(ata, part) } {
                    unsafe { w.write_file(BOOT_CFG, text.as_bytes()); }
                    kjson!("UPDATE", "BOOTCFG", "written", "slot", alloc::format!("\"{}\"", text));
                    return true;
                }
                break;
            }
        }
        false
    }

    fn write_kernel(name: &str, data: &[u8]) -> bool {
        let ata = ATA_DRIVER.lock();
        let ata = match ata.as_ref() { Some(a) => a, None => return false };
        let parts = unsafe { crate::fat32::read_mbr(ata) };
        for part in &parts {
            if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C {
                if let Some(w) = unsafe { crate::fat32::Fat32Writer::new(ata, part) } {
                    unsafe { w.write_file(name, data); }
                    kjson!("UPDATE", "KERNEL", "written", "slot", name);
                    return true;
                }
                break;
            }
        }
        false
    }

    pub fn channel_name(ch: &UpdateChannel) -> &'static str {
        match ch { UpdateChannel::Stable => "stable", UpdateChannel::Nightly => "nightly", UpdateChannel::Security => "security" }
    }

    pub fn status(&self) -> String {
        let slot = Self::active_slot();
        alloc::format!("[UPDATE] Active slot: {} (BOOTCFG~1), A/B switching ready", slot)
    }
}
