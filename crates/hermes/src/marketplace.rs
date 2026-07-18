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

/// Parse URL http://host[:port]/path — só HTTP plain (smoltcp).
pub fn parse_http_url(url: &str) -> Result<(String, u16, String), &'static str> {
    let u = url.trim();
    if u.starts_with("https://") {
        return Err("tls_not_ready");
    }
    let rest = u.strip_prefix("http://").ok_or("only_http")?;
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
            (hostport, 80u16)
        }
    } else {
        (hostport, 80u16)
    };
    if !host_allowed(host) {
        return Err("host_not_allowlisted");
    }
    Ok((String::from(host), port, String::from(path)))
}

/// Fetch allowlisted URL → validate → resign → stage install (DNS via net_bridge).
pub fn install_from_url(url: &str, kind: PackageKind, name: &str) -> String {
    let (host, _port, path) = match parse_http_url(url) {
        Ok(v) => v,
        Err(e) => {
            k_nano::slog_hermes!("Market", "info", "fetch=deny reason={}", e);
            return format!("[MARKET] fetch denied: {}", e);
        }
    };
    k_nano::slog_hermes!("Market", "info", "fetch host={} path={}", host, path);
    let bytes = match crate::net_bridge::http_get_url(url.trim()) {
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
