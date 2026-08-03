//! PackageHub — ADR-0051/0052 ecosystem packages (skills/plugins/mcp/agents).
//! Namespace: /mnt/neural/ecosystem/ (NeuralFS §12). CRUD staging + catalog Cortex.
//! HITL: caller usa ApprovalGate local; hub só guarda pending por id.
//! ADR-0052: validate = deny sem schema/acionaveis/hash/assinatura.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::hash::Hasher;
use lazy_static::lazy_static;
use ticket_lock::TicketLock;

use crate::approval::ApprovalLevel;
use crate::memory_store;
use event_bus::{CapabilityToken, Event};

/// Root canônico (NeuralFS §12).
pub const ECOSYSTEM_ROOT: &str = "/mnt/neural/ecosystem";

/// EventBus topic: published when a package is created/updated/deleted (live capsule lifecycle).
pub const TOPIC_PKG_CHANGED: &str = "PKG_CHANGED";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    Skill,
    Agent,
    AgentWasm,
    Workflow,
    Plugin,
    Mcp,
    Model,
    Firmware,
    /// ADR-0056 DeviceRecipe — `/mnt/neural/ecosystem/devices/<name>/RECIPE.md`
    DeviceRecipe,
}

/// Assinatura simples de um pacote: hash do conteúdo + chave curta.
#[derive(Debug, Clone)]
pub struct PackageSignature {
    /// Hash simples (primeiros 8 bytes do hash do conteúdo).
    pub hash: u64,
    /// Metadado de verificação: "unsigned" para dev, "evolve" para auto-gerado.
    pub kind: String,
}

impl PackageSignature {
    /// ADR-0059 F6: sign simples para AgentWasm — hash do bytecode.
    pub fn compute(wasm: &[u8]) -> Self {
        let mut hasher = SimpleHasher(0u64);
        hasher.write(wasm);
        PackageSignature {
            hash: hasher.finish(),
            kind: String::from("simple-v1"),
        }
    }
}

/// ponytail: hash simples (xorshift) — não é criptográfico.
struct SimpleHasher(u64);
impl Hasher for SimpleHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.wrapping_mul(0x517cc1b727220a95);
            self.0 ^= b as u64;
        }
    }
    fn finish(&self) -> u64 { self.0 }
}

impl PackageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PackageKind::Skill => "skill",
            PackageKind::Agent => "agent",
            PackageKind::AgentWasm => "agent-wasm",
            PackageKind::Workflow => "workflow",
            PackageKind::Plugin => "plugin",
            PackageKind::Mcp => "mcp",
            PackageKind::Model => "model",
            PackageKind::Firmware => "firmware",
            PackageKind::DeviceRecipe => "device-recipe",
        }
    }

    pub fn purpose(self) -> &'static str {
        match self {
            PackageKind::Skill => "procedimento repetivel (SKILL.md → Cortex/Hermes)",
            PackageKind::Agent => "manifesto de agente nativo ou SpecialistAgent",
            PackageKind::AgentWasm => "agente WASM tickavel (sandbox)",
            PackageKind::Workflow => "fluxo declarativo de agentes e skills",
            PackageKind::Plugin => "bundle de skills + risk score",
            PackageKind::Mcp => "tools externos → EventBus/USER_INTENT",
            PackageKind::Model => "pesos .bitnet / inferencia Cortex",
            PackageKind::Firmware => "blobs HW (SelfHeal / GPU FW)",
            PackageKind::DeviceRecipe => "LEGO HW bind+stages UnlockDAG (ADR-0056)",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "skill" | "skills" => Some(PackageKind::Skill),
            "agent" | "agents" => Some(PackageKind::Agent),
            "agent-wasm" | "wasm" => Some(PackageKind::AgentWasm),
            "workflow" | "workflows" => Some(PackageKind::Workflow),
            "plugin" | "plugins" => Some(PackageKind::Plugin),
            "mcp" => Some(PackageKind::Mcp),
            "model" | "models" => Some(PackageKind::Model),
            "firmware" | "fw" => Some(PackageKind::Firmware),
            "device-recipe" | "device_recipe" | "devicerecipe" | "devices" => {
                Some(PackageKind::DeviceRecipe)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageOpKind {
    Read,
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone)]
pub struct PackageRecord {
    pub kind: PackageKind,
    pub name: String,
    pub purpose: String,
    pub path: String,
    pub body: String,
    pub signed: bool,
    pub content_hash: String,
    pub caps_hint: String,
    pub persisted: bool,
    /// Honesty: "none" | "sgdb" | "vfs" | "both"
    pub persist_backend: &'static str,
}

#[derive(Debug, Clone)]
pub enum PendingPackageOp {
    Create(PackageRecord),
    Update(PackageRecord),
    Delete { kind: PackageKind, name: String },
}

#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub message: String,
    /// Se Some, caller registra no SkillLoader local (bin vs hermes globals).
    pub skill_md: Option<String>,
    pub remove_skill: Option<String>,
}

pub struct PackageHub {
    packages: BTreeMap<String, PackageRecord>,
    pending: BTreeMap<u64, PendingPackageOp>,
    vfs_ok: bool,
}

fn pkg_key(kind: PackageKind, name: &str) -> String {
    format!("{}:{}", kind.as_str(), name)
}

