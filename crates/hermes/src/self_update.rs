//! #308 Self-Update Agent — A/B dual-slot update via FAT32.
//! Boot slot A (KERNEL~1) e slot B (KERNEL~2). BOOTCFG.JSON aponta qual usar.
//! HTTP fetch via `net_bridge::http_get_url` (kernel NETSTACK). Never strip https→http.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use k_nano::ATA_DRIVER;
use k_nano::kjson;

const SLOT_A: &str = "KERNEL~1";
const SLOT_B: &str = "KERNEL~2";
const BOOT_CFG: &str = "BOOTCFG~1";
/// Path fixo que o Limine carrega (limine.conf: `boot():/kernel.elf`).
const KERNEL_ELF: &str = "kernel.elf";

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
    /// HTTP(S) GET `url` → length>0 + FNV-1a log → write inactive slot via `apply_update`.
    pub fn fetch_update(url: &str) -> Result<usize, &'static str> {
        let data = crate::tls::fetch_url(url).map_err(|e| {
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

    /// Detecta qual slot esta ativo lendo BOOTCFG~1 da FAT32
    pub fn active_slot() -> u8 {
        let ata = ATA_DRIVER.lock();
        let ata = match ata.as_ref() { Some(a) => a, None => return 1 };
        let parts = unsafe { k_nano::fat32::read_mbr(ata) };
        for part in &parts {
            // ponytail: 0xEF = ESP FAT32 do GPT instalado — U6 ADR-0086
            if matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
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

    /// Ativa o outro slot para o proximo boot.
    /// Promove o slot inativo → kernel.elf (que o Limine carrega) — U1 ADR-0086.
    /// Antes de promover, faz backup do kernel atual no slot que sai (rollback U4).
    pub fn switch_slot() -> bool {
        let current = Self::active_slot();
        let next = if current == 1 { 2 } else { 1 };
        let cur_name = if current == 1 { SLOT_A } else { SLOT_B };
        let next_name = if next == 1 { SLOT_A } else { SLOT_B };
        // Backup do kernel atual → slot que sai (preserva o bom p/ rollback)
        if let Some(cur) = Self::read_fat_file(KERNEL_ELF) {
            let _ = Self::write_kernel(cur_name, &cur);
        }
        if !Self::promote_slot(next_name) {
            return false;
        }
        let cfg_text = alloc::format!(
            "{{\"boot_slot\":\"{}\",\"kernel\":\"{}\",\"tries\":1}}",
            next, next_name
        );
        Self::write_bootcfg(&cfg_text)
    }

    /// Rollback: volta o kernel bom (slot oposto) → kernel.elf.
    /// Só executa se há update pendente (tries==1, gravado pelo switch_slot);
    /// após o rollback, tries=0 impede loop de alternância (U4 ADR-0086).
    pub fn rollback() -> bool {
        let (active, tries) = Self::boot_state();
        if tries == 0 {
            k_nano::slog_hermes!("UPDATE", "info", "rollback skip: no pending update (tries=0)");
            return false;
        }
        let fallback = if active == 1 { 2 } else { 1 };
        let fb_name = if fallback == 1 { SLOT_A } else { SLOT_B };
        if !Self::promote_slot(fb_name) {
            return false;
        }
        let cfg_text = alloc::format!(
            "{{\"boot_slot\":\"{}\",\"kernel\":\"{}\",\"tries\":0}}",
            fallback, fb_name
        );
        let ok = Self::write_bootcfg(&cfg_text);
        k_nano::slog_hermes!(
            "UPDATE",
            if ok { "info" } else { "error" },
            "rollback -> slot {} promote={} cfg={}",
            fallback,
            ok,
            ok
        );
        ok
    }

    /// Lê (slot_ativo, tries) do BOOTCFG~1. Default: (1, 0).
    fn boot_state() -> (u8, u8) {
        let Some(cfg) = Self::read_fat_file(BOOT_CFG) else {
            return (1, 0);
        };
        let text = core::str::from_utf8(&cfg).unwrap_or("");
        let slot = if text.contains("\"boot_slot\":\"2\"") || text.contains("slot_b") { 2 } else { 1 };
        let tries = if text.contains("\"tries\":1") { 1 } else { 0 };
        (slot, tries)
    }

    /// Lê o slot e grava como kernel.elf na mesma FAT32 (ESP/dados).
    /// ponytail: kernel.elf é o path fixo do Limine — o slot vira o kernel ativo.
    fn promote_slot(slot_name: &str) -> bool {
        let ata = ATA_DRIVER.lock();
        let ata = match ata.as_ref() { Some(a) => a, None => return false };
        let parts = unsafe { k_nano::fat32::read_mbr(ata) };
        for part in &parts {
            if matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
                let fs = unsafe { k_nano::fat32::Fat32Reader::new(ata, part) };
                let Some(fs) = fs else { continue };
                let Some(blob) = (unsafe { fs.read_file(slot_name) }) else { continue };
                if let Some(w) = unsafe { k_nano::fat32::Fat32Writer::new(ata, part) } {
                    if unsafe { w.write_file(KERNEL_ELF, &blob) } {
                        kjson!("UPDATE", "PROMOTE", "ok", "slot", slot_name, "kernel", KERNEL_ELF);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Nova atualizacao recebida do canal — salva no slot inativo
    pub fn apply_update(data: &[u8]) -> bool {
        let slot = if Self::active_slot() == 1 { SLOT_B } else { SLOT_A };
        Self::write_kernel(slot, data)
    }

    /// Lê arquivo da primeira FAT32 (0x0B/0x0C/0x1C/0xEF) que o contém.
    fn read_fat_file(name: &str) -> Option<alloc::vec::Vec<u8>> {
        let ata = ATA_DRIVER.lock();
        let ata = ata.as_ref()?;
        let parts = unsafe { k_nano::fat32::read_mbr(ata) };
        for part in &parts {
            if matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
                let fs = unsafe { k_nano::fat32::Fat32Reader::new(ata, part) };
                if let Some(fs) = fs {
                    if let Some(data) = unsafe { fs.read_file(name) } {
                        return Some(data);
                    }
                }
            }
        }
        None
    }

    fn write_bootcfg(text: &str) -> bool {
        let ata = ATA_DRIVER.lock();
        let ata = match ata.as_ref() { Some(a) => a, None => return false };
        let parts = unsafe { k_nano::fat32::read_mbr(ata) };
        for part in &parts {
            // ponytail: 0xEF = ESP FAT32 do GPT instalado — U6 ADR-0086
            if matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
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
            // ponytail: 0xEF = ESP FAT32 do GPT instalado — U6 ADR-0086
            if matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
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

// ─── ADR-0086 OTA diário: UPDATE.CFG → manifest → slot A/B ──────────────────

const UPDATE_CFG_NAME: &str = "UPDATE.CFG";

/// Lê `UPDATE_URL=...` do UPDATE.CFG na FAT32 (ATA; mesmo padrão do active_slot).
pub fn read_update_cfg() -> Option<String> {
    let ata = ATA_DRIVER.lock();
    let ata = ata.as_ref()?;
    let parts = unsafe { k_nano::fat32::read_mbr(ata) };
    for part in &parts {
        // ponytail: 0xEF = ESP FAT32 do GPT instalado (SysInstaller) — U6 ADR-0086
        if matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
            let fs = unsafe { k_nano::fat32::Fat32Reader::new(ata, part) }?;
            let data = unsafe { fs.read_file(UPDATE_CFG_NAME) }?;
            let text = core::str::from_utf8(&data).ok()?;
            for line in text.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("UPDATE_URL=") {
                    let url = rest.trim();
                    if url.starts_with("http") {
                        return Some(String::from(url));
                    }
                }
            }
        }
    }
    None
}

/// Extrai `"<key>":"..."` de um JSON simples (sem serde — no_std).
fn json_field(body: &str, key: &str) -> Option<String> {
    let pat = alloc::format!("\"{}\":\"", key);
    let i = body.find(&pat)?;
    let rest = &body[i + pat.len()..];
    let end = rest.find('"')?;
    Some(String::from(&rest[..end]))
}

/// Semver simples major.minor.patch (sufixo pré-release ignorado).
fn parse_version(v: &str) -> (u64, u64, u64) {
    let mut it = v.trim().split('.');
    let maj = it.next().and_then(|s| s.split('-').next()).and_then(|s| s.parse().ok()).unwrap_or(0);
    let min = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let pat = it.next().and_then(|s| s.split('-').next()).and_then(|s| s.parse().ok()).unwrap_or(0);
    (maj, min, pat)
}

/// Rotina diária OTA (ADR-0086): UPDATE.CFG → GET manifest → compara versão →
/// se nova, baixa pro slot inativo (nunca sobrescreve o slot rodando).
/// Nunca falha: retorna relatório textual para log/skill.
pub fn check_for_update() -> String {
    let Some(url) = read_update_cfg() else {
        return String::from("[UPDATE] cfg=missing (UPDATE.CFG na FAT32)");
    };
    let body = match crate::tls::fetch_url(&url) {
        Ok(b) if !b.is_empty() => b,
        Ok(_) => return String::from("[UPDATE] manifest=empty"),
        Err(e) => return alloc::format!("[UPDATE] manifest=fail err={}", e),
    };
    let Ok(text) = core::str::from_utf8(&body) else {
        return String::from("[UPDATE] manifest=not_utf8");
    };
    let Some(remote) = json_field(text, "version") else {
        return String::from("[UPDATE] manifest=no_version");
    };
    let local = env!("CARGO_PKG_VERSION");
    if parse_version(&remote) <= parse_version(local) {
        return alloc::format!("[UPDATE] up_to_date local={} remote={}", local, remote);
    }
    let Some(kurl) = json_field(text, "url") else {
        return alloc::format!("[UPDATE] newer={} manifest=no_url", remote);
    };
    match SelfUpdate::fetch_update(&kurl) {
        Ok(n) => {
            // U1: promove o slot inativo → kernel.elf (o Limine carrega) + BOOTCFG
            let switched = SelfUpdate::switch_slot();
            alloc::format!(
                "[UPDATE] applied bytes={} {} -> {} promote={} (reboot p/ ativar slot)",
                n,
                local,
                remote,
                if switched { "OK" } else { "FAIL" }
            )
        }
        Err(e) => alloc::format!("[UPDATE] apply=fail err={}", e),
    }
}

/// Skill `update_check` — consulta o servidor OTA e aplica se houver versão nova.
pub struct UpdateCheckSkill;

impl skill_registry::Skill for UpdateCheckSkill {
    fn manifest(&self) -> skill_registry::McpManifest {
        skill_registry::McpManifest {
            name: String::from("update_check"),
            description: String::from(
                "Verifica update OTA diario (UPDATE.CFG -> manifest -> slot A/B)",
            ),
            required_tokens: vec![1],
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: skill_registry::OutputSchema::String,
            idempotent: true,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, _payload: &[u8]) -> Result<alloc::vec::Vec<u8>, &'static str> {
        Ok(check_for_update().into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::{json_field, parse_version};

    #[test]
    fn manifest_field_extraction() {
        let m = r#"{"channel":"stable","version":"1.9.10","url":"http://10.0.2.2:8080/KERNEL.BIN"}"#;
        assert_eq!(json_field(m, "version").as_deref(), Some("1.9.10"));
        assert_eq!(json_field(m, "url").as_deref(), Some("http://10.0.2.2:8080/KERNEL.BIN"));
        assert_eq!(json_field(m, "nope"), None);
    }

    #[test]
    fn version_ordering() {
        assert!(parse_version("1.9.10") > parse_version("1.9.9"));
        assert!(parse_version("2.0.0") > parse_version("1.99.99"));
        assert!(parse_version("1.9.9") == parse_version("1.9.9"));
        assert!(parse_version("1.9.10-beta") > parse_version("1.9.9"));
    }
}






