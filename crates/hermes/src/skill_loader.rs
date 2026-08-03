use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Índice semântico de skills (jcode-style): rebuild lazy, invalidado por
/// `invalidate_skill_index()` quando o CHANGE_NOTIFY lane detecta mudança.
static SKILLS_INDEXED: AtomicBool = AtomicBool::new(false);
/// Geração monotônica do índice (incrementada a cada rebuild).
static SKILL_INDEX_GEN: AtomicU32 = AtomicU32::new(0);

/// Conteudo do skill_writer embutido em tempo de compilacao.
/// Hermes usa esta constante para pre-flight checks: antes de criar skill,
/// o skill_writer DEVE estar disponivel. Se nao estiver, a criacao e negada.
pub const SKILL_WRITER_CONTENT: &str = include_str!("../../../skills/skill_writer/SKILL.md");
#[derive(Clone, Debug)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub required_tokens: Vec<u64>,
    pub instructions: String,
    pub requires_network: bool,
}

pub struct SkillLoader {
    pub skills: Vec<SkillManifest>,
}

/// Macro para declarar skill manifest estaticamente (#280l)
#[macro_export]
macro_rules! skill_manifest {
    ($name:expr, $desc:expr, $tokens:expr, $instr:expr, $net:expr) => {
        $hermes::skill_loader::SkillManifest {
            name: $crate::alloc::string::String::from($name),
            description: $crate::alloc::string::String::from($desc),
            required_tokens: $tokens.to_vec(),
            instructions: $crate::alloc::string::String::from($instr),
            requires_network: $net,
        }
    };
    ($name:expr, $desc:expr) => {
        $crate::skill_manifest!($name, $desc, &[1], "", false)
    };
}

/// Converte manifest para formato SKILL.md
impl SkillManifest {
    pub fn to_skill_md(&self) -> String {
        let tokens = self.required_tokens.iter()
            .map(|t| t.to_string()).collect::<Vec<_>>().join(",");
        alloc::format!(
            "---\nname: {}\ndescription: {}\nrequired_tokens: [{}]\n---\n\n{}\n",
            self.name, self.description, tokens, self.instructions)
    }
}

impl SkillLoader {
    pub const fn new() -> Self {
        SkillLoader { skills: Vec::new() }
    }