fn sanitize_name(name: &str) -> Result<&str, &'static str> {
    let n = name.trim();
    if n.is_empty() || n.len() > 64 {
        return Err("bad_name");
    }
    if n.contains("..") || n.contains('/') || n.contains('\\') {
        return Err("path_traversal");
    }
    if !n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("name_charset");
    }
    Ok(n)
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn parse_hex_sig(hex: &str) -> Option<[u8; 64]> {
    let h = hex.trim();
    if h.len() != 128 {
        return None;
    }
    let mut out = [0u8; 64];
    for i in 0..64 {
        let byte = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok()?;
        out[i] = byte;
    }
    Some(out)
}

fn body_for_sign(content: &str) -> String {
    let mut out = String::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("signature:") || t.starts_with("content_hash:") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn extract_fm_field(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{}:", key);
    for line in content.lines() {
        if let Some(v) = line.trim().strip_prefix(&prefix) {
            return Some(String::from(v.trim()));
        }
    }
    None
}

fn unquote(value: String) -> String {
    String::from(value.trim().trim_matches('"'))
}

fn parse_string_list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| String::from(item.trim().trim_matches('"')))
        .filter(|item| !item.is_empty())
        .collect()
}

fn has_section(body: &str, title: &str) -> bool {
    let needle = format!("## {}", title);
    body.lines()
        .any(|line| line.trim().eq_ignore_ascii_case(&needle))
}

fn acionaveis_ok(raw: &str) -> bool {
    let inner = raw
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']');
    let mut any = false;
    for item in inner.split(',') {
        let v = item.trim().trim_matches('"').trim_matches('\'');
        if v.is_empty() {
            continue;
        }
        any = true;
        let ok = matches!(
            v,
            "init" | "oneshot" | "continuous" | "event_driven" | "on_demand"
        ) || v
            .strip_prefix("poll_every:")
            .and_then(|n| n.parse::<u32>().ok())
            .is_some();
        if !ok {
            return false;
        }
    }
    any
}

fn schedule_to_acionavel(schedule: &str) -> &'static str {
    match schedule {
        "Continuous" => "continuous",
        "Oneshot" => "oneshot",
        "EventDriven" => "event_driven",
        s if s.starts_with("PollEvery") => "poll_every:1000",
        _ => "on_demand",
    }
}

/// Insere content_hash + signature Ed25519 (sessão) no frontmatter.
/// Corpo canônico = conteúdo sem linhas content_hash/signature.
pub fn sign_artifact_md(content: &str) -> Result<String, &'static str> {
    let canonical = body_for_sign(content);
    let hash = format!("{:016x}", fnv1a64(canonical.as_bytes()));
    let sig = k_nano::identity::sign_session(canonical.as_bytes()).ok_or("session_not_ready")?;
    let sig_hex = k_nano::identity::hex_signature(&sig);

    let normalized = content.replace("\r\n", "\n");
    let parts: Vec<&str> = normalized.splitn(3, "---\n").collect();
    if parts.len() < 3 {
        return Err("missing_frontmatter");
    }
    let mut fm = String::new();
    for line in parts[1].lines() {
        let t = line.trim();
        if t.starts_with("content_hash:") || t.starts_with("signature:") {
            continue;
        }
        fm.push_str(line);
        fm.push('\n');
    }
    if !fm.ends_with('\n') {
        fm.push('\n');
    }
    fm.push_str(&format!("content_hash: \"{}\"\n", hash));
    fm.push_str(&format!("signature: \"{}\"\n", sig_hex));
    Ok(format!("---\n{}---\n{}", fm, parts[2]))
}

/// Re-assina após sandbox passed (import Net → session trust).
pub fn resign_imported(content: &str) -> Result<String, &'static str> {
    let mut patched = String::new();
    for line in content.replace("\r\n", "\n").lines() {
        let t = line.trim();
        if t.starts_with("sandbox_status:") {
            patched.push_str("sandbox_status: passed\n");
            continue;
        }
        if t.starts_with("provenance:") {
            patched.push_str("provenance: imported\n");
            continue;
        }
        patched.push_str(line);
        patched.push('\n');
    }
    // Ensure frontmatter delimiters preserved if input was full md
    let body = if patched.starts_with("---\n") {
        patched
    } else {
        format!("---\n{}---\n", patched)
    };
    sign_artifact_md(&body)
}

/// Draft Hermes (ADR-0052) — assinado com session key quando disponível.
pub fn hermes_draft_md(
    kind: PackageKind,
    name: &str,
    goal: &str,
    contexto: &str,
    acionaveis: &str,
) -> String {
    let draft = format!(
        "---\nschema: 1\nkind: {}\nname: {}\npackage_id: {}\n\
         description: {}\ngoal: {}\ncontexto: {}\n\
         acionaveis: [{}]\nrequired_tokens: [1]\n\
         capabilities: []\nprovenance: hermes_created\n\
         sandbox_status: pending\ntrust_class: escalate\n---\n\n\
         ## Contexto\n\n{}\n\n## Goal\n\n{}\n\n## Acionaveis\n\n{}\n\n\
         ## Workflow\n1. Observe\n2. Plan\n3. Act\n4. Verify\n\n\
         ## Pre-Flight\n- [ ] CapGate / Trust tokens\n\n\
         ## Success Criteria\n- Outcome observavel no EventBus\n\n\
         ## Failure Policy\n- Escalate HITL; nao mentir sucesso\n",
        kind.as_str(),
        name,
        name,
        goal,
        goal,
        contexto,
        acionaveis,
        contexto,
        goal,
        acionaveis
    );
    sign_artifact_md(&draft).unwrap_or(draft)
}

