//! Skill Manifest — schema canônico para descrever skills (ADR-0076 Onda 1).
//! Compatível com FYY Skill Manifest v1 spec, Anthropic Agent Skills, MCP, A2A.
//! Toda skill no PackageHub deve ter um SkillManifest.
//!
//! AIOS na veia: skills descritas por dados, não código.

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

// ─── Tipos FYY-compatíveis ───

/// Nível de risco da skill (FYY: risk_level).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "low" => RiskLevel::Low,
            "medium" => RiskLevel::Medium,
            "high" => RiskLevel::High,
            "critical" => RiskLevel::Critical,
            _ => RiskLevel::Medium,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
        }
    }
}

/// Tipo da skill (FYY: type = claw | service | mcp).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillType {
    /// FYY `claw` — reasoning (skill.md) + tools (scripts/). Nosso WASM nativo.
    Claw,
    /// Service RPC (FYY `service`) — listen_port + protocol.
    Service,
    /// MCP server (FYY `mcp`) — Model Context Protocol.
    Mcp,
}

impl SkillType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "claw" | "wasm" | "wasi" => SkillType::Claw,
            "service" | "legacy" => SkillType::Service,
            "mcp" => SkillType::Mcp,
            _ => SkillType::Claw,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SkillType::Claw => "claw",
            SkillType::Service => "service",
            SkillType::Mcp => "mcp",
        }
    }
}

/// Visibilidade remota (FYY: remote.visibility).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Private,   // Grant holders only
    Follows,   // Followers + grants
    Public,    // All
}

impl Visibility {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "private" => Visibility::Private,
            "follows" => Visibility::Follows,
            "public" => Visibility::Public,
            _ => Visibility::Private,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Private => "private",
            Visibility::Follows => "follows",
            Visibility::Public => "public",
        }
    }
}

/// Modelo de precificação (FYY: pricing.model).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PricingModel {
    Free,
    OneTime,
    Subscription,
    PerCall,
    PerOutput,
    RevenueShare,
}

impl PricingModel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "free" => PricingModel::Free,
            "one_time" | "onetime" => PricingModel::OneTime,
            "subscription" => PricingModel::Subscription,
            "per_call" | "percall" => PricingModel::PerCall,
            "per_output" | "peroutput" => PricingModel::PerOutput,
            "revenue_share" | "revenueshare" => PricingModel::RevenueShare,
            _ => PricingModel::Free,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PricingModel::Free => "free",
            PricingModel::OneTime => "one_time",
            PricingModel::Subscription => "subscription",
            PricingModel::PerCall => "per_call",
            PricingModel::PerOutput => "per_output",
            PricingModel::RevenueShare => "revenue_share",
        }
    }
}

// ─── Structs aninhadas ───

/// Permissões de filesystem.
#[derive(Debug, Clone)]
pub struct FsPermissions {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

/// Permissões da skill.
#[derive(Debug, Clone)]
pub struct Permissions {
    pub filesystem: FsPermissions,
    pub network: String,       // "none" | "allow" | "proxy"
    pub hardware: String,      // "none" | "display" | "audio"
    pub network_endpoints: Vec<String>, // FYY: network_endpoints
}

/// Resource limits.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub fuel_max: u64,
    pub heap_max: u64,
    pub timeout_ms: u64,
}

/// Exposição remota (FYY: remote).
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    pub enabled: bool,
    pub visibility: Visibility,
    pub max_concurrency: u32,
    pub timeout_s: u32,
    pub rate_limit_per_minute: u32,
    pub rate_limit_per_hour: u32,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            visibility: Visibility::Private,
            max_concurrency: 10,
            timeout_s: 60,
            rate_limit_per_minute: 60,
            rate_limit_per_hour: 1000,
        }
    }
}

/// Precificação (FYY: pricing).
#[derive(Debug, Clone)]
pub struct Pricing {
    pub model: PricingModel,
    pub unit_price: f64,
    pub currency: String,
}

impl Default for Pricing {
    fn default() -> Self {
        Self {
            model: PricingModel::Free,
            unit_price: 0.0,
            currency: String::from("credit"),
        }
    }
}

/// Interoperability (FYY: interop).
#[derive(Debug, Clone)]
pub struct Interop {
    pub mcp: bool,
    pub fyy: bool,
    pub agent_skills: bool,
    pub a2a: bool,         // Google Agent-to-Agent
    pub clawhub: bool,     // ClawHub marketplace
    pub skillnet: bool,    // SkillNet knowledge graph (ZJU/OpenKG)
}

