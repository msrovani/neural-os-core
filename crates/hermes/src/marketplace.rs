//! Marketplace HANR — loja local NeuralFS + install HTTP allowlist.
//! Deny-by-default: validate + sandbox + session re-sign.

use alloc::format;
use alloc::string::String;

use crate::approval::ApprovalLevel;
use crate::package_hub::{
    resign_imported, sign_artifact_md, PackageKind, PackageHub, ECOSYSTEM_ROOT, PACKAGE_HUB,
};

/// Hosts permitidos para fetch (MVP). IP literals ou hostnames resolvidos offline.
pub const ALLOWLIST_HOSTS: &[&str] = &[
    "127.0.0.1",
    "10.0.2.2",
    "raw.githubusercontent.com",
    "cdn.jsdelivr.net",
];

pub fn list_local() -> String {
    let hub = PACKAGE_HUB.lock();
    let market = crate::skill_market::SKILL_MARKET.lock();
    let mut msg = format!(
        "[MARKET] {} packages | reputation top:\n{}",
        hub.list(None).len(),
        market.report()
    );
    msg.push_str("--- installed ---\n");
    for p in hub.list(None).into_iter().take(64) {
        msg.push_str(&format!(
            "  {} '{}' signed={} persisted={}\n",
            p.kind.as_str(),
            p.name,
            p.signed,
            p.persisted
        ));
    }
    msg
}

pub fn search(query: &str) -> String {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return String::from("[MARKET] search <query>");
    }
    let hub = PACKAGE_HUB.lock();
    let mut hits = 0u32;
    let mut msg = format!("[MARKET] search '{}'\n", q);
    for p in hub.list(None) {
        let hay = format!(
            "{} {} {} {}",
            p.name, p.purpose, p.kind.as_str(), p.body
        )
        .to_ascii_lowercase();
        if hay.contains(&q) {
            hits += 1;
            msg.push_str(&format!(
                "  {} '{}' — {}\n",
                p.kind.as_str(),
                p.name,
                if p.purpose.len() > 60 {
                    format!("{}…", &p.purpose[..57])
                } else {
                    p.purpose.clone()
                }
            ));
        }
    }
    if hits == 0 {
        msg.push_str("(no hits)\n");
    }
    msg
}

/// Install local body (já markdown) → stage + pending approval.
pub fn install_local(
    kind: PackageKind,
    name: &str,
    body: &str,
) -> Result<(ApprovalLevel, u64), &'static str> {
    let sealed = if crate::package_hub::PackageHub::check_signature_pub(body) {
        String::from(body)
    } else {
        sign_artifact_md(body).map_err(|_| "sign_failed")?
    };
    let (level, op) = PACKAGE_HUB
        .lock()
        .stage_create(kind, name, &sealed, "market install")?;
    if level == ApprovalLevel::Deny {
        return Err("denied");
    }
    let id = crate::globals::APPROVAL_GATE.lock().request(
        name,
        "marketplace",
        "INSTALL",
        level,
    );
    PACKAGE_HUB.lock().bind_pending(id, op);
    Ok((level, id))
}

pub fn promote_draft(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("empty_name");
    }
    let body = crate::package_hub::minimal_skill_md(name, "market promote");
    let (level, id) = install_local(PackageKind::Skill, name, &body)?;
    Ok(format!(
        "[MARKET] promote '{}' pending #{} level={:?} — /approve {}",
        name, id, level, id
    ))
}

pub fn remove(kind: PackageKind, name: &str) -> Result<(ApprovalLevel, u64), &'static str> {
    let (level, op) = PACKAGE_HUB.lock().stage_delete(kind, name)?;
    let id = crate::globals::APPROVAL_GATE.lock().request(
        name,
        "marketplace",
        "REMOVE",
        level,
    );
    PACKAGE_HUB.lock().bind_pending(id, op);
    Ok((level, id))
}

pub fn host_allowed(host: &str) -> bool {
    let h = host.trim().to_ascii_lowercase();
    ALLOWLIST_HOSTS.iter().any(|a| *a == h)
}

/// Parse URL http://host[:port]/path or https://host[:port]/path.
/// HTTPS supported via TLS N4 (embedded-tls).
pub fn parse_http_url(url: &str) -> Result<(String, u16, String), &'static str> {
    let u = url.trim();
    let (rest, default_port) = if u.starts_with("https://") || u.starts_with("HTTPS://") {
        (u.strip_prefix("https://").or_else(|| u.strip_prefix("HTTPS://")).ok_or("bad_https")?, 443u16)
    } else if u.starts_with("http://") || u.starts_with("HTTP://") {
        (u.strip_prefix("http://").or_else(|| u.strip_prefix("HTTP://")).ok_or("bad_http")?, 80u16)
    } else {
        return Err("only_http_or_https");
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = if let Some(i) = hostport.rfind(':') {
        let maybe = &hostport[i + 1..];
        if maybe.chars().all(|c| c.is_ascii_digit()) {
            let p: u16 = maybe.parse().map_err(|_| "bad_port")?;
            (&hostport[..i], p)
        } else {
            (hostport, default_port)
        }
    } else {
        (hostport, default_port)
    };
    if !host_allowed(host) {
        return Err("host_not_allowlisted");
    }
    Ok((String::from(host), port, String::from(path)))
}