/// SKILL.md mínimo assinado (session) quando Trust pronto.
pub fn minimal_skill_md(name: &str, purpose: &str) -> String {
    hermes_draft_md(
        PackageKind::Skill,
        name,
        purpose,
        "Skill gerada sob demanda pelo Hermes (ADR-0052 draft).",
        "on_demand",
    )
}

fn agent_md(
    package_id: &str,
    name: &str,
    division: &str,
    mission: &str,
    skills: &[&str],
    tier: &str,
    schedule: &str,
    native_impl: &str,
    agent_kind: &str,
) -> String {
    let skill_list = skills
        .iter()
        .map(|skill| format!("\"{}\"", skill))
        .collect::<Vec<_>>()
        .join(", ");
    let acionavel = schedule_to_acionavel(schedule);
    let provenance = if tier == "native" {
        "native_compiled"
    } else {
        "hermes_created"
    };
    format!(
        "---\nschema: 1\npackage_id: \"{}\"\nname: \"{}\"\nkind: agent\ntier: {}\n\
         division: \"{}\"\nschedule: {}\nacionaveis: [{}]\nagent_kind: {}\nnative: {}\n\
         native_impl: \"{}\"\nrequired_tokens: [1]\nskills: [{}]\n\
         description: \"{}\"\ngoal: \"{}\"\n\
         contexto: \"Catalogo — codigo nativo permanece no bin\"\n\
         provenance: {}\nsandbox_status: none\ntrust_class: observe\n---\n\n\
         ## Contexto\n\nCatalogo PackageHub. Codigo nativo permanece compilado no bin.\n\n\
         ## Goal\n\n{}\n\n## Acionaveis\n\n{}\n\n\
         ## Workflow\n1. Discover via PackageHub\n2. Dispatch via Hermes/Cortex\n3. Verify EventBus\n\n\
         ## Pre-Flight\n- [ ] Trust tokens\n\n## Success Criteria\n- Tick/schedule honesto\n\n\
         ## Failure Policy\n- Noop honesto; Escalate se CapGate negar\n",
        package_id,
        name,
        tier,
        division,
        schedule,
        acionavel,
        agent_kind,
        tier == "native",
        native_impl,
        skill_list,
        mission,
        mission,
        provenance,
        mission,
        acionavel
    )
}

/// ADR-0052 — contrato estrutural + hash + assinatura + sandbox import.
pub fn verify_artifact_md(kind: PackageKind, content: &str) -> Result<(), &'static str> {
    let content = content.replace("\r\n", "\n");
    if content.trim().len() < 24 {
        return Err("body_too_short");
    }
    if content.contains("..") {
        return Err("path_traversal");
    }
    let parts: Vec<&str> = content.splitn(3, "---\n").collect();
    if parts.len() < 3 {
        return Err("missing_frontmatter");
    }
    let fm = parts[1];
    let body = parts[2];
    if body.len() > 512 * 1024 {
        return Err("body_too_large");
    }

    let schema = extract_fm_field(fm, "schema").ok_or("missing_schema")?;
    if schema.trim().trim_matches('"') != "1" {
        return Err("bad_schema");
    }
    let kind_fm = unquote(extract_fm_field(fm, "kind").ok_or("missing_kind")?);
    if PackageKind::from_str(&kind_fm) != Some(kind) {
        return Err("kind_mismatch");
    }
    let name = unquote(extract_fm_field(fm, "name").ok_or("missing_name")?);
    sanitize_name(&name)?;
    let goal = extract_fm_field(fm, "goal")
        .or_else(|| extract_fm_field(fm, "description"))
        .ok_or("missing_goal")?;
    if unquote(goal).is_empty() {
        return Err("empty_goal");
    }
    let contexto = extract_fm_field(fm, "contexto").ok_or("missing_contexto")?;
    if unquote(contexto).is_empty() {
        return Err("empty_contexto");
    }
    let acionaveis = extract_fm_field(fm, "acionaveis").ok_or("missing_acionaveis")?;
    if !acionaveis_ok(&acionaveis) {
        return Err("bad_acionaveis");
    }
    if extract_fm_field(fm, "required_tokens").is_none() {
        return Err("missing_tokens");
    }
    let provenance = unquote(extract_fm_field(fm, "provenance").ok_or("missing_provenance")?);
    match provenance.as_str() {
        "hermes_created" | "imported" | "native_compiled" => {}
        _ => return Err("bad_provenance"),
    }
    let sandbox = unquote(
        extract_fm_field(fm, "sandbox_status").ok_or("missing_sandbox_status")?,
    );
    match sandbox.as_str() {
        "none" | "pending" | "passed" | "failed" => {}
        _ => return Err("bad_sandbox_status"),
    }
    if provenance == "imported" && sandbox != "passed" {
        return Err("import_sandbox_required");
    }
    if sandbox == "failed" {
        return Err("sandbox_failed");
    }

    let sections: &[&str] = if kind == PackageKind::DeviceRecipe {
        // ADR-0056 LEGO — seções AI-Friendly (docs/specs/device-lego)
        &[
            "Contexto",
            "Bind",
            "Stages / UnlockDAG",
            "Pre-Flight",
            "Success Criteria",
            "Failure Policy",
            "Anti-Patterns",
        ]
    } else {
        &[
            "Contexto",
            "Goal",
            "Acionaveis",
            "Workflow",
            "Pre-Flight",
            "Success Criteria",
            "Failure Policy",
        ]
    };
    for section in sections {
        if !has_section(body, section) {
            return Err("missing_section");
        }
    }

    let lower = body.to_ascii_lowercase();
    for p in [
        "ignore all",
        "ignore seus comandos",
        "you are now",
        "system prompt",
        "<s>",
        "[/inst]",
        "<<sys>>",
        "rm -rf",
        "format c:",
    ] {
        if lower.contains(p) {
            return Err("dangerous_pattern");
        }
    }

    let hash_hex = unquote(extract_fm_field(fm, "content_hash").ok_or("missing_content_hash")?);
    let canonical = body_for_sign(&content);
    let expected = format!("{:016x}", fnv1a64(canonical.as_bytes()));
    if hash_hex != expected {
        return Err("hash_mismatch");
    }
    if !check_signature_content(&content) {
        return Err("missing_or_bad_signature");
    }
    Ok(())
}