impl Default for Interop {
    fn default() -> Self {
        Self {
            mcp: true,
            fyy: false,
            agent_skills: false,
            a2a: false,
            clawhub: false,
            skillnet: false,
        }
    }
}

/// Quality indicators (FYY: quality_indicators).
#[derive(Debug, Clone)]
pub struct QualityIndicators {
    pub verified: bool,
    pub security_audit: bool,
    pub quality_score: u8,         // 0-100
    pub source_type: String,       // "official" | "community" | "experimental"
}

impl Default for QualityIndicators {
    fn default() -> Self {
        Self {
            verified: false,
            security_audit: false,
            quality_score: 0,
            source_type: String::from("community"),
        }
    }
}

// ─── Skill Manifest (canônico) ───

/// Skill Manifest — schema canônico FYY v1 compatível.
#[derive(Debug, Clone)]
pub struct SkillManifest {
    // Core (FYY obrigatório)
    pub name: String,
    pub version: String,
    pub skill_type: SkillType,
    pub description: String,

    // AI readability
    pub when_to_use: Vec<String>,
    pub category: String,           // FYY category taxonomy
    pub tags: Vec<String>,
    pub input_examples: Vec<String>,

    // Segurança
    pub risk_level: RiskLevel,
    pub permissions: Permissions,
    pub capabilities: Vec<String>,

    // Runtime
    pub resource_limits: ResourceLimits,
    pub schedule: String,           // "ondemand" | "continuous" | cron expr
    pub depends_on: Vec<String>,    // Nomes de skills necessárias
    pub health_check: String,       // "none" | "tcp:port" | "http:path"

    // Remote exposure (FYY)
    pub remote: RemoteConfig,
    pub output_schema: String,      // JSON Schema string (vazia = qualquer)

    // Comercial (FYY)
    pub pricing: Pricing,
    pub sla: String,                // "none" | "99.9" | "99.99"
    pub resellable: bool,

    // Interoperability
    pub interop: Interop,

    // Qualidade
    pub quality: QualityIndicators,

    // Sistema
    pub system_skill: bool,         // FYY: system_skill
    pub auto_install: bool,         // FYY: auto_install
}

