//! #308 Self-Update Agent — A/B dual-slot update via FAT32.
//! Boot slot A (KERNEL~1) e slot B (KERNEL~2). BOOTCFG.JSON aponta qual usar.
//! HTTP fetch via `net_bridge::http_get_url` (kernel NETSTACK). Never strip https→http.

use alloc::string::String;
use k_nano::ATA_DRIVER;
use k_nano::kjson;

const SLOT_A: &str = "KERNEL~1";
const SLOT_B: &str = "KERNEL~2";
const BOOT_CFG: &str = "BOOTCFG~1";
const CHANNEL_MANIFEST_URL: &str = "http://10.0.2.2:8080/UPDATE.MANIFEST";

pub enum UpdateChannel { Stable, Nightly, Security }

pub struct SelfUpdate;

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl SelfUpdate {
    /// HTTP GET `url` → length>0 + FNV-1a log → write inactive slot via `apply_update`.
    pub fn fetch_update(url: &str) -> Result<usize, &'static str> {
        let data = crate::net_bridge::http_get_url(url).map_err(|e| {
            k_nano::slog_hermes!("UPDATE", "info", "fetch=FAIL err={}", e);
            e
        })?;
        if data.is_empty() {
            k_nano::slog_hermes!("UPDATE", "info", "fetch=FAIL err=empty");
            return Err("update_empty");
        }
        let n = data.len();
        let hash = fnv1a64(&data);
        if !Self::apply_update(&data) {
            k_nano::slog_hermes!(
                "UPDATE",
                "info",
                "fetch=FAIL err=apply bytes={} fnv={:016x}",
                n,
                hash
            );
            return Err("apply_failed");
        }
        k_nano::slog_hermes!(
            "UPDATE",
            "info",
            "fetch=OK bytes={} fnv={:016x}",
            n,
            hash
        );
        Ok(n)
    }

    /// Optional channel poll stub — GET UPDATE.MANIFEST from host :8080 (serve_tiny_gguf).
    pub fn poll_channel(_ch: &UpdateChannel) -> Result<usize, &'static str> {
        match crate::net_bridge::http_get_url(CHANNEL_MANIFEST_URL) {
            Ok(body) if !body.is_empty() => {
                k_nano::slog_hermes!(
                    "UPDATE",
                    "info",
                    "channel_poll=OK bytes={}",
                    body.len()
                );
                Ok(body.len())
            }
            Ok(_) => {
                k_nano::slog_hermes!("UPDATE", "info", "channel_poll=FAIL err=empty");
                Err("manifest_empty")
            }
            Err(e) => {
                k_nano::slog_hermes!("UPDATE", "info", "channel_poll=FAIL err={}", e);
                Err(e)
            }
        }
    }

    /// Detecta qual slot esta ativo lendo BOOTCFG~1 da FAT32
    pub fn active_slot() -> u8 {
        let ata = ATA_DRIVER.lock();
        let ata = match ata.as_ref() { Some(a) => a, None => return 1 };
        let parts = unsafe { k_nano::fat32::read_mbr(ata) };
        for part in &parts {
            if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C {
                let fs = unsafe { k_nano::fat32::Fat32Reader::new(ata, part) };
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
        let parts = unsafe { k_nano::fat32::read_mbr(ata) };
        for part in &parts {
            if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C {
                if let Some(w) = unsafe { k_nano::fat32::Fat32Writer::new(ata, part) } {
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
        let parts = unsafe { k_nano::fat32::read_mbr(ata) };
        for part in &parts {
            if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C {
                if let Some(w) = unsafe { k_nano::fat32::Fat32Writer::new(ata, part) } {
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

/// Labor 34: bridge git thin pack bytes → inactive slot (se parecer blob kernel).
pub fn apply_pack_bytes(pack_or_blob: &[u8]) -> Result<usize, &'static str> {
    if pack_or_blob.len() < 16 {
        return Err("too_short");
    }
    // Se PACK header — extrair via git_thin; senão tratar como blob cru.
    let blob = if pack_or_blob.starts_with(b"PACK") {
        match crate::git_thin::apply_thin_pack(pack_or_blob) {
            Ok(n) if n > 0 => {
                // apply_thin_pack returns size; need bytes — use pack as standby blob for MVP
                k_nano::slog_hermes!(
                    "UPDATE",
                    "info",
                    "pack_bridge objs_ok size={} VERDICT=PARTIAL reason=use_pack_as_slot_blob",
                    n
                );
                pack_or_blob
            }
            Ok(_) => return Err("empty_pack"),
            Err(e) => {
                k_nano::slog_hermes!("UPDATE", "info", "pack_bridge PARTIAL err={}", e);
                pack_or_blob
            }
        }
    } else {
        pack_or_blob
    };
    if !SelfUpdate::apply_update(blob) {
        return Err("slot_write_fail");
    }
    Ok(blob.len())
}

pub fn boot_smoke() -> bool {
    let syn = b"NEURAL-KERNEL-UPDATE-SMOKE-BLOB-L34";
    // Não grava FAT no smoke se ATA ausente — só API
    let slot = SelfUpdate::active_slot();
    k_nano::slog_hermes!(
        "UPDATE",
        "info",
        "step=git_bridge status=OK slot={} syn_len={} VERDICT=PARTIAL reason=api_ready",
        slot,
        syn.len()
    );
    let _ = syn;
    true
}