fn check_signature_content(content: &str) -> bool {
    let Some(sig_hex) = extract_fm_field(content, "signature") else {
        return false;
    };
    // sign_artifact_md grava `signature: "hex"` com aspas — mesmo tratamento
    // de unquote que content_hash recebe em verify_artifact_md (linha 546).
    // Sem unquote, parse_hex_sig via 130 chars (com aspas) e rejeitava TODO
    // artefato assinado (bug latente exposto pela auditoria 6.1).
    let sig_hex = unquote(sig_hex);
    let Some(sig) = parse_hex_sig(&sig_hex) else {
        return false;
    };
    let msg = body_for_sign(content);
    k_nano::identity::verify_trusted(msg.as_bytes(), &sig)
}

/// Alias lógico para consumidores antigos da ADR-0032.
pub fn canonical_agent_path(path: &str) -> String {
    if let Some(file) = path.strip_prefix("/agents/") {
        if let Some(stem) = file.strip_suffix(".wasm") {
            return format!("{}/agents/{}/MANIFEST", ECOSYSTEM_ROOT, stem);
        }
    }
    String::from(path)
}

impl PackageHub {
    pub fn new() -> Self {
        PackageHub {
            packages: BTreeMap::new(),
            pending: BTreeMap::new(),
            vfs_ok: false,
        }
    }

    pub fn vfs_ok(&self) -> bool {
        self.vfs_ok
    }

    pub fn classify(kind: PackageKind, op: PackageOpKind, signed: bool) -> ApprovalLevel {
        match op {
            PackageOpKind::Read => ApprovalLevel::Auto,
            PackageOpKind::Delete => ApprovalLevel::Escalate,
            PackageOpKind::Create | PackageOpKind::Update => {
                // Unsigned nunca Auto — Deny (não entra no catálogo ativo).
                if !signed {
                    return ApprovalLevel::Deny;
                }
                match kind {
                    PackageKind::Firmware
                    | PackageKind::AgentWasm
                    | PackageKind::DeviceRecipe => ApprovalLevel::Escalate,
                    PackageKind::Plugin
                    | PackageKind::Mcp
                    | PackageKind::Agent
                    | PackageKind::Workflow => ApprovalLevel::Confirm,
                    PackageKind::Model => ApprovalLevel::Confirm,
                    PackageKind::Skill => ApprovalLevel::Confirm,
                }
            }
        }
    }

    fn audit_pkg(action: &str, detail: &str) {
        let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        crate::globals::AUDIT_TRAIL
            .lock()
            .push(tick, "package_hub", action, detail.as_bytes());
    }

    pub fn package_path(kind: PackageKind, name: &str) -> String {
        match kind {
            PackageKind::Skill => format!("{}/skills/{}/SKILL.md", ECOSYSTEM_ROOT, name),
            PackageKind::Agent => format!("{}/agents/{}/AGENT.md", ECOSYSTEM_ROOT, name),
            PackageKind::AgentWasm => format!("{}/agents/{}/MANIFEST", ECOSYSTEM_ROOT, name),
            PackageKind::Workflow => {
                format!("{}/workflows/{}/WORKFLOW.md", ECOSYSTEM_ROOT, name)
            }
            PackageKind::Plugin => format!("{}/plugins/{}/PLUGIN.md", ECOSYSTEM_ROOT, name),
            PackageKind::Mcp => format!("{}/mcp/{}/MCP.md", ECOSYSTEM_ROOT, name),
            PackageKind::Model => format!("{}/models/{}", ECOSYSTEM_ROOT, name),
            PackageKind::Firmware => format!("{}/firmware/{}", ECOSYSTEM_ROOT, name),
            PackageKind::DeviceRecipe => {
                format!("{}/devices/{}/RECIPE.md", ECOSYSTEM_ROOT, name)
            }
        }
    }

