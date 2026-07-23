//! Memória persistente HANR-superior — USER / MEMORY / SOUL(Hermes) / PERSONA(Jarbas).
//! Progressive skills L0 + capability gating via cognitive_bridge.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub const USER_PATH: &str = "/mnt/neural/USER.md";
pub const MEMORY_PATH: &str = "/mnt/neural/MEMORY.md";
pub const SOUL_PATH: &str = "/mnt/neural/SOUL.md";
/// Persona de interação — só Jarbas (tom/voz/FB). Hermes não usa para orquestrar.
pub const PERSONA_PATH: &str = "/mnt/neural/PERSONA.md";

pub const USER_MAX: usize = 1375;
pub const MEMORY_MAX: usize = 2200;
pub const SOUL_MAX: usize = 1200;
pub const PERSONA_MAX: usize = 800;

pub fn clamp_public(s: &str, max: usize) -> String {
    clamp_chars(s, max)
}

fn clamp_chars(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max {
            break;
        }
        out.push(ch);
    }
    out
}

fn read_path(path: &str) -> String {
    match crate::globals::read_vfs(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}

fn write_path(path: &str, body: &str) -> Result<(), &'static str> {
    crate::globals::write_vfs(path, body.as_bytes())
}

/// Lê HANR: SGDB primeiro; miss → VFS e hydrate SGDB.
fn read_hanr(name: &str, vfs_path: &str) -> String {
    if let Ok(Some(s)) = k_ai::sgdb::get_hanr(name) {
        if !s.is_empty() {
            return s;
        }
    }
    let from_vfs = read_path(vfs_path);
    if !from_vfs.is_empty() && k_ai::sgdb::ready() {
        let _ = k_ai::sgdb::put_hanr(name, &from_vfs);
        k_nano::slog_hermes!("sgdb", "hanr", "hydrate {} from vfs", name);
    }
    from_vfs
}

/// Escreve HANR: SGDB sempre (se ready); VFS best-effort.
fn write_hanr(name: &str, vfs_path: &str, body: &str) -> Result<(), &'static str> {
    let mut sgdb_ok = false;
    if k_ai::sgdb::ready() {
        match k_ai::sgdb::put_hanr(name, body) {
            Ok(()) => sgdb_ok = true,
            Err(e) => k_nano::slog_hermes!("sgdb", "hanr", "put {} FAIL {}", name, e),
        }
    }
    let vfs_ok = write_path(vfs_path, body).is_ok();
    if sgdb_ok || vfs_ok {
        k_nano::slog_hermes!(
            "sgdb",
            "hanr",
            "write {} sgdb={} vfs={}",
            name,
            sgdb_ok,
            vfs_ok
        );
        Ok(())
    } else {
        Err("hanr persist fail (no sgdb/vfs)")
    }
}

pub fn read_user() -> String {
    read_hanr("user", USER_PATH)
}
pub fn read_memory() -> String {
    read_hanr("memory", MEMORY_PATH)
}
pub fn read_soul() -> String {
    read_hanr("soul", SOUL_PATH)
}
pub fn read_persona() -> String {
    read_hanr("persona", PERSONA_PATH)
}

pub fn write_user(body: &str) -> Result<(), &'static str> {
    write_hanr("user", USER_PATH, &clamp_chars(body, USER_MAX))
}
pub fn write_memory(body: &str) -> Result<(), &'static str> {
    write_hanr("memory", MEMORY_PATH, &clamp_chars(body, MEMORY_MAX))
}
pub fn write_soul(body: &str) -> Result<(), &'static str> {
    write_hanr("soul", SOUL_PATH, &clamp_chars(body, SOUL_MAX))
}
pub fn write_persona(body: &str) -> Result<(), &'static str> {
    write_hanr("persona", PERSONA_PATH, &clamp_chars(body, PERSONA_MAX))
}

pub fn remember(fact: &str) -> Result<String, &'static str> {
    let mut cur = read_memory();
    if !cur.is_empty() && !cur.ends_with('\n') {
        cur.push('\n');
    }
    cur.push_str("- ");
    cur.push_str(fact.trim());
    cur.push('\n');
    let clamped = clamp_chars(&cur, MEMORY_MAX);
    write_memory(&clamped)?;
    Ok(format!("[MEMORY] saved ({} chars)", clamped.len()))
}