/// Fetch allowlisted URL → validate → resign → stage install (DNS via net_bridge).
/// Supports HTTP and HTTPS (TLS N4).
pub fn install_from_url(url: &str, kind: PackageKind, name: &str) -> String {
    let (host, port, path) = match parse_http_url(url) {
        Ok(v) => v,
        Err(e) => {
            k_nano::slog_hermes!("Market", "info", "fetch=deny reason={}", e);
            return format!("[MARKET] fetch denied: {}", e);
        }
    };
    k_nano::slog_hermes!("Market", "info", "fetch host={} port={} path={}", host, port, path);
    let bytes = match crate::tls::fetch_url(url.trim()) {
        Ok(b) => b,
        Err(e) => {
            k_nano::slog_hermes!("Market", "info", "fetch=fail {}", e);
            return format!("[MARKET] fetch failed ({}) — noop honesto", e);
        }
    };
    let body = match core::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => return String::from("[MARKET] fetch: not utf8"),
    };
    // Body já vem sem headers se bridge stripou; fallback se raw
    let md = if let Some(idx) = body.find("\r\n\r\n") {
        &body[idx + 4..]
    } else if let Some(idx) = body.find("\n\n") {
        &body[idx + 2..]
    } else {
        body
    };
    let sealed = match resign_imported(md) {
        Ok(s) => s,
        Err(e) => {
            // tenta sign direto se já for manifesto válido
            match sign_artifact_md(md) {
                Ok(s) => s,
                Err(_) => {
                    k_nano::slog_hermes!("Market", "info", "fetch=fail resign={}", e);
                    return format!("[MARKET] resign failed: {}", e);
                }
            }
        }
    };
    match PackageHub::validate(kind, name, &sealed) {
        Ok(()) => {}
        Err(e) => {
            k_nano::slog_hermes!("Market", "info", "fetch=fail validate={}", e);
            return format!("[MARKET] validate failed: {}", e);
        }
    }
    match install_local(kind, name, &sealed) {
        Ok((level, id)) => {
            k_nano::slog_hermes!("Market", "info", "fetch=ok signed=true pending={} level={:?}", id, level);
            format!(
                "[MARKET] fetched+staged '{}' pending #{} — /approve {}",
                name, id, id
            )
        }
        Err(e) => format!("[MARKET] stage failed: {}", e),
    }
}

/// Search remote plugin registry via HTTP (IDEA #395).
/// ponytail: single registry URL, hardcoded host. Make configurable when >1 registry exists.
pub fn search_remote(query: &str) -> String {
    let q = query.trim();
    if q.is_empty() {
        return String::from("[MARKET] search_remote <query>");
    }
    // ponytail: registry on host bridge IP (VirtualBox 10.0.2.2, QEMU host)
    let encoded: String = q
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            c
        } else {
            '%' // ponytail: simple percent-encode for spaces etc.
        })
        .collect();
    let url = format!("http://10.0.2.2:8080/api/search?q={}", encoded);
    let bytes = match crate::tls::fetch_url(&url) {
        Ok(b) => b,
        Err(e) => return format!("[MARKET] remote search failed: {}", e),
    };
    let body = core::str::from_utf8(&bytes).unwrap_or("");
    // Strip HTTP headers
    let md = if let Some(idx) = body.find("\r\n\r\n") {
        &body[idx + 4..]
    } else if let Some(idx) = body.find("\n\n") {
        &body[idx + 2..]
    } else {
        body
    };
    format!("[MARKET] remote search '{}':\n{}", q, md)
}

/// Install com AI security scan antes de stage.
/// Bloqueia Plugin/AgentWasm com ScanVerdict::Blocked.
/// Suspicious loga alerta mas prossegue (HITL no approval gate).
pub fn install_scanned(url: &str, kind: PackageKind, name: &str) -> String {
    let _ = match parse_http_url(url) {
        Ok(v) => v,
        Err(e) => return format!("[MARKET] scan-install denied: {}", e),
    };
    let bytes = match crate::tls::fetch_url(url.trim()) {
        Ok(b) => b,
        Err(e) => return format!("[MARKET] scan-install fetch failed: {}", e),
    };

    // Run security scan for Plugin/AgentWasm kinds
    if kind == PackageKind::Plugin || kind == PackageKind::AgentWasm {
        let mut ph = crate::plugin_hub::PLUGIN_HUB.lock();
        let scan = ph.scan(name, &bytes);
        match scan.veredict {
            crate::plugin_hub::ScanVerdict::Blocked => {
                let details = scan.details.join("; ");
                k_nano::slog_hermes!("Market", "info", "scan BLOCKED '{}': {}", name, details);
                return format!("[MARKET] scan BLOCKED '{}': {}", name, details);
            }
            crate::plugin_hub::ScanVerdict::Suspicious => {
                k_nano::slog_hermes!("Market", "info", "scan SUSPICIOUS '{}': {:?}", name, scan.details);
                // Continua — approval gate pode pedir HITL
            }
            crate::plugin_hub::ScanVerdict::Safe => {}
        }
    }

    // Proceed with normal install flow
    install_from_url(url, kind, name)
}

/// Regenera INDEX.json textual no ecosystem (busca O(n) local).
pub fn rebuild_index() -> String {
    let hub = PACKAGE_HUB.lock();
    let mut idx = String::from("{\"packages\":[\n");
    let list = hub.list(None);
    for (i, p) in list.iter().enumerate() {
        if i > 0 {
            idx.push_str(",\n");
        }
        idx.push_str(&format!(
            "{{\"kind\":\"{}\",\"name\":\"{}\",\"signed\":{},\"purpose\":\"{}\"}}",
            p.kind.as_str(),
            p.name,
            p.signed,
            p.purpose.replace('"', "'")
        ));
    }
    idx.push_str("\n]}\n");
    let path = format!("{}/INDEX.json", ECOSYSTEM_ROOT);
    let ok = crate::globals::write_vfs(&path, idx.as_bytes()).is_ok();
    format!("[MARKET] INDEX.json entries={} vfs_ok={}", list.len(), ok)
}