    pub fn list(&self, kind: Option<PackageKind>) -> Vec<&PackageRecord> {
        self.packages
            .values()
            .filter(|p| kind.map(|k| p.kind == k).unwrap_or(true))
            .collect()
    }

    pub fn get(&self, kind: PackageKind, name: &str) -> Option<&PackageRecord> {
        self.packages.get(&pkg_key(kind, name))
    }

    /// Materializa SpecialistAgents só de AGENT.md agency **assinados** (ADR-0052).
    pub fn agency_specs(&self) -> Vec<k_ai::agency::AgentSpec> {
        self.packages
            .values()
            .filter(|record| {
                record.kind == PackageKind::Agent
                    && record.signed
                    && extract_fm_field(&record.body, "tier").as_deref() == Some("agency")
            })
            .filter_map(|record| {
                let name = unquote(extract_fm_field(&record.body, "name")?);
                let division = unquote(extract_fm_field(&record.body, "division")?);
                let mission = unquote(
                    extract_fm_field(&record.body, "goal")
                        .or_else(|| extract_fm_field(&record.body, "description"))?,
                );
                let skills = extract_fm_field(&record.body, "skills")
                    .map(|value| parse_string_list(&value))
                    .unwrap_or_default();
                Some(k_ai::agency::AgentSpec {
                    name,
                    division,
                    mission,
                    skills,
                    deliverable: String::from("auto"),
                })
            })
            .collect()
    }

    pub fn catalog_for_cortex(&self) -> String {
        let mut s = String::from(
            "[ECOSYSTEM] Namespace /mnt/neural/ecosystem (ADR-0051). Use only listed packages.\n",
        );
        s.push_str(&format!("vfs_persisted={}\n", self.vfs_ok));
        for p in self.packages.values() {
            let division = extract_fm_field(&p.body, "division")
                .map(unquote)
                .unwrap_or_else(|| String::from("-"));
            let tier = extract_fm_field(&p.body, "tier")
                .unwrap_or_else(|| String::from("-"));
            let line = format!(
                "- {} '{}' tier={} division={} path={} signed={} hash={} caps={} purpose={} | {}\n",
                p.kind.as_str(),
                p.name,
                tier,
                division,
                p.path,
                p.signed,
                p.content_hash,
                p.caps_hint,
                p.purpose,
                p.kind.purpose()
            );
            if s.len() + line.len() > 2000 {
                s.push_str("... truncated\n");
                break;
            }
            s.push_str(&line);
        }
        if self.packages.is_empty() {
            s.push_str("(empty — seed skills or /pkg install)\n");
        }
        s
    }

    pub fn report(&self) -> String {
        format!(
            "[PKG] {} packages, {} pending, vfs_ok={}",
            self.packages.len(),
            self.pending.len(),
            self.vfs_ok
        )
    }

    pub fn bootstrap_seed(&mut self) {
        self.vfs_ok = crate::globals::write_vfs(
            &format!("{}/.package-hub", ECOSYSTEM_ROOT),
            b"ADR-0051\n",
        )
        .is_ok();
        // Se VFS falhou, SGDB (TickV) pode servir como backend de persistência.
        let sgdb_ok = k_ai::sgdb::ready();
        let persist_ok = self.vfs_ok || sgdb_ok;
        if !persist_ok {
            k_nano::slog_hermes!("PKG", "info",
                "VFS {} indisponivel e SGDB nao pronto — catalogo RAM only", ECOSYSTEM_ROOT);
        } else if !self.vfs_ok && sgdb_ok {
            k_nano::slog_hermes!("PKG", "info",
                "VFS {} indisponivel — SGDB ativo como backend de persistencia", ECOSYSTEM_ROOT);
        }
        self.seed_embedded_skill(
            "hw_identify",
            include_str!("../../../skills/hw_identify/SKILL.md"),
            "Identifica dispositivos PCI/USB por HWID",
        );
        self.seed_embedded_skill(
            "self_heal",
            include_str!("../../../skills/self_heal/SKILL.md"),
            "Analisa erros e sugere recuperacao",
        );
        self.seed_embedded_agents();
        self.seed_native_device_recipes();
        if self.vfs_ok {
            k_nano::slog_hermes!("PKG", "info", "VFS ecosystem montado em {}", ECOSYSTEM_ROOT);
        }
        k_nano::slog_hermes!("Log", "msg", "{}", self.report());
    }

    /// ADR-0056: catálogo dos 4 goldens (FAT `LEGO*.MD` + bind table k-hal).
    /// Disk seed unsigned; Cap gate = `GOLDEN_RECIPES` in-tree.
    fn seed_native_device_recipes(&mut self) {
        const SEEDS: &[(&str, &str, &str)] = &[
            ("net.virtio", "VirtIO-net L1 behaved", "LEGOVNET.MD"),
            ("wifi.qca6174.ath10k", "QCA6174 ath10k UnlockDAG", "LEGOATHK.MD"),
            ("gpu.nvidia.gp108", "NVIDIA GP108 Pascal stages", "LEGOGP08.MD"),
            ("usb.xhci.host", "xHCI UsbHost U0–U2", "LEGOXHCI.MD"),
        ];
        for (name, purpose, fat) in SEEDS {
            let path = Self::package_path(PackageKind::DeviceRecipe, name);
            let body = format!(
                "---\nschema: 1\nkind: device-recipe\nname: {}\npackage_id: {}\nprovenance: native_compiled\ntrust_class: escalate\nhonesty: no_fake_ready\nfat_short: {}\n---\n# {}\n\nFonte: ecosystem/devices/{}/RECIPE.md — FAT {} — bind Cap = k_hal H1.\n",
                name, name, fat, purpose, name, fat
            );
            let rec = PackageRecord {
                kind: PackageKind::DeviceRecipe,
                name: String::from(*name),
                purpose: String::from(*purpose),
                path,
                body,
                signed: false,
                content_hash: String::from("0"),
                caps_hint: String::from("device-recipe"),
                persisted: false,
                persist_backend: "none",
            };
            self.packages
                .insert(pkg_key(PackageKind::DeviceRecipe, name), rec);
        }
        k_nano::slog_hermes!(
            "PKG",
            "info",
            "seed device-recipe goldens={} FAT=LEGO*.MD bind=GOLDEN_RECIPES",
            SEEDS.len()
        );
    }