impl SkillManifest {
    /// Cria skill Claw padrão.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: String::from(name),
            version: String::from("1.0.0"),
            skill_type: SkillType::Claw,
            description: String::from(description),
            when_to_use: Vec::new(),
            category: String::new(),
            tags: Vec::new(),
            input_examples: Vec::new(),
            risk_level: RiskLevel::Medium,
            permissions: Permissions {
                filesystem: FsPermissions {
                    allow: vec![String::from("/tmp/*")],
                    deny: Vec::new(),
                },
                network: String::from("none"),
                hardware: String::from("none"),
                network_endpoints: Vec::new(),
            },
            capabilities: Vec::new(),
            resource_limits: ResourceLimits {
                fuel_max: 1_000_000,
                heap_max: 64 * 1024 * 1024,
                timeout_ms: 30_000,
            },
            schedule: String::from("ondemand"),
            depends_on: Vec::new(),
            health_check: String::from("none"),
            remote: RemoteConfig::default(),
            output_schema: String::new(),
            pricing: Pricing::default(),
            sla: String::from("none"),
            resellable: false,
            interop: Interop::default(),
            quality: QualityIndicators::default(),
            system_skill: false,
            auto_install: false,
        }
    }

    /// Cria skill MCP.
    pub fn new_mcp(name: &str, description: &str) -> Self {
        let mut m = Self::new(name, description);
        m.skill_type = SkillType::Mcp;
        m
    }

    /// Cria skill Service.
    pub fn new_service(name: &str, description: &str) -> Self {
        let mut m = Self::new(name, description);
        m.skill_type = SkillType::Service;
        m
    }

    /// Valida o manifest — verifica campos obrigatórios.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.name.is_empty() { return Err("name is required"); }
        if self.version.is_empty() { return Err("version is required"); }
        if self.description.is_empty() { return Err("description is required"); }
        Ok(())
    }

    // ─── Serialização JSON (manual, sem serde, para no_std) ───

    /// Serializa para JSON compatível com FYY v1.
    pub fn to_json(&self) -> String {
        let when = self.when_to_use.iter()
            .map(|w| format!("\"{}\"", w))
            .collect::<Vec<_>>().join(",");
        let tags = self.tags.iter()
            .map(|t| format!("\"{}\"", t))
            .collect::<Vec<_>>().join(",");
        let caps = self.capabilities.iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>().join(",");
        let deps = self.depends_on.iter()
            .map(|d| format!("\"{}\"", d))
            .collect::<Vec<_>>().join(",");
        let examples = self.input_examples.iter()
            .map(|e| format!("\"{}\"", e))
            .collect::<Vec<_>>().join(",");
        let endpoints = self.permissions.network_endpoints.iter()
            .map(|e| format!("\"{}\"", e))
            .collect::<Vec<_>>().join(",");

        format!(
            r#"{{"name":"{}","version":"{}","type":"{}","description":"{}","category":"{}","risk_level":"{}","system_skill":{},"auto_install":{},"when_to_use":[{}],"tags":[{}],"capabilities":[{}],"depends_on":[{}],"input_examples":[{}],"schedule":"{}","health_check":"{}","output_schema":"{}","sla":"{}","resellable":{},"permissions":{{"filesystem":{{"allow":["{}"],"deny":["{}"]}},"network":"{}","hardware":"{}","network_endpoints":[{}]}},"resource_limits":{{"fuel_max":{},"heap_max":{},"timeout_ms":{}}},"remote":{{"enabled":{},"visibility":"{}","max_concurrency":{},"timeout_s":{},"rate_limit_per_minute":{},"rate_limit_per_hour":{}}},"pricing":{{"model":"{}","unit_price":{},"currency":"{}"}},"interop":{{"mcp":{},"fyy":{},"agent_skills":{},"a2a":{},"clawhub":{},"skillnet":{}}},"quality":{{"verified":{},"security_audit":{},"quality_score":{},"source_type":"{}"}}}}"#,
            self.name, self.version, self.skill_type.as_str(),
            self.description, self.category, self.risk_level.as_str(),
            bool_js(self.system_skill), bool_js(self.auto_install),
            when, tags, caps, deps, examples,
            self.schedule, self.health_check, self.output_schema,
            self.sla, bool_js(self.resellable),
            self.permissions.filesystem.allow.join(","),
            self.permissions.filesystem.deny.join(","),
            self.permissions.network, self.permissions.hardware,
            endpoints,
            self.resource_limits.fuel_max, self.resource_limits.heap_max,
            self.resource_limits.timeout_ms,
            bool_js(self.remote.enabled), self.remote.visibility.as_str(),
            self.remote.max_concurrency, self.remote.timeout_s,
            self.remote.rate_limit_per_minute, self.remote.rate_limit_per_hour,
            self.pricing.model.as_str(), self.pricing.unit_price, self.pricing.currency,
            bool_js(self.interop.mcp), bool_js(self.interop.fyy),
            bool_js(self.interop.agent_skills), bool_js(self.interop.a2a),
            bool_js(self.interop.clawhub), bool_js(self.interop.skillnet),
            bool_js(self.quality.verified), bool_js(self.quality.security_audit),
            self.quality.quality_score, self.quality.source_type,
        )
    }

    // ─── Parser JSON mínimo (from_slice) ───

    /// Faz parser de JSON simplificado para SkillManifest.
    /// Não é um parser JSON completo — cobre o subset usado pelo schema FYY.
    /// Para parser completo, usar a crate `hermes::skill_manifest::json_parser`
    /// quando disponível.
    pub fn from_slice(data: &[u8]) -> Result<Self, &'static str> {
        let s = core::str::from_utf8(data).map_err(|_| "invalid utf-8")?;
        Self::from_json_str(s)
    }

    fn extract_field<'a>(s: &'a str, field: &str) -> Option<&'a str> {
        let search = alloc::format!("\"{}\"", field);
        let start = s.find(&search)?;
        let after = start + search.len();
        // Pular `:` e whitespace
        let mut pos = after;
        while pos < s.len() && (s.as_bytes()[pos] == b':' || s.as_bytes()[pos] == b' ' || s.as_bytes()[pos] == b'\t' || s.as_bytes()[pos] == b'\n') {
            pos += 1;
        }
        if pos >= s.len() { return None; }

        let bytes = s.as_bytes();
        if bytes[pos] == b'"' {
            // String value
            pos += 1;
            let end = s[pos..].find('"')?;
            Some(&s[pos..pos + end])
        } else if bytes[pos] == b't' || bytes[pos] == b'f' {
            // Boolean
            let end = if s[pos..].starts_with("true") { pos + 4 }
                      else if s[pos..].starts_with("false") { pos + 5 }
                      else { return None };
            Some(&s[pos..end])
        } else if bytes[pos].is_ascii_digit() || bytes[pos] == b'-' {
            // Number
            let end = s[pos..].find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-').unwrap_or(s.len() - pos);
            Some(&s[pos..pos + end])
        } else {
            None
        }
    }

    /// Parse simplificado de JSON string para SkillManifest (subset FYY).
    pub fn from_json_str(s: &str) -> Result<Self, &'static str> {
        let name = Self::extract_field(s, "name").unwrap_or("").to_string();
        let version = Self::extract_field(s, "version").unwrap_or("1.0.0").to_string();
        let type_str = Self::extract_field(s, "type").unwrap_or("claw");
        let description = Self::extract_field(s, "description").unwrap_or("").to_string();
        let risk_str = Self::extract_field(s, "risk_level").unwrap_or("medium");
        let system = Self::extract_field(s, "system_skill").unwrap_or("false");
        let auto = Self::extract_field(s, "auto_install").unwrap_or("false");

        Ok(Self {
            name,
            version,
            skill_type: SkillType::from_str(type_str),
            description,
            when_to_use: Vec::new(), // parsing avançado seria necessário
            category: String::new(),
            tags: Vec::new(),
            input_examples: Vec::new(),
            risk_level: RiskLevel::from_str(risk_str),
            permissions: Permissions {
                filesystem: FsPermissions {
                    allow: Vec::new(),
                    deny: Vec::new(),
                },
                network: String::from("none"),
                hardware: String::from("none"),
                network_endpoints: Vec::new(),
            },
            capabilities: Vec::new(),
            resource_limits: ResourceLimits {
                fuel_max: 1_000_000,
                heap_max: 64 * 1024 * 1024,
                timeout_ms: 30_000,
            },
            schedule: String::from("ondemand"),
            depends_on: Vec::new(),
            health_check: String::from("none"),
            remote: RemoteConfig::default(),
            output_schema: String::new(),
            pricing: Pricing::default(),
            sla: String::from("none"),
            resellable: false,
            interop: Interop::default(),
            quality: QualityIndicators::default(),
            system_skill: system == "true",
            auto_install: auto == "true",
        })
    }

    // ─── Helpers ───

    /// Cria manifest para skill de planilha (exemplo concreto).
    pub fn office_spreadsheet() -> Self {
        let mut m = Self::new("planilha-editor", "Edita planilhas Excel (.xlsx)");
        m.when_to_use = vec![
            String::from("editar planilha"),
            String::from("modificar xlsx"),
            String::from("excel"),
            String::from("planilha"),
        ];
        m.category = String::from("productivity");
        m.tags = vec![String::from("office"), String::from("spreadsheet")];
        m.risk_level = RiskLevel::Low;
        m.permissions.filesystem.allow = vec![
            String::from("/tmp/*"),
            String::from("/home/*.xlsx"),
        ];
        m.capabilities = vec![
            String::from("vfs_read"),
            String::from("vfs_write"),
        ];
        m.resource_limits = ResourceLimits {
            fuel_max: 10_000_000,
            heap_max: 256 * 1024 * 1024,
            timeout_ms: 60_000,
        };
        m.interop.mcp = true;
        m.interop.fyy = true;
        m.interop.agent_skills = true;
        m.quality.quality_score = 80;
        m.quality.source_type = String::from("official");
        m
    }

    /// Cria manifest para agente nativo (system_skill + auto_install).
    pub fn system_agent(name: &str, description: &str, agent_id: &str) -> Self {
        let mut m = Self::new(name, description);
        m.system_skill = true;
        m.auto_install = true;
        m.risk_level = RiskLevel::Critical;
        m.permissions.hardware = String::from("allow");
        m.capabilities = vec![
            String::from("system"),
            format!("agent:{}", agent_id),
        ];
        m.quality.verified = true;
        m.quality.source_type = String::from("official");
        m
    }
}