/// Fatia Cortex: SOUL operacional + USER + MEMORY (não PERSONA — isso é Jarbas).
pub fn prompt_slice() -> String {
    let mut s = String::from("[HANR-MEMORY+]\n");
    let soul = read_soul();
    if !soul.trim().is_empty() {
        s.push_str("SOUL(Hermes/orchestrator):\n");
        s.push_str(&clamp_chars(soul.trim(), 400));
        s.push('\n');
    }
    let user = read_user();
    if !user.trim().is_empty() {
        s.push_str("USER:\n");
        s.push_str(&clamp_chars(user.trim(), 400));
        s.push('\n');
    }
    let mem = read_memory();
    if !mem.trim().is_empty() {
        s.push_str("MEMORY:\n");
        s.push_str(&clamp_chars(mem.trim(), 600));
        s.push('\n');
    }
    if s.len() < 24 {
        s.push_str("(empty — /remember | /soul | /persona)\n");
    }
    s
}

/// Fatia só para Jarbas (tom/voz).
pub fn persona_slice() -> String {
    let p = read_persona();
    if p.trim().is_empty() {
        String::from("name: Hermes\ntone: precise\nhumor: 0.3\nformality: 0.4\nempathy: 0.7\n")
    } else {
        clamp_chars(p.trim(), PERSONA_MAX)
    }
}

pub fn ensure_defaults() {
    if read_soul().trim().is_empty() {
        let _ = write_soul(
            "# SOUL — Hermes orchestrator\n\
             Thoughtful. Precise. Alive.\n\
             Orquestro Agents/Skills com Trust, CapGate e HITL via Jarbas.\n\
             Fail-closed; nunca minto sucesso; Escalate quando incerto.\n",
        );
    }
    if read_persona().trim().is_empty() {
        let _ = write_persona(
            "name: Hermes\n\
             tone: precise\n\
             humor: 0.35\n\
             formality: 0.4\n\
             empathy: 0.75\n\
             # PERSONA — só Jarbas (voz/FB/avatar). Não é política de orquestração.\n",
        );
    }
    if read_user().trim().is_empty() {
        let _ = write_user("# USER\nPreferencias e perfil do operador.\n");
    }
    if read_memory().trim().is_empty() {
        let _ = write_memory("# MEMORY\n- Boot Neural OS\n");
    }
}

fn skill_lines() -> Vec<(String, String, f32, String)> {
    let mut lines: Vec<(String, String, f32, String)> = Vec::new();
    {
        let hub = crate::package_hub::PACKAGE_HUB.lock();
        for p in hub.list(Some(crate::package_hub::PackageKind::Skill)) {
            if !crate::cognitive_bridge::skill_visible(&p.body) {
                continue;
            }
            let desc = if p.purpose.len() > 60 {
                format!("{}…", &p.purpose[..57])
            } else {
                p.purpose.clone()
            };
            lines.push((p.name.clone(), desc, 0.0, p.body.clone()));
        }
    }
    {
        let storage = crate::globals::SKILL_STORAGE.lock();
        for (n, d, _) in storage.list_skills() {
            if lines.iter().any(|(name, _, _, _)| name == &n) {
                continue;
            }
            let desc = if d.len() > 60 {
                format!("{}…", &d[..57.min(d.len())])
            } else {
                d
            };
            lines.push((n, desc, 0.0, String::new()));
        }
    }
    {
        let market = crate::skill_market::SKILL_MARKET.lock();
        for score in market.top_skills(64) {
            if let Some(entry) = lines.iter_mut().find(|(n, _, _, _)| n == &score.skill) {
                entry.2 = score.success_rate;
            }
        }
    }
    lines.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(core::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    lines
}

pub fn skills_l0() -> String {
    skills_l0_gated()
}

/// L0 com capability gating (HANR requires_toolsets — superior: CapGate-aware).
pub fn skills_l0_gated() -> String {
    let lines = skill_lines();
    let mut out = String::from("[SKILLS L0 gated] name — description\n");
    for (n, d, rate, _) in lines.iter().take(48) {
        if *rate > 0.0 {
            out.push_str(&format!("- {} — {} [{:.0}%]\n", n, d, rate * 100.0));
        } else {
            out.push_str(&format!("- {} — {}\n", n, d));
        }
    }
    if lines.is_empty() {
        out.push_str("(none)\n");
    }
    out.push_str("Use /skill <name> for L1.\n");
    out
}

pub fn skill_view(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return String::from("[SKILL] name required");
    }
    {
        let hub = crate::package_hub::PACKAGE_HUB.lock();
        if let Some(p) = hub.get(crate::package_hub::PackageKind::Skill, name) {
            if !crate::cognitive_bridge::skill_visible(&p.body) {
                return format!(
                    "[SKILL] '{}' hidden — capabilities not met (CapGate)",
                    name
                );
            }
            return format!("[SKILL L1] {} signed={}\n{}", p.name, p.signed, p.body);
        }
    }
    let storage = crate::globals::SKILL_STORAGE.lock();
    for s in &storage.skills {
        if s.name == name {
            return format!("[SKILL L1] {}\n{}", s.name, s.to_skill_md());
        }
    }
    format!("[SKILL] '{}' not found — try /skills", name)
}