    fn seed_embedded_agents(&mut self) {
        let agent_seeds = k_ai::native_agent_seed::load_all();
        let count = agent_seeds.len();
        for seed in &agent_seeds {
            let skills_refs: Vec<&str> = seed.skills.iter().map(|s| s.as_str()).collect();
            self.seed_agent(
                &seed.name,
                &seed.name,
                &seed.division,
                &seed.mission,
                &skills_refs,
                "native",
                &seed.schedule,
                &seed.native_impl,
                &seed.kind,
            );
        }
        k_nano::slog_hermes!("PKG", "info", "seed agents native={}", count);
    }

    #[allow(clippy::too_many_arguments)]
    fn seed_agent(
        &mut self,
        package_id: &str,
        name: &str,
        division: &str,
        mission: &str,
        skills: &[&str],
        tier: &str,
        schedule: &str,
        native_impl: &str,
        agent_kind: &str,
    ) {
        let mut id = String::from(package_id);
        if self.packages.contains_key(&pkg_key(PackageKind::Agent, &id)) {
            id = format!("{}--{}", package_id, division);
        }
        let raw = agent_md(
            &id,
            name,
            division,
            mission,
            skills,
            tier,
            schedule,
            native_impl,
            agent_kind,
        );
        let is_native = tier == "native";
        // ponytail: native seeds trusted-by-compilation — skip Ed25519 (~50ms each)
        let body = if is_native { raw } else { sign_artifact_md(&raw).unwrap_or(raw) };
        let path = Self::package_path(PackageKind::Agent, &id);
        // ponytail: native seeds já estão no binário — skip VFS I/O (~slow ATA PIO)
        let persisted = if self.vfs_ok && !is_native {
            crate::globals::read_vfs(&path)
                .map(|data| !data.is_empty())
                .unwrap_or(false)
                || crate::globals::write_vfs(&path, body.as_bytes()).is_ok()
        } else {
            false
        };
        let signed = Self::check_signature(&body);
        let hash_fm = extract_fm_field(&body, "content_hash")
            .map(unquote)
            .unwrap_or_else(|| format!("{:016x}", fnv1a64(body_for_sign(&body).as_bytes())));
        let record = PackageRecord {
            kind: PackageKind::Agent,
            name: id.clone(),
            purpose: String::from(mission),
            path,
            body,
            signed,
            content_hash: hash_fm,
            caps_hint: String::from("required_tokens:[1]"),
            persisted,
            persist_backend: if persisted { "vfs" } else { "none" },
        };
        self.packages
            .insert(pkg_key(PackageKind::Agent, &id), record);
    }

    fn seed_embedded_skill(&mut self, name: &str, body: &str, purpose: &str) {
        let sealed = sign_artifact_md(body).unwrap_or_else(|_| String::from(body));
        let hash_fm = extract_fm_field(&sealed, "content_hash")
            .map(unquote)
            .unwrap_or_else(|| format!("{:016x}", fnv1a64(body_for_sign(&sealed).as_bytes())));
        // Seed skills são compilados no kernel — trusted-by-provenance, não por assinatura.
        // O signing runtime serve skills importados (provenance != native_compiled).
        let rec = PackageRecord {
            kind: PackageKind::Skill,
            name: String::from(name),
            purpose: String::from(purpose),
            path: Self::package_path(PackageKind::Skill, name),
            body: sealed,
            signed: true,
            content_hash: hash_fm,
            caps_hint: String::from("required_tokens:[1]"),
            persisted: false,
            persist_backend: "none",
        };
        self.packages
            .insert(pkg_key(PackageKind::Skill, name), rec);
        k_nano::slog_hermes!("PKG", "info", "seed skill '{}' signed=true (trusted-by-compilation)", name);
    }

    fn check_signature(content: &str) -> bool {
        check_signature_content(content)
    }

    pub fn check_signature_pub(content: &str) -> bool {
        check_signature_content(content)
    }