fn bool_js(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

// ─── Testes ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_manifest_new() {
        let m = SkillManifest::new("test-skill", "A test skill");
        assert_eq!(m.name, "test-skill");
        assert_eq!(m.skill_type, SkillType::Claw);
        assert!(!m.system_skill);
        assert!(!m.auto_install);
        assert!(!m.remote.enabled);
        assert!(m.interop.mcp);
        assert!(!m.interop.fyy);
    }

    #[test]
    fn test_skill_manifest_type_mapping() {
        assert_eq!(SkillType::from_str("claw"), SkillType::Claw);
        assert_eq!(SkillType::from_str("wasm"), SkillType::Claw);
        assert_eq!(SkillType::from_str("wasi"), SkillType::Claw);
        assert_eq!(SkillType::from_str("service"), SkillType::Service);
        assert_eq!(SkillType::from_str("legacy"), SkillType::Service);
        assert_eq!(SkillType::from_str("mcp"), SkillType::Mcp);
    }

    #[test]
    fn test_skill_manifest_to_json() {
        let m = SkillManifest::office_spreadsheet();
        let json = m.to_json();
        assert!(json.contains(r#""name":"planilha-editor""#));
        assert!(json.contains(r#""type":"claw""#));
        assert!(json.contains(r#""risk_level":"low""#));
        assert!(json.contains(r#""system_skill":false"#));
        assert!(json.contains(r#""interop""#)); // interop fields
        assert!(json.contains(r#""mcp":true"#));
    }

    #[test]
    fn test_skill_manifest_from_slice() {
        let json = br#"{"name":"test","version":"1.0.0","type":"service","description":"desc","risk_level":"high"}"#;
        let m = SkillManifest::from_slice(json).unwrap();
        assert_eq!(m.name, "test");
        assert_eq!(m.version, "1.0.0");
        assert_eq!(m.skill_type, SkillType::Service);
        assert_eq!(m.risk_level, RiskLevel::High);
    }

    #[test]
    fn test_skill_manifest_system_agent() {
        let m = SkillManifest::system_agent("net-agent", "Network Agent", "A-004");
        assert!(m.system_skill);
        assert!(m.auto_install);
        assert_eq!(m.risk_level, RiskLevel::Critical);
        assert!(m.quality.verified);
    }

    #[test]
    fn test_skill_manifest_validate() {
        let m = SkillManifest::new("", "");
        assert!(m.validate().is_err());

        let m2 = SkillManifest::new("ok", "description");
        assert!(m2.validate().is_ok());
    }

    #[test]
    fn test_skill_manifest_roundtrip() {
        let m1 = SkillManifest::office_spreadsheet();
        let json = m1.to_json();
        let m2 = SkillManifest::from_json_str(&json).unwrap();
        assert_eq!(m1.name, m2.name);
        assert_eq!(m1.version, m2.version);
        assert_eq!(m1.skill_type, m2.skill_type);
        assert_eq!(m1.risk_level, m2.risk_level);
    }

    #[test]
    fn test_skill_manifest_interop_default() {
        let i = Interop::default();
        assert!(i.mcp);
        assert!(!i.fyy);
        assert!(!i.a2a);
        assert!(!i.clawhub);
        assert!(!i.skillnet);
    }

    #[test]
    fn test_skill_manifest_remote_default() {
        let r = RemoteConfig::default();
        assert!(!r.enabled);
        assert_eq!(r.max_concurrency, 10);
        assert_eq!(r.rate_limit_per_minute, 60);
    }

    #[test]
    fn test_skill_manifest_pricing_default() {
        let p = Pricing::default();
        assert_eq!(p.model, PricingModel::Free);
        assert_eq!(p.currency, "credit");
    }

    #[test]
    fn test_skill_manifest_fyy_compat() {
        // Testa que o JSON gerado tem campos que o schema FYY espera
        let mut m = SkillManifest::new("fyy-compat", "FYY compat test");
        m.interop.fyy = true;
        let json = m.to_json();
        // O JSON deve conter todos os campos do schema FYY v1
        assert!(json.contains(r#""name""#));
        assert!(json.contains(r#""version""#));
        assert!(json.contains(r#""type""#));
        assert!(json.contains(r#""description""#));
        assert!(json.contains(r#""risk_level""#));
        assert!(json.contains(r#""permissions""#));
        assert!(json.contains(r#""resource_limits""#));
        assert!(json.contains(r#""remote""#));
        assert!(json.contains(r#""pricing""#));
        assert!(json.contains(r#""interop""#));
        assert!(json.contains(r#""quality""#));
    }
}