    /// Parse a skill markdown file, validate security, and add to registry.
    /// Gate ESTRITO ADR-0052 (delega a verify_skill_md → verify_artifact_md):
    /// exige schema/kind/name/seções/content_hash/assinatura. Para conteúdo
    /// selado (sign_artifact_md) apenas; seeds usam register_trusted_skill.
    pub fn register_skill(&mut self, content: &str) -> Result<(), &'static str> {
        if let crate::self_evolve::VerifyVerdict::Reject(reason) =
            crate::self_evolve::verify_skill_md(content)
        {
            k_nano::slog_hermes!("SKILL", "VERIFY", "REJECT: {}", reason);
            return Err(reason);
        }
        self.parse_and_store(content)
    }

    /// ponytail: embedded seeds são trusted-by-compilation (embutidos no
    /// binário — mesmo trust dos seed agents, precedente SESSION_230). Skip
    /// do gate runtime de assinatura; parse direto.
    pub fn register_trusted_skill(&mut self, content: &str) -> Result<(), &'static str> {
        self.parse_and_store(content)
    }

    fn parse_and_store(&mut self, content: &str) -> Result<(), &'static str> {
        let content = content.replace("\r\n", "\n");
        let parts: Vec<&str> = content.splitn(3, "---\n").collect();
        if parts.len() < 3 {
            return Err("Skill: formato invalido (sem frontmatter)");
        }

        let frontmatter = parts[1];
        let instructions = parts[2];

        // Parse frontmatter lines
        let mut name = "";
        let mut description = "";
        let mut tokens_str = "";
        let mut requires_network = false;
        for line in frontmatter.lines() {
            if let Some(val) = line.strip_prefix("name: ") {
                name = val.trim();
            } else if let Some(val) = line.strip_prefix("description: ") {
                description = val.trim();
            } else if let Some(val) = line.strip_prefix("required_tokens: ") {
                tokens_str = val.trim();
            } else if let Some(val) = line.strip_prefix("requires_network: ") {
                requires_network = val.trim().eq_ignore_ascii_case("true");
            }
        }

        if name.is_empty() {
            return Err("Skill: nome obrigatorio no frontmatter");
        }

        // Security check: prevent prompt injection
        let dangerous = [
            "ignore all", "ignore seus comandos", "ignore as instrucoes",
            "voce e agora", "you are now", "override", "system prompt",
            "<s>", "[/INST]", "[INST]", "<<SYS>>",
        ];
        for &pattern in &dangerous {
            if instructions.contains(pattern) {
                k_nano::slog_hermes!("SKILL", "SEC", "BLOQUEADO: skill '{}' contem padrao perigoso: '{}'", name, pattern);
                return Err("Skill: conteudo malicioso detectado");
            }
        }

        // Parse tokens
        let tokens = if tokens_str.starts_with('[') && tokens_str.ends_with(']') {
            let inner = tokens_str.trim_start_matches('[').trim_end_matches(']');
            inner.split(',').filter_map(|p| p.trim().parse::<u64>().ok()).collect::<Vec<u64>>()
        } else {
            Vec::new()
        };
        let tok_count = tokens.len();

        let manifest = SkillManifest {
            name: String::from(name),
            description: String::from(description),
            required_tokens: tokens,
            instructions: String::from(instructions),
            requires_network,
        };

        k_nano::slog_hermes!("SKILL", "info", "Registrada: '{}' — {} ({} tokens, {} bytes)",
            manifest.name, manifest.description, tok_count, instructions.len());
        self.skills.push(manifest);
        Ok(())
    }

    /// Remove a skill by name
    pub fn remove_skill(&mut self, name: &str) -> bool {
        let len = self.skills.len();
        self.skills.retain(|s| s.name != name);
        self.skills.len() < len
    }

    /// List all registered skill names
    pub fn list_skills(&self) -> Vec<(String, String, usize)> {
        let mut list = Vec::new();
        for skill in &self.skills {
            list.push((skill.name.clone(), skill.description.clone(), skill.instructions.len()));
        }
        list
    }

    /// (Re)constrói o índice semântico com todas as skills (labels "skill:<name>").
    /// ponytail: re-indexa tudo a cada rebuild — labels são estáveis e
    /// semantic_search ordena por similaridade, então sem dedup.
    fn index_skills(&self) {
        for (name, desc, _len) in self.list_skills() {
            k_ai::memory_systems::index_embedding(
                &alloc::format!("skill:{}", name),
                &alloc::format!("{}: {}", name, desc),
            );
        }
        SKILLS_INDEXED.store(true, Ordering::Relaxed);
        SKILL_INDEX_GEN.fetch_add(1, Ordering::Relaxed);
    }

    /// Hint semântico jcode-style: busca embedding do intent no índice e
    /// devolve a 1ª skill relevante (label "skill:" + similaridade >= 0.4).
    pub fn find_skill_hint(&self, intent: &str) -> Option<String> {
        if !SKILLS_INDEXED.load(Ordering::Relaxed) {
            self.index_skills();
        }
        for (label, sim) in k_ai::memory_systems::semantic_search(intent, 3) {
            if let Some(name) = label.strip_prefix("skill:") {
                if sim >= 0.4 {
                    return Some(String::from(name));
                }
            }
        }
        None
    }

    /// Build system prompt — cognitive bridge (BGE+Trinity+SOUL+L0 gated).
    pub fn build_system_prompt(&self) -> String {
        crate::cognitive_bridge::cortex_system_prompt("")
    }

    /// Prompt contextualizado com intent do usuário (+ hint semântico de skill).
    pub fn build_system_prompt_for(&self, intent: &str) -> String {
        let mut prompt = crate::cognitive_bridge::cortex_system_prompt(intent);
        if !intent.is_empty() {
            if let Some(name) = self.find_skill_hint(intent) {
                prompt.push_str(&alloc::format!(
                    "\n[SKILL-HINT] {} — skill pode ser relevante ao pedido.\n",
                    name
                ));
            }
        }
        prompt
    }
}

/// Invalida o índice de skills — o próximo prompt reconstrói (consumido pelo
/// CHANGE_NOTIFY lane: skill mudou sob o loader).
pub fn invalidate_skill_index() {
    SKILLS_INDEXED.store(false, Ordering::Relaxed);
}

pub fn load_embedded_skills() -> SkillLoader {
    let mut loader = SkillLoader::new();

    // Skills embutidas via include_str! (path relativo ao workspace root)
    let skills_raw: [&str; 4] = [
        include_str!("../../../skills/hw_identify/SKILL.md"),
        include_str!("../../../skills/self_heal/SKILL.md"),
        include_str!("../../../skills/web_scrape/SKILL.md"),
        include_str!("../../../skills/skill_writer/SKILL.md"),
    ];

    for content in &skills_raw {
        if let Err(e) = loader.register_trusted_skill(content) {
            k_nano::slog_hermes!("SKILL", "info", "Erro ao carregar skill: {}", e);
        }
    }

    let count = loader.skills.len();
    let system = loader.build_system_prompt();
    k_nano::slog_hermes!("SKILL", "info", "{} skill(s) carregadas, prompt de {} bytes", count, system.len());
    loader
}