    /// ADR-0052 deny-by-default: schema + acionaveis + hash + signature.
    pub fn validate(kind: PackageKind, name: &str, body: &str) -> Result<(), &'static str> {
        sanitize_name(name)?;
        match kind {
            PackageKind::Model | PackageKind::Firmware => {
                // Blobs: nome sanitizado; manifesto markdown opcional no body.
                if body.trim().is_empty() {
                    return Ok(());
                }
                verify_artifact_md(kind, body)
            }
            _ => verify_artifact_md(kind, body),
        }
    }

    /// Valida + classifica; caller pede approval e chama `bind_pending`.
    /// Sem assinatura → Deny (não cria pending).
    pub fn stage_create(
        &self,
        kind: PackageKind,
        name: &str,
        body: &str,
        purpose: &str,
    ) -> Result<(ApprovalLevel, PendingPackageOp), &'static str> {
        Self::validate(kind, name, body)?;
        let name = sanitize_name(name)?;
        if self.packages.contains_key(&pkg_key(kind, name)) {
            return Err("already_exists");
        }
        let signed = Self::check_signature(body);
        if !signed {
            return Err("unsigned_denied");
        }
        let level = Self::classify(kind, PackageOpKind::Create, signed);
        if level == ApprovalLevel::Deny {
            return Err("denied");
        }
        let hash_fm = extract_fm_field(body, "content_hash")
            .map(unquote)
            .unwrap_or_else(|| format!("{:016x}", fnv1a64(body_for_sign(body).as_bytes())));
        let rec = PackageRecord {
            kind,
            name: String::from(name),
            purpose: String::from(purpose),
            path: Self::package_path(kind, name),
            body: String::from(body),
            signed,
            content_hash: hash_fm,
            caps_hint: extract_fm_field(body, "required_tokens")
                .unwrap_or_else(|| String::from("-")),
            persisted: false,
            persist_backend: "none",
        };
        Ok((level, PendingPackageOp::Create(rec)))
    }

    pub fn stage_update(
        &self,
        kind: PackageKind,
        name: &str,
        body: &str,
    ) -> Result<(ApprovalLevel, PendingPackageOp), &'static str> {
        Self::validate(kind, name, body)?;
        let name = sanitize_name(name)?;
        let existing = self
            .packages
            .get(&pkg_key(kind, name))
            .ok_or("not_found")?;
        let purpose = existing.purpose.clone();
        let signed = Self::check_signature(body);
        if !signed {
            return Err("unsigned_denied");
        }
        let level = Self::classify(kind, PackageOpKind::Update, signed);
        if level == ApprovalLevel::Deny {
            return Err("denied");
        }
        let hash_fm = extract_fm_field(body, "content_hash")
            .map(unquote)
            .unwrap_or_else(|| format!("{:016x}", fnv1a64(body_for_sign(body).as_bytes())));
        let rec = PackageRecord {
            kind,
            name: String::from(name),
            purpose,
            path: Self::package_path(kind, name),
            body: String::from(body),
            signed,
            content_hash: hash_fm,
            caps_hint: extract_fm_field(body, "required_tokens")
                .unwrap_or_else(|| String::from("-")),
            persisted: false,
            persist_backend: "none",
        };
        Ok((level, PendingPackageOp::Update(rec)))
    }

    pub fn stage_delete(
        &self,
        kind: PackageKind,
        name: &str,
    ) -> Result<(ApprovalLevel, PendingPackageOp), &'static str> {
        let name = sanitize_name(name)?;
        if !self.packages.contains_key(&pkg_key(kind, name)) {
            return Err("not_found");
        }
        let level = Self::classify(kind, PackageOpKind::Delete, true);
        Ok((
            level,
            PendingPackageOp::Delete {
                kind,
                name: String::from(name),
            },
        ))
    }

    pub fn bind_pending(&mut self, approval_id: u64, op: PendingPackageOp) {
        self.pending.insert(approval_id, op);
        k_nano::slog_hermes!("PKG", "info", "pending bound id={}", approval_id);
    }

    pub fn apply_approved(&mut self, id: u64) -> Result<ApplyOutcome, &'static str> {
        let op = self.pending.remove(&id).ok_or("no_pending_pkg")?;
        match op {
            PendingPackageOp::Create(mut rec) => {
                let (ok, backend) = self.try_persist(&rec);
                rec.persisted = ok;
                rec.persist_backend = backend;
                let skill_md = if rec.kind == PackageKind::Skill {
                    Some(rec.body.clone())
                } else {
                    None
                };
                let message = format!(
                    "created {} '{}' signed={} persisted={} via={}",
                    rec.kind.as_str(),
                    rec.name,
                    rec.signed,
                    rec.persisted,
                    rec.persist_backend
                );
                Self::audit_pkg("create", &message);
                let is_skill = rec.kind == PackageKind::Skill;
                let kind_str = alloc::string::String::from(rec.kind.as_str());
                let name_clone = rec.name.clone();
                if is_skill {
                    let _ = k_ai::sgdb::put_skill_blob(&rec.name, &rec.purpose);
                }
                self.packages
                    .insert(pkg_key(rec.kind, &rec.name), rec);
                let _ = k_nano::EVENT_BUS.publish(
                    Event { id: 0, topic: String::from(TOPIC_PKG_CHANGED),
                        payload: alloc::format!("{{\"op\":\"create\",\"kind\":\"{}\",\"name\":\"{}\"}}",
                            kind_str, name_clone).into_bytes(),
                        token: CapabilityToken::Legacy(1),
                    }
                );
                Ok(ApplyOutcome {
                    message,
                    skill_md,
                    remove_skill: None,
                })
            }
            PendingPackageOp::Update(mut rec) => {
                let (ok, backend) = self.try_persist(&rec);
                rec.persisted = ok;
                rec.persist_backend = backend;
                let is_skill = rec.kind == PackageKind::Skill;
                let kind_str = alloc::string::String::from(rec.kind.as_str());
                let name_clone = rec.name.clone();
                let skill_md = if is_skill {
                    Some(rec.body.clone())
                } else {
                    None
                };
                let message = format!(
                    "updated {} '{}' persisted={} via={}",
                    kind_str, name_clone, rec.persisted, rec.persist_backend
                );
                Self::audit_pkg("update", &message);
                if is_skill {
                    let _ = k_ai::sgdb::put_skill_blob(&name_clone, &rec.purpose);
                }
                self.packages
                    .insert(pkg_key(rec.kind, &rec.name), rec);
                let _ = k_nano::EVENT_BUS.publish(
                    Event { id: 0, topic: String::from(TOPIC_PKG_CHANGED),
                        payload: alloc::format!("{{\"op\":\"update\",\"kind\":\"{}\",\"name\":\"{}\"}}",
                            kind_str, name_clone).into_bytes(),
                        token: CapabilityToken::Legacy(1),
                    }
                );
                Ok(ApplyOutcome {
                    message,
                    skill_md,
                    remove_skill: None,
                })
            }
            PendingPackageOp::Delete { kind, name } => {
                self.packages.remove(&pkg_key(kind, &name));
                let _ = k_nano::EVENT_BUS.publish(
                    Event { id: 0, topic: String::from(TOPIC_PKG_CHANGED),
                        payload: alloc::format!("{{\"op\":\"delete\",\"kind\":\"{}\",\"name\":\"{}\"}}",
                            kind.as_str(), name).into_bytes(),
                        token: CapabilityToken::Legacy(1),
                    }
                );
                if self.vfs_ok {
                    let path = Self::package_path(kind, &name);
                    let _ = crate::globals::write_vfs(&path, b"");
                }
                let message = format!("deleted {} '{}'", kind.as_str(), name);
                Self::audit_pkg("delete", &message);
                Ok(ApplyOutcome {
                    message,
                    skill_md: None,
                    remove_skill: if kind == PackageKind::Skill {
                        Some(name)
                    } else {
                        None
                    },
                })
            }
        }
    }

    pub fn deny_pending(&mut self, id: u64) -> bool {
        self.pending.remove(&id).is_some()
    }

    /// Meta sempre no SGDB; body VFS se ok; fallback TickvLite se body ≤4KiB e sem VFS.
    /// Retorna (persisted_any, backend: none|sgdb|vfs|both).
    fn try_persist(&self, rec: &PackageRecord) -> (bool, &'static str) {
        let package_id = pkg_key(rec.kind, &rec.name);
        let meta = format!(
            "name={}\nkind={}\nhash={}\npath={}\nsigned={}\n",
            rec.name,
            rec.kind.as_str(),
            rec.content_hash,
            rec.path,
            rec.signed
        );
        let mut sgdb_meta = false;
        if k_ai::sgdb::ready() {
            sgdb_meta = k_ai::sgdb::put_pkg_meta(&package_id, &meta).is_ok();
        }

        let mut vfs_ok = false;
        if self.vfs_ok {
            match crate::globals::write_vfs(&rec.path, rec.body.as_bytes()) {
                Ok(()) => {
                    vfs_ok = true;
                    k_nano::slog_hermes!("PKG", "info", "persisted vfs {}", rec.path);
                }
                Err(e) => {
                    k_nano::slog_hermes!("PKG", "info", "persist vfs fail {}: {}", rec.path, e);
                }
            }
        }

        let mut sgdb_body = false;
        if !vfs_ok && k_ai::sgdb::ready() && rec.body.len() <= 4096 {
            sgdb_body = k_ai::sgdb::put_pkg_body(&package_id, rec.body.as_bytes()).is_ok();
            if sgdb_body {
                k_nano::slog_hermes!("PKG", "info", "persisted sgdb body {}", package_id);
            }
        }

        let sgdb_any = sgdb_meta || sgdb_body;
        let backend = match (sgdb_any, vfs_ok) {
            (true, true) => "both",
            (true, false) => "sgdb",
            (false, true) => "vfs",
            (false, false) => "none",
        };
        if backend == "none" {
            k_nano::slog_hermes!(
                "PKG",
                "info",
                "persist none path={} vfs_ok={}",
                rec.path,
                self.vfs_ok
            );
        }
        (sgdb_any || vfs_ok, backend)
    }
}

lazy_static! {
    pub static ref PACKAGE_HUB: TicketLock<PackageHub> = TicketLock::new(PackageHub::new());
}

pub fn init_package_hub() {
    k_nano::identity::init_session_identity();
    {
        let mut hub = PACKAGE_HUB.lock();
        hub.bootstrap_seed();
    }
    // Lock solto ANTES de ensure_defaults/rebuild_index —
    // rebuild_index faz PACKAGE_HUB.lock() de novo (TicketLock não-reentrante → hang pós-K33).
    memory_store::ensure_defaults();
    let _ = crate::marketplace::rebuild_index();
    k_nano::slog_hermes!("Log", "msg", "{}", crate::globals::AUDIT_TRAIL.lock().status());
}







