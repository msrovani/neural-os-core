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

/// SHA-256 hex do blob (S1 ADR-0086 — integridade real, não FNV).
fn sha256_hex(data: &[u8]) -> String {
    let h = k_nano::tpm::sha256(data);
    let mut s = String::with_capacity(64);
    for b in h {
        s.push_str(&alloc::format!("{:02x}", b));
    }
    s
}

/// Compara hex de hash case-insensitive (manifest pode vir em upper).
fn hex_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.chars()
            .zip(b.chars())
            .all(|(x, y)| x.eq_ignore_ascii_case(&y))
}

/// Nibble hex → 0..15, ou 0xFF se inválido.
fn hex_nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0xFF,
    }
}

/// Sanity check mínimo de ELF64 antes de promover (S6): magic + e_type + e_machine.
/// Kernel promovido que não é ELF válido nunca chega ao Limine.
fn is_valid_elf(data: &[u8]) -> bool {
    data.len() > 64
        && &data[0..4] == b"\x7fELF"
        && data[4] == 2        // ELF64
        && data[5] == 1        // LSB
        && data[16] == 2       // e_type = EXEC
        && data[18] == 0x3e    // e_machine = x86-64
}

impl SelfUpdate {
    /// HTTP(S) GET `url` → verifica sha256 (manifest) + assinatura Ed25519 do
    /// digest (auditoria #7 — obrigatória) + sanity ELF → grava slot inativo
    /// via `apply_update`. Retorna (bytes, sha256_hex).
    /// S1: blob NÃO verificado nunca entra no slot.
    pub fn fetch_update(
        url: &str,
        expected_sha256: Option<&str>,
        expected_sig: Option<&str>,
    ) -> Result<(usize, String), &'static str> {
        let data = crate::tls::fetch_url(url).map_err(|e| {
            k_nano::slog_hermes!("UPDATE", "info", "fetch=FAIL err={}", e);
            e
        })?;
        if data.is_empty() {
            k_nano::slog_hermes!("UPDATE", "info", "fetch=FAIL err=empty");
            return Err("update_empty");
        }
        let n = data.len();
        let digest = k_nano::tpm::sha256(&data);
        let hash = sha256_hex(&data);
        // S1: se o manifest anunciou sha256, exige igual — senão rejeita.
        if let Some(expected) = expected_sha256 {
            if !hex_eq(&hash, expected) {
                k_nano::slog_hermes!(
                    "UPDATE",
                    "error",
                    "fetch=REJECT hash mismatch bytes={} got={} expected={}",
                    n,
                    hash,
                    expected
                );
                return Err("hash_mismatch");
            }
        }
        // Auditoria #7: update SEM assinatura valida da release key é rejeitado.
        // Um MITM que controle o servidor pode forjar sha256 (self-consistent) —
        // a assinatura sobre o digest é o que impede instalar kernel arbitrário.
        let sig_hex = expected_sig.unwrap_or("");
        if sig_hex.len() != 128 {
            k_nano::slog_hermes!(
                "UPDATE",
                "error",
                "fetch=REJECT unsigned_update (manifest sem sig valido) bytes={}",
                n
            );
            return Err("unsigned_update");
        }
        let mut sig = [0u8; 64];
        let mut ok_hex = true;
        for (i, b) in sig_hex.as_bytes().chunks(2).enumerate() {
            let hi = hex_nibble(b[0]);
            let lo = hex_nibble(b.get(1).copied().unwrap_or(0));
            if hi == 0xFF || lo == 0xFF {
                ok_hex = false;
                break;
            }
            sig[i] = (hi << 4) | lo;
        }
        if ok_hex && !k_nano::identity::verify_update_signature(&digest, &sig) {
            ok_hex = false;
        }
        if !ok_hex {
            k_nano::slog_hermes!(
                "UPDATE",
                "error",
                "fetch=REJECT bad_signature bytes={} sha256={}",
                n,
                hash
            );
            return Err("bad_signature");
        }
        // S6: sanity de ELF mesmo com hash (protege contra blob truncado).
        if !is_valid_elf(&data) {
            k_nano::slog_hermes!(
                "UPDATE",
                "error",
                "fetch=REJECT not ELF bytes={} hash={}",
                n,
                hash
            );
            return Err("not_elf");
        }
        if !Self::apply_update(&data) {
            k_nano::slog_hermes!(
                "UPDATE",
                "info",
                "fetch=FAIL err=apply bytes={} sha256={}",
                n,
                hash
            );
            return Err("apply_failed");
        }
        k_nano::slog_hermes!(
            "UPDATE",
            "info",
            "fetch=OK bytes={} sha256={}",
            n,
            hash
        );
        Ok((n, hash))
    }

    /// Detecta qual slot esta ativo lendo BOOTCFG~1 da FAT32 (S7: todas partições)
    pub fn active_slot() -> u8 {
        match Self::read_fat_file(BOOT_CFG) {
            Some(cfg) => {
                let text = core::str::from_utf8(&cfg).unwrap_or("");
                if text.contains("slot_b") { 2 } else { 1 }
            }
            None => 1,
        }
    }

    /// Ativa o outro slot para o proximo boot.
    /// Promove o slot inativo → kernel.elf (que o Limine carrega) — U1 ADR-0086.
    /// Antes de promover, faz backup do kernel atual no slot que sai (rollback U4).
    /// S3: se o backup falha, ABORTA (não destrói o único kernel bom silenciosamente).
    pub fn switch_slot() -> bool {
        let current = Self::active_slot();
        let next = if current == 1 { 2 } else { 1 };
        let cur_name = if current == 1 { SLOT_A } else { SLOT_B };
        let next_name = if next == 1 { SLOT_A } else { SLOT_B };
        // Backup do kernel atual → slot que sai (preserva o bom p/ rollback).
        // Se falhar, aborta — promover sem backup = perder o único kernel bom.
        if let Some(cur) = Self::read_fat_file(KERNEL_ELF) {
            if !Self::write_kernel(cur_name, &cur) {
                k_nano::slog_hermes!(
                    "UPDATE",
                    "error",
                    "switch ABORT: backup do kernel atual em {} falhou",
                    cur_name
                );
                return false;
            }
        }
        if !Self::promote_slot(next_name) {
            return false;
        }
        let cfg_text = alloc::format!(
            "{{\"boot_slot\":\"{}\",\"kernel\":\"{}\",\"tries\":3,\"attempts\":0,\"last_good\":\"{}\"}}",
            next, next_name, current
        );
        Self::write_bootcfg(&cfg_text)
    }

    /// Rollback: volta `last_good` (ChromeOS-like, ADR-0100 T-030).
    /// Só se tries>0; após rollback, tries=0 impede loop.
    pub fn rollback() -> bool {
        let (active, tries, _attempts) = Self::boot_state();
        if tries == 0 {
            k_nano::slog_hermes!("UPDATE", "info", "rollback skip: no pending update (tries=0)");
            return false;
        }
        let cfg = Self::read_fat_file(BOOT_CFG);
        let text = cfg
            .as_ref()
            .and_then(|c| core::str::from_utf8(c).ok())
            .unwrap_or("");
        let (_s, _tr, _att, last_good) = parse_bootcfg(text);
        let fallback = if last_good == 1 || last_good == 2 {
            last_good
        } else if active == 1 {
            2
        } else {
            1
        };
        let fb_name = if fallback == 1 { SLOT_A } else { SLOT_B };
        if !Self::promote_slot(fb_name) {
            return false;
        }
        let cfg_text = alloc::format!(
            "{{\"boot_slot\":\"{}\",\"kernel\":\"{}\",\"tries\":0,\"attempts\":0,\"last_good\":\"{}\"}}",
            fallback, fb_name, fallback
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

    /// Lê (slot_ativo, tries, attempts) do BOOTCFG~1. Default: (1, 0, 0).
    fn boot_state() -> (u8, u8, u8) {
        let Some(cfg) = Self::read_fat_file(BOOT_CFG) else {
            return (1, 0, 0);
        };
        let text = core::str::from_utf8(&cfg).unwrap_or("");
        let (slot, tries, attempts, _) = parse_bootcfg(text);
        (slot, tries, attempts)
    }

    /// S5: marca boot bem-sucedido — zera tries/attempts (update aplicado e OK).
    /// Chamado quando o Runtime é atingido (boot OK). Evita rollback espúrio por
    /// crash não relacionado semanas depois (S5 oracle).
    pub fn mark_boot_ok() {
        let (active, tries, _) = Self::boot_state();
        if tries == 0 {
            return;
        }
        let cur_name = if active == 1 { SLOT_A } else { SLOT_B };
        let cfg_text = alloc::format!(
            "{{\"boot_slot\":\"{}\",\"kernel\":\"{}\",\"tries\":0,\"attempts\":0}}",
            active, cur_name
        );
        if Self::write_bootcfg(&cfg_text) {
            k_nano::slog_hermes!("UPDATE", "info", "mark_boot_ok: slot {} confirmado (tries zerado)", active);
        }
    }

    /// S5+S6: registra uma tentativa de boot com update pendente.
    /// Se attempts >= MAX (kernel novo não confirmou N boots), força rollback.
    /// Robusto a hang/early-panic onde o SelfHeal nem roda (S6 oracle).
    pub fn note_boot_attempt(max_attempts: u8) -> bool {
        let (active, tries, attempts) = Self::boot_state();
        if tries == 0 {
            return true; // sem update pendente — nada a fazer
        }
        let next = attempts.saturating_add(1);
        let cur_name = if active == 1 { SLOT_A } else { SLOT_B };
        let cfg_text = alloc::format!(
            "{{\"boot_slot\":\"{}\",\"kernel\":\"{}\",\"tries\":{},\"attempts\":{}}}",
            active, cur_name, tries, next
        );
        let _ = Self::write_bootcfg(&cfg_text);
        if next >= max_attempts {
            k_nano::slog_hermes!(
                "UPDATE",
                "warn",
                "{} boots falhos com update pendente — forçando rollback",
                next
            );
            return Self::rollback();
        }
        k_nano::slog_hermes!(
            "UPDATE",
            "info",
            "boot attempt {}/{} (update pendente no slot {})",
            next,
            max_attempts,
            active
        );
        true
    }

    /// Lê o slot e grava como kernel.elf na MESMA partição FAT32 (S7: pair).
    /// ponytail: kernel.elf é o path fixo do Limine — o slot vira o kernel ativo.
    /// S6: sanity de ELF antes de promover — blob inválido nunca chega ao Limine.
    fn promote_slot(slot_name: &str) -> bool {
        let ok = with_fat_pair(|fs, w| {
            let blob = unsafe { fs.read_file(slot_name) }?;
            if !is_valid_elf(&blob) {
                k_nano::slog_hermes!(
                    "UPDATE",
                    "error",
                    "promote REJECT: slot {} nao e ELF valido (corrompido?)",
                    slot_name
                );
                return Some(false);
            }
            if unsafe { w.write_file(KERNEL_ELF, &blob) } {
                kjson!("UPDATE", "PROMOTE", "ok", "slot", slot_name, "kernel", KERNEL_ELF);
                Some(true)
            } else {
                k_nano::slog_hermes!(
                    "UPDATE",
                    "error",
                    "promote FAIL: write kernel.elf a partir de {}",
                    slot_name
                );
                Some(false)
            }
        });
        ok.unwrap_or(false)
    }

    /// Nova atualizacao recebida do canal — salva no slot inativo
    pub fn apply_update(data: &[u8]) -> bool {
        let slot = if Self::active_slot() == 1 { SLOT_B } else { SLOT_A };
        Self::write_kernel(slot, data)
    }

    /// Lê arquivo de qualquer FAT32 (todas as partições — S7).
    fn read_fat_file(name: &str) -> Option<alloc::vec::Vec<u8>> {
        with_fat_reader(|fs| unsafe { fs.read_file(name) })
    }

    fn write_bootcfg(text: &str) -> bool {
        // S7: escreve na MESMA partição (pair) — todas as FAT32 são tentadas.
        let ok = with_fat_pair(|_fs, w| {
            let ok = unsafe { w.write_file(BOOT_CFG, text.as_bytes()) };
            if ok {
                kjson!("UPDATE", "BOOTCFG", "written", "slot", alloc::format!("\"{}\"", text));
                Some(true)
            } else {
                k_nano::slog_hermes!(
                    "UPDATE",
                    "error",
                    "BOOTCFG write falhou (FAT cheia?) — tries/rollback podem nao persistir"
                );
                Some(false)
            }
        });
        ok.unwrap_or(false)
    }

    fn write_kernel(name: &str, data: &[u8]) -> bool {
        // S7: escreve na MESMA partição (pair) — todas as FAT32 são tentadas.
        let ok = with_fat_pair(|_fs, w| {
            let ok = unsafe { w.write_file(name, data) };
            if ok {
                kjson!("UPDATE", "KERNEL", "written", "slot", name);
                Some(true)
            } else {
                k_nano::slog_hermes!(
                    "UPDATE",
                    "error",
                    "KERNEL write falhou slot={} (FAT cheia?)",
                    name
                );
                Some(false)
            }
        });
        ok.unwrap_or(false)
    }

    pub fn status(&self) -> String {
        let slot = Self::active_slot();
        alloc::format!("[UPDATE] Active slot: {} (BOOTCFG~1), A/B switching ready", slot)
    }
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

/// S7: itera TODAS as partições FAT32 (0x0B/0x0C/0x1C/0xEF) — semântica única.
/// A closure roda por partição; para na primeira que retorna Some. Conserta a
/// inconsistência "primeira FAT vs procurar em todas" (bug em stick híbrido MBR).
fn with_fat_reader<T>(mut f: impl FnMut(&k_nano::fat32::Fat32Reader) -> Option<T>) -> Option<T> {
    let ata = ATA_DRIVER.lock();
    let ata = ata.as_ref()?;
    let parts = unsafe { k_nano::fat32::read_mbr(ata) };
    for part in &parts {
        if matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
            if let Some(fs) = unsafe { k_nano::fat32::Fat32Reader::new(ata, part) } {
                if let Some(v) = f(&fs) {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// S7: mesmo padrão para escrita (reader+writer da MESMA partição — promote lê
/// e grava no mesmo lugar, evitando slot numa partição e kernel.elf em outra).
fn with_fat_pair<T>(
    mut f: impl FnMut(&k_nano::fat32::Fat32Reader, &k_nano::fat32::Fat32Writer) -> Option<T>,
) -> Option<T> {
    let ata = ATA_DRIVER.lock();
    let ata = ata.as_ref()?;
    let parts = unsafe { k_nano::fat32::read_mbr(ata) };
    for part in &parts {
        if matches!(part.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
            if let Some(fs) = unsafe { k_nano::fat32::Fat32Reader::new(ata, part) } {
                if let Some(w) = unsafe { k_nano::fat32::Fat32Writer::new(ata, part) } {
                    if let Some(v) = f(&fs, &w) {
                        return Some(v);
                    }
                }
            }
        }
    }
    None
}

/// Lê `UPDATE_URL=...` do UPDATE.CFG na FAT32 (todas as partições — S7).
pub fn read_update_cfg() -> Option<String> {
    with_fat_reader(|fs| {
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
        None
    })
}

/// Extrai `"<key>":"..."` de um JSON simples (sem serde — no_std).
/// Aceita `"key":"` e `"key": "` (json.dumps do Python emite espaço após `:`).
fn json_u8(body: &str, key: &str) -> Option<u8> {
    let pat = alloc::format!("\"{}\"", key);
    let i = body.find(&pat)?;
    let after = &body[i + pat.len()..];
    let rest = after.strip_prefix(':')?.trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// (slot, tries, attempts, last_good). last_good 0 = ausente.
/// T-030: ChromeOS-like — exposto para `ota.rs` teste host.
pub fn parse_bootcfg(text: &str) -> (u8, u8, u8, u8) {
    let slot = if text.contains("\"boot_slot\":\"2\"") || text.contains("slot_b") {
        2
    } else {
        1
    };
    let tries = json_u8(text, "tries").unwrap_or(0);
    let attempts = json_u8(text, "attempts").unwrap_or(0);
    let last_good = json_field(text, "last_good")
        .and_then(|s| s.parse().ok())
        .or_else(|| json_u8(text, "last_good"))
        .unwrap_or(0);
    (slot, tries, attempts, last_good)
}

fn json_field(body: &str, key: &str) -> Option<String> {
    let pat = alloc::format!("\"{}\"", key);
    let i = body.find(&pat)?;
    let after = &body[i + pat.len()..];
    // pula `:` + espaços opcionais
    let rest = after.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
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
/// S13: lock (AtomicBool) — cron + shell + skill podem disparar em paralelo;
/// só um check por vez (evita double-download e interleave de switch_slot).
/// T-022 QEMU: TCG skip ATA (T-011) → UPDATE.CFG ilegível. Flag RAM `O` + slirp.
pub fn check_for_update_qemu_slirp() -> String {
    check_for_update_ex(Some("http://10.0.2.2:8080/UPDATE.MANIFEST"))
}

pub fn check_for_update() -> String {
    check_for_update_ex(None)
}

fn check_for_update_ex(default_url: Option<&str>) -> String {
    use core::sync::atomic::{AtomicBool, Ordering};
    static IN_PROGRESS: AtomicBool = AtomicBool::new(false);
    if IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return String::from("[UPDATE] check ja em andamento (skip)");
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = IN_PROGRESS.store(false, Ordering::SeqCst);
        }
    }
    let _guard = Guard;

    let url = match read_update_cfg() {
        Some(u) => u,
        None => match default_url {
            Some(d) => {
                k_nano::slog_hermes!(
                    "UPDATE",
                    "info",
                    "cfg=missing (TCG/ATA skip) — QEMU slirp {}",
                    d
                );
                String::from(d)
            }
            None => {
                return String::from("[UPDATE] cfg=missing (UPDATE.CFG na FAT32)");
            }
        },
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
    // S19: env!("CARGO_PKG_VERSION") é a versão do crate HERMES, não do kernel.
    // Hoje sincronizados (ambos 1.9.9) — se hermes versionar independente, o
    // compare quebra. Alternativa futura: versão embutida no BOOTCFG/kernel.elf.
    let local = env!("CARGO_PKG_VERSION");
    // S15: version ilegível logada (não só "up_to_date" silencioso)
    let rv = parse_version(&remote);
    if rv == (0, 0, 0) {
        k_nano::slog_hermes!(
            "UPDATE",
            "warn",
            "version remota ilegivel: '{}' (parse falhou) — sem update",
            remote
        );
        return alloc::format!("[UPDATE] manifest=version_ilegivel '{}'", remote);
    }
    if rv <= parse_version(local) {
        return alloc::format!("[UPDATE] up_to_date local={} remote={}", local, remote);
    }
    let Some(kurl) = json_field(text, "url") else {
        return alloc::format!("[UPDATE] newer={} manifest=no_url", remote);
    };
    // S1 + auditoria #7: hash esperado + assinatura (sig) do manifest —
    // verificação antes de gravar o slot. Update sem sig é rejeitado.
    let expected = json_field(text, "sha256");
    let sig = json_field(text, "sig");
    match SelfUpdate::fetch_update(&kurl, expected.as_deref(), sig.as_deref()) {
        Ok((n, _hash)) => {
            // U1: promove o slot inativo → kernel.elf (o Limine carrega) + BOOTCFG
            let switched = SelfUpdate::switch_slot();
            // ADR-0086 §2.8: evento de vida — o OS lembra que atualizou o cérebro
            k_ai::self_state::record_life_event(&alloc::format!(
                "update aplicado {} -> {} (promote={})",
                local,
                remote,
                if switched { "OK" } else { "FAIL" }
            ));
            // S11: persiste last_update no SELF.STATE (não só episódico)
            k_ai::self_state::write_self_state(
                k_ai::self_state::LifePhase::Residente,
                None,
                false,
                None,
                Some(&remote),
            );
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
    use super::{json_field, parse_bootcfg, parse_version};

    #[test]
    fn bootcfg_tries3_last_good() {
        let m = r#"{"boot_slot":"2","kernel":"SLOT_B","tries":3,"attempts":1,"last_good":"1"}"#;
        assert_eq!(parse_bootcfg(m), (2, 3, 1, 1));
        let old = r#"{"boot_slot":"1","tries":1}"#;
        assert_eq!(parse_bootcfg(old), (1, 1, 0, 0));
        let zero = r#"{"boot_slot":"1","tries":0,"attempts":0}"#;
        assert_eq!(parse_bootcfg(zero), (1, 0, 0, 0));
    }

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






